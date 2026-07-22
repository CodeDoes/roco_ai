//! Server/gateway subcommands: `roco server` and `roco gateway`.

use std::path::PathBuf;
use std::sync::Arc;

pub fn cmd_gateway(extra: &[&str]) {
    use roco_gateway::Gateway;

    let host = crate::parse_opt("--host", extra).unwrap_or("127.0.0.1");
    let port_str = crate::parse_opt("--port", extra).unwrap_or("8000");
    let port = port_str.parse::<u16>().unwrap_or(8000);
    let target = crate::parse_opt("--target", extra).unwrap_or("http://127.0.0.1:8080");
    let limit_str = crate::parse_opt("--rate-limit", extra).unwrap_or("60");
    let limit = limit_str.parse::<usize>().unwrap_or(60);

    let detach = extra.iter().any(|&a| a == "--detach" || a == "-d");
    let is_child = extra.iter().any(|&a| a == "--_child-gateway");
    let log_path = crate::parse_opt("--log-file", extra)
        .map(PathBuf::from)
        .unwrap_or_else(|| crate::default_detach_path("gateway", port, "log"));
    let pid_path = crate::parse_opt("--pid-file", extra)
        .map(PathBuf::from)
        .unwrap_or_else(|| crate::default_detach_path("gateway", port, "pid"));

    if detach && !is_child {
        crate::spawn_detached("gateway", extra, &log_path, &pid_path);
        return;
    }

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

        // Auto-start inference server if not running
        let exe = std::env::current_exe().expect("failed to get current exe path");
        crate::daemon::ensure_daemon(&exe, "server", crate::daemon::INFERENCE_PORT, &["--detach"]);

        let gateway = Gateway::new(host.to_string(), port, target.to_string(), limit);
        println!(
            "Starting API Gateway on {host}:{port} targeting {target} (limit: {limit}/min)..."
        );
        if let Err(e) = gateway.run().await {
            eprintln!("Gateway error: {e}");
        }
    });
}

pub fn cmd_server(extra: &[&str]) {
    use roco_agent::story_engine::{StoryConfig, StoryEngine};
    use roco_infer_client::RemoteBackend;

    // Local GPU inference was split into `roco-inferd` so this CLI binary never
    // links wgpu/web-rwkv. `roco server` remains as:
    //   - LSP front-end (`--stdio-lsp`)
    //   - story HTTP façade (`--story`) proxying to inferd
    //   - plain reverse-compat entry that starts/waits for roco-inferd

    let host = crate::parse_opt("--host", extra).unwrap_or("127.0.0.1");
    let port_str = crate::parse_opt("--port", extra).unwrap_or("8080");
    let port = port_str.parse::<u16>().unwrap_or(8080);
    let story_mode = extra.iter().any(|&a| a == "--story" || a == "-s");
    let stdio_lsp = extra.iter().any(|&a| a == "--stdio-lsp");
    let inference_url = crate::parse_opt("--inference-url", extra)
        .map(|s| s.to_string())
        .unwrap_or_else(|| {
            std::env::var("ROCO_API_URL").unwrap_or_else(|_| "http://127.0.0.1:8080".to_string())
        });

    let detach = extra.iter().any(|&a| a == "--detach" || a == "-d");
    let is_child = extra.iter().any(|&a| a == "--_child-server");
    let log_path = crate::parse_opt("--log-file", extra)
        .map(PathBuf::from)
        .unwrap_or_else(|| crate::default_detach_path("server", port, "log"));
    let pid_path = crate::parse_opt("--pid-file", extra)
        .map(PathBuf::from)
        .unwrap_or_else(|| crate::default_detach_path("server", port, "pid"));

    if detach && !is_child {
        // Prefer handing off to roco-inferd when the user wants a detached
        // plain inference server (no --story / --stdio-lsp).
        if !story_mode && !stdio_lsp {
            let exe = std::env::current_exe().ok();
            if let Some(exe) = exe.as_ref() {
                if crate::daemon::ensure_inference_daemon(exe, port) || crate::daemon::is_running("inferd", port) || crate::daemon::is_running("server", port) {
                    println!("inference daemon already running or started on port {port}");
                    return;
                }
            }
            eprintln!(
                "error: could not start roco-inferd.\n                 Build it with: cargo build -p roco-inferd\n                 Then:          roco-inferd --port {port}"
            );
            std::process::exit(2);
        }
        crate::spawn_detached("server", extra, &log_path, &pid_path);
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
        // Plain `roco server` → tell the user to use roco-inferd (or start it).
        let exe = std::env::current_exe().expect("exe");
        println!("roco server: local model serving moved to `roco-inferd`.");
        if crate::daemon::is_running("inferd", port) || crate::daemon::is_running("server", port) {
            println!("inference already healthy on port {port}");
            loop {
                std::thread::sleep(std::time::Duration::from_secs(60));
                if !(crate::daemon::is_running("inferd", port) || crate::daemon::is_running("server", port)) {
                    eprintln!("inference daemon exited");
                    std::process::exit(1);
                }
            }
        }
        crate::daemon::ensure_inference_daemon(&exe, port);
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime");
        if let Err(e) = rt.block_on(crate::daemon::wait_for_healthy(
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
            if !(crate::daemon::is_running("inferd", port) || crate::daemon::is_running("server", port)) {
                eprintln!("inference daemon exited");
                std::process::exit(1);
            }
        }
    }

    // Story mode: HTTP façade that talks to inferd via RemoteBackend.
    use crate::story_routes::create_story_router;
    use roco_server::routes::create_router;

    let exe = std::env::current_exe().expect("exe");
    crate::daemon::ensure_inference_daemon(&exe, crate::daemon::INFERENCE_PORT);

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

        if let Err(e) = crate::daemon::wait_for_healthy(
            crate::daemon::INFERENCE_PORT,
            std::time::Duration::from_secs(120),
            "roco-inferd",
        )
        .await
        {
            eprintln!("{e}");
            std::process::exit(1);
        }

        let backend: Arc<dyn roco_engine::ModelBackend> = Arc::new(RemoteBackend::new(
            format!("http://127.0.0.1:{}", crate::daemon::INFERENCE_PORT),
        ));

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

        let app = create_router(backend.clone()).merge(create_story_router(backend.clone(), engine));
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
