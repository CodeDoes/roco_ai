//! The autonomous agent: a ReAct-style observe → think → act loop.
//!
//! Drives a [`ModelBackend`] through multiple turns, parsing tool calls from
//! the model's output, executing them via the [`ToolRegistry`], and feeding
//! the results back into context until the model produces a final answer or
//! the step/budget limit is reached.

use std::sync::Arc;

use roco_engine::{CompletionRequest, ModelBackend, TokenUsage};
use roco_message::{
    assistant_response_gbnf, error::complete_with_retry, error::RetryConfig, MessageFormatOptions,
};
use roco_tools::{
    all_tools, parse_assistant_response, AssistantSegment, Tool, ToolCall, ToolRegistry,
};

use crate::error::AgentError;
use crate::subtask::SubtaskOutput;

/// Configuration for an agent run.
#[derive(Clone)]
pub struct AgentConfig {
    /// System prompt that sets the agent's role/behavior.
    pub system_prompt: String,
    /// Maximum number of ReAct iterations before giving up.
    pub max_steps: u32,
    /// Tokens generated per step (before truncation/retry).
    pub max_tokens_per_step: usize,
    /// Hard cap on total completion tokens across the run.
    pub budget_tokens: usize,
    /// Sampling temperature.
    pub temperature: f32,
    /// Allow `<think>` reasoning blocks in model output.
    pub enable_think: bool,
    /// Allow `<tool_call>` blocks and tool dispatch.
    pub enable_tools: bool,
    /// Emit think/trace to stderr during the run.
    pub verbose: bool,
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            system_prompt: "You are a helpful AI agent. You can use tools when they help. \
                Think step by step inside <think>...</think> when useful. When you have the \
                final answer, reply directly without any tool calls."
                .to_string(),
            max_steps: 12,
            max_tokens_per_step: 512,
            budget_tokens: 8192,
            temperature: 0.5,
            enable_think: true,
            enable_tools: true,
            verbose: false,
        }
    }
}

/// One iteration of the agent loop.
#[derive(Debug, Clone)]
pub struct AgentStep {
    pub step: u32,
    pub assistant_text: String,
    pub tool_calls: Vec<ToolCall>,
    pub tool_results: Vec<String>,
    pub usage: TokenUsage,
}

impl AgentStep {
    pub fn new(step: u32) -> Self {
        Self {
            step,
            assistant_text: String::new(),
            tool_calls: Vec::new(),
            tool_results: Vec::new(),
            usage: TokenUsage { prompt_tokens: 0, completion_tokens: 0 },
        }
    }
}

/// The full record of one agent run.
#[derive(Debug, Clone)]
pub struct AgentTrace {
    pub steps: Vec<AgentStep>,
    pub final_text: String,
    pub total_usage: TokenUsage,
    /// True if the model returned a final answer (no pending tool calls).
    pub completed: bool,
    /// Why the run stopped (None if completed normally).
    pub stop_reason: Option<String>,
}

impl AgentTrace {
    pub fn new() -> Self {
        Self {
            steps: Vec::new(),
            final_text: String::new(),
            total_usage: TokenUsage { prompt_tokens: 0, completion_tokens: 0 },
            completed: false,
            stop_reason: None,
        }
    }
}

/// The autonomous agent.
pub struct Agent {
    config: AgentConfig,
    tools: ToolRegistry,
}

impl Agent {
    /// Create an agent with the default built-in tool set.
    pub fn new(config: AgentConfig) -> Self {
        let mut tools = ToolRegistry::new();
        for tool in all_tools() {
            tools.register(tool);
        }
        Self { config, tools }
    }

    /// Create an agent whose tool set is the default built-ins plus the
    /// long-term memory tools (`remember` / `recall`) bound to `mem`.
    pub fn with_memory(config: AgentConfig, mem: std::sync::Arc<crate::memory::MemoryStore>) -> Self {
        let mut tools = all_tools();
        tools.extend(crate::memory::MemoryStore::scoped_tools(mem));
        Self::with_tools(config, tools)
    }

    /// Create an agent whose tool set is the default built-ins plus the
    /// `search_sessions` tool bound to `sessions`.
    pub fn with_sessions(
        config: AgentConfig,
        sessions: std::sync::Arc<crate::sessions::SessionStore>,
    ) -> Self {
        let mut tools = all_tools();
        tools.extend(crate::sessions::SessionStore::scoped_tools(sessions));
        Self::with_tools(config, tools)
    }

