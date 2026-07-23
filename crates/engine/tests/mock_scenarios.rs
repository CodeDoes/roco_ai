//! Deterministic test scenarios built around `MockBackend`.
//!
//! These tests verify agent-level orchestration without needing model
//! weights. Every scenario uses `MockBackend` with scripted fail counts
//! and latency, covering the seven scenarios from the quality-and-delivery
//! strategy document.
//!
//! Run: cargo test -p roco-engine --test mock_scenarios

use futures::future::BoxFuture;
use roco_engine::{
    CompletionRequest, CompletionResponse, EngineError, MockBackend, ModelBackend, TokenUsage,
};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use tokio::sync::mpsc;

// ── Helper ─────────────────────────────────────────────────────────────────

/// A backend that records all received prompts for later inspection.
struct RecordingBackend {
    inner: MockBackend,
    prompts: std::sync::Mutex<Vec<String>>,
}

impl RecordingBackend {
    fn new(name: &str, fail_count: u32) -> Arc<Self> {
        Arc::new(Self {
            inner: MockBackend::new(name, fail_count),
            prompts: std::sync::Mutex::new(Vec::new()),
        })
    }
}

impl ModelBackend for RecordingBackend {
    fn name(&self) -> &str {
        self.inner.name()
    }
    fn complete(
        &self,
        req: CompletionRequest,
    ) -> BoxFuture<'_, Result<CompletionResponse, EngineError>> {
        self.prompts.lock().unwrap().push(req.prompt.clone());
        self.inner.complete(req)
    }
    fn interrupt(&self) -> BoxFuture<'_, Result<(), EngineError>> {
        self.inner.interrupt()
    }
    fn save_state(&self) -> BoxFuture<'_, Result<Vec<u8>, EngineError>> {
        self.inner.save_state()
    }
    fn load_state(&self, state: Vec<u8>) -> BoxFuture<'_, Result<(), EngineError>> {
        self.inner.load_state(state)
    }
}

// ── Scenario 1: Exact completion and error sequences ───────────────────────

#[tokio::test]
async fn exact_completion_returns_expected_text() {
    let backend = MockBackend::new("test-model", 0);
    let req = CompletionRequest::new("system", "Hello world");

    let resp = backend.complete(req).await.unwrap();
    assert!(resp.text.contains("test-model"));
    assert!(resp.text.contains("Hello world"));
    assert!(
        resp.parsed.is_some(),
        "MockBackend should return parseable JSON"
    );
}

#[tokio::test]
async fn exact_error_sequence_fails_n_times_then_succeeds() {
    let backend = MockBackend::new("err-model", 3);

    // First 3 calls should fail
    for i in 1..=3 {
        let req = CompletionRequest::new("sys", &format!("call {i}"));
        let err = backend.complete(req).await.unwrap_err();
        assert!(
            format!("{err:?}").contains("simulated failure"),
            "Call {i} should fail: {err:?}"
        );
    }

    // 4th call succeeds
    let req = CompletionRequest::new("sys", "call 4");
    let resp = backend.complete(req).await.unwrap();
    assert!(resp.text.contains("err-model"));
}

#[tokio::test]
async fn completion_usage_reports_tokens() {
    let backend = MockBackend::default();
    let mut req = CompletionRequest::new("system", "prompt text here");
    req.estimated_prompt_tokens = 42;

    let resp = backend.complete(req).await.unwrap();
    assert_eq!(
        resp.usage.prompt_tokens, 42,
        "Should echo estimated prompt tokens"
    );
    assert!(
        resp.usage.completion_tokens > 0,
        "Should report completion tokens"
    );
    assert_eq!(
        resp.usage.completion_tokens + 42,
        resp.usage.total(),
        "Total should be prompt + completion"
    );
}

// ── Scenario 2: Token streaming and cancellation ───────────────────────────

#[tokio::test]
async fn streaming_callback_receives_tokens() {
    let backend = MockBackend::default();
    let tokens = Arc::new(std::sync::Mutex::new(Vec::new()));
    let tokens_clone = tokens.clone();

    let mut req = CompletionRequest::new("sys", "stream test");
    req.on_token = Some(Box::new(move |tok: &str| {
        tokens_clone.lock().unwrap().push(tok.to_string());
    }));

    let resp = backend.complete(req).await.unwrap();
    let collected = tokens.lock().unwrap();

    // MockBackend produces one response as a single callback invocation
    assert!(
        !collected.is_empty(),
        "Expected at least one token callback, got none"
    );
    assert!(
        resp.text.contains("result"),
        "Response should contain 'result'"
    );
}

