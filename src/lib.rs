//! RoCo AI — core library.
//!
//! The orchestration layer (stateful RNN/RWKV/SSM agentic behavior) lives here
//! as a library so other crates — e.g. the `roco-gui` Dioxus visualizer — can
//! depend on it and run the real engine directly. The `roco` binary
//! (`src/main.rs`) is a thin CLI built on top of this library.

pub mod agent;
pub mod audio;
pub mod builtins;
pub mod capacity;
pub mod config;
pub mod engine;
pub mod eval;
pub mod grammar;
pub mod infer;
pub mod logger;
pub mod memory;
pub mod policy;
pub mod sandbox;
pub mod session;
pub mod toolcall;
pub mod tools;
pub mod trace;
pub mod vector;
pub mod visualizer;
pub mod workspace;

#[cfg(feature = "http-backends")]
pub mod backends;
