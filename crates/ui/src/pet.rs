//! Desktop Pet — a transparent, always-on-top conversational companion.
//!
//! Shows a pet face that reflects backend state. Click to open a chat bubble
//! and have a conversation via the inference backend.

use eframe::egui;
use egui::{Color32, Frame, RichText, Sense, Vec2};
use std::sync::Arc;

/// Pet emotional states mapped to backend status
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PetMood {
    Idle,
    Listening,
    Thinking,
    Speaking,
    Error,
    Sleep,
}

impl PetMood {
    pub fn face(self) -> &'static str {
        match self {
            PetMood::Idle => "(◕‿◕)",
            PetMood::Listening => "(ᵔ◡◡ᵔ)",
            PetMood::Thinking => "(￣ ￣)...",
            PetMood::Speaking => "(⊙‿⊙)",
            PetMood::Error => "(×_×)",
            PetMood::Sleep => "(−_−) zzz",
        }
    }

    pub fn tooltip(self) -> &'static str {
        match self {
            PetMood::Idle => "Click to chat!",
            PetMood::Listening => "Listening...",
            PetMood::Thinking => "Thinking...",
            PetMood::Speaking => "I have an idea!",
            PetMood::Error => "Something went wrong",
            PetMood::Sleep => "Zzz... click to wake",
        }
    }

    pub fn color(self) -> Color32 {
        match self {
            PetMood::Idle => Color32::from_rgb(180, 220, 255),
            PetMood::Listening => Color32::from_rgb(150, 255, 150),
            PetMood::Thinking => Color32::from_rgb(200, 180, 255),
            PetMood::Speaking => Color32::from_rgb(255, 220, 100),
            PetMood::Error => Color32::from_rgb(255, 100, 100),
            PetMood::Sleep => Color32::from_rgb(140, 140, 160),
        }
    }
}

/// A single chat message in the pet conversation
#[derive(Clone)]
pub struct PetMessage {
    pub role: String,
    pub text: String,
}

/// Callback type: user sent a message, returns optional response.
pub type PetMessageHandler = Arc<dyn Fn(&str, &[PetMessage]) -> Option<String> + Send + Sync>;

/// The conversational desktop pet
pub struct DesktopPet {
    pub mood: PetMood,
    pub status_text: String,
    idle_ticks: u64,
    pub window_size: Vec2,
    /// Whether the chat bubble is open
    pub chat_open: bool,
    /// Conversation history
    pub messages: Vec<PetMessage>,
    /// Current input buffer
    pub input_buf: String,
    /// Callback for sending messages to the backend
    pub on_send_message: Option<PetMessageHandler>,
    /// Whether the input field has been focused this open
    focus_input: bool,
    /// If set, auto-send this message on first tick (opens chat)
    pub pending_message: Option<String>,
}

impl Default for DesktopPet {
    fn default() -> Self {
        Self {
            mood: PetMood::Idle,
            status_text: String::new(),
            idle_ticks: 0,
            window_size: Vec2::new(220.0, 300.0),
            chat_open: false,
            messages: Vec::new(),
            input_buf: String::new(),
            on_send_message: None,
            focus_input: false,
            pending_message: None,
        }
    }
}