    /// Create an agent whose tool set is the default built-ins plus the
    /// `schedule` tool bound to `scheduler`.
    pub fn with_scheduler(
        config: AgentConfig,
        scheduler: std::sync::Arc<crate::scheduler::Scheduler>,
    ) -> Self {
        let mut tools = all_tools();
        tools.extend(crate::scheduler::Scheduler::scoped_tools(scheduler));
        Self::with_tools(config, tools)
    }

    /// Ask the backend to produce a structured plan for `task`.
    ///
    /// Returns a reviewable/resumable [`Plan`](crate::plan::Plan) (falls back to
    /// a single-step plan if the model does not emit valid plan JSON).
    pub async fn plan(&self, backend: &dyn ModelBackend, task: &str) -> Result<crate::plan::Plan, AgentError> {
        crate::plan::Planner::plan(backend, task).await
    }

    /// Create an agent with a custom tool set.
    pub fn with_tools(config: AgentConfig, tools: Vec<Arc<dyn Tool>>) -> Self {
        let mut registry = ToolRegistry::new();
        for tool in tools {
            registry.register(tool);
        }
        Self { config, tools: registry }
    }

    pub fn config(&self) -> &AgentConfig {
        &self.config
    }

    pub fn tools(&self) -> &ToolRegistry {
        &self.tools
    }

    /// Run the agent on a user task and return the full trace.
    pub async fn run(
        &self,
        backend: &dyn ModelBackend,
        task: &str,
    ) -> Result<AgentTrace, AgentError> {
        let mut trace = AgentTrace::new();
        let mut history: Vec<String> = Vec::new();
        let mut total_tokens: usize = 0;
        let mut step_count: u32 = 0;

        let options = MessageFormatOptions {
            think: self.config.enable_think,
            tools: self.config.enable_tools && !self.tools.is_empty(),
        };

        let tool_schemas: Vec<serde_json::Value> = if options.tools {
            self.tools
                .names()
                .iter()
                .filter_map(|name| self.tools.get(name).map(|t| t.schema()))
                .collect()
        } else {
            Vec::new()
        };

        loop {
            step_count += 1;
            if step_count > self.config.max_steps {
                trace.stop_reason = Some(format!("step limit ({}) reached", self.config.max_steps));
                return Ok(trace);
            }

            // Build the grammar (assistant-only response).
            let grammar = if options.tools || options.think {
                Some(assistant_response_gbnf(&options, &tool_schemas))
            } else {
                None
            };

            let prompt = self.render_prompt(task, &history);

            let req = CompletionRequest {
                system: String::new(),
                prompt,
                output_schema: None,
                grammar,
                temperature: self.config.temperature,
                max_tokens: self.config.max_tokens_per_step,
                estimated_prompt_tokens: 0,
                thinking: false,
                preserve_state: false,
                on_token: None,
                session: None,
            };

            let retry_config = RetryConfig::default();
            let resp = match complete_with_retry(backend, req, &retry_config).await {
                Ok(r) => r,
                Err(e) => return Err(AgentError::BackendError(e.to_string())),
            };

            total_tokens += resp.usage.completion_tokens as usize;
            trace.total_usage.prompt_tokens += resp.usage.prompt_tokens;
            trace.total_usage.completion_tokens += resp.usage.completion_tokens;

            if total_tokens > self.config.budget_tokens {
                trace.stop_reason =
                    Some(format!("token budget ({}) exceeded", self.config.budget_tokens));
                return Ok(trace);
            }

            let response_text = resp.text.clone();
            let segments = parse_assistant_response(&response_text);
            let mut step = AgentStep::new(step_count);
            step.usage = resp.usage.clone();

            let mut final_text = String::new();
            let mut has_tool_calls = false;

            for seg in &segments {
                match seg {
                    AssistantSegment::Text(t) => final_text.push_str(t),
                    AssistantSegment::Think(t) => {
                        if self.config.verbose {
                            eprintln!("[think:{}] {}", step_count, t);
                        }
                    }
                    AssistantSegment::ToolCall(call) => {
                        has_tool_calls = true;
                        step.tool_calls.push(call.clone());
                        let result = self.execute_tool(call).await;
                        step.tool_results.push(result.clone());
                        history.push(format!(
                            "<tool_call>{}</tool_call><tool_result>{}</tool_result>",
                            call.raw, result
                        ));
                    }
                    AssistantSegment::ToolResult(r) => {
                        if self.config.verbose {
                            eprintln!("[tool_result:{}] {}", step_count, r);
                        }
                    }
                }
            }

            step.assistant_text = final_text.clone();
            trace.steps.push(step);

            if !has_tool_calls {
                trace.final_text = final_text.trim().to_string();
                trace.completed = true;
                return Ok(trace);
            }
            // else loop again with updated history
        }
    }

