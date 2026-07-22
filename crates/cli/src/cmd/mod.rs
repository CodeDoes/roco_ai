//! Subcommand dispatch helpers for `roco` CLI.
//!
//! Each file in this module handles one or more related subcommands from
//! `crates/cli/src/bin/roco.rs`. The binary entry point delegates to these
//! after argument parsing.

pub mod desktop;
pub mod eval;
pub mod export;
pub mod gpu;
pub mod interact;
pub mod server;
pub mod story;