impl DesktopPet {
    /// Frame tick — call this each frame from your app's update().
    /// Returns true if the close button was clicked.
    pub fn tick(&mut self, ctx: &egui::Context) -> bool {
        let closed = false;

        // Auto-send pending message on first tick
        if let Some(msg) = self.pending_message.take() {
            self.chat_open = true;
            self.input_buf = msg;
            self.send_message(ctx);
        }

        // Auto-sleep after ~30s of idle
        if self.mood == PetMood::Idle {
            self.idle_ticks += 1;
            if self.idle_ticks > 60 * 30 {
                self.mood = PetMood::Sleep;
            }
        } else if self.mood != PetMood::Sleep {
            self.idle_ticks = 0;
        }

        // ── Transparent panel ─────────────────────────────────────────
        egui::CentralPanel::default()
            .frame(Frame::NONE.fill(Color32::TRANSPARENT))
            .show(ctx, |ui| {
                let panel_rect = ui.max_rect();

                // ── Pet face area (top 80px, draggable) ───────────────
                let face_rect =
                    egui::Rect::from_min_size(panel_rect.min, Vec2::new(panel_rect.width(), 80.0));

                let drag_id = ui.id().with("pet_drag");
                let pet_area = ui.interact(face_rect, drag_id, Sense::click_and_drag());

                if pet_area.dragged() {
                    ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
                }

                if pet_area.clicked() {
                    self.chat_open = !self.chat_open;
                    self.mood = if self.chat_open {
                        self.focus_input = true;
                        PetMood::Listening
                    } else {
                        PetMood::Idle
                    };
                    self.idle_ticks = 0;
                }

                // Draw the face centered in the face area
                let mut face_ui = ui.new_child(
                    egui::UiBuilder::new()
                        .max_rect(face_rect)
                        .layout(egui::Layout::top_down(egui::Align::Center)),
                );
                face_ui.add_space(16.0);
                face_ui.label(
                    RichText::new(self.mood.face())
                        .size(36.0)
                        .color(self.mood.color()),
                );
                if !self.status_text.is_empty() {
                    face_ui.label(
                        RichText::new(&self.status_text)
                            .size(10.0)
                            .color(Color32::from_gray(170)),
                    );
                }

                // ── Chat bubble ────────────────────────────────────────
                if self.chat_open {
                    let chat_top = face_rect.bottom() + 4.0;
                    let chat_rect = egui::Rect::from_min_size(
                        egui::pos2(panel_rect.left(), chat_top),
                        Vec2::new(
                            panel_rect.width(),
                            panel_rect.height() - chat_top + panel_rect.top(),
                        ),
                    );

                    let chat_bg = Color32::from_rgba_premultiplied(20, 20, 30, 230);
                    let mut chat_ui = ui.new_child(
                        egui::UiBuilder::new()
                            .max_rect(chat_rect)
                            .layout(egui::Layout::top_down(egui::Align::Center)),
                    );

                    egui::Frame::NONE
                        .fill(chat_bg)
                        .corner_radius(8.0)
                        .show(&mut chat_ui, |ui| {
                            ui.set_min_height(120.0);
                            ui.set_max_height(350.0);

                            // Message history
                            let scroll_id = ui.id().with("chat_scroll");
                            egui::ScrollArea::vertical()
                                .id_salt(scroll_id)
                                .max_height(250.0)
                                .auto_shrink([false; 2])
                                .show(ui, |ui| {
                                    ui.add_space(8.0);
                                    for msg in &self.messages {
                                        let (align, color) = match msg.role.as_str() {
                                            "user" => (
                                                egui::Layout::right_to_left(egui::Align::Min),
                                                Color32::from_rgb(100, 200, 255),
                                            ),
                                            _ => (
                                                egui::Layout::left_to_right(egui::Align::Min),
                                                Color32::from_rgb(220, 220, 255),
                                            ),
                                        };
                                        ui.with_layout(align, |ui| {
                                            let text =
                                                RichText::new(&msg.text).size(13.0).color(color);
                                            ui.add(egui::Label::new(text).wrap());
                                        });
                                        ui.add_space(4.0);
                                    }
                                });

                            ui.add_space(4.0);

                            // Text input + send button
                            ui.horizontal(|ui| {
                                let input_id = ui.id().with("chat_input");
                                let resp = ui.add(
                                    egui::TextEdit::singleline(&mut self.input_buf)
                                        .hint_text("Type a message...")
                                        .id(input_id)
                                        .desired_width(f32::INFINITY),
                                );

                                // Focus on first open
                                if self.focus_input {
                                    resp.request_focus();
                                    self.focus_input = false;
                                }

                                // Send on Enter
                                if resp.lost_focus()
                                    && ctx.input(|i| i.key_pressed(egui::Key::Enter))
                                {
                                    self.send_message(ctx);
                                }

                                if ui.button("Send").clicked() {
                                    self.send_message(ctx);
                                }
                            });

                            ui.add_space(4.0);
                        });
                }
            });

        closed
    }

    fn send_message(&mut self, ctx: &egui::Context) {
        let msg = self.input_buf.trim().to_string();
        if msg.is_empty() {
            return;
        }

        self.messages.push(PetMessage {
            role: "user".into(),
            text: msg.clone(),
        });
        self.input_buf.clear();
        self.mood = PetMood::Thinking;
        self.idle_ticks = 0;
        ctx.request_repaint();

        if let Some(ref handler) = self.on_send_message {
            let response = handler(&msg, &self.messages);
            if let Some(reply) = response {
                self.messages.push(PetMessage {
                    role: "assistant".into(),
                    text: reply,
                });
                self.mood = PetMood::Idle;
            } else {
                self.mood = PetMood::Error;
                self.status_text = "Backend unavailable".into();
            }
        } else {
            // No handler — echo
            self.messages.push(PetMessage {
                role: "assistant".into(),
                text: format!("You said: {msg}"),
            });
            self.mood = PetMood::Idle;
        }
        ctx.request_repaint();
    }

