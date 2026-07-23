//! RoCo Gateway — API gateway and request routing.
//!
//! Routes incoming requests to the appropriate inference backend or
//! service, handles authentication, rate limiting, and load balancing
//! across multiple backends.

pub mod gateway;
pub mod router;

pub use gateway::Gateway;
pub use router::Router;

#[cfg(test)]
mod tests;
