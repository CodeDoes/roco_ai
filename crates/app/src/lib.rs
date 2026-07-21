//! `roco_app` (`crates/app/`) — the single primitive every human-facing surface constructs.
//!
//! NOTE ON NAMING: `crates/app/` = this Rust library (`roco_app`). `apps/` = web frontends (`chat`, `studio`, `editor`). `crates/ui/` = desktop widgets (`roco_ui`). See `PROJECT_STRUCTURE.md`.
//!
//! `interact`, `tui`, `gui`, and `story` all build an [`AppContext`] and then
//! call its capability methods. None of them touch `RwkvBackend`,
//! `RemoteBackend`, `SessionStore`, `Workspace`, tokio runtimes, or the
//! `RWKV_MODEL` env var directly — that wiring lives here, in one place.
//!
//! Capability namespace (all surfaces share these):
//! - `session`                  — open / create / list conversation sessions
//! - `session_agent`            — bind an agent persona to a session
//! - `session_agent_message`    — append a user/assistant turn
//! - `model_state`              — save / load recurrent state
//! - `model_state_generate`     — generate while carrying a saved state
//! - `generate_stream`          — generate with a token callback
//! - `generate_poll_finish`     — run a generation to completion
//! - `workspace`                — create / resolve a sandbox workspace
//! - `workspace_transform`      — apply a workspace tool transform
//! - `workspace_timeline_reset` — snapshot (timeline checkpoint)
//! - `workspace_timeline_compare` — diff two checkpoints

use std::path::PathBuf;
use std::sync::Arc;

use roco_agent::SessionStore as AgentSessionStore;
use roco_engine::{CompletionRequest, CompletionResponse, EngineError, ModelBackend};
use roco_infer_client::RemoteBackend;
use roco_session::store::SessionStore as CoreSessionStore;
use roco_workspace::{Workspace, WorkspaceError};

pub mod context;
pub mod daemon;
pub mod session;
pub mod workspace;

pub use context::AppContext;
pub use session::{SessionAgent, SessionHandle};
pub use workspace::{AppWorkspace, Timeline};
pub use roco_workspace::WorkspaceKind;

/// Convenience result alias used by all surface-facing operations.
pub type AppResult<T> = Result<T, AppError>;

/// Uniform error type so surfaces don't import six error enums.
#[derive(Debug)]
pub enum AppError {
    Engine(EngineError),
    Workspace(WorkspaceError),
    Session(String),
    Agent(String),
    Other(String),
}

impl std::fmt::Display for AppError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AppError::Engine(e) => write!(f, "model error: {e}"),
            AppError::Workspace(e) => write!(f, "workspace error: {e}"),
            AppError::Session(e) => write!(f, "session error: {e}"),
            AppError::Agent(e) => write!(f, "agent error: {e}"),
            AppError::Other(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for AppError {}

impl From<EngineError> for AppError {
    fn from(e: EngineError) -> Self {
        AppError::Engine(e)
    }
}
impl From<WorkspaceError> for AppError {
    fn from(e: WorkspaceError) -> Self {
        AppError::Workspace(e)
    }
}
impl From<String> for AppError {
    fn from(e: String) -> Self {
        AppError::Other(e)
    }
}
impl From<anyhow::Error> for AppError {
    fn from(e: anyhow::Error) -> Self {
        AppError::Other(e.to_string())
    }
}

/// Blocking helper: run a future to completion from a sync caller.
///
/// Every surface that isn't already inside a tokio runtime uses this instead
/// of inventing its own runtime. If we're already in a runtime it uses
/// `Handle::block_on`; otherwise it builds a throwaway current-thread runtime.
pub(crate) fn block_on<F: std::future::Future>(fut: F) -> F::Output {
    match tokio::runtime::Handle::try_current() {
        Ok(handle) => handle.block_on(fut),
        Err(_) => {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("failed to build AppContext runtime");
            rt.block_on(fut)
        }
    }
}

/// Synchronous generation to completion (the `generate_poll_finish` op).
/// Surfaces call this instead of `futures::executor::block_on(backend.complete(...))`.
pub(crate) fn generate(req: CompletionRequest, backend: &dyn ModelBackend) -> AppResult<CompletionResponse> {
    block_on(backend.complete(req)).map_err(AppError::Engine)
}
