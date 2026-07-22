//! Subcommand implementations for the `roco` CLI.

pub mod coder;
pub mod eval;
pub mod export;
pub mod game;
pub mod gpu;
pub mod html;
pub mod interact;
pub mod pet;
pub mod router;
pub mod story;

#[cfg(feature = "desktop")]
pub mod desktop;

#[cfg(feature = "net")]
pub mod server;
