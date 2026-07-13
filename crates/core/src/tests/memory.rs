//! Tests for `crate::memory`.
//!
//! Included from `memory.rs` via `#[path = "tests/memory.rs"]` so this
//! file is treated as part of the lib crate's `tests` module and runs
//! in the same `cargo test` invocation as the other unit tests.

use super::*;
use crate::engine::{BoxFuture, CompletionResponse, EngineError};
use std::sync::Arc;

fn proc() -> Arc<MemoryProcessor> {
    Arc::new(MemoryProcessor::new(Arc::new(
        crate::vector::HashingEmbedder::new(256),
    )))
}

#[test]
fn deterministic_ingest_dedups_and_resolves() {
    let p = proc();
    // "Moved to Austin" and "Now living in Austin" should collide -> Update/None.
    let r1 = p.ingest_deterministic("u1", "I just moved to Austin, TX and I love it.");
    assert!(!r1.is_empty());
    let before = p.retrieve("u1", "Austin", 5);
    assert_eq!(before.len(), 1, "only one Austin fact should be stored");
    // Contradiction by a different city -> Add (model would DELETE; fallback adds).
    let r2 = p.ingest_deterministic("u1", "Actually I relocated to London last month.");
    let cities = p.retrieve("u1", "city relocation", 5);
    assert!(!cities.is_empty());
    // Both facts retained by the deterministic fallback.
    assert!(p.retrieve("u1", "Austin", 5).len() >= 1);
    assert!(p.retrieve("u1", "London", 5).len() >= 1);
    let _ = (r1, r2);
}

#[test]
fn zep_temporal_graph_closes_old_edges() {
    let p = proc();
    p.merge_triples(vec![TemporalEdge {
        subject: "User".into(),
        predicate: "lives_in".into(),
        object: "New York".into(),
        valid_from: 1,
        valid_to: None,
        source: "a".into(),
    }]);
    p.merge_triples(vec![TemporalEdge {
        subject: "User".into(),
        predicate: "lives_in".into(),
        object: "London".into(),
        valid_from: 2,
        valid_to: None,
        source: "b".into(),
    }]);
    let g = p.graph();
    assert_eq!(g.len(), 2);
    let cur = g.current_for("User");
    assert_eq!(cur.len(), 1);
    assert_eq!(cur[0].object, "London");
}

#[test]
fn letta_tiered_memory_pages() {
    let p = proc();
    p.letta_core_append("u1", "User name: Sam");
    p.letta_core_replace("u1", "User name: Sam", "User name: Samantha");
    p.letta_recall_append("u1", "turn 1", 10);
    p.letta_archival_add("u1", "Sam prefers concise answers");
    let hits = p.letta_archival_search("u1", "prefers concise", 3);
    assert!(!hits.is_empty());
    let m = p.letta("u1");
    assert!(m.core.contains("Samantha"));
    assert_eq!(m.recall.len(), 1);
}

/// A mock backend that returns canned JSON for the memory prompts.
struct MockMemoryBackend;
impl ModelBackend for MockMemoryBackend {
    fn name(&self) -> &str {
        "mock-memory"
    }
    fn complete(
        &self,
        req: CompletionRequest,
    ) -> BoxFuture<'_, Result<CompletionResponse, EngineError>> {
        Box::pin(async move {
            let text = if req.system.contains("conflict resolver") {
                // Mark the new memory as an UPDATE of the first existing fact.
                "{\"action\": \"UPDATE\", \"target_id\": \"f0\"}".to_string()
            } else if req.system.contains("dialectic") {
                "{\"identity\":\"Sam\",\"current_goals\":\"ship memory layer\",\"preferences\":\"concise\",\"open_loops\":\"\"}".to_string()
            } else if req.system.contains("subject-predicate-object") {
                "[[\"User\",\"lives_in\",\"Austin\"]]".to_string()
            } else {
                // Fact extraction.
                "[\"User moved to Austin, TX\", \"User has a dog named Barnaby\"]".to_string()
            };
            Ok(CompletionResponse {
                text: text.clone(),
                usage: crate::engine::TokenUsage::default(),
                parsed: serde_json::from_str(&text).ok(),
                think_trace: None,
            })
        })
    }
}

#[tokio::test]
async fn model_driven_ingest_uses_conflict_resolver() {
    let p = proc();
    let backend: Arc<MockMemoryBackend> = Arc::new(MockMemoryBackend);
    // Seed one fact so the resolver has something to compare against.
    p.ingest_deterministic("u1", "User lives in Austin, TX.");
    let res = p
        .ingest(backend.as_ref(), "u1", "User moved to Austin, TX.")
        .await
        .unwrap();
    // The mock resolver returns UPDATE for the colliding fact.
    assert!(
        res.contains(&Resolution::Update),
        "expected an UPDATE, got {res:?}"
    );
}

#[tokio::test]
async fn model_driven_state_and_graph() {
    let p = proc();
    let backend: Arc<MockMemoryBackend> = Arc::new(MockMemoryBackend);
    let state = p
        .synthesize_state(backend.as_ref(), "u1", "Sam is building an agent.")
        .await
        .unwrap();
    assert_eq!(state.identity, "Sam");
    let edges = p
        .extract_triples(backend.as_ref(), "Sam lives in Austin.")
        .await
        .unwrap();
    assert_eq!(edges.len(), 1);
    assert_eq!(edges[0].object, "Austin");
    p.merge_triples(edges);
    assert_eq!(p.graph().current_for("User").len(), 1);
}
