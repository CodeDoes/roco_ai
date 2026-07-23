//! Contract integration tests for the RoCo server HTTP API.
//!
//! These tests start a real axum server backed by `MockBackend` and exercise
//! every route — verifying status codes, content types, body shapes, and
//! error handling. They use fixture JSON files for request/response shapes.
//!
//! Run: cargo test -p roco-server --test server_contract

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use roco_engine::MockBackend;
use roco_protocol::{
    HealthResponse, OpenAiCompletionRequest, OpenAiCompletionResponse, OpenAiErrorBody,
    OpenAiStreamChunk,
};
use roco_server::create_router;
use serde_json::Value;
use std::sync::Arc;
use tower::util::ServiceExt;

/// Helper: load a JSON fixture from tests/fixtures/.
fn load_fixture(name: &str) -> Value {
    let path = format!("{}/tests/fixtures/{}", env!("CARGO_MANIFEST_DIR"), name);
    let content = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("Failed to read fixture '{name}': {e}"));
    serde_json::from_str(&content)
        .unwrap_or_else(|e| panic!("Failed to parse fixture '{name}': {e}"))
}

/// Build a test app with MockBackend.
fn test_app(fail_count: u32) -> axum::Router {
    let backend = Arc::new(MockBackend::new("test-model", fail_count));
    create_router(backend)
}

