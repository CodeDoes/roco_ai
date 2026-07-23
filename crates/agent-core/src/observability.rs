//! Observability — traces, logs, and audit trail for the mecha-agent.
//!
//! Every action the agent takes is logged:
//! - Model calls (input, output, grammar, params, latency)
//! - Decisions (what was decided, why, alternatives)
//! - Actions (what was done, where, when)
//! - Quality (scores, issues, suggestions)
//!
//! This enables:
//! - Debugging (replay actions, step through execution)
//! - Interpretability (understand why decisions were made)
//! - Auditing (track all changes)
//! - Improvement (analyze patterns, optimize prompts)

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

// ═════════════════════════════════════════════════════════════════════════════
// Trace Types
// ═════════════════════════════════════════════════════════════════════════════

/// Unique identifier for a trace
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct TraceId(pub String);

impl Default for TraceId {
    fn default() -> Self {
        Self::new()
    }
}

impl TraceId {
    pub fn new() -> Self {
        Self(unique_id("trace"))
    }
}

/// Unique identifier for a span within a trace
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct SpanId(pub String);

impl Default for SpanId {
    fn default() -> Self {
        Self::new()
    }
}

impl SpanId {
    pub fn new() -> Self {
        Self(unique_id("span"))
    }
}

fn unique_id(prefix: &str) -> String {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("{prefix}-{:x}-{:x}", now(), n)
}

/// A complete trace of an agent run
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Trace {
    pub trace_id: TraceId,
    pub started_at: u64,
    pub ended_at: Option<u64>,
    pub spans: Vec<Span>,
    pub metadata: HashMap<String, String>,
}

