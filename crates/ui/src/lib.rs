//! roco_ui — Human-facing widgets for RoCo AI
//!
//! This crate provides the UI widgets for the human-AI collaborative story writing experience.
//! Widgets are built STANDALONE-FIRST and tested in isolation before composition.

// Pacing Control Widget — Human controls the agent's pace
mod pacing;
pub use pacing::*;

// Markdown Editor Widget — The primary surface (prose is the product)
mod markdown_editor;
pub use markdown_editor::*;

// Chat Widget — The conversation surface
mod chat;
pub use chat::*;

// Desktop Application
mod desktop_app;
pub use desktop_app::*;

// Panel / Browser widgets
mod session_browser;
pub use session_browser::*;

mod file_tree;
pub use file_tree::*;

mod wiki_browser;
pub use wiki_browser::*;

mod change_timeline;
pub use change_timeline::*;

mod link_graph;
pub use link_graph::*;
