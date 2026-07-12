//! RoCo Inference Server — background daemon that manages model lifecycle,
//! RAM/VRAM, and serves an OpenAI-compatible completion API.
//!
//! ## Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────┐
//! │                    roco-infer                           │
//! │                                                         │
//! │  ┌──────────┐  ┌──────────┐  ┌──────────────────────┐  │
//! │  │  Model   │  │  Model   │  │   Memory Manager     │  │
//! │  │ Registry │  │  Loader  │  │  (RAM / VRAM budget) │  │
//! │  └──────────┘  └──────────┘  └──────────────────────┘  │
//! │         │              │               │                │
//! │         ▼              ▼               ▼                │
//! │  ┌──────────────────────────────────────────────────┐   │
//! │  │           HTTP API (axum)                        │   │
//! │  │  GET  /health                                    │   │
//! │  │  GET  /v1/models          — list loaded models   │   │
//! │  │  POST /v1/models/load    — load a model          │   │
//! │  │  POST /v1/models/unload  — unload a model        │   │
//! │  │  POST /v1/completions    — generate              │   │
//! │  │  POST /v1/chat/completions — chat completions    │   │
//! │  │  GET  /v1/memory         — memory usage report   │   │
//! │  └──────────────────────────────────────────────────┘   │
//! │                                                         │
//! │  Lock file protocol: /tmp/roco-infer/<model-hash>.lock  │
//! │  Removed on process stop. Stale locks cleaned on boot.  │
//! └─────────────────────────────────────────────────────────┘
//! ```
//!
//! ## Usage
//!
//! ```bash
//! # Start the server with default config
//! cargo run -p roco-infer
//!
//! # Start with specific models pre-loaded
//! cargo run -p roco-infer -- --model path/to/model.st --model path/to/other.st
//!
//! # Load a model at runtime
//! curl -X POST http://localhost:3002/v1/models/load \
//!   -H "Content-Type: application/json" \
//!   -d '{"path": "/home/kit/Documents/models/rwkv.st", "quant": "int8"}'
//!
//! # Generate text
//! curl -X POST http://localhost:3002/v1/completions \
//!   -H "Content-Type: application/json" \
//!   -d '{"model": "rwkv", "prompt": "Hello", "max_tokens": 100}'
//! ```

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use axum::{
    extract::State,
    http::StatusCode,
    response::Json,
    routing::{get, post},
    Router,
};
use clap::Parser;
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tracing::info;

use roco_core::engine::ModelBackend;

// ---------------------------------------------------------------------------
// CLI args
// ---------------------------------------------------------------------------

#[derive(Parser, Clone)]
#[command(name = "roco-infer", about = "RoCo AI inference server")]
pub struct Args {
    /// Address to bind
    #[arg(long, default_value = "0.0.0.0:3002")]
    pub addr: String,

    /// Model files to pre-load at startup (can be repeated)
    #[arg(long = "model")]
    pub preload_models: Vec<PathBuf>,

    /// Lock directory for model coordination
    #[arg(long, default_value = "/tmp/roco-infer")]
    pub lock_dir: PathBuf,

    /// Maximum VRAM to use (in GB). 0 = auto-detect.
    #[arg(long, default_value = "0")]
    pub max_vram_gb: u32,

    /// Maximum RAM to use (in GB). 0 = unlimited.
    #[arg(long, default_value = "0")]
    pub max_ram_gb: u32,
}

// ---------------------------------------------------------------------------
// Model metadata
// ---------------------------------------------------------------------------

/// Information about a loaded model.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelInfo {
    pub id: String,
    pub path: PathBuf,
    pub model_type: String,
    pub quant: String,
    pub vram_mb: u64,
    pub ram_mb: u64,
    pub loaded_at: String,
    pub backend_name: String,
}

// ---------------------------------------------------------------------------
// API types
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct LoadModelRequest {
    pub path: PathBuf,
    #[serde(default = "default_quant")]
    pub quant: String,
}

fn default_quant() -> String { "int8".into() }

