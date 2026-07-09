//! Execution tracing — the rustviz-style foundation for the visualizer.
//!
//! rustviz works by *instrumenting real execution* and recording a structured
//! event log that a viewer later renders. This module is that recording layer:
//! the [`Orchestrator`] accepts an optional [`Tracer`] and emits [`TraceEvent`]s
//! at each architectural step (decompose → fan-out → verify → retry → aggregate).
//!
//! The event log is the data contract. `visualizer.rs` renders it today (as an
//! HTML trace); a richer web frontend can consume the same [`TraceEvent`] stream
//! later without touching the agent core.

use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// A single recorded step in an agent run.
///
/// `ts_ms` is a monotonic-ish wall-clock timestamp (ms since epoch) so events
/// can be ordered into a timeline even when fanned out concurrently.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceEvent {
    pub ts_ms: u64,
    /// High-level stage: `decompose` | `execute` | `verify` | `retry` |
    /// `aggregate` | `done`.
    pub phase: String,
    /// Who acted: `orchestrator` | `worker-<id>` | `verifier` | `aggregator`.
    pub actor: String,
    /// Human-readable description of what happened.
    pub detail: String,
    /// Optional structured context (subtask ids, scores, attempt counts, ...).
    #[serde(default)]
    pub meta: Value,
}

impl TraceEvent {
    pub fn new(
        phase: impl Into<String>,
        actor: impl Into<String>,
        detail: impl Into<String>,
    ) -> Self {
        Self {
            ts_ms: now_ms(),
            phase: phase.into(),
            actor: actor.into(),
            detail: detail.into(),
            meta: Value::Null,
        }
    }

    /// Attach structured metadata to the event (builder style).
    pub fn with_meta(mut self, meta: Value) -> Self {
        self.meta = meta;
        self
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Recipient of trace events. Implemented by anything that wants to observe a
/// run (a collector, a live websocket sink, a file appender, ...).
pub trait Tracer: Send + Sync {
    fn record(&self, ev: TraceEvent);
}

/// Default in-process collector. Cloning shares the same underlying buffer, so
/// the orchestrator and the caller can both hold a handle.
#[derive(Default, Clone)]
pub struct CollectingTracer {
    events: Arc<Mutex<Vec<TraceEvent>>>,
}

impl CollectingTracer {
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns a time-ordered copy of everything recorded so far.
    pub fn snapshot(&self) -> Vec<TraceEvent> {
        let mut v = self.events.lock().unwrap().clone();
        v.sort_by_key(|e| e.ts_ms);
        v
    }

    pub fn len(&self) -> usize {
        self.events.lock().unwrap().len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl Tracer for CollectingTracer {
    fn record(&self, ev: TraceEvent) {
        self.events.lock().unwrap().push(ev);
    }
}
