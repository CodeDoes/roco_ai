//! RoCo Tools — tool abstraction and built-in tool definitions.
//!
//! Defines the [`Tool`] trait that agent-callable tools implement, plus a
//! [`ToolRegistry`] for registration and dispatch. Built-in tools include
//! file I/O, bash execution, and vector operations.

pub mod builtins;
pub mod parse;
pub mod registry;
pub mod tool;

pub use builtins::*;
pub use parse::*;
pub use registry::ToolRegistry;
pub use tool::Tool;
pub use tool::ToolError;
