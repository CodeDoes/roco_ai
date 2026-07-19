//! RoCo AI Desktop — Full GUI application built on egui/eframe.
//!
//! Wires together PacingWidget, ChatWidget, and MarkdownEditor into a
//! complete desktop experience with model loading, session management,
//! and the full human-paced interaction flow.

use crate::{
    ChatAction, ChatMessage, ChatWidget, ChatWidgetState, MarkdownEditor, MarkdownEditorState,
    MessageRole, PacingAction, PacingMode, PacingWidget, PacingWidgetState,
};
use eframe::egui;
use egui::{CentralPanel, Context, Layout, RichText, SidePanel, TopBottomPanel};
use roco_engine::{CompletionRequest, ModelBackend};
use std::sync::Arc;

/// The main desktop application
pub struct RocoDesktopApp {
    // Core state
    backend: Option<Arc<dyn ModelBackend>>,
    model_loaded: bool,
    model_error: Option<String>,

    // Widget states
    pacing_state: PacingWidgetState,
    chat_state: ChatWidgetState,
    editor_state: MarkdownEditorState,
    editor_visible: bool,

    // Session
    session_dir: std::path::PathBuf,
    session_path: Option<std::path::PathBuf>,

    // Layout
    left_panel_open: bool,
    right_panel_open: bool,
    chat_focused: bool,

    // Status
    status_message: String,
}

impl RocoDesktopApp {
    pub fn new(backend: Option<Arc<dyn ModelBackend>>) -> Self {
        let session_dir = std::path::PathBuf::from(".roco/sessions");
        std::fs::create_dir_all(&session_dir).ok();

        Self {
            model_loaded: backend.is_some(),
            model_error: if backend.is_none() {
                Some("No model loaded — use File → Load Model or restart with RWKV_MODEL".into())
            } else {
                None
            },
            backend,
            pacing_state: PacingWidgetState::new(PacingMode::Careful, 0),
            chat_state: ChatWidgetState::new().with_greeting(
                "Welcome to RoCo AI! Start by typing a message or loading a session.",
            ),
            editor_state: MarkdownEditorState::default(),
            editor_visible: false,
            session_dir,
            session_path: None,
            left_panel_open: true,
            right_panel_open: false,
            chat_focused: true,
            status_message: String::new(),
        }
    }

    /// Handle a chat action by sending to the model
    fn handle_chat_action(&mut self, action: ChatAction, ctx: &Context) {
        match action {
            ChatAction::SendMessage(text) => {
                if let Some(ref backend) = self.backend {
                    self.status_message = "Generating...".to_string();
                    let request = CompletionRequest {
                        system: "You are a creative writing assistant. Respond with vivid, engaging prose.".into(),
                        prompt: text,
                        temperature: 0.8,
                        max_tokens: 1024,
                        ..Default::default()
                    };
                    let result = futures::executor::block_on(backend.complete(request));
                    match result {
                        Ok(response) => {
                            let text = response.text.trim().to_string();
                            self.chat_state.add_message(ChatMessage::assistant(text));
                            self.status_message = "Ready".to_string();
                        }
                        Err(e) => {
                            self.status_message = format!("Error: {e}");
                            self.chat_state
                                .add_message(ChatMessage::assistant(format!("[Error: {e}]")));
                        }
                    }
                    self.auto_save();
                } else {
                    self.chat_state.add_message(ChatMessage::system(
                        "Model not loaded. Use File → Load Model or set RWKV_MODEL.".to_string(),
                    ));
                }
            }
            ChatAction::Accept => {
                self.status_message = "Accepted. Continuing...".to_string();
                self.auto_save();
            }
            ChatAction::Skip => {
                self.status_message = "Skipped.".to_string();
                // Remove last assistant message
                if let Some(pos) = self
                    .chat_state
                    .messages
                    .iter()
                    .rposition(|m| m.role == MessageRole::Assistant)
                {
                    self.chat_state.messages.remove(pos);
                }
            }
            ChatAction::Stop => {
                self.status_message = "Stopped.".to_string();
                self.chat_state.agent_generating = false;
            }
            ChatAction::Clear => {
                self.chat_state.clear();
                self.status_message = "Conversation cleared.".to_string();
            }
            ChatAction::Undo => {
                if self.chat_state.messages.len() >= 2 {
                    self.chat_state.messages.pop();
                    self.chat_state.messages.pop();
                    self.status_message = "Undone.".to_string();
                }
                self.auto_save();
            }
            ChatAction::Retry => {
                let last_user_content = self
                    .chat_state
                    .messages
                    .iter()
                    .rev()
                    .find(|m| m.role == MessageRole::User)
                    .map(|m| m.content.clone());
                if let Some(content) = last_user_content {
                    // Remove last user+assistant exchange, re-send
                    if self.chat_state.messages.len() >= 2 {
                        self.chat_state.messages.pop();
                        self.chat_state.messages.pop();
                    }
                    self.handle_chat_action(ChatAction::SendMessage(content), ctx);
                }
            }
            ChatAction::CopyMessage(content) => {
                // Copy to clipboard using egui's internal mechanism
                ctx.output_mut(|o| o.copied_text = content);
                self.status_message = "Copied to clipboard.".to_string();
            }
            _ => {}
        }
    }

