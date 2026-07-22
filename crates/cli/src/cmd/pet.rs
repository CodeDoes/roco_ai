//! Desktop Pet command — transparent always-on-top conversational companion.
//!
//! ```text
//! roco pet              → Launch pet (auto-starts daemon chain)
//! roco pet start        → Launch pet (explicit)
//! roco pet stop         → Stop running pet
//! roco pet "Hello!"     → Launch with initial message
//! roco pet --hide       → Start hidden (tray only, GNOME top bar)
//! roco pet --install    → Install .desktop file + auto-start
//! roco pet --uninstall  → Remove .desktop file
//! ```
//!
//! GNOME integration: system tray icon via AppIndicator, .desktop file
//! for launcher menu, auto-start on login.

use crate::rich_output as r;
use std::path::PathBuf;

pub fn cmd_pet(extra: &[&str]) {
    let sub = extra.first().copied().unwrap_or("");

    match sub {
        "stop" | "--stop" | "-s" => cmd_pet_stop(),
        "start" | "--start" => {
            #[cfg(feature = "desktop")]
            run_pet_desktop(&extra[1..]);
            #[cfg(not(feature = "desktop"))]
            need_desktop_feature();
        }
        "--install" | "-i" => install_desktop_file(),
        "--uninstall" | "-u" => uninstall_desktop_file(),
        "--hide" | "-H" => {
            #[cfg(feature = "desktop")]
            run_pet_hidden();
            #[cfg(not(feature = "desktop"))]
            need_desktop_feature();
        }
        "help" | "--help" | "-h" => print_help(),
        _ if sub.starts_with('-') => {
            r::warning(&format!("Unknown flag: {sub}"));
        }
        _ => {
            #[cfg(feature = "desktop")]
            run_pet_desktop(extra);
            #[cfg(not(feature = "desktop"))]
            need_desktop_feature();
        }
    }
}

fn print_help() {
    println!("Usage: roco pet [start|stop|\"message\"|--install|--hide]");
    println!("  (no args)    Launch the desktop pet");
    println!("  start        Launch the desktop pet (explicit)");
    println!("  stop         Stop the running pet (reads PID lock)");
    println!("  \"message\"    Launch pet with initial chat message");
    println!("  --hide       Start hidden in system tray");
    println!("  --install    Install .desktop file + auto-start");
    println!("  --uninstall  Remove .desktop file");
}

// ═══════════════════════════════════════════════════════════════════════
// Desktop file management
// ═══════════════════════════════════════════════════════════════════════

fn desktop_file_paths() -> (PathBuf, PathBuf) {
    let data_dir = dirs_data_dir();
    let autostart_dir = dirs_autostart_dir();
    let desktop_path = data_dir.join("roco-pet.desktop");
    let autostart_path = autostart_dir.join("roco-pet.desktop");
    (desktop_path, autostart_path)
}

fn dirs_data_dir() -> PathBuf {
    // $HOME/.local/share/applications/
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".into());
    PathBuf::from(home).join(".local/share/applications")
}

fn dirs_autostart_dir() -> PathBuf {
    // $HOME/.config/autostart/
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".into());
    PathBuf::from(home).join(".config/autostart")
}

fn install_desktop_file() {
    let exe = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("roco"));

    let (desktop_path, autostart_path) = desktop_file_paths();

    let desktop_content = format!(
        "[Desktop Entry]\n\
         Type=Application\n\
         Name=RoCo Pet\n\
         Comment=Conversational desktop pet companion\n\
         Exec={exe} pet --hide\n\
         Icon=face-smile\n\
         Terminal=false\n\
         Categories=Utility;\n\
         StartupNotify=false\n"
    );

    // Create directories
    let _ = std::fs::create_dir_all(desktop_path.parent().unwrap());
    let _ = std::fs::create_dir_all(autostart_path.parent().unwrap());

    // Write .desktop file
    match std::fs::write(&desktop_path, &desktop_content) {
        Ok(()) => r::success(&format!("Installed: {}", desktop_path.display())),
        Err(e) => r::error(&format!("Failed to write {}: {e}", desktop_path.display())),
    }

    // Write auto-start symlink (copy)
    match std::fs::write(&autostart_path, &desktop_content) {
        Ok(()) => r::success(&format!("Auto-start: {}", autostart_path.display())),
        Err(e) => r::error(&format!("Failed to write {}: {e}", autostart_path.display())),
    }

    r::info("Pet will now appear in GNOME application menu and start on login.");
    r::dim("To remove: roco pet --uninstall");
}