#[tokio::test]
async fn health_returns_200_and_status() {
    let app = test_app(0);
    let response = app
        .oneshot(Request::get("/health").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let health: HealthResponse = serde_json::from_slice(&body).unwrap();
    assert_eq!(health.status, "ok");
    assert_eq!(health.backend, "test-model");

    // Verify shape matches fixture
    let fixture = load_fixture("health_response.json");
    let parsed: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(parsed["status"], fixture["status"]);
}

#[tokio::test]
async fn complete_non_streaming_returns_200() {
    let app = test_app(0);
    let req_body = serde_json::to_string(&OpenAiCompletionRequest {
        model: None,
        prompt: "Hello world".into(),
        system: Some("You are a test bot.".into()),
        temperature: Some(0.5),
        max_tokens: Some(50),
        stream: Some(false),
        thinking: None,
        grammar: None,
        prefill: None,
        session: None,
        preserve_state: None,
    })
    .unwrap();

    let response = app
        .oneshot(
            Request::post("/v1/completions")
                .header("Content-Type", "application/json")
                .body(Body::from(req_body))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    // Check content type
    let content_type = response
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    assert!(
        content_type.contains("application/json"),
        "Expected JSON content type, got: {content_type}"
    );

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let resp: OpenAiCompletionResponse = serde_json::from_slice(&body)
        .unwrap_or_else(|e| panic!("Failed to deserialize response: {e}"));

    assert_eq!(resp.object, "text_completion");
    assert!(!resp.choices.is_empty());
    assert_eq!(resp.choices[0].finish_reason.as_deref(), Some("stop"));
    // Response should contain the mock output
    assert!(resp.choices[0].text.contains("test-model"));
    assert!(resp.choices[0].text.contains("Hello world"));
}

#[tokio::test]
async fn complete_with_all_fields_round_trips() {
    let app = test_app(0);
    let fixture = load_fixture("completion_request_full.json");
    let req_body = serde_json::to_string(&fixture).unwrap();

    let response = app
        .oneshot(
            Request::post("/v1/completions")
                .header("Content-Type", "application/json")
                .body(Body::from(req_body))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let resp: OpenAiCompletionResponse = serde_json::from_slice(&body).unwrap();
    assert_eq!(resp.model, "test-model");
    assert_eq!(resp.choices.len(), 1);
    assert!(resp.usage.completion_tokens > 0);
    // prompt_tokens comes from estimated_prompt_tokens which defaults to 0
    // when using the OpenAI-compatible endpoint (no field for it)
    assert_eq!(resp.usage.prompt_tokens, 0);
}

#[tokio::test]
async fn complete_with_minimal_request_works() {
    let app = test_app(0);
    let fixture = load_fixture("completion_request_minimal.json");
    let req_body = serde_json::to_string(&fixture).unwrap();

    let response = app
        .oneshot(
            Request::post("/v1/completions")
                .header("Content-Type", "application/json")
                .body(Body::from(req_body))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn complete_error_returns_500_and_error_body() {
    let app = test_app(1); // First call fails
    let req_body = r#"{"prompt": "fail me"}"#;

    let response = app
        .oneshot(
            Request::post("/v1/completions")
                .header("Content-Type", "application/json")
                .body(Body::from(req_body))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let err: OpenAiErrorBody = serde_json::from_slice(&body).unwrap();
    assert_eq!(err.error.error_type, "backend_error");
    assert!(err.error.message.contains("simulated failure"));

    // Verify shape matches error fixture
    let fixture = load_fixture("error_response.json");
    let parsed: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(parsed["error"]["type"], fixture["error"]["type"]);
}

#[tokio::test]
async fn complete_direct_route_works() {
    let app = test_app(0);
    let req_body = r#"{"system": "test", "prompt": "hello", "temperature": 0.5, "max_tokens": 10}"#;

    let response = app
        .oneshot(
            Request::post("/complete")
                .header("Content-Type", "application/json")
                .body(Body::from(req_body))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn streaming_returns_sse() {
    let app = test_app(0);
    let req_body = r#"{"prompt": "stream test", "stream": true}"#;

    let response = app
        .oneshot(
            Request::post("/v1/completions")
                .header("Content-Type", "application/json")
                .body(Body::from(req_body))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let content_type = response
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    assert!(
        content_type.contains("text/event-stream"),
        "Expected SSE content type, got: {content_type}"
    );
}

#[tokio::test]
async fn streaming_sse_can_be_parsed_chunk_by_chunk() {
    let app = test_app(0);
    let req_body = r#"{"prompt": "stream test", "stream": true}"#;

    let response = app
        .oneshot(
            Request::post("/v1/completions")
                .header("Content-Type", "application/json")
                .body(Body::from(req_body))
                .unwrap(),
        )
        .await
        .unwrap();

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let body_str = String::from_utf8(body.to_vec()).unwrap();

    // SSE format: "data: {json}\n\n"
    let mut chunk_count = 0;
    let mut saw_stop = false;

    for line in body_str.lines() {
        if let Some(data) = line.strip_prefix("data: ") {
            let chunk: OpenAiStreamChunk = serde_json::from_str(data)
                .unwrap_or_else(|e| panic!("Failed to parse SSE data chunk: {e}\nData: {data}"));

            chunk_count += 1;
            if chunk.choices[0].finish_reason.as_deref() == Some("stop") {
                saw_stop = true;
            } else {
                // Token chunks should have text
                assert!(
                    !chunk.choices[0].text.is_empty(),
                    "Token chunk should have text, got: {data}"
                );
            }
        }
    }

    assert!(chunk_count > 0, "Expected at least one SSE chunk");
    assert!(saw_stop, "Expected a final 'stop' chunk");

    // Verify token chunk shape matches fixture
    let fixture = load_fixture("stream_chunk_token.json");
    // Parse the first non-stop chunk and verify shape
    for line in body_str.lines() {
        if let Some(data) = line.strip_prefix("data: ") {
            let chunk: Value = serde_json::from_str(data).unwrap();
            if chunk["choices"][0]["finish_reason"].is_null() {
                // Compare shapes: both should have id, object, created, model, choices
                assert!(chunk.get("id").is_some());
                assert!(chunk.get("object").is_some());
                assert!(chunk.get("created").is_some());
                assert!(chunk.get("model").is_some());
                assert!(chunk["choices"].is_array());
                assert_eq!(
                    chunk["choices"][0]["finish_reason"],
                    serde_json::Value::Null,
                    "Token chunk should have null finish_reason"
                );
                break;
            }
        }
    }
}

#[tokio::test]
async fn health_route_returns_200() {
    let app = test_app(0);
    let response = app
        .oneshot(Request::get("/health").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn vocab_route_returns_not_implemented_for_mock() {
    let app = test_app(0);
    let response = app
        .oneshot(Request::get("/vocab").body(Body::empty()).unwrap())
        .await
        .unwrap();
    // MockBackend returns None for vocab_bytes, so /vocab returns 501
    assert_eq!(response.status(), StatusCode::NOT_IMPLEMENTED);
}

#[tokio::test]
async fn fixture_round_trip_deserialize_minimal() {
    let fixture = load_fixture("completion_request_minimal.json");
    let req: OpenAiCompletionRequest = serde_json::from_value(fixture).unwrap();
    assert_eq!(req.prompt, "Hello world");
}

#[tokio::test]
async fn fixture_round_trip_deserialize_full() {
    let fixture = load_fixture("completion_request_full.json");
    let req: OpenAiCompletionRequest = serde_json::from_value(fixture).unwrap();
    assert_eq!(req.prompt, "Once upon a time");
    assert_eq!(req.system.as_deref(), Some("You are a storyteller."));
    assert!((req.temperature.unwrap() - 0.8).abs() < 1e-6);
    assert_eq!(req.session.as_deref(), Some("story-session-1"));
    assert_eq!(req.preserve_state, Some(true));
}

#[tokio::test]
async fn fixture_round_trip_response() {
    let fixture = load_fixture("completion_response.json");
    let resp: OpenAiCompletionResponse = serde_json::from_value(fixture).unwrap();
    assert_eq!(resp.id, "cmpl-test-001");
    assert_eq!(resp.choices.len(), 1);
    assert_eq!(
        resp.choices[0].text,
        "Once upon a time, in a land far away..."
    );
    assert_eq!(resp.usage.total_tokens, 35);
}

#[tokio::test]
async fn fixture_round_trip_stream_token() {
    let fixture = load_fixture("stream_chunk_token.json");
    let chunk: OpenAiStreamChunk = serde_json::from_value(fixture).unwrap();
    assert_eq!(chunk.id, "cmpl-stream-001");
    assert_eq!(chunk.choices[0].text, "Once ");
    assert!(chunk.choices[0].finish_reason.is_none());
}

#[tokio::test]
async fn fixture_round_trip_stream_stop() {
    let fixture = load_fixture("stream_chunk_stop.json");
    let chunk: OpenAiStreamChunk = serde_json::from_value(fixture).unwrap();
    assert_eq!(chunk.choices[0].finish_reason.as_deref(), Some("stop"));
}

#[tokio::test]
async fn fixture_round_trip_error() {
    let fixture = load_fixture("error_response.json");
    let err: OpenAiErrorBody = serde_json::from_value(fixture).unwrap();
    assert_eq!(err.error.error_type, "backend_error");
    assert!(err.error.message.contains("simulated failure"));
}

#[tokio::test]
async fn unknown_route_returns_404() {
    let app = test_app(0);
    let response = app
        .oneshot(Request::get("/nonexistent").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn bad_json_returns_422() {
    let app = test_app(0);
    let response = app
        .oneshot(
            Request::post("/v1/completions")
                .header("Content-Type", "application/json")
                .body(Body::from(r#"this is not json"#))
                .unwrap(),
        )
        .await
        .unwrap();
    // axum returns 422 (Unprocessable Entity) for deserialization failures
    assert!(
        response.status() == StatusCode::UNPROCESSABLE_ENTITY
            || response.status() == StatusCode::BAD_REQUEST
    );
}
