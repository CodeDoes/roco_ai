//! RoCo Core — shared types, traits, and utilities used by every crate.
//!
//! This crate contains **only** definitions (types, traits, constants, errors).
//! It has **no** business logic, no I/O, and no heavy dependencies.
//! Every other crate in the workspace depends on this one.

pub mod backend;
pub mod error;
pub mod grammar;
pub mod protocol;
pub mod runtime;
pub mod session;
pub mod text;
pub mod tool;

pub use backend::ModelBackend;
pub use error::RoCoError;
pub use grammar::{StrategyKind, StrategySelector};
pub use protocol::*;
pub use runtime::Runtime;
pub use session::{SessionId, BakeRequest, BakeResponse};
pub use text::{clean_text, strip_thinking, fix_paragraphs};
pub use tool::{Tool, ToolError};