fn uninstall_desktop_file() {
    let (desktop_path, autostart_path) = desktop_file_paths();

    let mut removed = false;
    if desktop_path.exists() {
        std::fs::remove_file(&desktop_path).ok();
        r::success(&format!("Removed: {}", desktop_path.display()));
        removed = true;
    }
    if autostart_path.exists() {
        std::fs::remove_file(&autostart_path).ok();
        r::success(&format!("Removed: {}", autostart_path.display()));
        removed = true;
    }
    if !removed {
        r::warning("No desktop file found. Nothing to remove.");
    }
}

// ═══════════════════════════════════════════════════════════════════════
// Stop
// ═══════════════════════════════════════════════════════════════════════

fn cmd_pet_stop() {
    let lock_path = PathBuf::from("/tmp/roco/pet.pid");

    match std::fs::read_to_string(&lock_path) {
        Ok(pid_str) => {
            if let Ok(pid) = pid_str.trim().parse::<u32>() {
                r::info(&format!("Stopping pet (PID {pid})..."));
                let status = std::process::Command::new("kill")
                    .arg("-TERM")
                    .arg(pid.to_string())
                    .status();
                match status {
                    Ok(s) if s.success() => {
                        r::success("Pet stopped.");
                        let _ = std::fs::remove_file(&lock_path);
                    }
                    _ => {
                        r::warning("Could not stop pet (already dead?). Cleaning up lock.");
                        let _ = std::fs::remove_file(&lock_path);
                    }
                }
            } else {
                r::warning("Corrupt PID file. Cleaning up.");
                let _ = std::fs::remove_file(&lock_path);
            }
        }
        Err(_) => {
            r::warning("No pet running. Nothing to stop.");
        }
    }
}

#[cfg(not(feature = "desktop"))]
fn need_desktop_feature() {
    r::warning("Pet mode requires the 'desktop' feature.");
    r::info("cargo run --bin roco --features desktop -- pet");
    std::process::exit(1);
}

#[cfg(not(feature = "desktop"))]
fn run_pet_desktop(_extra: &[&str]) {
    need_desktop_feature();
}

// ═══════════════════════════════════════════════════════════════════════
// Desktop pet (feature = "desktop")
// ═══════════════════════════════════════════════════════════════════════

#[cfg(feature = "desktop")]
fn run_pet_desktop(extra: &[&str]) {
    run_pet_inner(extra, false)
}

#[cfg(feature = "desktop")]
fn run_pet_hidden() {
    run_pet_inner(&[], true)
}