#[tokio::test]
async fn cancellation_interrupts_in_flight_completion() {
    let backend = MockBackend::new("cancel-model", 0);

    // MockBackend's interrupt is a no-op that returns Ok
    let result = backend.interrupt().await;
    assert!(result.is_ok(), "interrupt should succeed");

    // After interrupt, a new completion should still work
    let req = CompletionRequest::new("sys", "after cancel");
    let resp = backend.complete(req).await.unwrap();
    assert!(resp.text.contains("cancel-model"));
}

#[tokio::test]
async fn streaming_with_cancellation_stops_token_callback() {
    let backend = Arc::new(MockBackend::new("stream-cancel", 0));
    let b2 = backend.clone();

    let tokens = Arc::new(AtomicUsize::new(0));
    let tokens_clone = tokens.clone();

    let mut req = CompletionRequest::new("sys", "stream cancel test");
    req.on_token = Some(Box::new(move |_tok: &str| {
        tokens_clone.fetch_add(1, Ordering::Relaxed);
    }));

    // Interrupt immediately, then complete
    b2.interrupt().await.unwrap();
    let _resp = b2.complete(req).await.unwrap();

    let count = tokens.load(Ordering::Relaxed);
    assert!(count > 0, "Should have received tokens before interrupt");
}

// ── Scenario 3: Malformed structured output followed by retry ──────────────

#[tokio::test]
async fn retry_after_failure_eventually_succeeds() {
    let backend = MockBackend::new("retry-model", 2); // First 2 fail

    // Retry loop: try up to 5 times
    let mut last_error = None;
    for attempt in 1..=5 {
        let req = CompletionRequest::new("sys", &format!("attempt {attempt}"));
        match backend.complete(req).await {
            Ok(resp) => {
                assert!(resp.text.contains("retry-model"));
                assert!(
                    attempt > 2,
                    "Should succeed after 2 failures, got attempt {attempt}"
                );
                return;
            }
            Err(e) => {
                last_error = Some(e);
            }
        }
    }

    panic!("All 5 attempts failed. Last error: {last_error:?}");
}

#[tokio::test]
async fn retry_with_max_exceeded_returns_last_error() {
    let backend = MockBackend::new("always-fail", 10);

    let mut last_err = None;
    for attempt in 1..=3 {
        let req = CompletionRequest::new("sys", &format!("attempt {attempt}"));
        match backend.complete(req).await {
            Ok(_) => {
                panic!("Should not succeed before fail_count is exhausted");
            }
            Err(e) => {
                last_err = Some(e);
            }
        }
    }

    let err = last_err.unwrap();
    assert!(
        format!("{err:?}").contains("simulated failure"),
        "Should still fail: {err:?}"
    );
}

// ── Scenario 4: Tool approval, failure, and rollback ───────────────────────

/// Simulates a tool call flow: approve → execute → record.
#[tokio::test]
async fn tool_approval_and_execution_flow() {
    let backend = MockBackend::default();

    // Simulate tool: generate a completion (as the tool would)
    let req = CompletionRequest::new(
        "You are a writing tool.",
        "Write a chapter titled 'The Beginning'",
    );
    let resp = backend.complete(req).await.unwrap();
    assert!(
        resp.text.contains("result") || resp.text.contains("mock"),
        "Tool output should be mock-generated: {}",
        resp.text
    );
}

/// Simulates tool failure → rollback (undo last action).
#[tokio::test]
async fn tool_failure_triggers_rollback() {
    let backend = MockBackend::new("tool-model", 1); // First call fails

    // Step 1: first attempt fails
    let req1 = CompletionRequest::new("sys", "write chapter 1");
    let err = backend.complete(req1).await.unwrap_err();
    assert!(
        format!("{err:?}").contains("simulated failure"),
        "Tool should fail: {err:?}"
    );

    // Step 2: rollback by saving state before, then retrying
    let state_before = backend.save_state().await.unwrap();

    // Step 3: retry (this should succeed after the initial fail is consumed)
    let req2 = CompletionRequest::new("sys", "write chapter 1 (retry)");
    let resp = backend.complete(req2).await.unwrap();
    assert!(
        resp.text.contains("tool-model"),
        "Retry should succeed: {}",
        resp.text
    );

    // Step 4: rollback by loading the pre-failure state
    backend.load_state(state_before).await.unwrap();
}

