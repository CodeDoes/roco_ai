//! RoCo AI — core library.
//!
//! The local-RWKV inference engine (RWKV-7 via `web-rwkv` + WGPU) lives here,
//! along with the eval harness and grammar-constrained decoding plumbing.
//! The only active inference path is `rwkv_backend.rs`.

pub mod engine;
pub mod eval_cases;
pub mod eval_suite;
pub mod jsonschema_to_gbnf;

#[cfg(feature = "grammar-rwkv")]
pub mod bnf_constraint;
#[cfg(feature = "local-rwkv")]
pub mod rwkv_backend;
#[cfg(feature = "local-rwkv")]
pub mod rwkv_quant_proxy;
