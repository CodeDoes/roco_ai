//! RoCo AI — core library.
//!
//! The orchestration layer (stateful RNN/RWKV/SSM agentic behavior) lives here
//! as a library so other crates — e.g. `roco-session`, `roco-cli`, the
//! `roco-gui` Dioxus visualizer, or a napi-rs addon — can depend on it and run
//! the real engine directly.

pub mod agent;
pub mod agent_profile;
pub mod audio;
pub mod builtins;
pub mod capacity;
pub mod config;
pub mod engine;
pub mod eval;
pub mod eval_suite;
pub mod inference;
pub mod grammar;
pub mod infer;
pub mod logger;
pub mod memory;
pub mod policy;
pub mod sandbox;
pub mod toolcall;
pub mod tools;
pub mod trace;
pub mod vector;
pub mod visualizer;

#[cfg(any(feature = "http-backends", feature = "local-rwkv"))]
pub mod backends;

#[cfg(feature = "local-rwkv")]
pub mod rwkv_backend;