    /// Run a single subtask (one model call with the subtask's context).
    pub async fn run_subtask(
        &self,
        backend: &dyn ModelBackend,
        subtask: &crate::subtask::Subtask,
    ) -> Result<SubtaskOutput, AgentError> {
        let prompt = format!(
            "System: {}\n\nUser: {}\n\n{}Assistant: ",
            self.config.system_prompt, subtask.objective, subtask.context
        );
        let req = CompletionRequest {
            system: String::new(),
            prompt,
            output_schema: None,
            grammar: None,
            temperature: subtask.temperature,
            max_tokens: subtask.max_tokens,
            estimated_prompt_tokens: 0,
            thinking: self.config.enable_think,
            preserve_state: false,
            on_token: None,
            session: None,
        };
        let resp = backend.complete(req).await.map_err(|e| AgentError::BackendError(e.to_string()))?;
        let text = resp.text.clone();
        let calls = roco_tools::extract_tool_calls(&text);
        let success = !calls.is_empty() || !text.trim().is_empty();
        Ok(SubtaskOutput {
            subtask_id: subtask.id.clone(),
            text,
            usage: resp.usage,
            success,
            tool_calls: calls,
        })
    }

    async fn execute_tool(&self, call: &ToolCall) -> String {
        match self.tools.get(&call.name) {
            None => serde_json::json!({
                "error": format!("tool '{}' not found", call.name),
                "available": self.tools.names(),
            })
            .to_string(),
            Some(tool) => match tool.call(call.arguments.clone()) {
                Ok(result) => result.to_string(),
                Err(e) => serde_json::json!({ "error": e.to_string() }).to_string(),
            },
        }
    }

    fn render_prompt(&self, task: &str, history: &[String]) -> String {
        let mut prompt = String::new();
        prompt.push_str(&format!("System: {}\n\n", self.config.system_prompt));
        prompt.push_str(&format!("User: {}\n\n", task));
        for h in history {
            prompt.push_str(&format!("Assistant: {}\n\n", h));
        }
        prompt.push_str("Assistant: ");
        prompt
    }
}

impl Default for Agent {
    fn default() -> Self {
        Self::new(AgentConfig::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use roco_engine::MockBackend;

    #[tokio::test]
    async fn agent_returns_final_text_without_tools() {
        // MockBackend returns JSON; with tools disabled the agent should
        // capture the text and mark the run complete.
        let backend = MockBackend::new("mock", 0);
        let config = AgentConfig {
            enable_tools: false,
            enable_think: false,
            ..Default::default()
        };
        let agent = Agent::new(config);
        let trace = agent.run(&backend, "What is 2+2?").await.unwrap();
        assert!(trace.completed, "run should complete");
        assert!(!trace.final_text.is_empty(), "should have final text");
        assert!(!trace.steps.is_empty(), "should have at least one step");
    }

    #[tokio::test]
    async fn agent_handles_tool_calls_via_mock() {
        // MockBackend returns canned JSON; tool execution should not panic.
        let backend = MockBackend::new("mock", 0);
        let config = AgentConfig {
            enable_tools: true,
            enable_think: false,
            max_steps: 3,
            ..Default::default()
        };
        let agent = Agent::new(config);
        let trace = agent.run(&backend, "Read the file notes.txt").await.unwrap();
        // Either completed or stopped by step limit — both are valid.
        assert!(trace.steps.len() >= 1);
    }

    #[tokio::test]
    async fn agent_respects_step_limit() {
        // A backend that always emits a tool call forces the step loop.
        // We cap max_steps low and verify the run terminates.
        let backend = MockBackend::new("mock", 0);
        let config = AgentConfig {
            enable_tools: true,
            max_steps: 2,
            ..Default::default()
        };
        let agent = Agent::new(config);
        let trace = agent.run(&backend, "do something recursive").await.unwrap();
        assert!(trace.steps.len() <= 2, "step limit should be respected");
        assert!(
            trace.stop_reason.is_some() || trace.completed,
            "run should stop with a reason or complete"
        );
    }

    #[test]
    fn agent_config_default_is_sane() {
        let config = AgentConfig::default();
        assert!(config.max_steps > 0);
        assert!(config.budget_tokens > 0);
    }
}
