//! Tests for `crate::agent`.
//!
//! Included from `agent.rs` via `#[path = "tests/agent.rs"]` so this
//! file is treated as part of the lib crate's `tests` module and runs
//! in the same `cargo test` invocation as the other unit tests.

use super::*;
use crate::engine::{BoxFuture, CompletionResponse, EngineError, MockBackend};
use crate::policy::{ComposedPolicy, Policy};
use crate::sandbox::Sandbox;
use crate::tools::{AddTool, ToolRegistry};
use std::sync::Arc;

#[test]
fn budget_hard_cap_is_3000() {
    let b = ContextBudget::default();
    assert_eq!(b.max_prompt(), 3000.min(b.total - b.generation));
    assert!(b.fits_prompt(2500));
    assert!(!b.fits_prompt(3100));
}

#[test]
fn escalation_levels_map_to_attempts() {
    let mut esc = EscalationController::new(RetryPolicy::default());
    // attempts 0..=2 stay at L1 self-recovery (per-step cap of 2 retries)
    assert_eq!(esc.current_level(), EscalationLevel::SelfRecovery);
    esc.record_attempt();
    assert_eq!(esc.current_level(), EscalationLevel::SelfRecovery);
    esc.record_attempt();
    assert_eq!(esc.current_level(), EscalationLevel::SelfRecovery);
    // attempt 3 escalates to L2 team replan
    esc.record_attempt();
    assert_eq!(esc.current_level(), EscalationLevel::TeamReplan);
    // beyond the task cap (3) -> L3 human, exhausted
    esc.record_attempt();
    assert!(esc.exhausted());
    assert_eq!(esc.current_level(), EscalationLevel::Human);
}

#[test]
fn chunking_respects_budget() {
    let text: String = (0..200).map(|i| format!("word{} ", i)).collect();
    let chunks = chunk_text(&text, 200);
    assert!(!chunks.is_empty());
    for c in &chunks {
        assert!(TokenCounter::estimate(c) <= 200);
    }
}

#[tokio::test]
async fn worker_rejects_overbudget_prompt() {
    let backend = Arc::new(MockBackend::default());
    let worker = Worker::new(backend, ContextBudget::default());
    let task = Subtask {
        id: "t1".into(),
        objective: "x".into(),
        context: String::new(),
        output_schema: "{}".into(),
        allow_abstain: false,
        prompt_tokens: 99999,
    };
    assert!(matches!(
        worker.execute(&task).await,
        Err(AgentError::BudgetExceeded { .. })
    ));
}

#[tokio::test]
async fn checklist_verifier_flags_invalid_json() {
    let v = ChecklistVerifier;
    let task = Subtask {
        id: "t1".into(),
        objective: "x".into(),
        context: String::new(),
        output_schema: r#"{"label": "string"}"#.into(),
        allow_abstain: false,
        prompt_tokens: 10,
    };
    let out = WorkerOutput {
        subtask_id: "t1".into(),
        raw: "transcript".into(),
        final_raw: "not json".into(),
        parsed: Value::Null,
        usage: TokenUsage::default(),
        aborted: false,
        tool_results: vec![],
    };
    let verdict = v.verify(&task, &out).await.unwrap();
    assert!(!verdict.passed);
    assert_eq!(verdict.checks.get("check_syntax"), Some(&false));
}

#[tokio::test]
async fn orchestrator_runs_mock_task_end_to_end() {
    let backend = Arc::new(MockBackend::default());
    let orchestrator = Orchestrator::new(
        backend,
        ContextBudget::default(),
        ChecklistVerifier,
        RetryPolicy::default(),
    );
    let big = (0..80)
        .map(|i| format!("Fact {} about the system. ", i))
        .collect::<String>();
    let task = Task {
        id: "review".into(),
        objective: "Summarize".into(),
        context: big,
        output_schema: r#"{"label": "pass", "notes": "string"}"#.into(),
        allow_abstain: true,
    };
    let result = orchestrator.run(&task).await.unwrap();
    assert!(result.subtask_count >= 1);
}

/// A mock backend that emits a tool call on the first turn, then a schema
/// JSON answer once it sees prior `[TOOL RESULTS]` in the transcript — so
/// we can exercise the worker's multi-step agentic loop without a model.
struct ToolCallingMockBackend;
impl ModelBackend for ToolCallingMockBackend {
    fn name(&self) -> &str {
        "mock-tool"
    }
    fn complete(
        &self,
        req: CompletionRequest,
    ) -> BoxFuture<'_, Result<CompletionResponse, EngineError>> {
        Box::pin(async move {
            let text = if req.prompt.contains("[TOOL RESULTS]") {
                // Second turn: the model now returns a final answer.
                "{\"label\":\"pass\",\"notes\":\"aggregated\"}".to_string()
            } else {
                // First turn: emit a tool call.
                "<tool_call>\n{\"name\":\"add\",\"arguments\":{\"numbers\":[1,2,3]}}\n</tool_call>"
                    .to_string()
            };
            Ok(CompletionResponse {
                text: text.clone(),
                usage: TokenUsage::default(),
                parsed: serde_json::from_str(&text).ok(),
                think_trace: None,
            })
        })
    }
}

