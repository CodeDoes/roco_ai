//! Server/gateway subcommands (feature `net`).
//!
//! Local GPU inference lives in `roco-inferd`. These commands are HTTP
//! surfaces / LSP / story façade that talk to it over the network.

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

    // Auto-start local GPU daemon if missing.
    let exe = std::env::current_exe().expect("exe");
    let _ = daemon::ensure_inference_daemon(&exe, daemon::INFERENCE_PORT);

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("Failed to build Tokio runtime");

    rt.block_on(async {
        let gateway = Gateway::new(host.to_string(), port, target.to_string(), limit);
        println!(
            "Starting API Gateway on {host}:{port} targeting {target} (limit: {limit}/min)..."
        );
        if let Err(e) = gateway.run().await {
            eprintln!("Gateway error: {e}");
        }
    });
    let _ = GATEWAY_PORT;
}

pub fn cmd_server(extra: &[&str]) {
    use roco_infer_client::RemoteBackend;

    let host = parse_opt("--host", extra).unwrap_or("127.0.0.1");
    let port_str = parse_opt("--port", extra).unwrap_or("8080");
    let port = port_str.parse::<u16>().unwrap_or(8080);
    let story_mode = extra.iter().any(|&a| a == "--story" || a == "-s");
    let stdio_lsp = extra.iter().any(|&a| a == "--stdio-lsp");
    let inference_url = parse_opt("--inference-url", extra)
        .map(|s| s.to_string())
        .unwrap_or_else(|| {
            std::env::var("ROCO_API_URL").unwrap_or_else(|_| "http://127.0.0.1:8080".to_string())
        });

    let detach = extra.iter().any(|&a| a == "--detach" || a == "-d");
    let is_child = extra.iter().any(|&a| a == "--_child-server");
    let log_path = parse_opt("--log-file", extra)
        .map(PathBuf::from)
        .unwrap_or_else(|| daemon::default_detach_path("server", port, "log"));
    let pid_path = parse_opt("--pid-file", extra)
        .map(PathBuf::from)
        .unwrap_or_else(|| daemon::default_detach_path("server", port, "pid"));

    if detach && !is_child {
        if !story_mode && !stdio_lsp {
            let exe = std::env::current_exe().ok();
            if let Some(exe) = exe.as_ref() {
                if daemon::ensure_inference_daemon(exe, port)
                    || daemon::is_running("inferd", port)
                    || daemon::is_running("server", port)
                {
                    println!("inference daemon already running or started on port {port}");
                    return;
                }
            }
            eprintln!(
                "error: could not start roco-inferd.\n\
                 Build it with: cargo build -p roco-inferd\n\
                 Then:          roco-inferd --port {port}"
            );
            std::process::exit(2);
        }
        daemon::spawn_detached("server", extra, &log_path, &pid_path);
        return;
    }

    if stdio_lsp {
        println!("Starting RoCo LSP (client → {inference_url})...");
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
            let client = Arc::new(RemoteBackend::new(inference_url));
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

    if !story_mode {
        let exe = std::env::current_exe().expect("exe");
        println!("roco server: local model serving moved to `roco-inferd`.");
        if daemon::is_running("inferd", port) || daemon::is_running("server", port) {
            println!("inference already healthy on port {port}");
            loop {
                std::thread::sleep(std::time::Duration::from_secs(60));
                if !(daemon::is_running("inferd", port) || daemon::is_running("server", port)) {
                    eprintln!("inference daemon exited");
                    std::process::exit(1);
                }
            }
        }
        daemon::ensure_inference_daemon(&exe, port);
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime");
        if let Err(e) = rt.block_on(daemon::wait_for_healthy(
            port,
            std::time::Duration::from_secs(120),
            "roco-inferd",
        )) {
            eprintln!("{e}");
            eprintln!("Build/run the daemon: cargo run -p roco-inferd -- --port {port}");
            std::process::exit(1);
        }
        println!("roco-inferd is healthy on port {port}");
        loop {
            std::thread::sleep(std::time::Duration::from_secs(60));
            if !(daemon::is_running("inferd", port) || daemon::is_running("server", port)) {
                eprintln!("inference daemon exited");
                std::process::exit(1);
            }
        }
    }

    // Story mode: HTTP façade over RemoteBackend → inferd.
    use crate::story_routes::create_story_router;
    use roco_agent::story_engine::{StoryConfig, StoryEngine};
    use roco_server::routes::create_router;

    let exe = std::env::current_exe().expect("exe");
    daemon::ensure_inference_daemon(&exe, daemon::INFERENCE_PORT);

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

        if let Err(e) = daemon::wait_for_healthy(
            daemon::INFERENCE_PORT,
            std::time::Duration::from_secs(120),
            "roco-inferd",
        )
        .await
        {
            eprintln!("{e}");
            std::process::exit(1);
        }

        let backend: Arc<dyn roco_engine::ModelBackend> = Arc::new(RemoteBackend::new(format!(
            "http://127.0.0.1:{}",
            daemon::INFERENCE_PORT
        )));

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

        let app =
            create_router(backend.clone()).merge(create_story_router(backend.clone(), engine));
        let addr = format!("{host}:{port}");
        println!("Starting story server on {addr} (model via roco-inferd)...");
        let listener = tokio::net::TcpListener::bind(&addr)
            .await
            .expect("Failed to bind TCP listener");
        if let Err(e) = axum::serve(listener, app).await {
            eprintln!("Server error: {e}");
        }
    });
}
