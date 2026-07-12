//! Agent profiles — complete agent personalities.
//!
//! An agent is more than just a model. It's a bundle of:
//! - **Role**: orchestrator, worker, verifier, critic, memory
//! - **Model**: which foundation model drives it (RWKV, Llama, Qwen, …)
//! - **Personality**: system prompt, few-shot examples, strategy
//! - **State**: conversation history, memory, token budget
//!
//! Different foundation models behave radically differently even with the same
//! prompt. A profile captures the *strategy* that works for that model on that
//! role — not just the model identity.
//!
//! ## Agent Variants
//!
//! You can have multiple profiles for the same role, each using a different
//! foundation model. For example:
//!
//! ```text
//! orchestrator/fast   → RWKV 2.9B (quick drafting, high throughput)
//! orchestrator/smart  → Llama-3 8B on CPU (deep reasoning, slower)
//! worker/code         → Qwen2.5-Coder 1.5B (fast code completions)
//! worker/code-review  → DeepSeek-Coder 6.7B on CPU (thorough review)
//! critic/creative     → RWKV 2.9B (style critique)
//! critic/logical      → Phi-3.5 (reasoning critique)
//! ```
//!
//! ## Grouping & Routing
//!
//! Profiles are organized into groups. A group defines how tasks are routed
//! among its members: first-available, ensemble voting, escalate-on-failure,
//! round-robin, or by capability tag.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::engine::{CompletionRequest, CompletionResponse, ModelBackend};

// ---------------------------------------------------------------------------
// Core types
// ---------------------------------------------------------------------------

/// The role an agent profile fills in the multi-agent system.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentRole {
    /// Plans work, decomposes tasks, delegates to workers.
    Orchestrator,
    /// Executes atomic subtasks (the 3B specialist).
    Worker,
    /// Checks output quality against criteria.
    Verifier,
    /// Critiques output and suggests improvements.
    Critic,
    /// Manages persistent cross-session memory.
    Memory,
    /// Routes tasks between models (meta-agent).
    Router,
}

impl std::fmt::Display for AgentRole {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", match self {
            Self::Orchestrator => "orchestrator",
            Self::Worker => "worker",
            Self::Verifier => "verifier",
            Self::Critic => "critic",
            Self::Memory => "memory",
            Self::Router => "router",
        })
    }
}

/// How this agent profile approaches its tasks.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AgentStrategy {
    /// Fast, creative, iterative — higher temperature, fewer constraints.
    #[serde(rename = "fast_iterative")]
    FastIterative {
        temperature: f32,
        /// Max tokens per generation.
        max_tokens: usize,
    },
    /// Careful step-by-step reasoning — low temperature, structured.
    #[serde(rename = "step_by_step")]
    StepByStep {
        temperature: f32,
    },
    /// Structured JSON/format output.
    #[serde(rename = "structured_output")]
    StructuredOutput {
        /// JSON schema to enforce.
        schema: Option<String>,
    },
    /// Chain-of-thought reasoning.
    #[serde(rename = "chain_of_thought")]
    ChainOfThought {
        temperature: f32,
        /// Enable self-consistency (multiple CoT paths, majority vote).
        self_consistency: bool,
    },
    /// Debate between two profiles (for theorizing / argument).
    #[serde(rename = "debate")]
    Debate {
        opponent_profile: String,
        rounds: usize,
    },
    /// Escalate: try fast profile first, fall back to thorough on failure.
    #[serde(rename = "escalate")]
    Escalate {
        fast_profile: String,
        thorough_profile: String,
        /// Max retries on fast before escalating.
        max_retries: usize,
    },
}

impl Default for AgentStrategy {
    fn default() -> Self {
        Self::FastIterative {
            temperature: 0.3,
            max_tokens: 512,
        }
    }
}

/// A few-shot example for an agent profile.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FewShotExample {
    pub input: String,
    pub output: String,
    /// Optional reasoning trace (used for CoT profiles).
    pub reasoning: Option<String>,
}

/// Persistent runtime state for an agent.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AgentState {
    /// Conversation history (most recent first).
    pub conversation: Vec<Message>,
    /// Memory entries (key-value, namespaced).
    pub memory_entries: Vec<MemoryEntry>,
    /// Total tokens consumed across all sessions.
    pub total_tokens_used: usize,
    /// Number of sessions this profile has been active.
    pub session_count: usize,
}

