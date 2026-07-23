//! RoCo Server — HTTP API server for the inference backend.
//!
//! Provides a REST/JSON-RPC server that exposes the inference backend
//! over HTTP, enabling remote clients and web UIs to interact with the
//! model.

pub mod config;
pub mod routes;
pub mod server;

pub use config::ServerConfig;
pub use routes::create_router;
pub use server::Server;