    fn auto_save(&self) {
        if let Some(ref path) = self.session_path {
            let state = ConversationState {
                id: self.chat_state.messages.len().to_string(),
                messages: self
                    .chat_state
                    .messages
                    .iter()
                    .map(|m| ConversationMessage {
                        role: m.role.label().to_lowercase(),
                        content: m.content.clone(),
                        timestamp: m.timestamp.to_rfc3339(),
                    })
                    .collect(),
                pacing: self.pacing_state.mode.label().to_string(),
                created_at: String::new(),
                updated_at: chrono::Utc::now().to_rfc3339(),
            };
            if let Ok(json) = serde_json::to_string_pretty(&state) {
                std::fs::write(path, &json).ok();
            }
        }
    }

    fn new_session(&mut self) {
        let session_id = format!("gui_{}", chrono::Utc::now().format("%Y%m%d_%H%M%S"));
        let path = self.session_dir.join(format!("{}.json", session_id));
        self.session_path = Some(path);
        self.chat_state.clear();
        self.chat_state
            .add_message(ChatMessage::system("New session started.".to_string()));
        self.status_message = format!("Session {session_id}");
        self.auto_save();
    }

    fn save_session(&mut self) {
        self.auto_save();
        if let Some(ref path) = self.session_path {
            self.status_message = format!("Saved: {}", path.display());
        }
    }

    fn load_session(&mut self, path: &std::path::Path) {
        if let Ok(json) = std::fs::read_to_string(path) {
            if let Ok(state) = serde_json::from_str::<ConversationState>(&json) {
                self.chat_state.clear();
                for msg in &state.messages {
                    let role = match msg.role.as_str() {
                        "system" => MessageRole::System,
                        "user" => MessageRole::User,
                        "assistant" | "ai" => MessageRole::Assistant,
                        _ => MessageRole::Event,
                    };
                    let mut chat_msg = ChatMessage::new(role, msg.content.clone());
                    chat_msg.timestamp = chrono::DateTime::parse_from_rfc3339(&msg.timestamp)
                        .map(|dt| dt.with_timezone(&chrono::Utc))
                        .unwrap_or_else(|_| chrono::Utc::now());
                    self.chat_state.add_message(chat_msg);
                }
                self.session_path = Some(path.to_path_buf());
                self.status_message = format!(
                    "Loaded: {} ({} messages)",
                    path.file_stem().unwrap().to_string_lossy(),
                    state.messages.len()
                );
            }
        }
    }
}

/// Conversation state for serialization
#[derive(serde::Serialize, serde::Deserialize)]
struct ConversationState {
    id: String,
    messages: Vec<ConversationMessage>,
    pacing: String,
    created_at: String,
    updated_at: String,
}

#[derive(serde::Serialize, serde::Deserialize)]
struct ConversationMessage {
    role: String,
    content: String,
    timestamp: String,
}

