//! RoCo TUI — terminal user interface for chat and debugging.
//!
//! A ratatui-based terminal app for interacting with the model, inspecting
//! state, and running evals.

pub mod app;
pub mod widgets;

pub use app::App;
