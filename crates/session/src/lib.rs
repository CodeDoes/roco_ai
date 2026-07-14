//! RoCo Session — session state management for stateful conversations.
//!
//! Provides a generic session pool with LRU eviction for storing and
//! retrieving inference state across turns. This crate defines the
//! [`SessionPool`] trait and an LRU implementation.

pub mod pool;
pub mod error;

pub use pool::{SessionPool, LruSessionPool};
pub use error::SessionError;
