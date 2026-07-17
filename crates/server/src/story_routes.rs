use std::sync::Arc;
use axum::{
    extract::{Path, State},
    response::IntoResponse,
    routing::{get, post, put},
    Json, Router,
};
use roco_engine::{CompletionRequest, ModelBackend};
use roco_agent::story_engine::{StoryEngine, StoryConfig, PlotState};
use roco_agent::outline_editing::OutlineEditor;
use roco_agent::natural_feedback::FeedbackParser;
use roco_agent::quality::QualityAnalyzer;
use serde::{Deserialize, Serialize};
use tracing::info;

// ═════════════════════════════════════════════════════════════════════════════
// Story API Types
// ═════════════════════════════════════════════════════════════════════════════

#[derive(Debug, Serialize, Deserialize)]
pub struct ChapterInfo {
    pub number: u64,
    pub title: String,
    pub summary: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Outline {
    pub chapters: Vec<ChapterInfo>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Chapter {
    pub number: usize,
    pub title: String,
    pub content: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct GenerateRequest {
    pub direction: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ReviseRequest {
    pub feedback: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SuggestRequest {
    pub text: String,
    pub chapter_num: Option<usize>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ContinueRequest {
    pub text: String,
    pub direction: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct FeedbackRequest {
    pub feedback: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Suggestion {
    pub suggestion_type: String,
    pub text: String,
    pub reasoning: Option<String>,
    pub confidence: f32,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct QualityScore {
    pub overall: f32,
    pub pacing: f32,
    pub engagement: f32,
    pub plot_coherence: f32,
    pub issues: Vec<String>,
    pub suggestions: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PlotStateResponse {
    pub characters: Vec<String>,
    pub locations: Vec<String>,
    pub conflicts: Vec<String>,
    pub themes: Vec<String>,
    pub arc_stage: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct FeedbackResponse {
    pub intent: String,
    pub response: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PublishResponse {
    pub path: String,
    pub chapters: usize,
    pub words: usize,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct StatusResponse {
    pub chapters: usize,
    pub words: usize,
    pub quality: f32,
}

// ═════════════════════════════════════════════════════════════════════════════
// Story API State
// ═════════════════════════════════════════════════════════════════════════════

#[derive(Clone)]
pub struct StoryState {
    pub backend: Arc<dyn ModelBackend>,
    pub engine: Arc<tokio::sync::Mutex<StoryEngine>>,
}

// ═════════════════════════════════════════════════════════════════════════════
// Story API Routes
// ═════════════════════════════════════════════════════════════════════════════

pub fn create_story_router(backend: Arc<dyn ModelBackend>, engine: StoryEngine) -> Router {
    let state = StoryState {
        backend,
        engine: Arc::new(tokio::sync::Mutex::new(engine)),
    };

    Router::new()
        // Outline
        .route("/outline", get(get_outline))
        .route("/outline", put(update_outline))
        // Chapters
        .route("/chapters/:num", get(get_chapter))
        .route("/chapters/:num", put(save_chapter))
        .route("/chapters/:num/generate", post(generate_chapter))
        .route("/chapters/:num/revise", post(revise_chapter))
        .route("/chapters/:num/quality", get(evaluate_quality))
        // Suggestions
        .route("/suggestions", post(get_suggestions))
        .route("/suggestions/apply", post(apply_suggestion))
        // Continue
        .route("/continue", post(continue_writing))
        // Feedback
        .route("/feedback", post(send_feedback))
        // Plot State
        .route("/plot-state", get(get_plot_state))
        // Publish
        .route("/publish", post(publish_story))
        // Status
        .route("/status", get(get_status))
        .with_state(state)
}

// ═════════════════════════════════════════════════════════════════════════════
// Story API Handlers
// ═════════════════════════════════════════════════════════════════════════════

async fn get_outline(State(state): State<StoryState>) -> impl IntoResponse {
    let engine = state.engine.lock().await;
    let outline: Vec<ChapterInfo> = engine.outline().iter().map(|ch| ChapterInfo {
        number: ch.number,
        title: ch.title.clone(),
        summary: ch.summary.clone(),
    }).collect();

    Json(Outline { chapters: outline })
}

async fn update_outline(
    State(state): State<StoryState>,
    Json(outline): Json<Outline>,
) -> impl IntoResponse {
    let mut engine = state.engine.lock().await;
    // TODO: Update outline in engine
    Json(serde_json::json!({ "status": "ok" }))
}

async fn get_chapter(
    State(state): State<StoryState>,
    Path(num): Path<usize>,
) -> impl IntoResponse {
    let engine = state.engine.lock().await;
    let chapters = engine.chapters();

    if num == 0 || num > chapters.len() {
        return Json(serde_json::json!({ "error": "Chapter not found" }));
    }

    let content = &chapters[num - 1];
    Json(Chapter {
        number: num,
        title: format!("Chapter {}", num),
        content: content.clone(),
    })
}

async fn save_chapter(
    State(state): State<StoryState>,
    Path(num): Path<usize>,
    Json(chapter): Json<Chapter>,
) -> impl IntoResponse {
    // TODO: Save chapter to engine
    Json(serde_json::json!({ "status": "ok" }))
}

async fn generate_chapter(
    State(state): State<StoryState>,
    Path(num): Path<usize>,
    Json(req): Json<GenerateRequest>,
) -> impl IntoResponse {
    let backend = state.backend.clone();
    let mut engine = state.engine.lock().await;

    match engine.generate_chapter(&*backend) {
        Ok(content) => {
            let chapters = engine.chapters();
            Json(serde_json::json!({
                "number": chapters.len(),
                "title": format!("Chapter {}", chapters.len()),
                "content": content,
            }))
        }
        Err(e) => {
            Json(serde_json::json!({ "error": e }))
        }
    }
}

async fn revise_chapter(
    State(state): State<StoryState>,
    Path(num): Path<usize>,
    Json(req): Json<ReviseRequest>,
) -> impl IntoResponse {
    let backend = state.backend.clone();
    let mut engine = state.engine.lock().await;

    // TODO: Implement revision with feedback
    Json(serde_json::json!({ "status": "ok", "message": "Revision not yet implemented" }))
}

async fn evaluate_quality(
    State(state): State<StoryState>,
    Path(num): Path<usize>,
) -> impl IntoResponse {
    let backend = state.backend.clone();
    let mut engine = state.engine.lock().await;

    match engine.evaluate_chapter_quality(&*backend, num) {
        Ok(critique) => {
            Json(serde_json::json!({
                "overall": critique.scores.overall,
                "pacing": critique.scores.pacing,
                "engagement": critique.scores.engagement,
                "plot_coherence": critique.scores.plot_coherence,
                "issues": critique.scores.issues.iter().map(|i| i.description.clone()).collect::<Vec<_>>(),
                "suggestions": critique.scores.suggestions,
            }))
        }
        Err(e) => {
            Json(serde_json::json!({ "error": e }))
        }
    }
}

async fn get_suggestions(
    State(state): State<StoryState>,
    Json(req): Json<SuggestRequest>,
) -> impl IntoResponse {
    // TODO: Get suggestions from writing assistant
    Json(serde_json::json!({
        "suggestions": [
            {
                "type": "continuation",
                "text": "The knight hesitated, his hand trembling on the hilt of his sword...",
                "reasoning": "Natural continuation from the current scene",
                "confidence": 0.8
            },
            {
                "type": "alternative",
                "text": "Instead of drawing his sword, the knight dropped to one knee...",
                "reasoning": "Alternative approach showing humility",
                "confidence": 0.7
            }
        ]
    }))
}

async fn apply_suggestion(
    State(state): State<StoryState>,
    Json(suggestion): Json<Suggestion>,
) -> impl IntoResponse {
    // TODO: Apply suggestion to editor
    Json(serde_json::json!({ "status": "ok", "text": suggestion.text }))
}

async fn continue_writing(
    State(state): State<StoryState>,
    Json(req): Json<ContinueRequest>,
) -> impl IntoResponse {
    // TODO: Continue writing from current text
    Json(serde_json::json!({
        "text": "The knight drew his sword, the blade gleaming in the moonlight..."
    }))
}

async fn send_feedback(
    State(state): State<StoryState>,
    Json(req): Json<FeedbackRequest>,
) -> impl IntoResponse {
    let backend = state.backend.clone();

    // Parse feedback
    if let Some(parsed) = FeedbackParser::quick_parse(&req.feedback) {
        Json(serde_json::json!({
            "intent": format!("{:?}", parsed.intent),
            "response": format!("Understood: {}", parsed.summary()),
        }))
    } else {
        // Use model to parse
        Json(serde_json::json!({
            "intent": "general",
            "response": "Feedback noted",
        }))
    }
}

async fn get_plot_state(State(state): State<StoryState>) -> impl IntoResponse {
    let engine = state.engine.lock().await;
    let plot = engine.plot_state();

    Json(PlotStateResponse {
        characters: plot.characters.iter().map(|c| c.name.clone()).collect(),
        locations: vec![plot.current_location.clone()],
        conflicts: plot.active_conflicts.clone(),
        themes: plot.themes.clone(),
        arc_stage: plot.arc_stage.clone(),
    })
}

async fn publish_story(State(state): State<StoryState>) -> impl IntoResponse {
    let engine = state.engine.lock().await;

    match engine.publish() {
        Ok(story) => {
            let path = engine.workspace_path().join("06-STORY.md");
            Json(serde_json::json!({
                "path": path.to_string_lossy(),
                "chapters": engine.chapters().len(),
                "words": story.split_whitespace().count(),
            }))
        }
        Err(e) => {
            Json(serde_json::json!({ "error": e }))
        }
    }
}

async fn get_status(State(state): State<StoryState>) -> impl IntoResponse {
    let engine = state.engine.lock().await;
    let chapters = engine.chapters();
    let words: usize = chapters.iter().map(|c| c.split_whitespace().count()).sum();
    let quality = engine.average_quality();

    Json(StatusResponse {
        chapters: chapters.len(),
        words,
        quality,
    })
}
