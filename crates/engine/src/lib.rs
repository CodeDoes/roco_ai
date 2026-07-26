//! RoCo Engine — core trait definitions, data types, and eval framework.
//!
//! This crate defines the [`ModelBackend`] trait that every inference backend
//! implements, the [`CompletionRequest`]/[`CompletionResponse`] types that
//! flow through the pipeline, and the eval suite for benchmarking backends.

pub mod backend;
pub mod cache;
pub mod cases;
pub mod eval;
pub mod grammar;
pub mod story_evals;
pub mod types;
pub mod util;

pub use backend::*;
pub use cache::*;
pub use cases::*;
pub use eval::*;
pub use grammar::*;
pub use types::BnfMask;
pub use types::*;
