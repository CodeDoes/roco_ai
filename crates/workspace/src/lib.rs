//! RoCo Workspace — agent workspace management.
//!
//! Provides the workspace abstraction that scopes file access and tool
//! execution to a project directory, plus workspace-scoped tools that
//! enforce the boundary.

pub mod workspace;
pub mod error;
pub mod tools;

pub use workspace::Workspace;
pub use workspace::WorkspaceKind;
pub use error::WorkspaceError;
pub use tools::{
    WorkspaceReadTool, WorkspaceWriteTool, WorkspaceEditTool, WorkspaceSearchTool,
    WorkspaceListTool, WorkspaceBashTool,
};