/// A single message in an agent's conversation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: String,
    pub content: String,
    pub timestamp_ms: u64,
}

/// A key-value memory entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryEntry {
    pub key: String,
    pub value: String,
    pub namespace: String,
    pub timestamp_ms: u64,
    pub importance: f32,
}

impl MemoryEntry {
    pub fn new(namespace: &str, key: &str, value: &str, importance: f32) -> Self {
        Self {
            key: key.to_string(),
            value: value.to_string(),
            namespace: namespace.to_string(),
            timestamp_ms: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis() as u64)
                .unwrap_or(0),
            importance,
        }
    }
}

// ---------------------------------------------------------------------------
// Agent Profile
// ---------------------------------------------------------------------------

/// A complete agent personality: role + model + system prompt + strategy + state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentProfile {
    /// Unique identifier (e.g. "orchestrator/fast", "worker/code").
    pub id: String,
    /// Human-readable name.
    pub name: String,
    /// The role this profile fulfills.
    pub role: AgentRole,
    /// Reference to the foundation model (key into [`ModelRegistry`] or
    /// inference server model ID).
    pub model_ref: String,
    /// System prompt that sets the agent's personality and behavior.
    pub system_prompt: String,
    /// Few-shot examples for in-context learning.
    pub few_shot_examples: Vec<FewShotExample>,
    /// Strategy for approaching tasks.
    pub strategy: AgentStrategy,
    /// Tags describing what this variant is strong at.
    pub capabilities: Vec<String>,
    /// Tags describing weaknesses.
    pub weaknesses: Vec<String>,
    /// Runtime state (conversation, memory, tokens used).
    #[serde(default)]
    pub state: AgentState,
}

impl AgentProfile {
    /// Create a new agent profile with default strategy and empty state.
    pub fn new(
        id: impl Into<String>,
        name: impl Into<String>,
        role: AgentRole,
        model_ref: impl Into<String>,
        system_prompt: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            role,
            model_ref: model_ref.into(),
            system_prompt: system_prompt.into(),
            few_shot_examples: Vec::new(),
            strategy: AgentStrategy::default(),
            capabilities: Vec::new(),
            weaknesses: Vec::new(),
            state: AgentState::default(),
        }
    }

    pub fn with_few_shot(mut self, examples: Vec<FewShotExample>) -> Self {
        self.few_shot_examples = examples;
        self
    }

    pub fn with_strategy(mut self, strategy: AgentStrategy) -> Self {
        self.strategy = strategy;
        self
    }

    pub fn with_capabilities(mut self, caps: Vec<&str>) -> Self {
        self.capabilities = caps.into_iter().map(|s| s.to_string()).collect();
        self
    }

    pub fn with_weaknesses(mut self, weaks: Vec<&str>) -> Self {
        self.weaknesses = weaks.into_iter().map(|s| s.to_string()).collect();
        self
    }

    /// Build the full system prompt including few-shot examples.
    pub fn build_system_prompt(&self) -> String {
        let mut prompt = self.system_prompt.clone();
        if !self.few_shot_examples.is_empty() {
            prompt.push_str("\n\n--- Examples ---\n");
            for (i, ex) in self.few_shot_examples.iter().enumerate() {
                prompt.push_str(&format!(
                    "\nExample {}:\nInput: {}\nOutput: {}\n",
                    i + 1,
                    ex.input,
                    ex.output
                ));
                if let Some(ref reasoning) = ex.reasoning {
                    prompt.push_str(&format!("Reasoning: {}\n", reasoning));
                }
            }
        }
        prompt
    }

    /// Build a completion request from a user prompt, using this profile's
    /// system prompt, strategy settings, and conversation state.
    pub fn build_request(&self, user_input: &str, max_tokens: Option<usize>) -> CompletionRequest {
        let (temperature, default_max) = match &self.strategy {
            AgentStrategy::FastIterative { temperature, max_tokens } => {
                (*temperature, *max_tokens)
            }
            AgentStrategy::StepByStep { temperature } => (*temperature, 1024),
            AgentStrategy::StructuredOutput { .. } => (0.1, 512),
            AgentStrategy::ChainOfThought { temperature, .. } => (*temperature, 2048),
            AgentStrategy::Debate { .. } => (0.5, 1024),
            AgentStrategy::Escalate { .. } => (0.2, 512),
        };

        CompletionRequest {
            system: self.build_system_prompt(),
            prompt: user_input.to_string(),
            output_schema: match &self.strategy {
                AgentStrategy::StructuredOutput { schema } => schema.clone(),
                _ => None,
            },
            temperature,
            max_tokens: max_tokens.unwrap_or(default_max),
            estimated_prompt_tokens: 0,
        }
    }

    /// Record a completion into this profile's state.
    pub fn record_completion(&mut self, request: &CompletionRequest, response: &CompletionResponse) {
        self.state.conversation.push(Message {
            role: "user".into(),
            content: request.prompt.clone(),
            timestamp_ms: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis() as u64)
                .unwrap_or(0),
        });
        self.state.conversation.push(Message {
            role: "assistant".into(),
            content: response.text.clone(),
            timestamp_ms: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis() as u64)
                .unwrap_or(0),
        });
        self.state.total_tokens_used += response.usage.total();
    }
}