impl eframe::App for RocoDesktopApp {
    fn update(&mut self, ctx: &Context, _frame: &mut eframe::Frame) {
        // ── Menu bar ────────────────────────────────────────────────────
        TopBottomPanel::top("menu_bar").show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                ui.menu_button("File", |ui| {
                    if ui.button("New Session").clicked() {
                        self.new_session();
                        ui.close_menu();
                    }
                    if ui.button("Save Session").clicked() {
                        self.save_session();
                        ui.close_menu();
                    }
                    if ui.button("Open Session…").clicked() {
                        // List sessions from the session directory
                        self.status_message = "List sessions via View → Sessions".into();
                        ui.close_menu();
                    }
                    ui.separator();
                    if ui.button("Quit").clicked() {
                        self.auto_save();
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                });
                ui.menu_button("View", |ui| {
                    if ui
                        .selectable_label(self.left_panel_open, "Show Side Panel")
                        .clicked()
                    {
                        self.left_panel_open = !self.left_panel_open;
                        ui.close_menu();
                    }
                    if ui
                        .selectable_label(self.editor_visible, "Show Editor")
                        .clicked()
                    {
                        self.editor_visible = !self.editor_visible;
                        ui.close_menu();
                    }
                });
                ui.menu_button("Help", |ui| {
                    if ui.button("About").clicked() {
                        self.status_message =
                            "RoCo AI Desktop — Collaborative Story Writing".into();
                        ui.close_menu();
                    }
                });

                // Status on the right
                ui.with_layout(Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(RichText::new(&self.status_message).size(11.0).color(
                        if self.status_message.starts_with("Error") {
                            egui::Color32::RED
                        } else {
                            ui.visuals().weak_text_color()
                        },
                    ));
                    if !self.model_loaded {
                        ui.label(
                            RichText::new("⚠ No Model")
                                .size(11.0)
                                .color(egui::Color32::YELLOW),
                        );
                    }
                });
            });
        });

        // ── Left panel: pacing + session info ──────────────────────────
        if self.left_panel_open {
            SidePanel::left("left_panel")
                .resizable(true)
                .default_width(220.0)
                .show(ctx, |ui| {
                    ui.vertical(|ui| {
                        // Pacing widget
                        ui.label(RichText::new("Pacing").strong().size(14.0));
                        if let Some(action) = PacingWidget::show(ui, &mut self.pacing_state) {
                            self.status_message = match action {
                                PacingAction::Accept => "Accepted.".into(),
                                PacingAction::Skip => "Skipped.".into(),
                                PacingAction::Stop => "Stopped.".into(),
                                PacingAction::Undo => {
                                    self.handle_chat_action(ChatAction::Undo, ctx);
                                    "Undone.".into()
                                }
                                PacingAction::GoHam => {
                                    self.pacing_state.mode = PacingMode::AutoAccept;
                                    "Pacing: Auto-Accept".into()
                                }
                                _ => format!("{:?}", action),
                            };
                        }

                        ui.add_space(12.0);
                        ui.separator();
                        ui.add_space(8.0);

                        // Session info
                        ui.label(RichText::new("Session").strong().size(14.0));
                        if let Some(ref path) = self.session_path {
                            if let Some(name) = path.file_stem() {
                                ui.label(
                                    RichText::new(name.to_string_lossy())
                                        .size(11.0)
                                        .color(ui.visuals().weak_text_color()),
                                );
                            }
                        }
                        ui.label(format!("{} messages", self.chat_state.messages.len()));
                        let word_count: usize = self
                            .chat_state
                            .messages
                            .iter()
                            .map(|m| m.content.split_whitespace().count())
                            .sum();
                        ui.label(format!("{word_count} words total"));

                        ui.add_space(8.0);
                        if ui.button("📁 New Session").clicked() {
                            self.new_session();
                        }
                        if ui.button("💾 Save").clicked() {
                            self.save_session();
                        }
                    });
                });
        }

        // ── Right panel: markdown editor ────────────────────────────────
        if self.editor_visible {
            SidePanel::right("editor_panel")
                .resizable(true)
                .default_width(400.0)
                .show(ctx, |ui| {
                    ui.label(RichText::new("Editor").strong().size(14.0));
                    ui.separator();
                    // Show the most recent assistant message in the editor
                    if let Some(last) = self.chat_state.last_assistant_message() {
                        if self.editor_state.document.text != last.content {
                            self.editor_state.document.text = last.content.clone();
                        }
                    }
                    let mut editor = MarkdownEditor::new();
                    editor.show(ui, &mut self.editor_state, ctx);
                });
        }

        // ── Central panel: chat ─────────────────────────────────────────
        CentralPanel::default().show(ctx, |ui| {
            // Model not loaded warning
            if !self.model_loaded {
                ui.label(RichText::new(
                    "⚠ Model not loaded. The model loads automatically when RWKV_MODEL is set.\n\
                     You can still browse past sessions or configure via File → Load Model."
                ).size(14.0).color(egui::Color32::YELLOW));
                ui.add_space(8.0);
            }

            // Chat widget (main area)
            if let Some(action) = ChatWidget::show(ui, &mut self.chat_state) {
                self.handle_chat_action(action, ctx);
            }
        });
    }
}
