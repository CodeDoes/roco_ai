//! Desktop subcommands: `roco gui` (requires feature `desktop`).

use crate::daemon;

pub fn cmd_gui(_extra: &[&str]) {
    use eframe::egui;
    use roco_app::AppContext;
    use roco_infer_client::RemoteBackend;
    use roco_ui::RocoDesktopApp;
    use std::sync::Arc;

    let exe = std::env::current_exe().expect("failed to get current exe path");

    println!("Checking gateway daemon on port {}...", daemon::GATEWAY_PORT);
    let already_running =
        daemon::ensure_daemon(&exe, "gateway", daemon::GATEWAY_PORT, &["--detach"]);

    if !already_running {
        println!("Gateway starting...");
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("Failed to build Tokio runtime");
        rt.block_on(async {
            match daemon::wait_for_healthy(
                daemon::GATEWAY_PORT,
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

    let gateway_url = format!("http://127.0.0.1:{}", daemon::GATEWAY_PORT);
    let backend: Option<Arc<dyn roco_engine::ModelBackend>> = Some(Arc::new(RemoteBackend::new(
        gateway_url.clone(),
    ))
        as Arc<dyn roco_engine::ModelBackend>);
    let app_context = AppContext::connect_remote(&gateway_url);

    println!("Starting GUI (backend: {gateway_url})...");
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