// ---------------------------------------------------------------------------
// Agent Groups
// ---------------------------------------------------------------------------

/// How tasks are routed among profiles in a group.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RoutingStrategy {
    /// Use the first profile that's available/loaded.
    FirstAvailable,
    /// Run all profiles and combine results by voting.
    Ensemble {
        vote: VoteStrategy,
    },
    /// Try the fast profile first; escalate to thorough on failure.
    EscalateOnFailure {
        on_failure: String,
    },
    /// Route by capability tag (e.g., "code", "creative", "math").
    ByCapability,
    /// Round-robin across profiles.
    RoundRobin,
}

/// Voting strategies for ensemble routing.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VoteStrategy {
    /// Majority vote wins.
    Majority,
    /// All must agree.
    Unanimous,
    /// Pick the best of N (requires a scoring function).
    BestOfN,
}

/// A named group of agent profiles with a routing strategy.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentGroup {
    pub name: String,
    pub description: String,
    /// Profile IDs that belong to this group.
    pub profile_ids: Vec<String>,
    pub routing: RoutingStrategy,
    /// Default profile to use when routing doesn't select one.
    pub default_profile: Option<String>,
}

impl AgentGroup {
    pub fn new(name: impl Into<String>, description: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            description: description.into(),
            profile_ids: Vec::new(),
            routing: RoutingStrategy::FirstAvailable,
            default_profile: None,
        }
    }

    pub fn with_profiles(mut self, ids: Vec<&str>) -> Self {
        self.profile_ids = ids.into_iter().map(|s| s.to_string()).collect();
        self
    }

    pub fn with_routing(mut self, routing: RoutingStrategy) -> Self {
        self.routing = routing;
        self
    }
}

// ---------------------------------------------------------------------------
// Agent Profile Registry
// ---------------------------------------------------------------------------

/// Manages agent profiles, their backends, and routing groups.
pub struct AgentProfileRegistry {
    profiles: HashMap<String, AgentProfile>,
    groups: HashMap<String, AgentGroup>,
    /// Active model backends keyed by profile ID (not model ID — multiple
    /// profiles can share a backend if they use the same model).
    active_backends: HashMap<String, Box<dyn ModelBackend + Send + Sync>>,
}

impl AgentProfileRegistry {
    pub fn new() -> Self {
        Self {
            profiles: HashMap::new(),
            groups: HashMap::new(),
            active_backends: HashMap::new(),
        }
    }

    // --- Profile management --- //

    pub fn register(&mut self, profile: AgentProfile) {
        self.profiles.insert(profile.id.clone(), profile);
    }

    pub fn get(&self, id: &str) -> Option<&AgentProfile> {
        self.profiles.get(id)
    }

    pub fn get_mut(&mut self, id: &str) -> Option<&mut AgentProfile> {
        self.profiles.get_mut(id)
    }

    pub fn unregister(&mut self, id: &str) -> Option<AgentProfile> {
        self.active_backends.remove(id);
        self.profiles.remove(id)
    }

    pub fn all_profiles(&self) -> Vec<&AgentProfile> {
        self.profiles.values().collect()
    }

