//! RoCo Workspace — agent workspace management.
//!
//! Provides the workspace abstraction that scopes file access and tool
//! execution to a project directory, plus workspace-scoped tools that
//! enforce the boundary.

pub mod error;
pub mod tools;
pub mod version;
pub mod workspace;

pub use error::WorkspaceError;
pub use tools::{
    WorkspaceBashTool, WorkspaceEditTool, WorkspaceListTool, WorkspaceReadTool,
    WorkspaceSearchTool, WorkspaceWriteTool,
};
pub use workspace::blocked_command_reason;
pub use workspace::Workspace;
pub use workspace::WorkspaceKind;

pub use version::{ReversibleAction, Snapshot, SnapshotSummary, VersionControl};
