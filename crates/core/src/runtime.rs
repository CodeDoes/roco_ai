//! Runtime abstraction — eliminates `futures::executor::block_on` everywhere.
//!
//! Instead of calling `block_on` directly, code receives a `&dyn Runtime`.
//! This allows:
//! - Production: tokio multi-threaded runtime
//! - Tests: single-threaded blocking runtime
//! - Mock: deterministic synchronous "runtime"

use std::future::Future;

/// Abstract runtime for executing async code.
///
/// Production code uses `TokioRuntime`. Tests use `BlockingRuntime`.
/// This trait lets library code stay runtime-agnostic.
pub trait Runtime: Send + Sync {
    /// Block on a future and return its output.
    fn block_on<F>(&self, f: F) -> F::Output
    where
        F: Future;

    /// Spawn a future in the background.
    fn spawn<F>(&self, f: F)
    where
        F: Future + Send + 'static,
        F::Output: Send;
}

/// Production runtime backed by tokio.
pub struct TokioRuntime;

impl Runtime for TokioRuntime {
    fn block_on<F>(&self, f: F) -> F::Output
    where
        F: Future,
    {
        tokio::runtime::Handle::current().block_on(f)
    }

    fn spawn<F>(&self, f: F)
    where
        F: Future + Send + 'static,
        F::Output: Send,
    {
        tokio::spawn(f);
    }
}

/// Single-threaded blocking runtime for tests.
pub struct BlockingRuntime;

impl Runtime for BlockingRuntime {
    fn block_on<F>(&self, f: F) -> F::Output
    where
        F: Future,
    {
        futures::executor::block_on(f)
    }

    fn spawn<F>(&self, _f: F)
    where
        F: Future + Send + 'static,
        F::Output: Send,
    {
        // In blocking runtime, spawn is a no-op
    }
}

/// Mock runtime for deterministic testing.
pub struct MockRuntime;

impl Runtime for MockRuntime {
    fn block_on<F>(&self, f: F) -> F::Output
    where
        F: Future,
    {
        // For mock, we just poll once — assumes future is ready
        futures::executor::block_on(f)
    }

    fn spawn<F>(&self, _f: F)
    where
        F: Future + Send + 'static,
        F::Output: Send,
    {
    }
}


