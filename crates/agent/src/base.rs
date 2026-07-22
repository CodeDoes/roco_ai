//! Shared trait for all agent harnesses.
//!
//! Both [`CommonAgent`](crate::CommonAgent) (ReAct loop) and
//! [`MechanicAgent`](crate::MechanicAgent) (code-driven pipeline) implement
//! this trait, guaranteeing the same external interface: take a human message,
//! produce a human response, possibly reading/writing files via tools along the
//! way.

use async_trait::async_trait;
use roco_engine::ModelBackend;

use crate::error::AgentError;

/// Common interface for all agent harnesses.
///
/// Implementors share:
/// - **Input**: human-readable message string
/// - **Output**: human-readable response string
/// - **Side effects**: tools (read/write/edit/search/list/bash) in a sandboxed workspace
/// - **Backend**: model inference capability via [`ModelBackend`]
///
/// Note: this trait uses native async fn (Rust 2021), which requires static
/// dispatch (`fn foo<T: BaseAgent>(agent: &T)`). Dynamic dispatch
/// (`Box<dyn BaseAgent>`) would require the `async-trait` crate.
#[async_trait]
pub trait BaseAgent: Send + Sync {
    /// Run the agent pipeline for a single human message.
    ///
    /// Returns the final human-readable output after all reasoning/tool use
    /// is complete. For detailed traces (steps, tokens, plans), call the
    /// harness-specific `run_detail()` method directly.
    async fn run(&self, backend: &dyn ModelBackend, msg: &str) -> Result<String, AgentError>;

    /// Whether verbose traces are emitted to stderr during execution.
    fn verbose(&self) -> bool;
}