    pub fn all_profiles_for_role(&self, role: AgentRole) -> Vec<&AgentProfile> {
        self.profiles.values().filter(|p| p.role == role).collect()
    }

    pub fn profile_count(&self) -> usize {
        self.profiles.len()
    }

    // --- Group management --- //

    pub fn register_group(&mut self, group: AgentGroup) {
        self.groups.insert(group.name.clone(), group);
    }

    pub fn get_group(&self, name: &str) -> Option<&AgentGroup> {
        self.groups.get(name)
    }

    pub fn all_groups(&self) -> Vec<&AgentGroup> {
        self.groups.values().collect()
    }

    /// Select profiles from a group based on the routing strategy and a
    /// capability tag (used with `ByCapability` routing).
    pub fn select_from_group(&self, group_name: &str, task_capability: Option<&str>) -> Vec<&AgentProfile> {
        let group = match self.groups.get(group_name) {
            Some(g) => g,
            None => return Vec::new(),
        };

        match &group.routing {
            RoutingStrategy::FirstAvailable => {
                // Return the first profile that has an active backend
                for id in &group.profile_ids {
                    if self.active_backends.contains_key(id) {
                        if let Some(p) = self.profiles.get(id) {
                            return vec![p];
                        }
                    }
                }
                // Fall back to default
                if let Some(default) = &group.default_profile {
                    if let Some(p) = self.profiles.get(default) {
                        return vec![p];
                    }
                }
                Vec::new()
            }
            RoutingStrategy::Ensemble { .. } => {
                group.profile_ids.iter()
                    .filter_map(|id| self.profiles.get(id))
                    .collect()
            }
            RoutingStrategy::EscalateOnFailure { .. } => {
                // Return all profiles; caller escalates on failure
                group.profile_ids.iter()
                    .filter_map(|id| self.profiles.get(id))
                    .collect()
            }
            RoutingStrategy::ByCapability => {
                if let Some(cap) = task_capability {
                    group.profile_ids.iter()
                        .filter_map(|id| self.profiles.get(id))
                        .filter(|p| p.capabilities.iter().any(|c| c == cap))
                        .collect()
                } else {
                    group.profile_ids.iter()
                        .filter_map(|id| self.profiles.get(id))
                        .collect()
                }
            }
            RoutingStrategy::RoundRobin => {
                // Simple: return all and let caller round-robin
                group.profile_ids.iter()
                    .filter_map(|id| self.profiles.get(id))
                    .collect()
            }
        }
    }

    // --- Backend management --- //

    /// Attach a model backend to a profile, making it "active" (usable).
    pub fn attach_backend(
        &mut self,
        profile_id: &str,
        backend: Box<dyn ModelBackend + Send + Sync>,
    ) -> Result<(), String> {
        if !self.profiles.contains_key(profile_id) {
            return Err(format!("profile '{profile_id}' not found"));
        }
        self.active_backends.insert(profile_id.to_string(), backend);
        Ok(())
    }

    /// Detach (unload) a profile's backend, freeing memory.
    pub fn detach_backend(&mut self, profile_id: &str) -> Option<Box<dyn ModelBackend + Send + Sync>> {
        self.active_backends.remove(profile_id)
    }

    /// Get a reference to a profile's backend, if loaded.
    pub fn get_backend(&self, profile_id: &str) -> Option<&(dyn ModelBackend + Send + Sync)> {
        self.active_backends.get(profile_id).map(|b| b.as_ref())
    }

    /// Check if a profile's backend is currently loaded.
    pub fn is_active(&self, profile_id: &str) -> bool {
        self.active_backends.contains_key(profile_id)
    }

    /// List all active profile IDs.
    pub fn active_profiles(&self) -> Vec<&str> {
        self.active_backends.keys().map(|s| s.as_str()).collect()
    }

    /// Swap a profile's model reference (does NOT reload the backend).
    pub fn swap_model_ref(&mut self, profile_id: &str, new_model_ref: &str) -> Result<(), String> {
        let profile = self.profiles.get_mut(profile_id)
            .ok_or_else(|| format!("profile '{profile_id}' not found"))?;
        profile.model_ref = new_model_ref.to_string();
        // Backend must be re-attached separately (or via inference server)
        self.active_backends.remove(profile_id);
        Ok(())
    }
}