#[derive(Debug, Serialize)]
pub struct LoadModelResponse {
    pub id: String,
    pub status: String,
    pub model: ModelInfo,
}

#[derive(Debug, Deserialize)]
pub struct UnloadModelRequest {
    pub id: String,
}

#[derive(Debug, Serialize)]
pub struct UnloadModelResponse {
    pub id: String,
    pub status: String,
}

#[derive(Debug, Deserialize)]
pub struct CompletionRequest {
    pub model: String,
    pub prompt: String,
    pub system: Option<String>,
    #[serde(default = "default_max_tokens")]
    pub max_tokens: usize,
    #[serde(default = "default_temperature")]
    pub temperature: f32,
}

fn default_max_tokens() -> usize { 256 }
fn default_temperature() -> f32 { 0.2 }

#[derive(Debug, Serialize)]
pub struct CompletionResponse {
    pub text: String,
    pub model: String,
    pub usage: Usage,
}

#[derive(Debug, Serialize)]
pub struct Usage {
    pub prompt_tokens: usize,
    pub completion_tokens: usize,
    pub total_tokens: usize,
}

#[derive(Debug, Serialize)]
pub struct MemoryReport {
    pub vram_used_mb: u64,
    pub vram_budget_mb: u64,
    pub ram_used_mb: u64,
    pub ram_budget_mb: u64,
    pub models: Vec<ModelInfo>,
}

#[derive(Debug, Serialize)]
pub struct HealthResponse {
    pub status: String,
    pub service: String,
    pub version: &'static str,
    pub models_loaded: usize,
}

// ---------------------------------------------------------------------------
// App state
// ---------------------------------------------------------------------------

/// Registry of loaded models.
pub struct ModelRegistry {
    /// Loaded model backends, keyed by model ID.
    pub backends: HashMap<String, Box<dyn ModelBackend + Send + Sync>>,
    /// Metadata about each model.
    pub infos: HashMap<String, ModelInfo>,
    /// Memory tracking
    pub vram_used_mb: u64,
    pub ram_used_mb: u64,
}

impl ModelRegistry {
    pub fn new() -> Self {
        Self {
            backends: HashMap::new(),
            infos: HashMap::new(),
            vram_used_mb: 0,
            ram_used_mb: 0,
        }
    }

    pub fn model_count(&self) -> usize {
        self.backends.len()
    }
}

pub struct AppState {
    pub registry: RwLock<ModelRegistry>,
    pub args: Args,
}

impl AppState {
    pub fn new(args: Args) -> Self {
        Self {
            registry: RwLock::new(ModelRegistry::new()),
            args,
        }
    }
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

/// GET /health
async fn health(State(state): State<Arc<AppState>>) -> Json<HealthResponse> {
    let count = state.registry.read().await.model_count();
    Json(HealthResponse {
        status: "ok".into(),
        service: "roco-infer".into(),
        version: env!("CARGO_PKG_VERSION"),
        models_loaded: count,
    })
}

/// GET /v1/models — list loaded models
async fn list_models(State(state): State<Arc<AppState>>) -> Json<Vec<ModelInfo>> {
    let registry = state.registry.read().await;
    let models: Vec<ModelInfo> = registry.infos.values().cloned().collect();
    Json(models)
}

/// POST /v1/models/load — load a model into memory
async fn load_model(
    State(state): State<Arc<AppState>>,
    Json(req): Json<LoadModelRequest>,
) -> Result<Json<LoadModelResponse>, (StatusCode, String)> {
    let path = &req.path;

    if !path.exists() {
        return Err((StatusCode::NOT_FOUND, format!("model not found: {}", path.display())));
    }

    // Generate a model ID from the filename
    let id = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("unknown")
        .to_string();

    // Check if already loaded
    {
        let registry = state.registry.read().await;
        if registry.infos.contains_key(&id) {
            return Err((StatusCode::CONFLICT, format!("model '{id}' is already loaded")));
        }
    }

    // Create the lock file
    let lock_path = state.args.lock_dir.join(format!("{id}.lock"));
    if let Some(parent) = lock_path.parent() {
        tokio::fs::create_dir_all(parent).await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("cannot create lock dir: {e}")))?;
    }

    // Check for stale lock
    if lock_path.exists() {
        info!(path = %lock_path.display(), "stale lock found, removing");
        tokio::fs::remove_file(&lock_path).await.ok();
    }

    // Write lock
    tokio::fs::write(&lock_path, format!("pid={}", std::process::id()))
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("cannot write lock: {e}")))?;

    // TODO: Actually load the model based on file type
    // For now, return a placeholder
    let model_info = ModelInfo {
        id: id.clone(),
        path: path.clone(),
        model_type: path.extension().and_then(|s| s.to_str()).unwrap_or("unknown").into(),
        quant: req.quant,
        vram_mb: 0,
        ram_mb: 0,
        loaded_at: chrono_now(),
        backend_name: "placeholder".into(),
    };

    {
        let mut registry = state.registry.write().await;
        registry.infos.insert(id.clone(), model_info.clone());
    }

    info!(model_id = %id, path = %path.display(), "model loaded");

    Ok(Json(LoadModelResponse {
        id,
        status: "loaded".into(),
        model: model_info,
    }))
}

