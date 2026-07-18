//! RoCo Engine — core trait definitions, data types, and eval framework.
//!
//! This crate defines the [`ModelBackend`] trait that every inference backend
//! implements, the [`CompletionRequest`]/[`CompletionResponse`] types that
//! flow through the pipeline, and the eval suite for benchmarking backends.

pub mod backend;
pub mod types;
pub mod eval;
pub mod cases;

pub use backend::*;
pub use types::*;
pub use eval::*;
pub use cases::*;

pub use types::BnfMask;
pub use roco_grammar::Schema;
