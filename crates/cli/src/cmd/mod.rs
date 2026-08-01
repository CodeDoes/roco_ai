//! Subcommand implementations for the `roco` CLI.

pub mod coder;
pub mod debug;
pub mod eval;
pub mod eval_suite;
pub mod export;
pub mod game;
pub mod gpu;
pub mod html;
pub mod inspect;
pub mod interact;
pub mod jobs;
pub mod pet;
pub mod router;
pub mod session;
pub mod solution_bench;
pub mod stats;
pub mod status;
pub mod story;
pub mod story_mode;
pub mod ttrpg;
pub mod vector_search;
pub mod wfc;
pub mod workspace;
pub mod world_sim;

#[cfg(any(feature = "gui", feature = "desktop"))]
pub mod desktop;

#[cfg(feature = "net")]
pub mod server;