impl Default for AgentProfileRegistry {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Built-in profile presets
// ---------------------------------------------------------------------------

/// Preset profiles for common model+role combinations.
pub mod presets {
    use super::*;

    /// A fast, creative storyteller (RWKV 2.9B).
    pub fn storyteller_rwkv() -> AgentProfile {
        AgentProfile::new(
            "storyteller/fast",
            "Storyteller (RWKV)",
            AgentRole::Worker,
            "rwkv-2.9b",
            "You are a creative storyteller. Write vivid, engaging prose. \
             Show, don't tell. Use sensory details and varied sentence structure.",
        )
        .with_strategy(AgentStrategy::FastIterative {
            temperature: 0.6,
            max_tokens: 1024,
        })
        .with_capabilities(vec!["creative", "prose", "fast"])
        .with_weaknesses(vec!["code", "math", "factual"])
    }

    /// A precise, structured code writer (Qwen2.5-Coder 1.5B).
    pub fn coder_fast() -> AgentProfile {
        AgentProfile::new(
            "coder/fast",
            "Coder (Qwen-Coder)",
            AgentRole::Worker,
            "qwen-coder-1.5b",
            "You are a precise code generator. Output only valid code. \
             Add minimal comments. Follow the requested language's conventions.",
        )
        .with_strategy(AgentStrategy::StructuredOutput {
            schema: None,
        })
        .with_capabilities(vec!["code", "rust", "typescript", "python"])
        .with_weaknesses(vec!["creative", "explanation"])
    }

    /// A thorough code reviewer (DeepSeek-Coder 6.7B on CPU).
    pub fn code_reviewer_thorough() -> AgentProfile {
        AgentProfile::new(
            "coder/review",
            "Code Reviewer (DeepSeek-Coder)",
            AgentRole::Verifier,
            "deepseek-coder-6.7b",
            "You are a thorough code reviewer. Check for: correctness, \
             edge cases, performance issues, security vulnerabilities, \
             and style consistency. Provide specific, actionable feedback.",
        )
        .with_strategy(AgentStrategy::StepByStep {
            temperature: 0.1,
        })
        .with_capabilities(vec!["code-review", "security", "refactoring"])
        .with_weaknesses(vec!["speed"])
    }

    /// A deep reasoning orchestrator (Llama-3 8B on CPU).
    pub fn orchestrator_smart() -> AgentProfile {
        AgentProfile::new(
            "orchestrator/smart",
            "Orchestrator (Llama-3)",
            AgentRole::Orchestrator,
            "llama-3-8b",
            "You are a strategic planner. Break down complex tasks into \
             clear, executable steps. Assign each step to the right specialist. \
             Verify results before accepting them. Think step by step.",
        )
        .with_strategy(AgentStrategy::ChainOfThought {
            temperature: 0.2,
            self_consistency: true,
        })
        .with_capabilities(vec!["planning", "decomposition", "reasoning"])
        .with_weaknesses(vec!["speed", "real-time"])
    }

    /// A fast, chatty assistant (RWKV 2.9B).
    pub fn assistant_fast() -> AgentProfile {
        AgentProfile::new(
            "assistant/fast",
            "Assistant (RWKV)",
            AgentRole::Worker,
            "rwkv-2.9b",
            "You are a friendly, helpful assistant. Be concise but warm. \
             If you don't know something, say so. Use simple language.",
        )
        .with_strategy(AgentStrategy::FastIterative {
            temperature: 0.4,
            max_tokens: 256,
        })
        .with_capabilities(vec!["chat", "quick-answers", "creative"])
        .with_weaknesses(vec!["complex-reasoning", "code"])
    }

    /// A debate agent for theorizing.
    pub fn theorist() -> AgentProfile {
        AgentProfile::new(
            "meta/theorist",
            "Theorist",
            AgentRole::Critic,
            "rwkv-2.9b",
            "You are a creative theorist. Generate novel hypotheses, \
             challenge assumptions, and explore edge cases. Think outside the box.",
        )
        .with_strategy(AgentStrategy::Debate {
            opponent_profile: "meta/critic".into(),
            rounds: 3,
        })
        .with_capabilities(vec!["theorizing", "brainstorming", "what-if"])
        .with_weaknesses(vec!["practicality", "grounding"])
    }

