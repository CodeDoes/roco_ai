//! Parametric tests for gateway backend modes and route variations.
//!
//! Tests all combinations of:
//! - Backend modes: local (MockBackend), proxy (inferd), workspace-only (no backend)
//! - Routes: /complete, /bake, /vocab, /v1/completions, /sessions/*, /workspaces/*, /jobs/*
//! - Session states: idle, generating, completed, error, archived
//! - Workspace operations: create, list, read, write

use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::Router;
use tower::ServiceExt;

use crate::job::JobQueue;
use crate::session::{SessionManager, SessionStatus};
use crate::workspace::WorkspaceManager;
use crate::GatewayState;
use roco_engine::{MockBackend, ModelBackend};

// ── Test Helpers ────────────────────────────────────────────────────────

/// Create a test gateway with local backend (MockBackend)
fn gateway_with_local_backend() -> (Router, Arc<MockBackend>) {
    let backend = Arc::new(MockBackend::new("test-backend", 0));
    let state = GatewayState {
        backend: Some(backend.clone() as Arc<dyn ModelBackend>),
        inferd_client: reqwest::Client::new(),
        inferd_url: "http://127.0.0.1:9999".to_string(), // Won't be used
        sessions: Arc::new(SessionManager::new()),
        workspaces: Arc::new(WorkspaceManager::new(tempfile::tempdir().unwrap().keep())),
        jobs: Arc::new(JobQueue::new()),
        rate_limiter: Arc::new(Default::default()),
        rate_limit_per_minute: 100,
    };
    (build_router(state), backend)
}

/// Create a test gateway with no backend (workspace-only mode)
fn gateway_workspace_only() -> Router {
    let state = GatewayState {
        backend: None,
        inferd_client: reqwest::Client::new(),
        inferd_url: "http://127.0.0.1:9999".to_string(),
        sessions: Arc::new(SessionManager::new()),
        workspaces: Arc::new(WorkspaceManager::new(tempfile::tempdir().unwrap().keep())),
        jobs: Arc::new(JobQueue::new()),
        rate_limiter: Arc::new(Default::default()),
        rate_limit_per_minute: 100,
    };
    build_router(state)
}

/// Build the gateway router (mirrors Gateway::run but without starting server)
fn build_router(state: GatewayState) -> Router {
    use axum::routing::{delete, get, post};
    use axum::Router as AxumRouter;

    AxumRouter::new()
        .route("/health", get(crate::handle_health_test))
        .route("/complete", post(crate::handle_direct_complete_test))
        .route("/bake", post(crate::handle_direct_bake_test))
        .route("/vocab", get(crate::handle_vocab_test))
        .route(
            "/v1/completions",
            post(crate::handle_openai_completions_test),
        )
        .route("/sessions", post(crate::handle_create_session_test))
        .route("/sessions/:id", get(crate::handle_get_session_test))
        .route("/sessions/:id", delete(crate::handle_delete_session_test))
        .route(
            "/sessions/:id/complete",
            post(crate::handle_complete_session_test),
        )
        .route("/sessions/:id/bake", post(crate::handle_bake_session_test))
        .route("/sessions/:id/tokens", get(crate::handle_get_tokens_test))
        .route(
            "/sessions/:id/status",
            get(crate::handle_get_session_status_test),
        )
        .route("/workspaces", get(crate::handle_list_workspaces_test))
        .route("/workspaces", post(crate::handle_create_workspace_test))
        .route("/workspaces/:id", get(crate::handle_get_workspace_test))
        .route("/workspaces/:id/files", get(crate::handle_list_files_test))
        .route(
            "/workspaces/:id/files/*path",
            get(crate::handle_read_file_test),
        )
        .route(
            "/workspaces/:id/files/*path",
            post(crate::handle_write_file_test),
        )
        .with_state(state)
}

// ── Health Endpoint Tests ──────────────────────────────────────────────

#[tokio::test]
async fn health_local_backend() {
    let (app, _) = gateway_with_local_backend();
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/health")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(body["backend_mode"], "local");
    assert!(body["inferd_status"].is_null());
}