/// POST /v1/models/unload — unload a model from memory
async fn unload_model(
    State(state): State<Arc<AppState>>,
    Json(req): Json<UnloadModelRequest>,
) -> Result<Json<UnloadModelResponse>, (StatusCode, String)> {
    let mut registry = state.registry.write().await;

    if !registry.infos.contains_key(&req.id) {
        return Err((StatusCode::NOT_FOUND, format!("model '{}' not found", req.id)));
    }

    registry.backends.remove(&req.id);
    registry.infos.remove(&req.id);

    // Remove lock file
    let lock_path = state.args.lock_dir.join(format!("{}.lock", req.id));
    tokio::fs::remove_file(&lock_path).await.ok();

    info!(model_id = %req.id, "model unloaded");

    Ok(Json(UnloadModelResponse {
        id: req.id,
        status: "unloaded".into(),
    }))
}

/// POST /v1/completions — generate completion
async fn completions(
    State(state): State<Arc<AppState>>,
    Json(req): Json<CompletionRequest>,
) -> Result<Json<CompletionResponse>, (StatusCode, String)> {
    let registry = state.registry.read().await;

    let backend = registry.backends.get(&req.model).ok_or_else(|| {
        (StatusCode::NOT_FOUND, format!("model '{}' not loaded. Available: {:?}",
            req.model, registry.infos.keys().collect::<Vec<_>>()))
    })?;

    let sys = req.system.unwrap_or_default();
    let request = roco_core::engine::CompletionRequest {
        system: sys,
        prompt: req.prompt,
        output_schema: None,
        temperature: req.temperature,
        max_tokens: req.max_tokens,
        estimated_prompt_tokens: 0,
    };

    let response = backend.complete(request).await.map_err(|e| {
        (StatusCode::INTERNAL_SERVER_ERROR, format!("inference failed: {e}"))
    })?;

    let usage = Usage {
        prompt_tokens: response.usage.prompt_tokens,
        completion_tokens: response.usage.completion_tokens,
        total_tokens: response.usage.total(),
    };

    Ok(Json(CompletionResponse {
        text: response.text,
        model: req.model,
        usage,
    }))
}