    /// A sharp critic for the debate.
    pub fn critic() -> AgentProfile {
        AgentProfile::new(
            "meta/critic",
            "Critic",
            AgentRole::Critic,
            "phi-3.5",
            "You are a sharp, logical critic. Identify flaws in arguments, \
             point out assumptions, demand evidence. Be constructive but rigorous.",
        )
        .with_strategy(AgentStrategy::StepByStep {
            temperature: 0.2,
        })
        .with_capabilities(vec!["critique", "logic", "analysis"])
        .with_weaknesses(vec!["creativity"])
    }

    /// Return all preset profiles.
    pub fn all_presets() -> Vec<AgentProfile> {
        vec![
            storyteller_rwkv(),
            coder_fast(),
            code_reviewer_thorough(),
            orchestrator_smart(),
            assistant_fast(),
            theorist(),
            critic(),
        ]
    }

    /// Default groups for the preset profiles.
    pub fn default_groups() -> Vec<AgentGroup> {
        vec![
            AgentGroup::new("writing", "Creative writing tasks")
                .with_profiles(vec!["storyteller/fast"])
                .with_routing(RoutingStrategy::FirstAvailable),
            AgentGroup::new("coding", "Software development tasks")
                .with_profiles(vec!["coder/fast", "coder/review"])
                .with_routing(RoutingStrategy::EscalateOnFailure {
                    on_failure: "coder/review".into(),
                }),
            AgentGroup::new("orchestration", "Task planning and decomposition")
                .with_profiles(vec!["orchestrator/smart"])
                .with_routing(RoutingStrategy::FirstAvailable),
            AgentGroup::new("assistance", "General chat and quick tasks")
                .with_profiles(vec!["assistant/fast"])
                .with_routing(RoutingStrategy::FirstAvailable),
            AgentGroup::new("meta", "Self-reflection and theorizing")
                .with_profiles(vec!["meta/theorist", "meta/critic"])
                .with_routing(RoutingStrategy::Ensemble {
                    vote: VoteStrategy::BestOfN,
                }),
        ]
    }
}

// ---------------------------------------------------------------------------
// Profile serialization (from JSON/YAML config files)
// ---------------------------------------------------------------------------

impl AgentProfile {
    /// Save this profile to a JSON file.
    pub fn save_to_file(&self, path: impl AsRef<std::path::Path>) -> Result<(), std::io::Error> {
        let json = serde_json::to_string_pretty(self)?;
        if let Some(parent) = path.as_ref().parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, json)
    }

