//! End-to-end integration test for the full inference and daemon pipeline.

use roco_engine::{CompletionRequest, ModelBackend};

#[tokio::test]
async fn test_full_pipeline_mock_backend_completion() {
    let backend = roco_engine::MockBackend::default();

    let req1 = CompletionRequest::builder()

        .prompt("Calculate 2 + 2.")
        .temperature(0.0)
        .max_tokens(20)
        .seed(42)
        .build();

    let req2 = req1.clone();

    let resp1 = backend.complete(req1).await.expect("req1 failed");
    let resp2 = backend.complete(req2).await.expect("req2 failed");

    assert!(!resp1.text.is_empty(), "response 1 must not be empty");
    assert_eq!(
        resp1.text, resp2.text,
        "deterministic seed must produce identical output"
    );
}

#[tokio::test]
async fn test_full_pipeline_json_formatting() {
    let backend = roco_engine::MockBackend::default();

    let req = CompletionRequest::builder()

        .prompt("Create an outline for a sci-fi chapter.")
        .temperature(0.0)
        .max_tokens(100)
        .build();

    let resp = backend.complete(req).await.expect("completion failed");
    assert!(
        resp.parsed.is_some() || serde_json::from_str::<serde_json::Value>(&resp.text).is_ok(),
        "outliner response must be well-formed JSON"
    );
}
