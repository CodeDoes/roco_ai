//! RoCo Chat Common — shared types across CLI, TUI, and web frontends.
//!
//! Defines types that multiple frontend crates share: conversation state,
//! display preferences, and frontend-agnostic chat data.

pub mod conversation;
pub mod display;

pub use conversation::{Conversation, ConversationId, ConversationTurn};
pub use display::{DisplaySettings, OutputFormat};

#[cfg(test)]
mod tests;