/// Emits a tool call for the first two turns (counting `[TOOL RESULTS]`
/// markers in the transcript), then a final answer — to drive a 2-step loop.
struct MultiStepMockBackend;
impl ModelBackend for MultiStepMockBackend {
    fn name(&self) -> &str {
        "mock-multistep"
    }
    fn complete(
        &self,
        req: CompletionRequest,
    ) -> BoxFuture<'_, Result<CompletionResponse, EngineError>> {
        Box::pin(async move {
            let rounds = req.prompt.matches("[TOOL RESULTS]").count();
            let text = if rounds >= 2 {
                "{\"label\":\"pass\",\"notes\":\"done\"}".to_string()
            } else if rounds == 1 {
                "<tool_call>\n{\"name\":\"add\",\"arguments\":{\"numbers\":[10,20,30]}}\n</tool_call>"
                .to_string()
            } else {
                "<tool_call>\n{\"name\":\"add\",\"arguments\":{\"numbers\":[1,2,3]}}\n</tool_call>"
                    .to_string()
            };
            Ok(CompletionResponse {
                text: text.clone(),
                usage: TokenUsage::default(),
                parsed: serde_json::from_str(&text).ok(),
                think_trace: None,
            })
        })
    }
}

#[tokio::test]
async fn worker_runs_multi_step_tool_loop() {
    let backend = Arc::new(MultiStepMockBackend);
    let tools = Arc::new({
        let mut r = ToolRegistry::new();
        r.register(Arc::new(AddTool));
        r
    });
    let sandbox = Arc::new(Sandbox::new());
    let policy: Arc<dyn Policy> = Arc::new(ComposedPolicy::new());
    let worker = Worker::new(backend, ContextBudget::default())
        .with_tooling(tools, sandbox, policy)
        .with_max_tool_rounds(4);

    let subtask = Subtask {
        id: "t1".into(),
        objective: "add twice".into(),
        context: String::new(),
        output_schema: "{}".into(),
        allow_abstain: false,
        prompt_tokens: 10,
    };
    let out = worker.execute(&subtask).await.unwrap();
    // Two tool rounds were taken before the model gave a final answer.
    assert_eq!(out.tool_results.len(), 2);
    assert_eq!(out.tool_results[0].output.as_ref().unwrap()["sum"], 6.0);
    assert_eq!(out.tool_results[1].output.as_ref().unwrap()["sum"], 60.0);
}

#[tokio::test]
async fn worker_executes_tool_calls_when_tooling_present() {
    let backend = Arc::new(ToolCallingMockBackend);
    let tools = Arc::new({
        let mut r = ToolRegistry::new();
        r.register(Arc::new(AddTool));
        r
    });
    let sandbox = Arc::new(Sandbox::new());
    let policy: Arc<dyn Policy> = Arc::new(ComposedPolicy::new());
    let worker =
        Worker::new(backend, ContextBudget::default()).with_tooling(tools, sandbox, policy);

    let subtask = Subtask {
        id: "t1".into(),
        objective: "add".into(),
        context: String::new(),
        output_schema: "{}".into(),
        allow_abstain: false,
        prompt_tokens: 10,
    };
    let out = worker.execute(&subtask).await.unwrap();
    assert_eq!(out.tool_results.len(), 1);
    let res = &out.tool_results[0];
    assert_eq!(res.verdict, crate::policy::PolicyVerdict::Allow);
    assert_eq!(res.output.as_ref().unwrap()["sum"], 6.0);
}

/// Drives a full RAG turn through the agentic loop: the model emits a
/// `vector_upsert`, then a `vector_search` whose result comes back from
/// the shared store. Exercises the new RAG tools end-to-end (no model).
#[tokio::test]
async fn worker_surfaces_tool_results_in_parsed() {
    use crate::builtins::{VectorSearchTool, VectorUpsertTool};
    use crate::vector::{HashingEmbedder, SharedVectorStore, VectorStore};
    use std::sync::Mutex;

    struct RagMockBackend;
    impl ModelBackend for RagMockBackend {
        fn name(&self) -> &str {
            "mock-rag"
        }
        fn complete(
            &self,
            req: CompletionRequest,
        ) -> BoxFuture<'_, Result<CompletionResponse, EngineError>> {
            Box::pin(async move {
                let rounds = req.prompt.matches("[TOOL RESULTS]").count();
                let text = if rounds >= 1 {
                    "{\"label\":\"pass\"}".to_string()
                } else {
                    "<tool_call>\n{\"name\":\"vector_upsert\",\"arguments\":{\"id\":\"d1\",\"text\":\"val\"}}\n</tool_call>".to_string()
                };
                Ok(CompletionResponse {
                    text: text.clone(),
                    usage: TokenUsage::default(),
                    parsed: serde_json::from_str(&text).ok(),
                    think_trace: None,
                })
            })
        }
    }

    let store: SharedVectorStore = Arc::new(Mutex::new(VectorStore::new(256)));
    let embedder: Arc<dyn crate::vector::Embedder> = Arc::new(HashingEmbedder::new(256));
    let mut tools = ToolRegistry::new();
    tools.register(Arc::new(VectorUpsertTool::new(
        store.clone(),
        embedder.clone(),
    )));
    tools.register(Arc::new(VectorSearchTool::new(store, embedder)));

    let worker = Worker::new(Arc::new(RagMockBackend), ContextBudget::default()).with_tooling(
        Arc::new(tools),
        Arc::new(Sandbox::new()),
        Arc::new(ComposedPolicy::new()),
    );

    let subtask = Subtask {
        id: "test".into(),
        objective: "test".into(),
        context: String::new(),
        output_schema: "{}".into(),
        allow_abstain: false,
        prompt_tokens: 10,
    };
    let out = worker.execute(&subtask).await.unwrap();

    // Verify tool results are merged into parsed
    assert!(out.parsed.get("tool_results").is_some());
    let results = out.parsed.get("tool_results").unwrap().as_array().unwrap();
    assert_eq!(results.len(), 1);
}
