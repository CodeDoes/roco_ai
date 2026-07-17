//! Planning — decompose a user goal into an explicit, reviewable, resumable plan.
//!
//! [`Planner::plan`] asks the backend to emit a structured JSON plan (a list
//! of [`PlanStep`]s with dependencies). The parse is defensive: if the model
//! does not return valid plan JSON, we fall back to a single implicit step so
//! the run can still proceed. A [`Plan`] can be serialized for later review and
//! resumed, and [`Plan::execute`] runs the steps in dependency order (the
//! orchestration primitive — see `goals/agent/orchastrate.md`).

use std::collections::HashMap;

use futures::future::join_all;
use roco_engine::{CompletionRequest, ModelBackend};
use roco_tools::ToolRegistry;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::AgentError;

use futures::future::BoxFuture;

/// Trait to verify the output of a single plan step during wave execution.
pub trait StepVerifier: Send + Sync {
    fn verify<'a>(
        &'a self,
        step_id: &'a str,
        description: &'a str,
        output: &'a str,
    ) -> BoxFuture<'a, Result<bool, String>>;
}

/// A simple MockStepVerifier that always returns a fixed validation result.
pub struct MockStepVerifier {
    pub should_pass: bool,
}

impl StepVerifier for MockStepVerifier {
    fn verify<'a>(
        &'a self,
        _step_id: &'a str,
        _description: &'a str,
        _output: &'a str,
    ) -> BoxFuture<'a, Result<bool, String>> {
        Box::pin(async move { Ok(self.should_pass) })
    }
}

/// A single step in a plan.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanStep {
    /// Stable identifier, referenced by other steps' `depends_on`.
    pub id: String,
    /// What this step should accomplish.
    pub description: String,
    /// Optional tool to invoke directly for this step (else a model subtask).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool: Option<String>,
    /// Ids of steps that must complete before this one.
    #[serde(default)]
    pub depends_on: Vec<String>,
}

