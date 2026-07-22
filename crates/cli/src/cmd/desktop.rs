//! Desktop subcommands: `roco gui`.

pub fn cmd_gui(_extra: &[&str]) {
    use eframe::egui;
    use roco_app::AppContext;
    use roco_infer_client::RemoteBackend;
    use roco_ui::RocoDesktopApp;
    use std::sync::Arc;

    let exe = std::env::current_exe().expect("failed to get current exe path");

    // 1. Start gateway daemon if not running
    println!(
        "Checking gateway daemon on port {}...",
        crate::daemon::GATEWAY_PORT
    );
    let already_running =
        crate::daemon::ensure_daemon(&exe, "gateway", crate::daemon::GATEWAY_PORT, &["--detach"]);

    if !already_running {
        println!("Gateway starting...");
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("Failed to build Tokio runtime");
        rt.block_on(async {
            match crate::daemon::wait_for_healthy(
                crate::daemon::GATEWAY_PORT,
                std::time::Duration::from_secs(15),
                "Gateway",
            )
            .await
            {
                Ok(()) => println!("Gateway is ready."),
                Err(e) => {
                    eprintln!("Warning: {e}");
                    eprintln!("GUI will start without backend connection.");
                }
            }
        });
    } else {
        println!("Gateway already running.");
    }

    // 2. Construct the shared AppContext.
    let gateway_url = format!("http://127.0.0.1:{}", crate::daemon::GATEWAY_PORT);
    let backend: Option<Arc<dyn roco_engine::ModelBackend>> = Some(Arc::new(RemoteBackend::new(
        gateway_url.clone(),
    ))
        as Arc<dyn roco_engine::ModelBackend>);
    let app_context = AppContext::connect_remote(&gateway_url);

    println!("Starting GUI (backend: {})...", gateway_url);
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1200.0, 800.0])
            .with_title("RoCo AI — Collaborative Story Writing"),
        ..Default::default()
    };

    let app = RocoDesktopApp::with_context(backend, Some(app_context));
    eframe::run_native(
        "RoCo AI Desktop",
        options,
        Box::new(|_cc| Ok(Box::new(app))),
    )
    .expect("GUI failed to start");
}
