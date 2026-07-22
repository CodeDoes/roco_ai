//! `roco-inferd` — local RWKV inference HTTP daemon.
//!
//! This binary is the *only* default place that links `web-rwkv` / `wgpu`.
//! The main `roco` CLI talks to it over HTTP via `RemoteBackend`, so everyday
//! `cargo build` / `cargo check` never compile the GPU stack.
//!
//! Build:  cargo build -p roco-inferd
//! Run:    roco-inferd [--host 127.0.0.1] [--port 8080]

use std::sync::Arc;

use clap::Parser;
use roco_inference::RwkvBackend;
use roco_server::{Server, ServerConfig};

#[derive(Parser, Debug)]
#[command(name = "roco-inferd", about = "RoCo local RWKV inference daemon")]
struct Args {
    /// Bind address
    #[arg(long, default_value = "127.0.0.1")]
    host: String,

    /// Bind port (default matches roco_app::daemon::INFERENCE_PORT)
    #[arg(long, default_value_t = 8080)]
    port: u16,
}

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let args = Args::parse();

    eprintln!("roco-inferd: loading RWKV model (wgpu)…");
    let backend = match RwkvBackend::from_env() {
        Ok(b) => Arc::new(b),
        Err(e) => {
            eprintln!("error loading backend: {e}");
            eprintln!("hint: set RWKV_MODEL / RWKV_VOCAB or place a .st model under models/");
            std::process::exit(1);
        }
    };
    eprintln!("roco-inferd: model loaded");

    let config = ServerConfig {
        host: args.host.clone(),
        port: args.port,
    };
    let server = Server::new(config, backend);

    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("tokio runtime");

    eprintln!("roco-inferd: listening on http://{}:{}", args.host, args.port);
    if let Err(e) = rt.block_on(server.run()) {
        eprintln!("roco-inferd error: {e}");
        std::process::exit(1);
    }
}