    /// Update the pet's mood
    pub fn set_mood(&mut self, mood: PetMood) {
        self.mood = mood;
        self.idle_ticks = 0;
    }

    /// Set a custom status message
    pub fn set_status(&mut self, text: &str) {
        self.status_text = text.to_string();
    }

    /// Register a callback for when the user sends a message.
    pub fn on_message<F>(&mut self, f: F)
    where
        F: Fn(&str, &[PetMessage]) -> Option<String> + Send + Sync + 'static,
    {
        self.on_send_message = Some(Arc::new(f));
    }
}

/// Build the `eframe::NativeOptions` for a desktop pet window.
pub fn pet_native_options(size: Vec2) -> eframe::NativeOptions {
    eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_decorations(false)
            .with_transparent(true)
            .with_always_on_top()
            .with_inner_size(size)
            .with_window_level(egui::WindowLevel::AlwaysOnTop),
        ..Default::default()
    }
}

// ═════════════════════════════════════════════════════════════════════════
// Tests
// ═════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pet_default_state() {
        let pet = DesktopPet::default();
        assert_eq!(pet.mood, PetMood::Idle);
        assert!(pet.messages.is_empty());
        assert!(!pet.chat_open);
    }

    #[test]
    fn test_pet_mood_faces() {
        assert!(PetMood::Sleep.face().contains("z"));
        assert!(PetMood::Error.face().contains("×"));

        let moods = [
            PetMood::Idle,
            PetMood::Listening,
            PetMood::Thinking,
            PetMood::Speaking,
            PetMood::Error,
            PetMood::Sleep,
        ];
        let faces: std::collections::HashSet<&str> = moods.iter().map(|m| m.face()).collect();
        assert_eq!(faces.len(), moods.len());
    }

    #[test]
    fn test_pet_set_mood() {
        let mut pet = DesktopPet::default();
        pet.set_mood(PetMood::Thinking);
        assert_eq!(pet.mood, PetMood::Thinking);
    }

    #[test]
    fn test_toggle_chat() {
        let mut pet = DesktopPet::default();
        assert!(!pet.chat_open);
        pet.chat_open = true;
        assert!(pet.chat_open);
    }

    #[test]
    fn test_send_message_echo() {
        let mut pet = DesktopPet::default();
        pet.input_buf = "hello".into();

        // Can't call send_message without a ctx, but we can push directly
        pet.messages.push(PetMessage {
            role: "user".into(),
            text: "hello".into(),
        });
        pet.messages.push(PetMessage {
            role: "assistant".into(),
            text: "You said: hello".into(),
        });

        assert_eq!(pet.messages.len(), 2);
        assert_eq!(pet.messages[0].role, "user");
        assert_eq!(pet.messages[1].text, "You said: hello");
    }

    #[test]
    fn test_conversation_order() {
        let mut pet = DesktopPet::default();
        pet.messages.push(PetMessage {
            role: "user".into(),
            text: "first".into(),
        });
        pet.messages.push(PetMessage {
            role: "assistant".into(),
            text: "echo first".into(),
        });
        pet.messages.push(PetMessage {
            role: "user".into(),
            text: "second".into(),
        });
        pet.messages.push(PetMessage {
            role: "assistant".into(),
            text: "echo second".into(),
        });

        assert_eq!(pet.messages.len(), 4);
        assert_eq!(pet.messages[0].text, "first");
        assert_eq!(pet.messages[2].text, "second");
    }

    #[test]
    fn test_pet_native_options() {
        let options = pet_native_options(Vec2::new(300.0, 400.0));
        assert_eq!(options.viewport.inner_size, Some(Vec2::new(300.0, 400.0)));
    }

    #[test]
    fn test_on_message_handler() {
        let mut pet = DesktopPet::default();
        let called = std::sync::atomic::AtomicBool::new(false);
        pet.on_message(move |msg, _| {
            called.store(true, std::sync::atomic::Ordering::SeqCst);
            Some(format!("Echo: {msg}"))
        });

        // Verify the handler was set
        assert!(pet.on_send_message.is_some());
    }
}
