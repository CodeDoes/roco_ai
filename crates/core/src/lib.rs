//! RoCo AI — core library.
//!
//! The local-RWKV inference engine (RWKV-7 via `web-rwkv` + WGPU) lives here,
//! along with the eval harness and grammar-constrained decoding plumbing.
//! The only active path is `rwkv_backend.rs`; see `crates/core/examples`
//! for the runnable entry points (`eval_suite`, `rwkv_test`, `grammar_smoke`).

pub mod agent;
pub mod agent_profile;
pub mod builtins;
pub mod config;
pub mod engine;
pub mod eval;
pub mod eval_suite;
pub mod grammar;
pub mod jsonschema_to_gbnf;
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
pub mod handler;
