//! Server/gateway subcommands: `roco server` and `roco gateway`.

use std::path::PathBuf;
use std::sync::Arc;

use crate::daemon::{self, GATEWAY_PORT};
use crate::parse_opt;

pub fn cmd_gateway(extra: &[&str]) {
    use roco_gateway::Gateway;

    let host = parse_opt("--host", extra).unwrap_or("127.0.0.1");
    let port_str = parse_opt("--port", extra).unwrap_or("8000");
    let port = port_str.parse::<u16>().unwrap_or(8000);
    let target = parse_opt("--target", extra).unwrap_or("http://127.0.0.1:8080");
    let limit_str = parse_opt("--rate-limit", extra).unwrap_or("60");
    let limit = limit_str.parse::<usize>().unwrap_or(60);

    let detach = extra.iter().any(|&a| a == "--detach" || a == "-d");
    let is_child = extra.iter().any(|&a| a == "--_child-gateway");
    let log_path = parse_opt("--log-file", extra)
        .map(PathBuf::from)
        .unwrap_or_else(|| daemon::default_detach_path("gateway", port, "log"));
    let pid_path = parse_opt("--pid-file", extra)
        .map(PathBuf::from)
        .unwrap_or_else(|| daemon::default_detach_path("gateway", port, "pid"));

    if detach && !is_child {
        daemon::spawn_detached("gateway", extra, &log_path, &pid_path);
        return;
    }

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("Failed to build Tokio runtime");

    rt.block_on(async {
        let gateway = Gateway::new(host.to_string(), port, target.to_string(), limit);
        println!("Starting API Gateway on {host}:{port} targeting {target} (limit: {limit}/min)...");
        if let Err(e) = gateway.run().await {
            eprintln!("Gateway error: {e}");
        }
    });
}

pub fn cmd_server(extra: &[&str]) {
    use roco_agent::story_engine::{StoryConfig, StoryEngine};
    use roco_infer_client::RemoteBackend;
    use roco_inference::RwkvBackend;
    use roco_server::{Server, ServerConfig};
    use std::sync::Arc;

    let host = parse_opt("--host", extra).unwrap_or("127.0.0.1");
    let port_str = parse_opt("--port", extra).unwrap_or("8080");
    let port = port_str.parse::<u16>().unwrap_or(8080);
    let story_mode = extra.iter().any(|&a| a == "--story" || a == "-s");
    let stdio_lsp = extra.iter().any(|&a| a == "--stdio-lsp");
    let inference_url = parse_opt("--inference-url", extra)
        .map(|s| s.to_string())
        .unwrap_or_else(|| {
            std::env::var("ROCO_API_URL")
                .unwrap_or_else(|_| "http://127.0.0.1:8080".to_string())
        });

    // Detach mode
    let detach = extra.iter().any(|&a| a == "--detach" || a == "-d");
    let is_child = extra.iter().any(|&a| a == "--_child-server");
    let log_path = parse_opt("--log-file", extra)
        .map(PathBuf::from)
        .unwrap_or_else(|| daemon::default_detach_path("server", port, "log"));
    let pid_path = parse_opt("--pid-file", extra)
        .map(PathBuf::from)
        .unwrap_or_else(|| daemon::default_detach_path("server", port, "pid"));

    if detach && !is_child {
        daemon::spawn_detached("server", extra, &log_path, &pid_path);
        return;
    }

    // LSP mode — stdin/stdout protocol for editor plugins
    if stdio_lsp {
        let url = inference_url.clone();
        let rt = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("Failed to build Tokio runtime");
        rt.block_on(async move {
            tracing_subscriber::fmt()
                .with_env_filter(
                    tracing_subscriber::EnvFilter::try_from_default_env()
                        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
                )
                .init();
            let client = Arc::new(RemoteBackend::new(url));
            match crate::lsp_handler::run_lsp(client).await {
                Ok(()) => tracing::info!("RoCo LSP session ended"),
                Err(e) => {
                    eprintln!("RoCo LSP error: {e}");
                    std::process::exit(1);
                }
            }
        });
        return;
    }

    // Resolve backend
    let backend: Arc<dyn roco_engine::ModelBackend> = if inference_url.contains("localhost")
        || inference_url.contains("127.0.0.1")
    {
        match RwkvBackend::from_env() {
            Ok(b) => Arc::new(b) as Arc<dyn roco_engine::ModelBackend>,
            Err(e) => {
                eprintln!("Error loading backend: {e}");
                std::process::exit(1);
            }
        }
    } else {
        Arc::new(RemoteBackend::new(inference_url)) as Arc<dyn roco_engine::ModelBackend>
    };

    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("Failed to build Tokio runtime");

    rt.block_on(async {
        if story_mode {
            // Story mode: build axum app with story routes
            use crate::story_routes::create_story_router;

            println!("Model loaded successfully.");
            println!("Story mode enabled — initializing story engine...");
            let story_config = StoryConfig {
                interactive: true,
                validate_quality: true,
                ..Default::default()
            };
            let engine = match StoryEngine::new(story_config) {
                Ok(e) => e,
                Err(e) => {
                    eprintln!("Error creating story engine: {e}");
                    std::process::exit(1);
                }
            };

            let app = roco_server::routes::create_router(backend.clone())
                .merge(create_story_router(backend.clone(), engine));

            let addr = format!("{host}:{port}");
            println!("Starting story server on {addr}...");
            let listener = tokio::net::TcpListener::bind(&addr)
                .await
                .expect("Failed to bind TCP listener");
            if let Err(e) = axum::serve(listener, app).await {
                eprintln!("Server error: {e}");
            }
        } else {
            // Normal server mode
            let config = ServerConfig {
                host: host.to_string(),
                port,
            };
            let server = Server::new(config, backend);
            println!("Starting server on {host}:{port}...");
            if let Err(e) = server.run().await {
                eprintln!("Server error: {e}");
            }
        }
    });
}