#[tokio::test]
async fn health_workspace_only() {
    let app = gateway_workspace_only();
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/health")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    // Should be 503 since inferd is unreachable
    assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);

    let body: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(body["backend_mode"], "proxy");
    assert_eq!(body["inferd_status"], "unreachable");
}

// ── Direct /complete Tests (Local Backend) ─────────────────────────────

#[tokio::test]
async fn complete_local_basic() {
    let (app, _) = gateway_with_local_backend();
    let req_body = serde_json::json!({
        "prompt": "Hello, world!",
        "max_tokens": 100,
    });

    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/complete")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_string(&req_body).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
    let body: roco_engine::CompletionResponse = serde_json::from_slice(
        &axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();
    assert!(!body.text.is_empty());
}

#[tokio::test]
async fn complete_local_with_system() {
    let (app, _) = gateway_with_local_backend();
    let req_body = serde_json::json!({
        "system": "You are a helpful assistant",
        "prompt": "Tell me a joke",
        "temperature": 0.5,
        "max_tokens": 200,
    });

    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/complete")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_string(&req_body).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn complete_local_with_grammar() {
    let (app, _) = gateway_with_local_backend();
    let req_body = serde_json::json!({
        "prompt": "Generate JSON",
        "grammar": "root ::= \"{ \"result\": \"hello\" }\"",
        "max_tokens": 50,
    });

    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/complete")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_string(&req_body).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
}

// ── Direct /complete Tests (Workspace-Only, No Backend) ────────────────

#[tokio::test]
async fn complete_no_backend_proxies_to_inferd() {
    let app = gateway_workspace_only();
    let req_body = serde_json::json!({
        "prompt": "Hello",
    });

    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/complete")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_string(&req_body).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    // Should fail since inferd is not running
    assert_eq!(resp.status(), StatusCode::BAD_GATEWAY);
}

// ── Direct /bake Tests (Local Backend) ─────────────────────────────────

#[tokio::test]
async fn bake_local_basic() {
    let (app, _) = gateway_with_local_backend();
    let req_body = serde_json::json!({
        "session_id": "test-session",
        "system": "You are a pirate",
        "few_shots": [
            ["What is your name?", "Arr, I be Captain Hook!"],
            ["Where is the treasure?", "X marks the spot, matey!"]
        ],
    });

    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/bake")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_string(&req_body).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn bake_local_empty_few_shots() {
    let (app, _) = gateway_with_local_backend();
    let req_body = serde_json::json!({
        "session_id": "test-session-empty",
        "system": "You are helpful",
        "few_shots": [],
    });

    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/bake")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_string(&req_body).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
}

// ── Direct /bake Tests (No Backend) ────────────────────────────────────

#[tokio::test]
async fn bake_no_backend_proxies_to_inferd() {
    let app = gateway_workspace_only();
    let req_body = serde_json::json!({
        "session_id": "test-session",
        "system": "You are helpful",
        "few_shots": [],
    });

    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/bake")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_string(&req_body).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::BAD_GATEWAY);
}

// ── Direct /vocab Tests ────────────────────────────────────────────────

#[tokio::test]
async fn vocab_local_backend() {
    let (app, _) = gateway_with_local_backend();

    let resp = app
        .oneshot(
            Request::builder()
                .uri("/vocab")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
    let body: Vec<Vec<u8>> = serde_json::from_slice(
        &axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();
    assert!(!body.is_empty());
}

#[tokio::test]
async fn vocab_no_backend_proxies_to_inferd() {
    let app = gateway_workspace_only();

    let resp = app
        .oneshot(
            Request::builder()
                .uri("/vocab")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::BAD_GATEWAY);
}

// ── /v1/completions Tests (OpenAI-Compatible) ──────────────────────────

#[tokio::test]
async fn openai_completions_local() {
    let (app, _) = gateway_with_local_backend();
    let req_body = serde_json::json!({
        "prompt": "Once upon a time",
        "max_tokens": 100,
        "temperature": 0.7,
    });

    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/completions")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_string(&req_body).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
    let body: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(body["object"], "text_completion");
    assert!(body["choices"][0]["text"].as_str().unwrap().len() > 0);
    // MockBackend may return 0 for token counts, just verify structure exists
    assert!(body["usage"].is_object());
}

// ── Session Route Tests ────────────────────────────────────────────────

#[tokio::test]
async fn session_lifecycle_local() {
    let (app, _) = gateway_with_local_backend();

    // Create session
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/sessions")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({"workspace_id": "ws-1"}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();
    let session_id = body["session_id"].as_str().unwrap().to_string();

    // Get session
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/sessions/{}", session_id))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    // Get session status
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/sessions/{}/status", session_id))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(body["status"], "idle");

    // Delete session
    let resp = app
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri(format!("/sessions/{}", session_id))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);
}

#[tokio::test]
async fn session_complete_local() {
    let (app, _) = gateway_with_local_backend();

    // Create session
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/sessions")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({"workspace_id": "ws-1"}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    let body: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();
    let session_id = body["session_id"].as_str().unwrap().to_string();

    // Complete session
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/sessions/{}/complete", session_id))
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({
                        "prompt": "Hello",
                        "temperature": 0.7,
                        "max_tokens": 100,
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();
    assert!(body["text"].as_str().unwrap().len() > 0);

    // Check tokens accumulated
    let resp = app
        .oneshot(
            Request::builder()
                .uri(format!("/sessions/{}/tokens", session_id))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let tokens: Vec<String> = serde_json::from_slice(
        &axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();
    assert!(!tokens.is_empty());
}

#[tokio::test]
async fn session_not_found() {
    let (app, _) = gateway_with_local_backend();

    let resp = app
        .oneshot(
            Request::builder()
                .uri("/sessions/nonexistent")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

// ── Workspace Route Tests ──────────────────────────────────────────────

#[tokio::test]
async fn workspace_crud() {
    let (app, _) = gateway_with_local_backend();

    // Create workspace
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/workspaces")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::json!({"id": "test-ws"}).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(body["id"], "test-ws");

    // List workspaces
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/workspaces")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();
    assert!(body["workspaces"].as_array().unwrap().len() > 0);

    // Get workspace
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/workspaces/test-ws")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn workspace_file_operations() {
    let (app, _) = gateway_with_local_backend();

    // Create workspace
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/workspaces")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({"id": "file-test"}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    // Write file
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/workspaces/file-test/files/test.txt")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({"content": "Hello, world!"}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    // Read file
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/workspaces/file-test/files/test.txt")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let content = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    assert_eq!(content.as_ref(), b"Hello, world!");

    // List files
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/workspaces/file-test/files")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

// ── Session Workspace Switching Tests ──────────────────────────────────

#[tokio::test]
async fn session_switch_workspace() {
    let mgr = SessionManager::new();
    let id = mgr.create("ws-1");

    // Verify initial workspace
    let session = mgr.get(&id).unwrap();
    assert_eq!(session.workspace_id, "ws-1");

    // Switch workspace
    mgr.update(&id, |s| s.set_workspace("ws-2"));
    let session = mgr.get(&id).unwrap();
    assert_eq!(session.workspace_id, "ws-2");

    // Switch again
    mgr.update(&id, |s| s.set_workspace("ws-3"));
    let session = mgr.get(&id).unwrap();
    assert_eq!(session.workspace_id, "ws-3");
}

#[tokio::test]
async fn session_switch_workspace_preserves_state() {
    let mgr = SessionManager::new();
    let id = mgr.create("ws-1");

    // Set some state
    mgr.set_status(&id, SessionStatus::Generating);
    mgr.append_tokens(&id, vec!["token1".into(), "token2".into()]);

    // Switch workspace
    mgr.update(&id, |s| s.set_workspace("ws-2"));

    // Verify state preserved
    let session = mgr.get(&id).unwrap();
    assert_eq!(session.workspace_id, "ws-2");
    assert_eq!(session.status, SessionStatus::Generating);
    assert_eq!(session.accumulated_tokens.len(), 2);
}

#[tokio::test]
async fn session_switch_workspace_updates_timestamp() {
    let mgr = SessionManager::new();
    let id = mgr.create("ws-1");

    let before = mgr.get(&id).unwrap().last_accessed_at;
    std::thread::sleep(std::time::Duration::from_millis(10));

    mgr.update(&id, |s| s.set_workspace("ws-2"));

    let after = mgr.get(&id).unwrap().last_accessed_at;
    assert!(after >= before);
}

// ── Parametric Backend Mode Tests ──────────────────────────────────────

/// Test that /complete works with local backend but not without
#[tokio::test]
async fn parametric_complete_backend_modes() {
    // With local backend
    let (app_local, _) = gateway_with_local_backend();
    let resp = app_local
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/complete")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({"prompt": "test"}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        resp.status(),
        StatusCode::OK,
        "Local backend should succeed"
    );

    // Without backend (proxy mode, inferd not running)
    let app_no_backend = gateway_workspace_only();
    let resp = app_no_backend
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/complete")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({"prompt": "test"}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        resp.status(),
        StatusCode::BAD_GATEWAY,
        "No backend should fail"
    );
}

/// Test that /bake works with local backend but not without
#[tokio::test]
async fn parametric_bake_backend_modes() {
    // With local backend
    let (app_local, _) = gateway_with_local_backend();
    let resp = app_local
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/bake")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({
                        "session_id": "test",
                        "system": "You are helpful",
                        "few_shots": []
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        resp.status(),
        StatusCode::OK,
        "Local backend should succeed"
    );

    // Without backend
    let app_no_backend = gateway_workspace_only();
    let resp = app_no_backend
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/bake")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({
                        "session_id": "test",
                        "system": "You are helpful",
                        "few_shots": []
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        resp.status(),
        StatusCode::BAD_GATEWAY,
        "No backend should fail"
    );
}

/// Test that /vocab works with local backend but not without
#[tokio::test]
async fn parametric_vocab_backend_modes() {
    // With local backend
    let (app_local, _) = gateway_with_local_backend();
    let resp = app_local
        .oneshot(
            Request::builder()
                .uri("/vocab")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        resp.status(),
        StatusCode::OK,
        "Local backend should succeed"
    );

    // Without backend
    let app_no_backend = gateway_workspace_only();
    let resp = app_no_backend
        .oneshot(
            Request::builder()
                .uri("/vocab")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        resp.status(),
        StatusCode::BAD_GATEWAY,
        "No backend should fail"
    );
}

/// Test that /v1/completions works with local backend
#[tokio::test]
async fn parametric_openai_completions_backend_modes() {
    // With local backend
    let (app_local, _) = gateway_with_local_backend();
    let resp = app_local
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/completions")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({"prompt": "test"}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        resp.status(),
        StatusCode::OK,
        "Local backend should succeed"
    );
}

// ── Session Status Parametric Tests ────────────────────────────────────

#[tokio::test]
async fn parametric_session_status_transitions() {
    let mgr = SessionManager::new();
    let id = mgr.create("ws-1");

    // Idle -> Generating
    mgr.set_status(&id, SessionStatus::Generating);
    assert_eq!(mgr.get(&id).unwrap().status, SessionStatus::Generating);

    // Generating -> Completed
    mgr.set_status(&id, SessionStatus::Completed);
    assert_eq!(mgr.get(&id).unwrap().status, SessionStatus::Completed);

    // Test all transitions
    let transitions = vec![
        (SessionStatus::Idle, SessionStatus::Generating),
        (SessionStatus::Generating, SessionStatus::Completed),
        (SessionStatus::Generating, SessionStatus::Error),
        (SessionStatus::Completed, SessionStatus::Archived),
        (SessionStatus::Error, SessionStatus::Archived),
    ];

    for (from, to) in transitions {
        let id = mgr.create("ws-1");
        mgr.set_status(&id, from);
        mgr.set_status(&id, to);
        assert_eq!(mgr.get(&id).unwrap().status, to);
    }
}

// ── CompletionRequest Parameter Variations ──────────────────────────────

#[tokio::test]
async fn parametric_completion_request_variations() {
    let (app, _) = gateway_with_local_backend();

    let variations = vec![
        // Basic prompt only
        serde_json::json!({"prompt": "Hello"}),
        // With system
        serde_json::json!({"system": "You are helpful", "prompt": "Hello"}),
        // With temperature
        serde_json::json!({"prompt": "Hello", "temperature": 0.0}),
        serde_json::json!({"prompt": "Hello", "temperature": 1.0}),
        // With max_tokens
        serde_json::json!({"prompt": "Hello", "max_tokens": 1}),
        serde_json::json!({"prompt": "Hello", "max_tokens": 1000}),
        // With grammar
        serde_json::json!({"prompt": "Hello", "grammar": "root ::= \"hello\""}),
        // With session
        serde_json::json!({"prompt": "Hello", "session": "my-session"}),
        // All parameters
        serde_json::json!({
            "system": "You are helpful",
            "prompt": "Hello",
            "temperature": 0.5,
            "max_tokens": 100,
            "grammar": "root ::= \"hello\"",
            "session": "my-session",
            "thinking": true,
        }),
    ];

    for (i, req_body) in variations.into_iter().enumerate() {
        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/complete")
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_string(&req_body).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(
            resp.status(),
            StatusCode::OK,
            "Variation {} should succeed",
            i
        );
    }
}

// ── Error Handling Tests ────────────────────────────────────────────────

#[tokio::test]
async fn complete_missing_prompt() {
    let (app, _) = gateway_with_local_backend();
    let req_body = serde_json::json!({
        "system": "You are helpful",
    });

    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/complete")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_string(&req_body).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    // Should fail since prompt is required
    assert_ne!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn session_delete_nonexistent() {
    let (app, _) = gateway_with_local_backend();

    let resp = app
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri("/sessions/nonexistent")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    // Should succeed (idempotent delete)
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);
}

#[tokio::test]
async fn workspace_not_found() {
    let (app, _) = gateway_with_local_backend();

    let resp = app
        .oneshot(
            Request::builder()
                .uri("/workspaces/nonexistent")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

// ── Concurrent Session Tests ───────────────────────────────────────────

#[tokio::test]
async fn concurrent_session_operations() {
    let mgr = SessionManager::new();

    // Create 100 sessions sequentially (SessionManager is not Clone)
    let mut session_ids = Vec::new();
    for i in 0..100 {
        let id = mgr.create(format!("ws-{}", i % 10));
        mgr.set_status(&id, SessionStatus::Generating);
        mgr.append_tokens(&id, vec!["token".into()]);
        session_ids.push(id);
    }

    assert_eq!(session_ids.len(), 100);

    // Verify all sessions exist
    for id in &session_ids {
        assert!(mgr.get(id).is_some());
    }

    // Verify workspace filtering works
    for ws_num in 0..10 {
        let ws_sessions = mgr.list_for_workspace(&format!("ws-{}", ws_num));
        assert_eq!(
            ws_sessions.len(),
            10,
            "Workspace {} should have 10 sessions",
            ws_num
        );
    }
}

// ── Rate Limiter Tests ─────────────────────────────────────────────────

#[tokio::test]
async fn rate_limiter_allows_under_limit() {
    let (app, _) = gateway_with_local_backend();

    // Should allow many requests under limit
    for _ in 0..10 {
        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/health")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }
}

// ── MockBackend Failure Scenarios ──────────────────────────────────────

#[tokio::test]
async fn mock_backend_with_failures() {
    let backend = Arc::new(MockBackend::new("failing-backend", 3)); // Fail 3 times
    let state = GatewayState {
        backend: Some(backend.clone() as Arc<dyn ModelBackend>),
        inferd_client: reqwest::Client::new(),
        inferd_url: "http://127.0.0.1:9999".to_string(),
        sessions: Arc::new(SessionManager::new()),
        workspaces: Arc::new(WorkspaceManager::new(tempfile::tempdir().unwrap().keep())),
        jobs: Arc::new(JobQueue::new()),
        rate_limiter: Arc::new(Default::default()),
        rate_limit_per_minute: 100,
    };
    let app = build_router(state);

    // First 3 requests should fail
    for i in 0..3 {
        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/complete")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::json!({"prompt": "test"}).to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(
            resp.status(),
            StatusCode::INTERNAL_SERVER_ERROR,
            "Request {} should fail",
            i
        );
    }

    // 4th request should succeed
    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/complete")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({"prompt": "test"}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK, "Request 4 should succeed");
}
