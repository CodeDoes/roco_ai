//! `AppContext` — the one object every human-facing surface constructs.
//!
//! Construction is the ONLY place that knows about model resolution,
//! daemon lifecycle, and backend selection. Surfaces receive an `AppContext`
//! and call capability methods; they never see `RWKV_MODEL`, `RwkvBackend`,
//! `RemoteBackend`, or a tokio runtime.

use std::path::PathBuf;
use std::sync::Arc;

use roco_engine::{ModelBackend, CompletionRequest, CompletionResponse};
use roco_infer_client::RemoteBackend;
use roco_workspace::WorkspaceKind;

use crate::{
    AppError, AppResult, SessionAgent, SessionHandle, AppWorkspace, Timeline, block_on, generate,
};

/// Default ports for the daemon chain. Re-exported from `daemon` so surfaces
/// can reference them without importing the daemon module directly.
pub use crate::daemon::{GATEWAY_PORT, INFERENCE_PORT};

/// The shared primitive. Build it once, pass `&AppContext` (or clone the
/// `Arc` fields you need) to every surface.
pub struct AppContext {
    /// The model backend. Already connected (daemon started if needed).
    pub backend: Arc<dyn ModelBackend>,
    /// Session store root (where conversations are persisted).
    pub session_root: PathBuf,
    /// Workspace root (where generated artifacts live).
    pub workspace_root: PathBuf,
}

impl AppContext {
    /// Connect to inference, auto-starting the daemon chain if needed.
    ///
    /// This is the single entry point every CLI/TUI/GUI command calls.
    /// It hides: `RWKV_MODEL` resolution, `RwkvBackend::from_env`, the
    /// gateway/inference daemon spawn, and the tokio runtime needed by
    /// `RemoteBackend`.
    pub fn connect() -> Self {
        let backend = connect_backend();
        let base = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        Self {
            backend,
            session_root: base.join("sessions"),
            workspace_root: base.join("workspaces"),
        }
    }

    /// Connect to a remote inference URL directly (no daemon management).
    /// Used by the LSP client and by `gui` when pointing at an external server.
    pub fn connect_remote(url: &str) -> Self {
        let backend: Arc<dyn ModelBackend> = Arc::new(RemoteBackend::new(url.to_string()));
        let base = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        Self {
            backend,
            session_root: base.join("sessions"),
            workspace_root: base.join("workspaces"),
        }
    }

    // ── session ──────────────────────────────────────────────────────────
    /// Open (or create) a conversation session. Returns a handle the surface
    /// uses for all `session_*` operations.
    pub fn session(&self, id: &str) -> AppResult<SessionHandle> {
        SessionHandle::open(&self.session_root, id)
    }

    /// List all known session ids.
    pub fn list_sessions(&self) -> Vec<String> {
        SessionHandle::list(&self.session_root)
    }

    // ── session_agent ────────────────────────────────────────────────────
    /// Bind an agent persona to a session. The returned `SessionAgent` is the
    /// surface's handle for `session_agent_message`.
    pub fn session_agent(&self, session: &SessionHandle, agent: &str) -> AppResult<SessionAgent> {
        SessionAgent::bind(&Arc::new(session.clone()), agent)
    }

    // ── model_state ──────────────────────────────────────────────────────
    /// Save the current recurrent model state to bytes.
    pub fn model_state_save(&self) -> AppResult<Vec<u8>> {
        block_on(self.backend.save_state()).map_err(AppError::Engine)
    }

    /// Load a recurrent model state from bytes.
    pub fn model_state_load(&self, state: Vec<u8>) -> AppResult<()> {
        block_on(self.backend.load_state(state)).map_err(AppError::Engine)
    }

    // ── model_state_generate / generate_poll_finish ──────────────────────
    /// Generate while carrying a saved model state. `state` is loaded, the
    /// completion runs (including streaming via `on_token`), then the state is
    /// restored to what it was before — so the caller's session is untouched.
    pub fn model_state_generate(
        &self,
        req: CompletionRequest,
        state: Vec<u8>,
    ) -> AppResult<CompletionResponse> {
        // Load the provided state, generate, then restore prior state.
        let prior = block_on(self.backend.save_state()).map_err(AppError::Engine)?;
        block_on(self.backend.load_state(state)).map_err(AppError::Engine)?;
        let out = generate(req, self.backend.as_ref())?;
        block_on(self.backend.load_state(prior)).map_err(AppError::Engine)?;
        Ok(out)
    }

    // ── generate_stream ──────────────────────────────────────────────────
    /// Generate with a per-token callback (streaming). The callback receives
    /// each emitted token as it is produced.
    pub fn generate_stream(
        &self,
        mut req: CompletionRequest,
        on_token: impl FnMut(&str) + Send + 'static,
    ) -> AppResult<CompletionResponse> {
        use std::sync::Mutex;
        let cb = Arc::new(Mutex::new(on_token));
        let cb_clone = cb.clone();
        req.on_token = Some(Box::new(move |tok: &str| {
            let mut f = cb_clone.lock().unwrap();
            f(tok);
        }));
        generate(req, self.backend.as_ref())
    }

    // ── generate_poll_finish ─────────────────────────────────────────────
    /// Run a generation to completion (blocking until done). This is the
    /// default surface operation; `generate_stream` is the callback variant.
    pub fn generate_poll_finish(&self, req: CompletionRequest) -> AppResult<CompletionResponse> {
        generate(req, self.backend.as_ref())
    }

    // ── workspace ────────────────────────────────────────────────────────
    /// Create (or open) a sandbox workspace of the given kind.
    pub fn workspace(&self, name: &str, kind: WorkspaceKind) -> AppResult<AppWorkspace> {
        AppWorkspace::open(&self.workspace_root, name, kind)
    }

    // ── workspace_timeline_reset ─────────────────────────────────────────
    /// Take a timeline checkpoint of a workspace.
    pub fn workspace_timeline_reset(&self, ws: &AppWorkspace, label: &str) -> AppResult<Timeline> {
        ws.checkpoint(label)
    }

    // ── workspace_timeline_compare ───────────────────────────────────────
    /// Diff two timeline checkpoints.
    pub fn workspace_timeline_compare(
        &self,
        ws: &AppWorkspace,
        a: &Timeline,
        b: &Timeline,
    ) -> AppResult<String> {
        ws.diff(a, b)
    }
}

/// Internal: resolve and connect the backend. Mirrors the daemon logic that
/// used to live in `crates/cli/src/daemon.rs` but is now the single source.
fn connect_backend() -> Arc<dyn ModelBackend> {
    // If a gateway is already running, connect to it directly.
    if crate::daemon::is_running("gateway", GATEWAY_PORT) {
        return Arc::new(RemoteBackend::new(format!("http://127.0.0.1:{GATEWAY_PORT}")));
    }
    // Otherwise auto-start the daemon chain via the existing module.
    crate::daemon::ensure_sync_backend()
}
