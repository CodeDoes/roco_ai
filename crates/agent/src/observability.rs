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
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

// ═════════════════════════════════════════════════════════════════════════════
// Trace Types
// ═════════════════════════════════════════════════════════════════════════════

/// Unique identifier for a trace
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct TraceId(pub String);

impl TraceId {
    pub fn new() -> Self {
        Self(uuid::Uuid::new_v4().to_string())
    }
}

/// Unique identifier for a span within a trace
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct SpanId(pub String);

impl SpanId {
    pub fn new() -> Self {
        Self(uuid::Uuid::new_v4().to_string())
    }
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
    pub timestamp: u64,
    pub system_prompt: String,
    pub user_prompt: String,
    pub grammar: Option<String>,
    pub temperature: f32,
    pub max_tokens: usize,
    pub output: String,
    pub output_parsed: Option<String>,
    pub latency_ms: u64,
    pub tokens_generated: usize,
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

    /// Flush all logs to disk
    pub fn flush(&self) {
        let timestamp = now();

        // Save trace
        if let Some(trace) = self.current_trace.lock().unwrap().clone() {
            let path = self.output_dir.join(format!("trace_{}.json", trace.trace_id.0));
            if let Ok(json) = serde_json::to_string_pretty(&trace) {
                std::fs::write(path, json).ok();
            }
        }

        // Save model calls
        {
            let calls = self.model_calls.lock().unwrap();
            if !calls.is_empty() {
                let path = self.output_dir.join(format!("model_calls_{}.json", timestamp));
                if let Ok(json) = serde_json::to_string_pretty(&*calls) {
                    std::fs::write(path, json).ok();
                }
            }
        }

        // Save decisions
        {
            let decisions = self.decisions.lock().unwrap();
            if !decisions.is_empty() {
                let path = self.output_dir.join(format!("decisions_{}.json", timestamp));
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
                model_calls.iter().filter(|c| c.success).count() as f64
                    / model_calls.len() as f64
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
        let trace_id = obs.start_trace(HashMap::new());

        // Start span
        let span_id = obs.start_span("test_span", SpanType::ModelCall);

        // Record model call
        obs.record_model_call(ModelCallRecord {
            call_id: "test".into(),
            timestamp: now(),
            system_prompt: "test".into(),
            user_prompt: "test".into(),
            grammar: None,
            temperature: 0.7,
            max_tokens: 100,
            output: "test output".into(),
            output_parsed: None,
            latency_ms: 100,
            tokens_generated: 50,
            success: true,
            error: None,
        });

        // End span
        obs.end_span(&span_id, SpanStatus::Ok);

        // End trace
        obs.end_trace();

        // Check summary
        let summary = obs.summary();
        assert_eq!(summary.total_model_calls, 1);
        assert_eq!(summary.success_rate, 1.0);

        // Cleanup
        let _ = std::fs::remove_dir_all(&temp_dir);
    }
}