#[cfg(feature = "desktop")]
fn run_pet_inner(extra: &[&str], start_hidden: bool) {
    use std::sync::Arc;

    use eframe::egui;
    use roco_engine::ModelBackend;
    use roco_ui::{pet_native_options, DesktopPet};

    // ── PID lock ─────────────────────────────────────────────────────
    let _lock = match acquire_pid_lock() {
        Some(l) => l,
        None => return,
    };

    // ── Parse initial message ────────────────────────────────────────
    let initial_message: Option<String> = extra
        .first()
        .filter(|s| !s.starts_with('-'))
        .map(|s| s.to_string());

    // ── Connect to backend if running, don't start it ────────────────
    // The pet should never start the inference daemon (that uses 6GB RAM).
    // It only talks to an already-running gateway.
    let backend: Option<Arc<dyn ModelBackend>> = try_connect_existing_backend();

    // ── Pet app ──────────────────────────────────────────────────────
    struct PetApp {
        pet: DesktopPet,
        backend: Option<Arc<dyn ModelBackend>>,
        tray: Option<tray_icon::TrayIcon>,
        tray_event_receiver: Option<std::sync::maven::Receiver<tray_icon::TrayIconEvent>>,
        visible: bool,
    }

    impl eframe::App for PetApp {
        fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
            // Poll tray events
            self.poll_tray_events(ctx);

            if !self.visible {
                ctx.request_repaint_after(std::time::Duration::from_millis(500));
                return;
            }

            if self.pet.tick(ctx) {
                // Close button → hide to tray instead of closing
                self.hide_to_tray(ctx);
                return;
            }

            match self.pet.mood {
                roco_ui::PetMood::Sleep => {
                    ctx.request_repaint_after(std::time::Duration::from_secs(2))
                }
                _ => ctx.request_repaint_after(std::time::Duration::from_millis(100)),
            }
        }

        fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
            egui::Color32::TRANSPARENT.to_normalized_gamma_f32()
        }
    }

    impl PetApp {
        fn hide_to_tray(&mut self, ctx: &egui::Context) {
            self.visible = false;
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
        }

        fn show_from_tray(&mut self, ctx: &egui::Context) {
            self.visible = true;
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
        }

        fn poll_tray_events(&mut self, ctx: &egui::Context) {
            let Some(ref rx) = self.tray_event_receiver else { return };
            while let Ok(event) = rx.try_recv() {
                match event {
                    tray_icon::TrayIconEvent::MenuItemClick(id) => {
                        let id_str = id.id.0.as_str();
                        match id_str {
                            "show" => self.show_from_tray(ctx),
                            "hide" => self.hide_to_tray(ctx),
                            "quit" => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
                            _ => {}
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    // ── Pet setup ────────────────────────────────────────────────────
    let mut pet = DesktopPet::default();

    if let Some(msg) = initial_message {
        pet.pending_message = Some(msg);
    } else {
        pet.set_status("Click to chat!");
    }

    // Wire backend into pet (if running)
    if let Some(ref backend) = backend {
        let backend_clone = backend.clone();
        pet.on_message(move |msg, history| {
            let history_text: String = history
                .iter()
                .map(|m| format!("{}: {}", m.role, m.text))
                .collect::<Vec<_>>()
                .join("\n");

            let system = "\
                You are a cute desktop pet sitting on the user's screen.\n\
                Be warm, playful, and conversational. Keep responses short (1-3 sentences).\n\
                React to what the user says. Use emoticons. Be expressive and friendly."
                .to_string();

            let prompt = format!("Conversation:\n{history_text}\nUser: {msg}\nYou:");

            let request = roco_engine::CompletionRequest {
                system,
                prompt,
                temperature: 0.8,
                max_tokens: 256,
                prefill: Some(" ".into()),
                ..Default::default()
            };

            match futures::executor::block_on(backend_clone.complete(request)) {
                Ok(resp) => {
                    let text = resp.text.trim().to_string();
                    if text.is_empty() { None } else { Some(text) }
                }
                Err(e) => {
                    eprintln!("Pet backend error: {e}");
                    None
                }
            }
        });
        pet.set_status("Connected — click to chat!");
    } else {
        pet.set_status("No backend — echo mode");
        pet.on_message(|msg, _| {
            Some(format!("🐱 You said: {msg}\n(Start the backend with `./dev.sh` for real conversation!)"))
        });
    }

    // ── Build tray icon ──────────────────────────────────────────────
    let (tray, tray_rx) = build_tray_icon();

    // ── Window options ───────────────────────────────────────────────
    let mut options = pet_native_options(egui::Vec2::new(260.0, 400.0));

    // If starting hidden, tell the window manager not to show it yet
    if start_hidden {
        options.viewport = options.viewport.with_visible(false);
    }

    // ── Run ───────────────────────────────────────────────────────────
    let result = eframe::run_native(
        "RoCo Pet",
        options,
        Box::new(move |_cc| {
            Ok(Box::new(PetApp {
                pet,
                backend,
                tray,
                tray_event_receiver: tray_rx,
                visible: !start_hidden,
            }))
        }),
    );

    match result {
        Ok(()) => {}
        Err(e) => r::error(&format!("Pet exited: {e}")),
    }
}

// ═══════════════════════════════════════════════════════════════════════
// System tray
// ═══════════════════════════════════════════════════════════════════════

/// Build the system tray icon with a menu (Show Pet, Hide Pet, Quit).
/// Returns (TrayIcon, receiver for events).
#[cfg(feature = "desktop")]
fn build_tray_icon() -> (Option<tray_icon::TrayIcon>, Option<std::sync::mpsc::Receiver<tray_icon::TrayIconEvent>>) {
    use tray_icon::menu::{Menu, MenuItem};
    use tray_icon::TrayIconBuilder;

    let mut menu = Menu::new();
    let show_item = MenuItem::new("Show Pet", true, None);
    let hide_item = MenuItem::new("Hide Pet", true, None);
    let quit_item = MenuItem::new("Quit", true, None);

    // Set IDs for event matching
    show_item.set_id("show");
    hide_item.set_id("hide");
    quit_item.set_id("quit");

    let _ = menu.append_items(&[&show_item, &hide_item, &quit_item]);

    // Create a simple 32x32 icon (RGBA)
    let icon_data = create_pet_icon_rgba();
    let icon = tray_icon::Icon::from_rgba(icon_data, 32, 32).ok();

    let (tx, rx) = std::sync::mpsc::channel();

    let mut builder = TrayIconBuilder::new()
        .with_menu(Box::new(menu))
        .with_tooltip("RoCo Pet — Click to chat")
        .with_icon(icon.unwrap_or_else(|| {
            tray_icon::Icon::from_rgba(vec![0u8; 32 * 32 * 4], 32, 32).unwrap()
        }));

    // Set up event handler
    builder = builder.on_menu_event(move |event| {
        let _ = tx.send(event);
    });

    let tray = builder.build().ok();

    (tray, Some(rx))
}

/// Create a simple RGBA icon for the pet (32x32 smiley face).
#[cfg(feature = "desktop")]
fn create_pet_icon_rgba() -> Vec<u8> {
    let size = 32;
    let mut pixels = vec![0u8; size * size * 4];

    for y in 0..size {
        for x in 0..size {
            let i = (y * size + x) * 4;
            // Center of icon
            let cx = 16.0;
            let cy = 16.0;
            let dx = (x as f64 - cx) / 12.0;
            let dy = (y as f64 - cy) / 12.0;
            let dist = (dx * dx + dy * dy).sqrt();

            // Yellow circle
            if dist < 1.0 {
                pixels[i + 0] = 255; // R
                pixels[i + 1] = 200; // G
                pixels[i + 2] = 50;  // B
                pixels[i + 3] = 255; // A

                // Eyes
                let eye_y = cy - 4.0;
                let left_eye_x = cx - 4.0;
                let right_eye_x = cx + 4.0;
                let eye_dx_l = (x as f64 - left_eye_x).abs();
                let eye_dx_r = (x as f64 - right_eye_x).abs();
                let eye_dy = (y as f64 - eye_y).abs();

                if (eye_dx_l < 2.0 && eye_dy < 2.0) || (eye_dx_r < 2.0 && eye_dy < 2.0) {
                    pixels[i + 0] = 0;
                    pixels[i + 1] = 0;
                    pixels[i + 2] = 0;
                }

                // Mouth (smile)
                let mouth_y = cy + 3.0;
                if (y as f64 - mouth_y).abs() < 1.5 && (x as f64 - cx).abs() > 3.0 && (x as f64 - cx).abs() < 8.0 {
                    pixels[i + 0] = 0;
                    pixels[i + 1] = 0;
                    pixels[i + 2] = 0;
                }
            }
        }
    }

    pixels
}

// ═══════════════════════════════════════════════════════════════════════
// PID lock
// ═══════════════════════════════════════════════════════════════════════

#[cfg(feature = "desktop")]
fn acquire_pid_lock() -> Option<PidLock> {
    use std::fs;

    let lock_dir = PathBuf::from("/tmp/roco");
    let _ = std::fs::create_dir_all(&lock_dir);
    let lock_path = lock_dir.join("pet.pid");

    if let Ok(pid_str) = fs::read_to_string(&lock_path) {
        if let Ok(pid) = pid_str.trim().parse::<u32>() {
            #[cfg(unix)]
            {
                let alive = std::process::Command::new("kill")
                    .arg("-0")
                    .arg(pid.to_string())
                    .status()
                    .map(|s| s.success())
                    .unwrap_or(false);
                if alive {
                    r::warning(&format!(
                        "Pet is already running (PID {pid}). Use `roco pet stop` first."
                    ));
                    return None;
                }
            }
            let _ = fs::remove_file(&lock_path);
        }
    }

    let our_pid = std::process::id();
    match fs::write(&lock_path, our_pid.to_string()) {
        Ok(()) => {
            r::info(&format!("Pet lock acquired (PID {our_pid})"));
            Some(PidLock { path: lock_path })
        }
        Err(e) => {
            r::warning(&format!("Could not write pet lock: {e}"));
            None
        }
    }
}

#[cfg(feature = "desktop")]
struct PidLock {
    path: PathBuf,
}

#[cfg(feature = "desktop")]
impl Drop for PidLock {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}