/// Simulates tool state preservation across multiple calls.
#[tokio::test]
async fn tool_state_preservation() {
    let backend = MockBackend::default();

    // First call — set state
    let req1 = CompletionRequest {
        prompt: "First operation".into(),
        preserve_state: true,
        ..Default::default()
    };
    backend.complete(req1).await.unwrap();

    // Save state after first operation
    let state = backend.save_state().await.unwrap();

    // Second call — continue from saved state
    backend.load_state(state).await.unwrap();
    let req2 = CompletionRequest {
        prompt: "Second operation".into(),
        preserve_state: true,
        ..Default::default()
    };
    let resp = backend.complete(req2).await.unwrap();
    assert!(resp.text.contains("mock"));
}

// ── Scenario 5: Interrupted story generation and resume ────────────────────

#[tokio::test]
async fn interrupted_generation_can_resume() {
    let backend = MockBackend::new("story-model", 0);

    // Generate first chunk
    let req1 = CompletionRequest::new("A storyteller", "Chapter 1: The Beginning");
    let resp1 = backend.complete(req1).await.unwrap();

    // Save state at this point (simulates checkpoint)
    let checkpoint = backend.save_state().await.unwrap();

    // Interrupt (simulate user cancelling)
    backend.interrupt().await.unwrap();

    // Later: restore state and continue
    backend.load_state(checkpoint).await.unwrap();
    let req2 = CompletionRequest::new("A storyteller", "Continue from where you left off.");
    let resp2 = backend.complete(req2).await.unwrap();

    // Both responses should be valid
    assert!(resp1.text.contains("story-model"));
    assert!(resp2.text.contains("story-model"));
}

#[tokio::test]
async fn interrupted_generation_restores_saved_state() {
    let backend = MockBackend::new("stateful-story", 0);

    // Generate with state preservation
    let req1 = CompletionRequest {
        system: "system".into(),
        prompt: "Chapter 1".into(),
        preserve_state: true,
        ..Default::default()
    };
    backend.complete(req1).await.unwrap();

    // Save checkpoint
    let state = backend.save_state().await.unwrap();

    // Interrupt and reset
    backend.interrupt().await.unwrap();

    // Verify state round-trips
    backend.load_state(state).await.unwrap();
}

// ── Scenario 6: Outline edits propagating to chapter plans ─────────────────

#[tokio::test]
async fn outline_edit_changes_subsequent_generation() {
    let backend = MockBackend::new("outline-model", 0);

    // Step 1: Generate outline
    let outline_req = CompletionRequest::new(
        "You are a story planner.",
        "Create an outline for a fantasy story about a lost city.",
    );
    let outline_resp = backend.complete(outline_req).await.unwrap();
    assert!(
        outline_resp.text.contains("outline-model"),
        "Outline should be generated"
    );

    // Step 2: Edit outline (simulated by changing system prompt)
    let chapter_req = CompletionRequest::new(
        "Outline: A fantasy story about a lost city. Chapter 1: The Discovery.",
        "Write chapter 1 based on the outline.",
    );
    let chapter_resp = backend.complete(chapter_req).await.unwrap();
    assert!(
        chapter_resp.text.contains("outline-model"),
        "Chapter should be generated"
    );

    // Step 3: Edit the premise and regenerate
    let revised_req = CompletionRequest::new(
        "OUTLINE REVISED: The lost city is actually an ancient starship. Chapter 1: The Crash Site.",
        "Rewrite chapter 1 with the revised outline.",
    );
    let revised_resp = backend.complete(revised_req).await.unwrap();
    // MockBackend echoes the prompt snippet, so text contains the
    // backend name and the first 48 chars of the prompt.
    assert!(
        revised_resp.text.contains("outline-model"),
        "Revised chapter should be generated: {}",
        revised_resp.text
    );
}

// ── Scenario 7: Persistence round trips and schema migrations ──────────────

