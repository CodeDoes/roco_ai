# Desktop Pet with Rust & eframe/egui

> Reference guide for building a transparent, always-on-top desktop companion
> using egui's native window system via eframe.

## Core Native Features Required

1. **Window Transparency** — OS compositor alpha blending
2. **Always-on-Top** — Window lives above other applications
3. **No Decorations** — Borderless, no titlebar
4. **Custom Dragging** — Drag the window by clicking on the pet surface
5. **System Tray** — Live in notification area (GNOME top bar / Windows taskbar)

---

## 1. Minimal Transparent Pet Window

### Cargo.toml

```toml
[package]
name = "desktop-pet"
version = "0.1.0"
edition = "2021"

[dependencies]
eframe = "0.27"
```

### Pet App

```rust
use eframe::egui;

pub struct DesktopPet {
    is_happy: bool,
}

impl Default for DesktopPet {
    fn default() -> Self {
        Self { is_happy: true }
    }
}

impl eframe::App for DesktopPet {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Transparent main panel
        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(egui::Color32::TRANSPARENT))
            .show(ctx, |ui| {
                // Click-and-drag sense on entire area → native window drag
                let pet_area = ui.interact(
                    ui.max_rect(),
                    ui.id().with("pet_drag"),
                    egui::Sense::click_and_drag(),
                );

                if pet_area.dragged() {
                    ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
                }

                if pet_area.clicked() {
                    self.is_happy = !self.is_happy;
                }

                ui.vertical_centered(|ui| {
                    let face = if self.is_happy { "(◕‿◕)" } else { "(>_<)" };
                    ui.heading(
                        egui::RichText::new(face)
                            .size(40.0)
                            .color(egui::Color32::WHITE),
                    );
                    if ui.button("Close").clicked() {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                });
            });
    }
}

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_decorations(false)         // No titlebar / borders
            .with_transparent(true)          // Alpha channel on window
            .with_always_on_top()            // Float above other windows
            .with_inner_size([180.0, 180.0])
            .with_window_level(egui::WindowLevel::AlwaysOnTop),
        clear_color: egui::Color32::TRANSPARENT.into(),  // ★ Critical: transparent clear
        ..Default::default()
    };

    eframe::run_native(
        "Desktop Pet",
        options,
        Box::new(|_cc| Box::<DesktopPet>::default()),
    )
}
```

### Critical Details

| Setting | Why |
|---------|-----|
| `clear_color: TRANSPARENT` | Without this the OS compositor renders solid black/grey regardless of `Frame::none()` |
| `ViewportCommand::StartDrag` | Passes native window drag events to the OS window manager |
| `Frame::none().fill(TRANSPARENT)` | Makes the egui panel itself transparent so only your content shows |

### Platform Quirks

- **Windows 10/11**: Transparency works OOTB with GPU compositor
- **Linux (X11/Wayland)**: Requires an active compositor (Picom, GNOME/Wayland native)

---

## 2. System Tray / Notification Area

egui/eframe manages standard viewports — it has no tray API. Use `tray-icon`
(by the Tauri team) for system tray integration.

### Cargo.toml

```toml
[dependencies]
eframe = "0.27"
tray-icon = "0.14"
image = "0.24"
```

### Tray-Enabled Pet

```rust
use eframe::egui;
use tray_icon::{
    menu::{Menu, MenuEvent, MenuItem},
    TrayIcon, TrayIconBuilder,
};

pub struct DesktopPet {
    tray_icon: Option<TrayIcon>,
    show_menu_id: String,
    hide_menu_id: String,
    quit_menu_id: String,
    is_visible: bool,
}

impl DesktopPet {
    pub fn new() -> Self {
        let tray_menu = Menu::new();
        let show_item = MenuItem::new("Show Pet", true, None);
        let hide_item = MenuItem::new("Hide Pet", true, None);
        let quit_item = MenuItem::new("Quit", true, None);
        let _ = tray_menu.append_items(&[&show_item, &hide_item, &quit_item]);

        let show_id = show_item.id().0.clone();
        let hide_id = hide_item.id().0.clone();
        let quit_id = quit_item.id().0.clone();

        // 32x32 RGBA dummy icon — replace with real PNG
        let icon_data = vec![255u8; 32 * 32 * 4];
        let icon = tray_icon::Icon::from_rgba(icon_data, 32, 32).unwrap();

        let tray_icon = TrayIconBuilder::new()
            .with_menu(Box::new(tray_menu))
            .with_tooltip("Desktop Pet")
            .with_icon(icon)
            .build()
            .ok();

        Self {
            tray_icon,
            show_menu_id: show_id,
            hide_menu_id: hide_id,
            quit_menu_id: quit_id,
            is_visible: true,
        }
    }

    fn handle_tray_events(&mut self, ctx: &egui::Context) {
        if let Ok(event) = MenuEvent::receiver().try_recv() {
            if event.id.0 == self.show_menu_id {
                self.is_visible = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
            } else if event.id.0 == self.hide_menu_id {
                self.is_visible = false;
                ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
            } else if event.id.0 == self.quit_menu_id {
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }
    }
}

impl eframe::App for DesktopPet {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.handle_tray_events(ctx);

        if !self.is_visible {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
            return;
        }

        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(egui::Color32::TRANSPARENT))
            .show(ctx, |ui| {
                let pet_area = ui.interact(
                    ui.max_rect(),
                    ui.id().with("pet_drag"),
                    egui::Sense::click_and_drag(),
                );
                if pet_area.dragged() {
                    ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
                }
                ui.vertical_centered(|ui| {
                    ui.heading(
                        egui::RichText::new("(◕‿◕)")
                            .size(36.0)
                            .color(egui::Color32::WHITE),
                    );
                    if ui.button("Hide to Tray").clicked() {
                        self.is_visible = false;
                        ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
                    }
                });
            });
    }
}

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_decorations(false)
            .with_transparent(true)
            .with_always_on_top()
            .with_inner_size([160.0, 160.0]),
        clear_color: egui::Color32::TRANSPARENT.into(),
        ..Default::default()
    };

    eframe::run_native(
        "Desktop Pet",
        options,
        Box::new(|_cc| Box::new(DesktopPet::new())),
    )
}
```

### Platform Requirements

**GNOME (Linux):**
- Requires [AppIndicator and KStatusNotifierItem Support](https://extensions.gnome.org/extension/615/appindicator-support/) GNOME extension
- System library: `libayatana-appindicator3` or `libappindicator-gtk3`
  ```bash
  # Fedora
  sudo dnf install libayatana-appindicator-gtk3
  # Ubuntu/Debian
  sudo apt install libayatana-appindicator3-1
  ```

**Windows:**
- Uses Win32 `Shell_NotifyIconW` API — works OOTB
- Icon renders in the system notification overflow (bottom-right taskbar)

---

## 3. Key Takeaways for RoCo AI

| Technique | Where it applies |
|-----------|-----------------|
| `ViewportCommand::StartDrag` | Custom chrome / draggable floating panels |
| `with_transparent(true)` + `clear_color: TRANSPARENT` | Overlay UI, desktop widgets |
| `tray-icon` crate | Background daemon with system tray presence |
| `with_always_on_top()` | Persistent floating tools (link graph, wiki) |
| `with_decorations(false)` | Custom titlebar in desktop app |
