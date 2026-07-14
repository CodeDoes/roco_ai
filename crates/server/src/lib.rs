//! RoCo Server — HTTP API server for the inference backend.
//!
//! Provides a REST/JSON-RPC server that exposes the inference backend
//! over HTTP, enabling remote clients and web UIs to interact with the
//! model.

pub mod server;
pub mod routes;
pub mod config;

pub use server::Server;
pub use config::ServerConfig;
