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

use std::path::{Path, PathBuf};
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

// ---------------------------------------------------------------------------
// Trace persistence — save/load structured runs for replay and diff
// ---------------------------------------------------------------------------

/// A complete, replayable execution trace.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Trace {
    /// Unique identifier for this run (UUID or timestamp-based).
    pub id: String,
    /// Human-readable task objective.
    pub objective: String,
    /// Time-ordered execution events.
    pub events: Vec<TraceEvent>,
    /// Conversation messages (role + content pairs).
    #[serde(default)]
    pub messages: serde_json::Value,
    /// Knowledge graph edges (optional).
    #[serde(default)]
    pub memory: serde_json::Value,
    /// Summary stats.
    #[serde(default)]
    pub summary: TraceSummary,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TraceSummary {
    pub subtask_count: usize,
    pub failed_subtasks: usize,
    pub model_calls: usize,
    pub tool_calls: usize,
    pub tool_errors: usize,
    pub retries: usize,
    pub duration_ms: u64,
}

impl Trace {
    /// Build a Trace from a CollectingTracer snapshot plus metadata.
    pub fn from_collector(
        id: impl Into<String>,
        objective: impl Into<String>,
        tracer: &CollectingTracer,
        messages: serde_json::Value,
        memory: serde_json::Value,
        summary: TraceSummary,
    ) -> Self {
        Self {
            id: id.into(),
            objective: objective.into(),
            events: tracer.snapshot(),
            messages,
            memory,
            summary,
        }
    }

    /// Write the trace to a JSON file.
    pub fn save(&self, dir: impl AsRef<Path>) -> anyhow::Result<PathBuf> {
        let dir = dir.as_ref();
        std::fs::create_dir_all(dir)?;
        let path = dir.join(format!("{}.json", self.id));
        let json = serde_json::to_string_pretty(self)?;
        std::fs::write(&path, json)?;
        Ok(path)
    }

    /// Load a trace from a JSON file.
    pub fn load(path: impl AsRef<Path>) -> anyhow::Result<Self> {
        let json = std::fs::read_to_string(path)?;
        Ok(serde_json::from_str(&json)?)
    }
}

/// A store that manages saved traces in a directory.
///
/// Provides listing, loading, and diffing capabilities.
pub struct TraceStore {
    dir: PathBuf,
}

impl TraceStore {
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into() }
    }

    /// Save a trace and return the file path.
    pub fn save(&self, trace: &Trace) -> anyhow::Result<PathBuf> {
        trace.save(&self.dir)
    }

    /// List all trace IDs in the store (sorted by file name).
    pub fn list(&self) -> anyhow::Result<Vec<String>> {
        let mut ids = Vec::new();
        if self.dir.exists() {
            for entry in std::fs::read_dir(&self.dir)? {
                let entry = entry?;
                if let Some(name) = entry.file_name().to_str() {
                    if name.ends_with(".json") {
                        ids.push(name.trim_end_matches(".json").to_string());
                    }
                }
            }
        }
        ids.sort();
        Ok(ids)
    }

    /// Load a trace by ID.
    pub fn load(&self, id: &str) -> anyhow::Result<Trace> {
        Trace::load(self.dir.join(format!("{id}.json")))
    }

    /// Diff two traces, returning a summary of what changed.
    pub fn diff(id1: &str, t1: &Trace, id2: &str, t2: &Trace) -> TraceDiff {
        let new_phases: Vec<_> = t2
            .events
            .iter()
            .filter(|e| !t1.events.iter().any(|e1| e1.phase == e.phase && e1.detail == e.detail))
            .map(|e| e.phase.clone())
            .collect();
        let removed_phases: Vec<_> = t1
            .events
            .iter()
            .filter(|e| !t2.events.iter().any(|e2| e2.phase == e.phase && e2.detail == e.detail))
            .map(|e| e.phase.clone())
            .collect();
        TraceDiff {
            id1: id1.to_string(),
            id2: id2.to_string(),
            events_added: new_phases.len(),
            events_removed: removed_phases.len(),
            subtask_delta: t2.summary.subtask_count as isize - t1.summary.subtask_count as isize,
            failed_delta: t2.summary.failed_subtasks as isize - t1.summary.failed_subtasks as isize,
            retries_delta: t2.summary.retries as isize - t1.summary.retries as isize,
        }
    }
}

/// Summary of differences between two traces.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceDiff {
    pub id1: String,
    pub id2: String,
    pub events_added: usize,
    pub events_removed: usize,
    pub subtask_delta: isize,
    pub failed_delta: isize,
    pub retries_delta: isize,
}

impl std::fmt::Display for TraceDiff {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "Trace diff: {} vs {}", self.id1, self.id2)?;
        writeln!(f, "  events: +{} / -{}", self.events_added, self.events_removed)?;
        writeln!(f, "  subtasks: {:+}", self.subtask_delta)?;
        writeln!(f, "  failed: {:+}", self.failed_delta)?;
        writeln!(f, "  retries: {:+}", self.retries_delta)?;
        Ok(())
    }
}
