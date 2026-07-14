//! RoCo Message — chat protocol, formatting, and structured output.
//!
//! Defines role markers (System/User/Assistant), prompt formatting
//! strategies, and GBNF templates for message structures.

pub mod error;
pub mod format;
pub mod gbnf;
pub mod roles;

pub use format::{PromptStyle, build_prompt, ChatMessage};
pub use gbnf::{assistant_response_gbnf, message_format_gbnf, MessageFormatOptions};
pub use roles::*;