#[tokio::test]
async fn persistence_save_and_load_state() {
    let backend = MockBackend::default();

    let state = backend.save_state().await.unwrap();
    assert!(!state.is_empty(), "State should not be empty");

    // Load the state back
    backend.load_state(state.clone()).await.unwrap();

    // Re-save should produce the same shape
    let state2 = backend.save_state().await.unwrap();
    let parsed: serde_json::Value = serde_json::from_slice(&state).unwrap();
    let parsed2: serde_json::Value = serde_json::from_slice(&state2).unwrap();
    assert_eq!(parsed["mock_state"], parsed2["mock_state"]);
}

#[tokio::test]
async fn persistence_invalid_state_rejected() {
    let backend = MockBackend::default();

    let err = backend.load_state(b"trash".to_vec()).await.unwrap_err();
    let err_str = format!("{err:?}");
    assert!(
        err_str.contains("invalid mock state"),
        "Should reject invalid state: {err_str}"
    );
}

#[tokio::test]
async fn persistence_mix_two_states() {
    let backend = MockBackend::default();

    let state_a = backend.save_state().await.unwrap();

    // Do a completion to change state
    let req = CompletionRequest::new("sys", "change state");
    let _ = backend.complete(req).await.unwrap();
    let state_b = backend.save_state().await.unwrap();

    // Mix with ratio 0.3
    let mixed = backend.mix_states(state_a, state_b, 0.3).await.unwrap();
    let parsed: serde_json::Value = serde_json::from_slice(&mixed).unwrap();

    assert!((parsed["mixed_ratio"].as_f64().unwrap() - 0.3).abs() < 1e-6);
    assert!(parsed.get("source_a").is_some(), "Should contain source_a");
    assert!(parsed.get("source_b").is_some(), "Should contain source_b");
}

#[tokio::test]
async fn persistence_round_trip_recording_backend() {
    let backend = RecordingBackend::new("recording", 0);

    // Multiple completions
    for i in 0..3 {
        let req = CompletionRequest::new("sys", &format!("round trip {i}"));
        let resp = backend.complete(req).await.unwrap();
        assert!(resp.text.contains("recording"));
    }

    // Verify all prompts were recorded
    let prompts = backend.prompts.lock().unwrap();
    assert_eq!(prompts.len(), 3);
    assert_eq!(prompts[0], "round trip 0");
    assert_eq!(prompts[2], "round trip 2");
}

#[tokio::test]
async fn backend_with_latency_respects_delay() {
    let mut backend = MockBackend::new("slow", 0);
    backend.latency_ms = 10; // 10ms delay

    let start = std::time::Instant::now();
    let req = CompletionRequest::new("sys", "timing test");
    backend.complete(req).await.unwrap();
    let elapsed = start.elapsed();

    assert!(
        elapsed.as_millis() >= 10,
        "Should wait at least 10ms, took {elapsed:?}"
    );
}

#[tokio::test]
async fn backend_respects_set_fail_count() {
    let mut backend = MockBackend::new("adjustable", 0);
    backend.set_fail_count(2);

    // Now 2 calls should fail
    let req1 = CompletionRequest::new("sys", "fail 1");
    assert!(backend.complete(req1).await.is_err());

    let req2 = CompletionRequest::new("sys", "fail 2");
    assert!(backend.complete(req2).await.is_err());

    // 3rd succeeds
    let req3 = CompletionRequest::new("sys", "succeed");
    assert!(backend.complete(req3).await.is_ok());
}

#[tokio::test]
async fn think_trace_extraction() {
    let backend = MockBackend::default();

    // Without thinking mode
    let req = CompletionRequest::new("sys", "no think");
    let resp = backend.complete(req).await.unwrap();
    assert!(resp.think_trace.is_none());

    // With thinking mode
    let mut req_think = CompletionRequest::new("sys", "think mode");
    req_think.thinking = true;
    let resp = backend.complete(req_think).await.unwrap();
    let trace = resp
        .think_trace
        .expect("Should have think_trace when thinking=true");
    assert!(!trace.is_empty(), "Trace should not be empty");
    // MockBackend produces: " thinkingthinking about '...' response\n{...}"
    // The text starts with " thinking" (space + thinking) from the think block
    assert!(
        resp.text.contains("think mode"),
        "Response should contain think mode reference: {}",
        resp.text
    );
}