/// A single span within a trace (one operation)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Span {
    pub span_id: SpanId,
    pub parent_id: Option<SpanId>,
    pub name: String,
    pub span_type: SpanType,
    pub started_at: u64,
    pub ended_at: Option<u64>,
    pub status: SpanStatus,
    pub events: Vec<Event>,
    pub attributes: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum SpanType {
    ModelCall,
    Decision,
    Action,
    QualityCheck,
    UserInteraction,
    FileOperation,
    ToolCall,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum SpanStatus {
    Ok,
    Error,
    Cancelled,
}

/// An event within a span
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Event {
    pub name: String,
    pub timestamp: u64,
    pub attributes: HashMap<String, String>,
}

// ═════════════════════════════════════════════════════════════════════════════
// Model Call Recording
// ═════════════════════════════════════════════════════════════════════════════

/// Complete record of a model call
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelCallRecord {
    pub call_id: String,
    pub request_id: Option<String>,
    pub session_id: Option<String>,
    pub timestamp: u64,
    /// Pipeline phase (e.g. "classify", "think", "derive", "validate", "generate").
    pub phase: String,
    pub system_prompt: String,
    pub user_prompt: String,
    pub grammar: Option<String>,
    pub temperature: f32,
    pub max_tokens: usize,
    pub output: String,
    pub output_parsed: Option<String>,
    pub latency_ms: u64,
    pub tokens_generated: usize,
    /// Prompt token count reported by the backend.
    pub prompt_tokens: usize,
    /// Completion token count reported by the backend.
    pub completion_tokens: usize,
    /// Finish reason ("stop", "length", "timeout", "cancelled", "error").
    pub finish_reason: Option<String>,
    /// Retry count for this call (0 = first attempt).
    pub retry_count: u32,
    /// Stable error code for machine parsing, when `success` is false.
    pub error_code: Option<String>,
    pub success: bool,
    pub error: Option<String>,
}

// ═════════════════════════════════════════════════════════════════════════════
// Decision Recording
// ═════════════════════════════════════════════════════════════════════════════

/// Record of a decision made by the agent
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecisionRecord {
    pub decision_id: String,
    pub timestamp: u64,
    pub decision_type: DecisionType,
    pub description: String,
    pub reasoning: String,
    pub alternatives: Vec<String>,
    pub chosen: String,
    pub confidence: f32,
    pub retry_count: u32,
    /// Stable error code for machine parsing, if the decision was forced by error.
    pub error_code: Option<String>,
    pub context: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DecisionType {
    IntentClassification,
    RouteSelection,
    PlanGeneration,
    TaskDispatch,
    QualityAssessment,
    RevisionDecision,
    UserFeedbackResponse,
}

// ═════════════════════════════════════════════════════════════════════════════
// Action Recording
// ═════════════════════════════════════════════════════════════════════════════

/// Record of an action taken by the agent
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionRecord {
    pub action_id: String,
    pub timestamp: u64,
    pub action_type: ActionType,
    pub description: String,
    pub target: String,
    pub payload: String,
    pub reversible: bool,
    pub undo_payload: Option<String>,
    /// ID of the tool call that produced this action.
    pub tool_call_id: Option<String>,
    /// Approval outcome ("approved", "rejected", "pending", "auto-granted").
    pub approval_status: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ActionType {
    FileWrite,
    FileEdit,
    FileDelete,
    OutlineExpand,
    ChapterGenerate,
    ChapterRevise,
    PlotStateUpdate,
    QualityEvaluate,
    UserPrompt,
}

// ═════════════════════════════════════════════════════════════════════════════
// Quality Recording
// ═════════════════════════════════════════════════════════════════════════════

/// Record of a quality assessment
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QualityRecord {
    pub assessment_id: String,
    pub timestamp: u64,
    pub target: String,
    pub scores: QualityScores,
    pub issues: Vec<QualityIssue>,
    pub passed: bool,
    pub recommendations: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QualityScores {
    pub overall: f32,
    pub pacing: f32,
    pub show_dont_tell: f32,
    pub character_voice: f32,
    pub tense_consistency: f32,
    pub plot_coherence: f32,
    pub engagement: f32,
    pub prose_quality: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QualityIssue {
    pub category: String,
    pub severity: String,
    pub description: String,
    pub location: Option<String>,
}

// ═════════════════════════════════════════════════════════════════════════════
// Checkpoint Recording
// ═════════════════════════════════════════════════════════════════════════════

/// Snapshot of workspace state before and after a mutation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckpointRecord {
    pub checkpoint_id: String,
    pub timestamp: u64,
    pub workspace_id: String,
    pub phase: String,
    /// Files present before the mutation.
    pub files_before: Vec<String>,
    /// Files present after the mutation.
    pub files_after: Vec<String>,
    /// Files that were added.
    pub files_added: Vec<String>,
    /// Files that were modified.
    pub files_modified: Vec<String>,
    /// Files that were deleted.
    pub files_deleted: Vec<String>,
    /// Associated trace or span id.
    pub span_id: Option<String>,
}

// ═════════════════════════════════════════════════════════════════════════════
// Observability System
// ═════════════════════════════════════════════════════════════════════════════

/// The observability system records all agent activity.
pub struct ObservabilitySystem {
    /// Current trace
    current_trace: Mutex<Option<Trace>>,
    /// All model calls
    model_calls: Mutex<Vec<ModelCallRecord>>,
    /// All decisions
    decisions: Mutex<Vec<DecisionRecord>>,
    /// All actions
    actions: Mutex<Vec<ActionRecord>>,
    /// All quality assessments
    quality_assessments: Mutex<Vec<QualityRecord>>,
    /// All checkpoints
    checkpoints: Mutex<Vec<CheckpointRecord>>,
    /// Output directory for logs
    output_dir: PathBuf,
}

impl ObservabilitySystem {
    /// Create a new observability system
    pub fn new(output_dir: PathBuf) -> Self {
        std::fs::create_dir_all(&output_dir).ok();
        Self {
            current_trace: Mutex::new(None),
            model_calls: Mutex::new(Vec::new()),
            decisions: Mutex::new(Vec::new()),
            actions: Mutex::new(Vec::new()),
            quality_assessments: Mutex::new(Vec::new()),
            checkpoints: Mutex::new(Vec::new()),
            output_dir,
        }
    }

    /// Start a new trace
    pub fn start_trace(&self, metadata: HashMap<String, String>) -> TraceId {
        let trace = Trace {
            trace_id: TraceId::new(),
            started_at: now(),
            ended_at: None,
            spans: Vec::new(),
            metadata,
        };
        let trace_id = trace.trace_id.clone();
        *self.current_trace.lock().unwrap() = Some(trace);
        trace_id
    }

    /// End the current trace
    pub fn end_trace(&self) {
        if let Some(ref mut trace) = *self.current_trace.lock().unwrap() {
            trace.ended_at = Some(now());
        }
        self.flush();
    }

    /// Start a new span
    pub fn start_span(&self, name: &str, span_type: SpanType) -> SpanId {
        let span = Span {
            span_id: SpanId::new(),
            parent_id: None,
            name: name.to_string(),
            span_type,
            started_at: now(),
            ended_at: None,
            status: SpanStatus::Ok,
            events: Vec::new(),
            attributes: HashMap::new(),
        };
        let span_id = span.span_id.clone();

        if let Some(ref mut trace) = *self.current_trace.lock().unwrap() {
            trace.spans.push(span);
        }

        span_id
    }

    /// End a span
    pub fn end_span(&self, span_id: &SpanId, status: SpanStatus) {
        if let Some(ref mut trace) = *self.current_trace.lock().unwrap() {
            if let Some(span) = trace.spans.iter_mut().find(|s| s.span_id == *span_id) {
                span.ended_at = Some(now());
                span.status = status;
            }
        }
    }

    /// Add an event to a span
    pub fn add_event(&self, span_id: &SpanId, name: &str, attributes: HashMap<String, String>) {
        if let Some(ref mut trace) = *self.current_trace.lock().unwrap() {
            if let Some(span) = trace.spans.iter_mut().find(|s| s.span_id == *span_id) {
                span.events.push(Event {
                    name: name.to_string(),
                    timestamp: now(),
                    attributes,
                });
            }
        }
    }

    /// Record a model call
    pub fn record_model_call(&self, record: ModelCallRecord) {
        self.model_calls.lock().unwrap().push(record);
    }

    /// Record a decision
    pub fn record_decision(&self, record: DecisionRecord) {
        self.decisions.lock().unwrap().push(record);
    }

    /// Record an action
    pub fn record_action(&self, record: ActionRecord) {
        self.actions.lock().unwrap().push(record);
    }

    /// Record a quality assessment
    pub fn record_quality(&self, record: QualityRecord) {
        self.quality_assessments.lock().unwrap().push(record);
    }

    /// Record a workspace checkpoint.
    pub fn record_checkpoint(&self, record: CheckpointRecord) {
        self.checkpoints.lock().unwrap().push(record);
    }

    /// Get all model calls
    pub fn model_calls(&self) -> Vec<ModelCallRecord> {
        self.model_calls.lock().unwrap().clone()
    }

    /// Get all decisions
    pub fn decisions(&self) -> Vec<DecisionRecord> {
        self.decisions.lock().unwrap().clone()
    }

    /// Get all actions
    pub fn actions(&self) -> Vec<ActionRecord> {
        self.actions.lock().unwrap().clone()
    }

    /// Get all quality assessments
    pub fn quality_assessments(&self) -> Vec<QualityRecord> {
        self.quality_assessments.lock().unwrap().clone()
    }

    /// Get all checkpoints.
    pub fn checkpoints(&self) -> Vec<CheckpointRecord> {
        self.checkpoints.lock().unwrap().clone()
    }

    /// Flush all logs to disk
    pub fn flush(&self) {
        let timestamp = now();

        // Save trace
        if let Some(trace) = self.current_trace.lock().unwrap().clone() {
            let path = self
                .output_dir
                .join(format!("trace_{}.json", trace.trace_id.0));
            if let Ok(json) = serde_json::to_string_pretty(&trace) {
                std::fs::write(path, json).ok();
            }
        }

        // Save model calls
        {
            let calls = self.model_calls.lock().unwrap();
            if !calls.is_empty() {
                let path = self
                    .output_dir
                    .join(format!("model_calls_{}.json", timestamp));
                if let Ok(json) = serde_json::to_string_pretty(&*calls) {
                    std::fs::write(path, json).ok();
                }
            }
        }

        // Save decisions
        {
            let decisions = self.decisions.lock().unwrap();
            if !decisions.is_empty() {
                let path = self
                    .output_dir
                    .join(format!("decisions_{}.json", timestamp));
                if let Ok(json) = serde_json::to_string_pretty(&*decisions) {
                    std::fs::write(path, json).ok();
                }
            }
        }

        // Save actions
        {
            let actions = self.actions.lock().unwrap();
            if !actions.is_empty() {
                let path = self.output_dir.join(format!("actions_{}.json", timestamp));
                if let Ok(json) = serde_json::to_string_pretty(&*actions) {
                    std::fs::write(path, json).ok();
                }
            }
        }

        // Save quality assessments
        {
            let quality = self.quality_assessments.lock().unwrap();
            if !quality.is_empty() {
                let path = self.output_dir.join(format!("quality_{}.json", timestamp));
                if let Ok(json) = serde_json::to_string_pretty(&*quality) {
                    std::fs::write(path, json).ok();
                }
            }
        }

        // Save checkpoints
        {
            let cps = self.checkpoints.lock().unwrap();
            if !cps.is_empty() {
                let path = self
                    .output_dir
                    .join(format!("checkpoints_{}.json", timestamp));
                if let Ok(json) = serde_json::to_string_pretty(&*cps) {
                    std::fs::write(path, json).ok();
                }
            }
        }
    }

    /// Export a diagnostic snapshot (prompts and manuscript **redacted**).
    ///
    /// Secret-like values (API keys, file paths containing "secret" or "key")
    /// are replaced with `[REDACTED]`. Returns the path to the exported file.
    pub fn export_diagnostic(&self) -> std::io::Result<PathBuf> {
        let timestamp = now();
        let path = self
            .output_dir
            .join(format!("diagnostic_{}.json", timestamp));

        let model_calls_redacted: Vec<serde_json::Value> = self
            .model_calls
            .lock()
            .unwrap()
            .iter()
            .map(|mc| {
                let mut v = serde_json::to_value(mc).unwrap_or_default();
                // Redact prompts and output — they contain manuscript content.
                if let Some(obj) = v.as_object_mut() {
                    obj.insert("system_prompt".into(), "[REDACTED]".into());
                    obj.insert("user_prompt".into(), "[REDACTED]".into());
                    obj.insert("output".into(), "[REDACTED]".into());
                    // Also redact any field whose name hints at secrets.
                    for key in obj.keys().cloned().collect::<Vec<_>>() {
                        let lower = key.to_lowercase();
                        if lower.contains("secret")
                            || lower.contains("key")
                            || lower.contains("token")
                        {
                            obj.insert(key, "[REDACTED]".into());
                        }
                    }
                }
                v
            })
            .collect();

        let diagnostic = serde_json::json!({
            "exported_at": timestamp,
            "summary": self.summary(),
            "model_calls": model_calls_redacted,
            "decisions": self.decisions(),
            "actions": self.actions(),
            "quality_assessments": self.quality_assessments(),
            "checkpoints": self.checkpoints(),
        });

        let json = serde_json::to_string_pretty(&diagnostic)?;
        std::fs::write(&path, json)?;
        Ok(path)
    }

    /// Generate a summary report
    pub fn summary(&self) -> ObservabilitySummary {
        let model_calls = self.model_calls.lock().unwrap();
        let decisions = self.decisions.lock().unwrap();
        let actions = self.actions.lock().unwrap();
        let quality = self.quality_assessments.lock().unwrap();

        ObservabilitySummary {
            total_model_calls: model_calls.len(),
            total_decisions: decisions.len(),
            total_actions: actions.len(),
            total_quality_assessments: quality.len(),
            average_latency_ms: if model_calls.is_empty() {
                0.0
            } else {
                model_calls.iter().map(|c| c.latency_ms as f64).sum::<f64>()
                    / model_calls.len() as f64
            },
            success_rate: if model_calls.is_empty() {
                1.0
            } else {
                model_calls.iter().filter(|c| c.success).count() as f64 / model_calls.len() as f64
            },
            reversible_actions: actions.iter().filter(|a| a.reversible).count(),
        }
    }
}

/// Summary of observability data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObservabilitySummary {
    pub total_model_calls: usize,
    pub total_decisions: usize,
    pub total_actions: usize,
    pub total_quality_assessments: usize,
    pub average_latency_ms: f64,
    pub success_rate: f64,
    pub reversible_actions: usize,
}

// ═════════════════════════════════════════════════════════════════════════════
// Helper functions
// ═════════════════════════════════════════════════════════════════════════════

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_observability_system() {
        let temp_dir = std::env::temp_dir().join("roco_test_observability");
        let _ = std::fs::remove_dir_all(&temp_dir);

        let obs = ObservabilitySystem::new(temp_dir.clone());

        // Start trace
        let _trace_id = obs.start_trace(HashMap::new());

        // Start span
        let span_id = obs.start_span("test_span", SpanType::ModelCall);

        // Record model call
        obs.record_model_call(ModelCallRecord {
            call_id: "test".into(),
            request_id: Some("req-1".into()),
            session_id: Some("sess-1".into()),
            timestamp: now(),
            phase: "generate".into(),
            system_prompt: "test".into(),
            user_prompt: "test".into(),
            grammar: None,
            temperature: 0.7,
            max_tokens: 100,
            output: "test output".into(),
            output_parsed: None,
            latency_ms: 100,
            tokens_generated: 50,
            prompt_tokens: 10,
            completion_tokens: 50,
            finish_reason: Some("stop".into()),
            retry_count: 0,
            error_code: None,
            success: true,
            error: None,
        });

        // End span
        obs.end_span(&span_id, SpanStatus::Ok);

        // Record a checkpoint
        obs.record_checkpoint(CheckpointRecord {
            checkpoint_id: "cp-1".into(),
            timestamp: now(),
            workspace_id: "test-ws".into(),
            phase: "generate".into(),
            files_before: vec!["outline.md".into()],
            files_after: vec!["outline.md".into(), "chapters/01-chapter.md".into()],
            files_added: vec!["chapters/01-chapter.md".into()],
            files_modified: vec![],
            files_deleted: vec![],
            span_id: Some(span_id.0.clone()),
        });

        // End trace
        obs.end_trace();

        // Check summary
        let summary = obs.summary();
        assert_eq!(summary.total_model_calls, 1);
        assert_eq!(summary.success_rate, 1.0);

        // Check checkpoints
        let cps = obs.checkpoints();
        assert_eq!(cps.len(), 1);
        assert_eq!(cps[0].workspace_id, "test-ws");

        // Diagnostic export — redacts secrets
        let diag_path = obs.export_diagnostic().unwrap();
        let diag_content = std::fs::read_to_string(&diag_path).unwrap();
        assert!(diag_content.contains("[REDACTED]"));
        assert!(diag_content.contains("exported_at"));
        let _ = std::fs::remove_file(&diag_path);

        // Cleanup
        let _ = std::fs::remove_dir_all(&temp_dir);
    }
}