    /// Load a profile from a JSON file.
    pub fn load_from_file(path: impl AsRef<std::path::Path>) -> Result<Self, std::io::Error> {
        let data = std::fs::read_to_string(path)?;
        let profile: AgentProfile = serde_json::from_str(&data)?;
        Ok(profile)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use super::presets::*;

    #[test]
    fn test_profile_creation() {
        let p = storyteller_rwkv();
        assert_eq!(p.id, "storyteller/fast");
        assert_eq!(p.role, AgentRole::Worker);
        assert_eq!(p.model_ref, "rwkv-2.9b");
        assert!(p.capabilities.contains(&"creative".to_string()));
    }

    #[test]
    fn test_coder_profile() {
        let p = coder_fast();
        assert_eq!(p.id, "coder/fast");
        assert!(p.capabilities.contains(&"code".to_string()));
        assert!(p.weaknesses.contains(&"creative".to_string()));
    }

    #[test]
    fn test_build_system_prompt_without_examples() {
        let p = assistant_fast();
        let prompt = p.build_system_prompt();
        assert!(prompt.contains("helpful assistant"));
        assert!(!prompt.contains("--- Examples ---"));
    }

    #[test]
    fn test_build_system_prompt_with_examples() {
        let p = coder_fast().with_few_shot(vec![
            FewShotExample {
                input: "Write a function that adds two numbers".into(),
                output: "fn add(a: i32, b: i32) -> i32 { a + b }".into(),
                reasoning: None,
            },
        ]);
        let prompt = p.build_system_prompt();
        assert!(prompt.contains("--- Examples ---"));
        assert!(prompt.contains("fn add"));
    }

    #[test]
    fn test_build_request_uses_strategy_settings() {
        let p = storyteller_rwkv();
        let req = p.build_request("Write a story", None);
        assert!((req.temperature - 0.6).abs() < 0.01);
        assert_eq!(req.max_tokens, 1024);

        let p2 = coder_fast();
        let req2 = p2.build_request("Write a function", None);
        assert!((req2.temperature - 0.1).abs() < 0.01);
        assert_eq!(req2.max_tokens, 512);
    }

    #[test]
    fn test_registry_basic_ops() {
        let mut reg = AgentProfileRegistry::new();
        assert_eq!(reg.profile_count(), 0);

        reg.register(storyteller_rwkv());
        reg.register(coder_fast());
        assert_eq!(reg.profile_count(), 2);

        assert!(reg.get("storyteller/fast").is_some());
        assert!(reg.get("coder/fast").is_some());
        assert!(reg.get("nonexistent").is_none());

        let removed = reg.unregister("coder/fast");
        assert!(removed.is_some());
        assert_eq!(reg.profile_count(), 1);
    }

    #[test]
    fn test_registry_role_filter() {
        let mut reg = AgentProfileRegistry::new();
        reg.register(storyteller_rwkv());  // Worker
        reg.register(coder_fast());        // Worker
        reg.register(orchestrator_smart());// Orchestrator
        reg.register(assistant_fast());    // Worker
        reg.register(theorist());          // Critic

        let workers = reg.all_profiles_for_role(AgentRole::Worker);
        assert_eq!(workers.len(), 3);

        let orchestrators = reg.all_profiles_for_role(AgentRole::Orchestrator);
        assert_eq!(orchestrators.len(), 1);

        let critics = reg.all_profiles_for_role(AgentRole::Critic);
        assert_eq!(critics.len(), 1);
    }

    #[test]
    fn test_registry_groups() {
        let mut reg = AgentProfileRegistry::new();
        for p in presets::all_presets() {
            reg.register(p);
        }

        for g in presets::default_groups() {
            reg.register_group(g);
        }

        assert_eq!(reg.all_groups().len(), 5);

        let writing = reg.get_group("writing").unwrap();
        assert_eq!(writing.profile_ids, vec!["storyteller/fast"]);
    }

    #[test]
    fn test_select_from_group_by_capability() {
        let mut reg = AgentProfileRegistry::new();
        reg.register(coder_fast());
        reg.register(code_reviewer_thorough());
        reg.register(storyteller_rwkv());

        reg.register_group(
            AgentGroup::new("all", "All profiles")
                .with_profiles(vec!["coder/fast", "coder/review", "storyteller/fast"])
                .with_routing(RoutingStrategy::ByCapability),
        );

        let code_profiles = reg.select_from_group("all", Some("code"));
        assert_eq!(code_profiles.len(), 1);
        assert_eq!(code_profiles[0].id, "coder/fast");

        let review_profiles = reg.select_from_group("all", Some("code-review"));
        assert_eq!(review_profiles.len(), 1);
        assert_eq!(review_profiles[0].id, "coder/review");
    }

    #[test]
    fn test_memory_entry() {
        let entry = MemoryEntry::new("user", "favorite_color", "blue", 0.8);
        assert_eq!(entry.key, "favorite_color");
        assert_eq!(entry.value, "blue");
        assert_eq!(entry.namespace, "user");
        assert!(entry.timestamp_ms > 0);
    }

    #[test]
    fn test_serialization_roundtrip() {
        let p = storyteller_rwkv();
        let json = serde_json::to_string_pretty(&p).unwrap();
        let deserialized: AgentProfile = serde_json::from_str(&json).unwrap();
        assert_eq!(p.id, deserialized.id);
        assert_eq!(p.name, deserialized.name);
        assert_eq!(p.role, deserialized.role);
        assert_eq!(p.model_ref, deserialized.model_ref);
        assert_eq!(p.capabilities, deserialized.capabilities);
    }

    #[test]
    fn test_save_load_roundtrip() {
        let p = coder_fast();
        let path = std::env::temp_dir().join("test_agent_profile.json");
        p.save_to_file(&path).unwrap();
        let loaded = AgentProfile::load_from_file(&path).unwrap();
        assert_eq!(p.id, loaded.id);
        assert_eq!(p.name, loaded.name);
        assert_eq!(p.model_ref, loaded.model_ref);
        std::fs::remove_file(path).ok();
    }
}
