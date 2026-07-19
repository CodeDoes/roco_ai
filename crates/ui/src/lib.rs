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