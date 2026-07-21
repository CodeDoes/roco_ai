//! Integration test: the `AppContext` facade exposes every capability the
//! human-facing surfaces need, through one constructor, hiding the backend
//! and daemon details. Uses `MockBackend` so it runs without a GPU.

use std::sync::Arc;

use roco_app::{AppContext, AppError, AppResult};
use roco_engine::{CompletionRequest, CompletionResponse, EngineError, ModelBackend};
use roco_workspace::WorkspaceKind;

/// A minimal backend that records calls, so we can assert the facade routes
/// through it correctly.
struct TestBackend {
    calls: std::sync::Mutex<Vec<String>>,
}

impl TestBackend {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            calls: std::sync::Mutex::new(Vec::new()),
        })
    }
}

impl ModelBackend for TestBackend {
    fn name(&self) -> &str {
        "test"
    }
    fn complete(
        &self,
        _req: CompletionRequest,
    ) -> futures::future::BoxFuture<'_, Result<CompletionResponse, EngineError>> {
        self.calls.lock().unwrap().push("complete".into());
        Box::pin(async move {
            Ok(CompletionResponse {
                text: "ok".into(),
                usage: roco_engine::TokenUsage::default(),
                parsed: None,
                think_trace: None,
            })
        })
    }
    fn save_state(&self) -> futures::future::BoxFuture<'_, Result<Vec<u8>, EngineError>> {
        self.calls.lock().unwrap().push("save_state".into());
        Box::pin(async move { Ok(vec![1, 2, 3]) })
    }
    fn load_state(&self, _s: Vec<u8>) -> futures::future::BoxFuture<'_, Result<(), EngineError>> {
        self.calls.lock().unwrap().push("load_state".into());
        Box::pin(async move { Ok(()) })
    }
}

/// Build an `AppContext` backed by `TestBackend` without touching the daemon.
fn test_ctx(backend: Arc<TestBackend>) -> AppContext {
    AppContext {
        backend,
        session_root: std::env::temp_dir().join("roco_test_sessions"),
        workspace_root: std::env::temp_dir().join("roco_test_workspaces"),
    }
}

#[test]
fn facade_exposes_generate_poll_finish() {
    let b = TestBackend::new();
    let ctx = test_ctx(b.clone());
    let res: AppResult<CompletionResponse> = ctx.generate_poll_finish(CompletionRequest::default());
    assert!(res.is_ok());
    assert_eq!(res.unwrap().text, "ok");
    assert!(b.calls.lock().unwrap().contains(&"complete".to_string()));
}

#[test]
fn facade_exposes_generate_stream() {
    let b = TestBackend::new();
    let ctx = test_ctx(b.clone());
    let mut seen = Vec::new();
    let res = ctx.generate_stream(CompletionRequest::default(), move |tok| seen.push(tok.to_string()));
    assert!(res.is_ok());
    assert!(b.calls.lock().unwrap().contains(&"complete".to_string()));
}

#[test]
fn facade_exposes_model_state_save_load() {
    let b = TestBackend::new();
    let ctx = test_ctx(b.clone());
    let state = ctx.model_state_save().unwrap();
    assert_eq!(state, vec![1, 2, 3]);
    ctx.model_state_load(state).unwrap();
    let calls = b.calls.lock().unwrap();
    assert!(calls.contains(&"save_state".to_string()));
    assert!(calls.contains(&"load_state".to_string()));
}

#[test]
fn facade_exposes_session_and_message() {
    let b = TestBackend::new();
    let ctx = test_ctx(b);
    let sess = ctx.session("test-session-1").unwrap();
    sess.message("user", "hello").unwrap();
    // listing should include our session
    let ids = ctx.list_sessions();
    assert!(ids.iter().any(|s| s.contains("test-session-1")));
}

#[test]
fn facade_exposes_session_agent() {
    let b = TestBackend::new();
    let ctx = test_ctx(b);
    let sess = ctx.session("test-session-2").unwrap();
    let agent = ctx.session_agent(&sess, "story").unwrap();
    agent.message("once upon a time").unwrap();
}

#[test]
fn facade_exposes_workspace_and_timeline() {
    let b = TestBackend::new();
    let ctx = test_ctx(b);
    // Use a unique workspace name to avoid temp dir pollution between test runs
    let ws_name = format!("test-ws-{}", std::process::id());
    let ws = ctx.workspace(&ws_name, WorkspaceKind::Generic).unwrap();
    let t1 = ctx.workspace_timeline_reset(&ws, "init").unwrap();
    std::thread::sleep(std::time::Duration::from_secs(1));
    ws.transform("write file", |w| {
        let p = w.resolve("draft.md").map_err(|e| AppError::Workspace(e))?;
        std::fs::write(&p, "# Draft").map_err(|e| AppError::Other(e.to_string()))?;
        Ok(())
    })
    .unwrap();
    let t2 = ctx.workspace_timeline_reset(&ws, "after-edit").unwrap();
    let diff = ctx.workspace_timeline_compare(&ws, &t1, &t2).unwrap();
    assert!(diff.contains("draft.md"), "diff should mention the new file: {diff}");
}