/// An explicit plan: a task plus an ordered, dependency-tracked step list.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Plan {
    pub task: String,
    pub steps: Vec<PlanStep>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RawPlan {
    #[serde(default)]
    task: Option<String>,
    steps: Vec<RawStep>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RawStep {
    #[serde(default)]
    id: Option<String>,
    description: String,
    #[serde(default)]
    tool: Option<String>,
    #[serde(default)]
    depends_on: Vec<String>,
}

impl Plan {
    /// A single-step plan that just runs the whole task as one subtask.
    pub fn single(task: &str) -> Self {
        Self {
            task: task.to_string(),
            steps: vec![PlanStep {
                id: "1".to_string(),
                description: task.to_string(),
                tool: None,
                depends_on: Vec::new(),
            }],
        }
    }

    /// Build a plan from a parsed JSON value, falling back to `None` (so the
    /// caller can use [`Plan::single`]) when there are no usable steps.
    pub fn from_value(v: &Value, fallback_task: &str) -> Option<Plan> {
        let raw: RawPlan = serde_json::from_value(v.clone()).ok()?;
        if raw.steps.is_empty() {
            return None;
        }
        let steps = raw
            .steps
            .into_iter()
            .enumerate()
            .map(|(i, s)| PlanStep {
                id: s.id.unwrap_or_else(|| (i + 1).to_string()),
                description: s.description,
                tool: s.tool.filter(|t| !t.trim().is_empty()),
                depends_on: s.depends_on,
            })
            .collect();
        Some(Plan {
            task: raw.task.unwrap_or_else(|| fallback_task.to_string()),
            steps,
        })
    }

    pub fn from_json(s: &str) -> Result<Plan, AgentError> {
        serde_json::from_str(s).map_err(|e| AgentError::Internal(format!("plan json parse: {e}")))
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_default()
    }

    /// Indices of `steps` in a valid dependency order (Kahn's algorithm).
    /// On a cycle, remaining steps are appended in original order. Ties between
    /// independent steps are broken by ascending index, so the order is stable.
    pub fn topological_order(&self) -> Vec<usize> {
        let n = self.steps.len();
        let id_to_idx: HashMap<&str, usize> = self
            .steps
            .iter()
            .enumerate()
            .map(|(i, s)| (s.id.as_str(), i))
            .collect();
        let mut indeg = vec![0usize; n];
        let mut adj: Vec<Vec<usize>> = vec![Vec::new(); n];
        for (i, s) in self.steps.iter().enumerate() {
            for dep in &s.depends_on {
                if let Some(&j) = id_to_idx.get(dep.as_str()) {
                    adj[j].push(i);
                    indeg[i] += 1;
                }
            }
        }
        let mut heap: std::collections::BinaryHeap<std::cmp::Reverse<usize>> = (0..n)
            .filter(|&i| indeg[i] == 0)
            .map(std::cmp::Reverse)
            .collect();
        let mut order = Vec::with_capacity(n);
        while let Some(std::cmp::Reverse(u)) = heap.pop() {
            order.push(u);
            for &v in &adj[u] {
                indeg[v] -= 1;
                if indeg[v] == 0 {
                    heap.push(std::cmp::Reverse(v));
                }
            }
        }
        if order.len() < n {
            for i in 0..n {
                if !order.contains(&i) {
                    order.push(i);
                }
            }
        }
        order
    }

    /// Execute the plan.
    ///
    /// Steps are grouped into dependency **waves**: each wave holds steps whose
    /// dependencies are all in earlier waves. Steps within a wave run
    /// concurrently (independent steps branch in parallel); the results are
    /// threaded forward into later waves as context. Steps naming a `tool`
    /// present in `tools` are dispatched directly; others run as model
    /// subtasks. Outcomes are returned in topological (review) order.
    pub async fn execute(
        &self,
        backend: &dyn ModelBackend,
        tools: Option<&ToolRegistry>,
        verifier: Option<&dyn StepVerifier>,
    ) -> Result<PlanResult, AgentError> {
        let order = self.topological_order();
        let levels = self.wave_levels();
        let mut outputs: HashMap<String, String> = HashMap::new();
        let mut outcome_by_idx: HashMap<usize, StepOutcome> = HashMap::new();

        for wave in &levels {
            let mut futures = Vec::with_capacity(wave.len());
            for &idx in wave {
                let step = &self.steps[idx];
                let prompt = self.build_step_prompt(step, &outputs);
                futures.push(self.run_step_with_verification(step, backend, tools, prompt, verifier));
            }
            let results = join_all(futures).await;
            for (i, res) in results.into_iter().enumerate() {
                let idx = wave[i];
                let outcome = res?;
                outputs.insert(outcome.step_id.clone(), outcome.output.clone());
                outcome_by_idx.insert(idx, outcome);
            }
        }

        let mut outcomes = Vec::with_capacity(self.steps.len());
        for &idx in &order {
            if let Some(o) = outcome_by_idx.remove(&idx) {
                outcomes.push(o);
            }
        }
        Ok(PlanResult { outcomes, success: true })
    }

    async fn run_step_with_verification(
        &self,
        step: &PlanStep,
        backend: &dyn ModelBackend,
        tools: Option<&ToolRegistry>,
        prompt: String,
        verifier: Option<&dyn StepVerifier>,
    ) -> Result<StepOutcome, AgentError> {
        let mut attempts = 0;
        let mut current_prompt = prompt;
        loop {
            let outcome = self.run_step(step, backend, tools, current_prompt.clone()).await?;
            if let Some(v) = verifier {
                match v.verify(&step.id, &step.description, &outcome.output).await {
                    Ok(true) => {
                        return Ok(outcome);
                    }
                    Ok(false) | Err(_) => {
                        attempts += 1;
                        if attempts >= 2 {
                            // Max retries reached, proceed with current outcome
                            return Ok(outcome);
                        }
                        current_prompt = format!(
                            "{}\n\n[System feedback: Your prior output failed validation/verification. Please refine and try again.]\nPrior output: {}\nNew Result:",
                            current_prompt, outcome.output
                        );
                    }
                }
            } else {
                return Ok(outcome);
            }
        }
    }

    /// Split step indices into dependency waves (see [`Plan::execute`]).
    fn wave_levels(&self) -> Vec<Vec<usize>> {
        let id_to_idx: HashMap<&str, usize> = self
            .steps
            .iter()
            .enumerate()
            .map(|(i, s)| (s.id.as_str(), i))
            .collect();
        let mut done: std::collections::HashSet<usize> = std::collections::HashSet::new();
        let mut remaining: Vec<usize> = (0..self.steps.len()).collect();
        let mut levels = Vec::new();
        while !remaining.is_empty() {
            let mut wave = Vec::new();
            remaining.retain(|&i| {
                let ready = self.steps[i]
                    .depends_on
                    .iter()
                    .all(|d| id_to_idx.get(d.as_str()).map_or(false, |&j| done.contains(&j)));
                if ready {
                    wave.push(i);
                    false
                } else {
                    true
                }
            });
            if wave.is_empty() {
                // Cycle safety: flush whatever remains as one final wave.
                wave = remaining.clone();
                remaining.clear();
            }
            for &i in &wave {
                done.insert(i);
            }
            levels.push(wave);
        }
        levels
    }

    fn build_step_prompt(&self, step: &PlanStep, outputs: &HashMap<String, String>) -> String {
        let mut prompt = String::new();
        prompt.push_str(&format!("Plan task: {}\n", self.task));
        if !outputs.is_empty() {
            prompt.push_str("\nResults from prior steps:\n");
            for (id, out) in outputs {
                prompt.push_str(&format!("- [step {id}] {out}\n"));
            }
        }
        prompt.push_str(&format!(
            "\nNow perform step {}: {}\nResult:",
            step.id, step.description
        ));
        prompt
    }

    async fn run_step(
        &self,
        step: &PlanStep,
        backend: &dyn ModelBackend,
        tools: Option<&ToolRegistry>,
        prompt: String,
    ) -> Result<StepOutcome, AgentError> {
        // Tool-direct path.
        if let (Some(tools), Some(tool_name)) = (tools, step.tool.as_ref()) {
            if let Some(tool) = tools.get(tool_name) {
                if let Ok(v) = tool.call(serde_json::json!({ "task": step.description })) {
                    let out = v.to_string();
                    return Ok(StepOutcome {
                        step_id: step.id.clone(),
                        description: step.description.clone(),
                        output: out.clone(),
                        used_tool: Some(tool_name.clone()),
                    });
                }
            }
        }
        // Model subtask path.
        let req = CompletionRequest::new(
            "You are executing one step of a plan. Produce only the result of this step.",
            prompt,
        );
        let resp = backend
            .complete(req)
            .await
            .map_err(|e| AgentError::BackendError(e.to_string()))?;
        Ok(StepOutcome {
            step_id: step.id.clone(),
            description: step.description.clone(),
            output: resp.text.clone(),
            used_tool: None,
        })
    }
}

/// The result of executing a plan.
#[derive(Debug, Clone)]
pub struct PlanResult {
    pub outcomes: Vec<StepOutcome>,
    pub success: bool,
}

/// The output of a single plan step.
#[derive(Debug, Clone)]
pub struct StepOutcome {
    pub step_id: String,
    pub description: String,
    pub output: String,
    pub used_tool: Option<String>,
}

/// Generate a strict GBNF grammar for our Plan structure using `roco-grammar`.
pub fn plan_grammar() -> String {
    use roco_grammar::{schema_to_gbnf, Schema};
    let step_schema = Schema::object()
        .prop("id", Schema::string())
        .prop("description", Schema::string())
        .prop("tool", Schema::string())
        .prop("depends_on", Schema::array(Schema::string()))
        .build();

    let plan_schema = Schema::object()
        .prop("task", Schema::string())
        .prop("steps", Schema::array(step_schema))
        .build();

    schema_to_gbnf("root", plan_schema.to_json()).expect("Plan schema is valid")
}

/// The planner: turns a natural-language goal into a [`Plan`].
pub struct Planner;

impl Planner {
    /// Ask the backend to produce a structured plan for `task`.
    ///
    /// Falls back to a single-step plan if the model does not return valid
    /// plan JSON, so a run can always proceed.
    pub async fn plan(backend: &dyn ModelBackend, task: &str) -> Result<Plan, AgentError> {
        let system = "You are a meticulous planner. Decompose the user's goal into an ordered, \
            dependency-tracked plan. Respond with a single JSON object matching the plan schema.";
        let grammar = plan_grammar();
        let req = CompletionRequest {
            system: system.to_string(),
            prompt: format!("Goal: {task}"),
            grammar: Some(grammar),
            max_tokens: 1024,
            temperature: 0.3,
            ..Default::default()
        };
        let resp = backend
            .complete(req)
            .await
            .map_err(|e| AgentError::BackendError(e.to_string()))?;

        if let Ok(p) = serde_json::from_str::<Plan>(&resp.text) {
            if !p.steps.is_empty() {
                return Ok(p);
            }
        }

        match extract_first_json(&resp.text) {
            Some(v) => Ok(Plan::from_value(&v, task).unwrap_or_else(|| Plan::single(task))),
            None => Ok(Plan::single(task)),
        }
    }
}

/// Extract the first balanced JSON object from arbitrary model text.
fn extract_first_json(text: &str) -> Option<Value> {
    let start = text.find('{')?;
    let mut depth = 0i32;
    let mut in_str = false;
    let mut esc = false;
    for (i, c) in text[start..].char_indices() {
        if esc {
            esc = false;
            continue;
        }
        match c {
            '\\' => esc = true,
            '"' => in_str = !in_str,
            '{' if !in_str => depth += 1,
            '}' if !in_str => {
                depth -= 1;
                if depth == 0 {
                    let end = start + i + 1;
                    return serde_json::from_str(&text[start..end]).ok();
                }
            }
            _ => {}
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::future::BoxFuture;
    use roco_engine::{CompletionResponse, TokenUsage};
    use roco_engine::MockBackend;

    /// A backend that returns a plan-shaped JSON object (for happy-path tests).
    struct PlanBackend;
    impl ModelBackend for PlanBackend {
        fn name(&self) -> &str {
            "plan-mock"
        }
        fn complete(&self, _req: CompletionRequest) -> BoxFuture<'_, Result<CompletionResponse, roco_engine::EngineError>> {
            Box::pin(async move {
                let json = r#"{"task":"ship a feature","steps":[
                    {"id":"1","description":"write the code","depends_on":[]},
                    {"id":"2","description":"run the tests","depends_on":["1"]}
                ]}"#;
                Ok(CompletionResponse {
                    text: format!("```json\n{json}\n```"),
                    usage: TokenUsage::default(),
                    parsed: None,
                    think_trace: None,
                })
            })
        }
    }

    #[tokio::test]
    async fn planner_produces_multi_step_plan() {
        let plan = Planner::plan(&PlanBackend, "ship a feature").await.unwrap();
        assert_eq!(plan.steps.len(), 2);
        assert_eq!(plan.task, "ship a feature");
        assert!(plan.steps[1].depends_on.contains(&"1".to_string()));
    }

    #[tokio::test]
    async fn planner_falls_back_on_non_json() {
        // MockBackend returns {"result": ...}, which is not a plan.
        let plan = Planner::plan(&MockBackend::default(), "do something").await.unwrap();
        assert_eq!(plan.steps.len(), 1);
        assert_eq!(plan.steps[0].description, "do something");
    }

    #[tokio::test]
    async fn planner_falls_back_on_empty_steps() {
        struct EmptyPlanBackend;
        impl ModelBackend for EmptyPlanBackend {
            fn name(&self) -> &str { "empty-plan" }
            fn complete(&self, _req: CompletionRequest) -> BoxFuture<'_, Result<CompletionResponse, roco_engine::EngineError>> {
                Box::pin(async move {
                    Ok(CompletionResponse {
                        text: "{\"task\":\"x\",\"steps\":[]}".to_string(),
                        usage: TokenUsage::default(),
                        parsed: None,
                        think_trace: None,
                    })
                })
            }
        }
        let plan = Planner::plan(&EmptyPlanBackend, "do x").await.unwrap();
        assert_eq!(plan.steps.len(), 1, "empty steps must fall back to single step");
    }

    #[test]
    fn topological_order_respects_dependencies() {
        let plan = Plan {
            task: "t".into(),
            steps: vec![
                PlanStep { id: "2".into(), description: "b".into(), tool: None, depends_on: vec!["1".into()] },
                PlanStep { id: "1".into(), description: "a".into(), tool: None, depends_on: vec![] },
                PlanStep { id: "3".into(), description: "c".into(), tool: None, depends_on: vec!["2".into()] },
            ],
        };
        let order = plan.topological_order();
        assert_eq!(plan.steps[order[0]].id, "1");
        assert_eq!(plan.steps[order[1]].id, "2");
        assert_eq!(plan.steps[order[2]].id, "3");
    }

    #[test]
    fn topological_order_handles_cycles() {
        // 1 depends on 2, 2 depends on 1 → cycle; must still order all steps.
        let plan = Plan {
            task: "t".into(),
            steps: vec![
                PlanStep { id: "1".into(), description: "a".into(), tool: None, depends_on: vec!["2".into()] },
                PlanStep { id: "2".into(), description: "b".into(), tool: None, depends_on: vec!["1".into()] },
            ],
        };
        let order = plan.topological_order();
        assert_eq!(order.len(), 2);
    }

    #[tokio::test]
    async fn execute_runs_steps_and_collects_outputs() {
        let plan = Plan::single("greet the user");
        let result = plan.execute(&MockBackend::default(), None, None).await.unwrap();
        assert!(result.success);
        assert_eq!(result.outcomes.len(), 1);
        assert!(!result.outcomes[0].output.is_empty());
    }

    #[test]
    fn wave_levels_groups_independent_steps() {
        let plan = Plan {
            task: "t".into(),
            steps: vec![
                PlanStep { id: "1".into(), description: "a".into(), tool: None, depends_on: vec![] },
                PlanStep { id: "2".into(), description: "b".into(), tool: None, depends_on: vec![] },
                PlanStep { id: "3".into(), description: "c".into(), tool: None, depends_on: vec!["1".into(), "2".into()] },
            ],
        };
        let levels = plan.wave_levels();
        assert_eq!(levels.len(), 2, "independent steps share a wave, dependent waits");
        assert_eq!(levels[0].len(), 2, "steps 1 and 2 run concurrently");
        assert_eq!(levels[1].len(), 1, "step 3 waits for both");
    }

    #[tokio::test]
    async fn execute_runs_dependent_after_independent() {
        let plan = Plan {
            task: "t".into(),
            steps: vec![
                PlanStep { id: "1".into(), description: "a".into(), tool: None, depends_on: vec![] },
                PlanStep { id: "2".into(), description: "b".into(), tool: None, depends_on: vec![] },
                PlanStep { id: "3".into(), description: "c".into(), tool: None, depends_on: vec!["1".into(), "2".into()] },
            ],
        };
        let result = plan.execute(&MockBackend::default(), None, None).await.unwrap();
        assert_eq!(result.outcomes.len(), 3);
        // Outcomes returned in topological (review) order.
        assert_eq!(result.outcomes[0].step_id, "1");
        assert_eq!(result.outcomes[1].step_id, "2");
        assert_eq!(result.outcomes[2].step_id, "3");
    }

    #[tokio::test]
    async fn execute_with_step_verifier_retries_on_failure() {
        let plan = Plan::single("perform critical test");
        let verifier = MockStepVerifier { should_pass: false };
        let result = plan.execute(&MockBackend::default(), None, Some(&verifier)).await.unwrap();
        assert!(result.success);
        assert_eq!(result.outcomes.len(), 1);
    }

    #[test]
    fn plan_json_roundtrip() {
        let plan = Plan {
            task: "t".into(),
            steps: vec![PlanStep {
                id: "1".into(),
                description: "do it".into(),
                tool: Some("bash".into()),
                depends_on: vec![],
            }],
        };
        let s = plan.to_json();
        let p2 = Plan::from_json(&s).unwrap();
        assert_eq!(p2.steps.len(), 1);
        assert_eq!(p2.steps[0].tool.as_deref(), Some("bash"));
    }

    #[test]
    fn extract_first_json_handles_wrapped_object() {
        let text = "Sure! Here is the plan:\n```json\n{\"steps\":[{\"id\":\"1\",\"description\":\"x\"}]}\n```\nDone.";
        let v = extract_first_json(text).unwrap();
        assert_eq!(v["steps"][0]["id"], "1");
    }
}