/// GET /v1/memory — memory usage report
async fn memory_report(State(state): State<Arc<AppState>>) -> Json<MemoryReport> {
    let registry = state.registry.read().await;
    let models: Vec<ModelInfo> = registry.infos.values().cloned().collect();
    Json(MemoryReport {
        vram_used_mb: registry.vram_used_mb,
        vram_budget_mb: (state.args.max_vram_gb as u64) * 1024,
        ram_used_mb: registry.ram_used_mb,
        ram_budget_mb: (state.args.max_ram_gb as u64) * 1024,
        models,
    })
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn chrono_now() -> String {
    // Simple timestamp without pulling in chrono
    use std::time::{SystemTime, UNIX_EPOCH};
    let d = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    format!("{}", d.as_secs())
}

/// Clean up stale lock files on startup.
/// On Unix, checks if the owning process is still alive via `kill(pid, 0)`.
/// On other platforms, removes all locks (conservative).
async fn clean_stale_locks(lock_dir: &PathBuf) {
    let Ok(mut entries) = tokio::fs::read_dir(lock_dir).await else { return };
    while let Ok(Some(entry)) = entries.next_entry().await {
        if entry.path().extension().map_or(false, |e| e == "lock") {
            let content = tokio::fs::read_to_string(entry.path()).await.unwrap_or_default();
            let pid_str = content.strip_prefix("pid=").unwrap_or("").trim();
            if let Ok(pid) = pid_str.parse::<u32>() {
                let alive = is_pid_alive(pid);
                if !alive {
                    info!(path = %entry.path().display(), pid, "removing stale lock");
                    tokio::fs::remove_file(entry.path()).await.ok();
                }
            } else {
                // Can't parse PID, remove stale lock
                tokio::fs::remove_file(entry.path()).await.ok();
            }
        }
    }
}

/// Check if a PID is alive. Uses `kill(pid, 0)` on Unix, assumes alive elsewhere.
fn is_pid_alive(pid: u32) -> bool {
    #[cfg(unix)]
    {
        // SAFETY: kill with signal 0 is a standard POSIX check; no side effects.
        unsafe { libc::kill(pid as i32, 0) == 0 }
    }
    #[cfg(not(unix))]
    {
        let _ = pid;
        true // can't check, assume alive to avoid false cleanup
    }
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"))
        )
        .init();

    let args = Args::parse();
    info!("RoCo Inference Server starting up");
    info!("  Lock directory: {}", args.lock_dir.display());
    info!("  Max VRAM: {} GB", if args.max_vram_gb > 0 { format!("{}", args.max_vram_gb) } else { "auto".into() });
    info!("  Max RAM:  {} GB", if args.max_ram_gb > 0 { format!("{}", args.max_ram_gb) } else { "unlimited".into() });
    info!("  Pre-load models: {}", args.preload_models.len());

    // Ensure lock directory exists
    tokio::fs::create_dir_all(&args.lock_dir).await?;

    // Clean stale locks
    clean_stale_locks(&args.lock_dir).await;

    let state = Arc::new(AppState::new(args.clone()));

    // Pre-load models
    for model_path in &args.preload_models {
        info!(path = %model_path.display(), "pre-loading model");
        // TODO: wire up actual model loading
    }

    let app = Router::new()
        .route("/health", get(health))
        .route("/v1/models", get(list_models))
        .route("/v1/models/load", post(load_model))
        .route("/v1/models/unload", post(unload_model))
        .route("/v1/completions", post(completions))
        .route("/v1/chat/completions", post(completions))
        .route("/v1/memory", get(memory_report))
        .with_state(state);

    let addr = &args.addr;
    info!("Inference server listening on http://{addr}");
    info!("  GET  /health                  — health check");
    info!("  GET  /v1/models               — list loaded models");
    info!("  POST /v1/models/load          — load a model");
    info!("  POST /v1/models/unload        — unload a model");
    info!("  POST /v1/completions          — generate text");
    info!("  POST /v1/chat/completions     — chat completions");
    info!("  GET  /v1/memory               — memory usage");

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    // Clean up lock files on shutdown
    info!("Shutting down, cleaning up lock files...");
    for entry in std::fs::read_dir(&args.lock_dir).unwrap_or_else(|_| std::fs::read_dir("/tmp").unwrap()) {
        if let Ok(entry) = entry {
            if entry.path().extension().map_or(false, |e| e == "lock") {
                std::fs::remove_file(entry.path()).ok();
            }
        }
    }

    Ok(())
}
