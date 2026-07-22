//! Chat Widget — The conversation surface for human-AI collaboration.
//!
//! Per UX spec (roadmap/ux.md):
//! - Message parts: system message, user message, think part, text part,
//!   tool_call part, tool_result part, event message
//! - User input: text area, capabilities toggles, send button, attachments bar,
//!   context info, agent pacing control

use crate::markdown_editor::render_markdown_preview;
use egui::{self, Color32, FontId, Layout, RichText, ScrollArea, Ui};
use roco_agent::interaction::InteractionMode;

/// Role of a message in the conversation
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageRole {
    System,
    User,
    Assistant,
    Think,
    ToolCall,
    ToolResult,
    Event,
}

impl MessageRole {
    pub fn label(self) -> &'static str {
        match self {
            MessageRole::System => "System",
            MessageRole::User => "You",
            MessageRole::Assistant => "AI",
            MessageRole::Think => "AI (thinking)",
            MessageRole::ToolCall => "Tool Call",
            MessageRole::ToolResult => "Tool Result",
            MessageRole::Event => "Event",
        }
    }

    pub fn color(self) -> Color32 {
        match self {
            MessageRole::System => Color32::from_rgb(100, 100, 100),
            MessageRole::User => Color32::from_rgb(50, 150, 255),
            MessageRole::Assistant => Color32::from_rgb(50, 200, 100),
            MessageRole::Think => Color32::from_rgb(200, 180, 50),
            MessageRole::ToolCall => Color32::from_rgb(150, 100, 200),
            MessageRole::ToolResult => Color32::from_rgb(100, 150, 200),
            MessageRole::Event => Color32::from_rgb(150, 150, 150),
        }
    }

    pub fn bg_color(self) -> Color32 {
        match self {
            MessageRole::System => Color32::from_rgba_premultiplied(100, 100, 100, 30),
            MessageRole::User => Color32::from_rgba_premultiplied(50, 150, 255, 40),
            MessageRole::Assistant => Color32::from_rgba_premultiplied(50, 200, 100, 40),
            MessageRole::Think => Color32::from_rgba_premultiplied(200, 180, 50, 40),
            MessageRole::ToolCall => Color32::from_rgba_premultiplied(150, 100, 200, 40),
            MessageRole::ToolResult => Color32::from_rgba_premultiplied(100, 150, 200, 40),
            MessageRole::Event => Color32::from_rgba_premultiplied(150, 150, 150, 20),
        }
    }
}

/// A single message in the conversation
#[derive(Debug, Clone)]
pub struct ChatMessage {
    pub role: MessageRole,
    pub content: String,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub id: String,
    pub tool_name: Option<String>,
    pub is_error: bool,
    pub streaming: bool,
    pub accepted: bool,
}

impl ChatMessage {
    pub fn new(role: MessageRole, content: String) -> Self {
        Self {
            role,
            content,
            timestamp: chrono::Utc::now(),
            id: uuid::Uuid::new_v4().to_string(),
            tool_name: None,
            is_error: false,
            streaming: false,
            accepted: true,
        }
    }

    pub fn system(content: String) -> Self {
        Self::new(MessageRole::System, content)
    }
    pub fn user(content: String) -> Self {
        Self::new(MessageRole::User, content)
    }
    pub fn assistant(content: String) -> Self {
        Self::new(MessageRole::Assistant, content)
    }
    pub fn think(content: String) -> Self {
        Self::new(MessageRole::Think, content)
    }

    pub fn tool_call(name: &str, args: String) -> Self {
        Self {
            role: MessageRole::ToolCall,
            content: args,
            tool_name: Some(name.to_string()),
            ..Self::new(MessageRole::ToolCall, String::new())
        }
    }

    pub fn tool_result(content: String, is_error: bool) -> Self {
        Self {
            is_error,
            ..Self::new(MessageRole::ToolResult, content)
        }
    }

    pub fn event(content: String) -> Self {
        Self::new(MessageRole::Event, content)
    }
}

/// Capability toggle for the input area
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Capability {
    Generate,
    Research,
    Edit,
    Critique,
    Outline,
    Brainstorm,
}

impl Capability {
    pub const ALL: [Capability; 6] = [
        Capability::Generate,
        Capability::Research,
        Capability::Edit,
        Capability::Critique,
        Capability::Outline,
        Capability::Brainstorm,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Capability::Generate => "Generate",
            Capability::Research => "Research",
            Capability::Edit => "Edit",
            Capability::Critique => "Critique",
            Capability::Outline => "Outline",
            Capability::Brainstorm => "Brainstorm",
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            Capability::Generate => "Generate new content",
            Capability::Research => "Search for information",
            Capability::Edit => "Edit existing content",
            Capability::Critique => "Get feedback on writing",
            Capability::Outline => "Plan story structure",
            Capability::Brainstorm => "Explore creative ideas",
        }
    }
}

#[derive(Debug, Clone)]
pub struct Attachment {
    pub name: String,
    pub kind: AttachmentKind,
    pub content: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttachmentKind {
    File,
    Image,
    Snippet,
    Reference,
}

#[derive(Debug, Clone, Default)]
pub struct ContextInfo {
    pub document: Option<String>,
    pub section: Option<String>,
    pub selection_summary: Option<String>,
    pub estimated_tokens: usize,
}

#[derive(Debug, Clone)]
pub enum ChatAction {
    SendMessage(String),
    Accept,
    AcceptAll,
    Revise(String),
    Skip,
    Stop,
    ToggleCapability(Capability),
    AddAttachment(Attachment),
    RemoveAttachment(usize),
    SetPacingMode(InteractionMode),
    Undo,
    Redo,
    CopyMessage(String),
    Retry,
    Clear,
}

#[derive(Debug, Clone)]
pub struct ChatWidgetState {
    pub messages: Vec<ChatMessage>,
    pub input_text: String,
    pub active_capabilities: Vec<Capability>,
    pub attachments: Vec<Attachment>,
    pub context: ContextInfo,
    pub agent_generating: bool,
    pub waiting_for_human: bool,
    pub show_capabilities: bool,
    pub show_context: bool,
    pub show_attachments: bool,
    pub streaming_content: Option<String>,
    pub auto_scroll: bool,
    pub max_visible_messages: usize,
}

impl Default for ChatWidgetState {
    fn default() -> Self {
        Self {
            messages: Vec::new(),
            input_text: String::new(),
            active_capabilities: Vec::new(),
            attachments: Vec::new(),
            context: ContextInfo::default(),
            agent_generating: false,
            waiting_for_human: false,
            show_capabilities: false,
            show_context: false,
            show_attachments: false,
            streaming_content: None,
            auto_scroll: true,
            max_visible_messages: 200,
        }
    }
}

impl ChatWidgetState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_greeting(mut self, greeting: &str) -> Self {
        self.messages
            .push(ChatMessage::system(greeting.to_string()));
        self
    }

    pub fn add_message(&mut self, msg: ChatMessage) -> String {
        let id = msg.id.clone();
        self.messages.push(msg);
        id
    }

    pub fn clear(&mut self) {
        self.messages.clear();
        self.input_text.clear();
        self.attachments.clear();
        self.streaming_content = None;
    }

    pub fn last_assistant_message(&self) -> Option<&ChatMessage> {
        self.messages
            .iter()
            .rev()
            .find(|m| m.role == MessageRole::Assistant)
    }

    /// Add a model response, splitting `<think>...</think>` blocks
    /// into their own collapsible `MessageRole::Think` entries.
    /// See `split_response_with_thinking` for the behaviour.
    pub fn add_assistant_response(&mut self, raw_text: &str) {
        for chunk in split_response_with_thinking(raw_text) {
            self.messages.push(chunk);
        }
    }
}

pub fn split_response_with_thinking(raw_text: &str) -> Vec<ChatMessage> {
    let trimmed = raw_text.trim();
    if trimmed.is_empty() {
        return Vec::new();
    }
    let open_tag = "<think>";
    let close_tag = "</think>";
    // We walk the input linearly and append to a sequence of
    // chunks. Each `<think>` block pushes via `push_or_coalesce_think`
    // so adjacent think blocks (or one with intervening assistant
    // content that we later re-emit into the same chunk) appear as
    // a single `Think` entry in the chat list. The chat UI's
    // `collapsing("Thinking trace", ...)` panel reads from a single
    // `Think` message so any number of reasoning steps still show
    // collapsed under one heading.
    let mut chunks: Vec<ChatMessage> = Vec::new();
    let mut pending_assistant: Option<String> = None;
    let mut cursor: usize = 0;

    // pending_assistant buffers prose we want to emit ahead of the
    // next think (so a leading assistant gets pushed first).
    macro_rules! flush_assistant {
        () => {{
            if let Some(t) = pending_assistant.take() {
                {
                    chunks.push(ChatMessage::assistant(t));
                }
            }
        }};
    }

    let push_think = |chunks: &mut Vec<ChatMessage>, content: &str| {
        let content = content.trim();
        if content.is_empty() {
            return;
        }
        // Try to coalesce with the previous Think chunk.
        let last_think = chunks
            .iter_mut()
            .rev()
            .find(|c| c.role == MessageRole::Think);
        if let Some(last) = last_think {
            if !last.content.is_empty() {
                last.content.push_str("\n\n");
            }
            last.content.push_str(content);
        } else {
            chunks.push(ChatMessage::think(content.to_string()));
        }
    };

    while let Some(rel) = trimmed[cursor..].find(open_tag) {
        let abs_open = cursor + rel;
        // Pre-tag prose is buffered into pending_assistant so it
        // can be emitted in original order (before-or-after any think
        // that follows).
        let pre = trimmed[cursor..abs_open].trim();
        if !pre.is_empty() {
            pending_assistant = Some(pre.to_string());
        }
        let body_start = abs_open + open_tag.len();
        let Some(close_rel) = trimmed[body_start..].find(close_tag) else {
            // Unclosed: drop the partial content as a single
            // assistant bubble, flush any pending assistant, stop.
            let tail = trimmed[abs_open..].trim();
            if !tail.is_empty() {
                pending_assistant = Some(tail.to_string());
            }
            cursor = trimmed.len();
            break;
        };
        let body_end = body_start + close_rel;
        let content = trimmed[body_start..body_end].trim();
        let has_assistant = pending_assistant.is_some();
        if has_assistant {
            flush_assistant!();
        }
        push_think(&mut chunks, content);
        cursor = body_end + close_tag.len();
        // Capture any post-think assistant prose.
        let next_think_rel = trimmed[cursor..].find(open_tag);
        let between_end = match next_think_rel {
            Some(i) => cursor + i,
            None => trimmed.len(),
        };
        let between = trimmed[cursor..between_end].trim();
        if !between.is_empty() {
            // Intervening prose between two think blocks becomes a
            // standalone assistant message. It is NOT pushed into the
            // preceding think via coalescing: even if the previous chunk
            // was a Think, prose is plain assistant output.
            chunks.push(ChatMessage::assistant(between.to_string()));
        }
        cursor = between_end;
    }
    // Trailing assistant prose, if any.
    let tail = trimmed[cursor..].trim();
    if !tail.is_empty() {
        pending_assistant = Some(tail.to_string());
    }
    flush_assistant!();
    chunks
}

/// Chat rendering widget
pub struct ChatWidget;

impl ChatWidget {
    pub fn show(ui: &mut Ui, state: &mut ChatWidgetState) -> Option<ChatAction> {
        let mut action: Option<ChatAction> = None;
        let available = ui.available_height();
        let input_height = 140.0;
        let messages_height = (available - input_height).max(100.0);

        // Messages area
        let messages_rect = egui::Rect::from_min_size(
            ui.min_rect().left_top(),
            egui::vec2(ui.available_width(), messages_height),
        );
        let mut messages_ui = ui.child_ui(
            messages_rect,
            egui::Layout::top_down(egui::Align::LEFT),
            None,
        );
        messages_ui.set_min_height(messages_height);
        let act = &mut action;
        Self::show_messages(&mut messages_ui, state, act);

        // Input area
        let input_rect = egui::Rect::from_min_size(
            egui::pos2(ui.min_rect().left(), messages_ui.min_rect().bottom()),
            egui::vec2(ui.available_width(), input_height),
        );
        let mut input_ui = ui.child_ui(input_rect, egui::Layout::top_down(egui::Align::LEFT), None);
        let act = &mut action;
        Self::show_input_area(&mut input_ui, state, act);

        action
    }

    fn show_messages(ui: &mut Ui, state: &mut ChatWidgetState, action: &mut Option<ChatAction>) {
        ScrollArea::vertical()
            .auto_shrink([false, false])
            .stick_to_bottom(true)
            .show(ui, |ui| {
                ui.set_min_width(ui.available_width());
                for (i, message) in state.messages.iter().enumerate() {
                    if let Some(a) = Self::show_message(ui, message, i) {
                        *action = Some(a);
                    }
                    ui.add_space(4.0);
                }
                if let Some(ref streaming) = state.streaming_content {
                    let stream_msg = ChatMessage {
                        role: MessageRole::Assistant,
                        content: streaming.clone(),
                        streaming: true,
                        ..ChatMessage::assistant(String::new())
                    };
                    Self::show_message(ui, &stream_msg, state.messages.len());
                } else if state.agent_generating {
                    // Streaming indicator (animated dots)
                    ui.group(|ui| {
                        ui.horizontal(|ui| {
                            let role_label = RichText::new("AI")
                                .color(Color32::from_rgb(100, 180, 255))
                                .size(11.0)
                                .strong();
                            ui.label(role_label);
                            ui.label(
                                RichText::new(" \u{25cf}\u{25cf}\u{25cf}")
                                    .size(10.0)
                                    .color(Color32::GRAY),
                            );
                        });
                    });
                }
            });
    }

    fn show_message(ui: &mut Ui, message: &ChatMessage, _index: usize) -> Option<ChatAction> {
        let mut action = None;

        ui.group(|ui| {
            // Header: role badge + timestamp + actions
            ui.horizontal(|ui| {
                let role_label = RichText::new(message.role.label())
                    .color(message.role.color())
                    .size(11.0)
                    .strong();
                ui.label(role_label);

                if message.streaming {
                    ui.label(
                        RichText::new(" ● streaming")
                            .size(10.0)
                            .color(Color32::GRAY),
                    );
                }

                ui.with_layout(Layout::right_to_left(egui::Align::Center), |ui| {
                    let time_str = message.timestamp.format("%H:%M:%S").to_string();
                    ui.label(RichText::new(time_str).size(10.0).color(Color32::GRAY));

                    ui.menu_button("⋯", |ui| {
                        if ui.button("Copy").clicked() {
                            action = Some(ChatAction::CopyMessage(message.content.clone()));
                            ui.close_menu();
                        }
                        if ui.button("Retry").clicked() {
                            action = Some(ChatAction::Retry);
                            ui.close_menu();
                        }
                    });

                    if message.role == MessageRole::Assistant && message.streaming {
                        if ui.button("✓ Accept").clicked() {
                            action = Some(ChatAction::Accept);
                        }
                        if ui.button("✗ Skip").clicked() {
                            action = Some(ChatAction::Skip);
                        }
                    }
                });
            });

            ui.separator();

            match message.role {
                MessageRole::Think => {
                    ui.collapsing("Thinking trace", |ui| {
                        ui.label(
                            RichText::new(&message.content)
                                .size(13.0)
                                .color(Color32::from_rgb(180, 160, 40)),
                        );
                    });
                }
                MessageRole::ToolCall => {
                    let name = message.tool_name.as_deref().unwrap_or("unknown");
                    ui.label(RichText::new(format!("🔧 {}", name)).strong().size(13.0));
                    ui.code(&message.content);
                }
                MessageRole::ToolResult => {
                    if message.is_error {
                        ui.colored_label(Color32::RED, "❌ Error:");
                    } else {
                        ui.label("📎 Result:");
                    }
                    ui.code(&message.content);
                }
                MessageRole::Event => {
                    ui.label(
                        RichText::new(&message.content)
                            .size(12.0)
                            .color(Color32::GRAY)
                            .italics(),
                    );
                }
                _ => {
                    // Render assistant/system/user messages as markdown
                    render_markdown_preview(ui, &message.content);
                }
            }
        });

        action
    }

    fn show_input_area(ui: &mut Ui, state: &mut ChatWidgetState, action: &mut Option<ChatAction>) {
        ui.group(|ui| {
            ui.horizontal(|ui| {
                if ui
                    .selectable_label(state.show_capabilities, "✨ Capabilities")
                    .clicked()
                {
                    state.show_capabilities = !state.show_capabilities;
                }
                if ui
                    .selectable_label(state.show_context, "ℹ Context")
                    .clicked()
                {
                    state.show_context = !state.show_context;
                }
                if ui
                    .selectable_label(state.show_attachments, "📎 Attach")
                    .clicked()
                {
                    state.show_attachments = !state.show_attachments;
                }

                ui.with_layout(Layout::right_to_left(egui::Align::Center), |ui| {
                    let send_enabled = !state.input_text.is_empty() && !state.agent_generating;
                    let send_label = if state.agent_generating {
                        "⏳ Generating..."
                    } else {
                        "Send"
                    };
                    if ui
                        .add_enabled(send_enabled, egui::Button::new(send_label))
                        .clicked()
                    {
                        let text = state.input_text.trim().to_string();
                        if !text.is_empty() {
                            *action = Some(ChatAction::SendMessage(text.clone()));
                            state.messages.push(ChatMessage::user(text));
                            state.input_text.clear();
                        }
                    }
                    if state.agent_generating && ui.button("⏹ Stop").clicked() {
                        *action = Some(ChatAction::Stop);
                    }
                });
            });

            if state.show_capabilities {
                ui.separator();
                Self::show_capabilities_panel(ui, state, action);
            }
            if state.show_context {
                ui.separator();
                Self::show_context_panel(ui, state);
            }
            if state.show_attachments {
                ui.separator();
                Self::show_attachments_bar(ui, state, action);
            }

            // Attachments display
            if !state.attachments.is_empty() {
                ui.horizontal_wrapped(|ui| {
                    for (i, att) in state.attachments.iter().enumerate() {
                        ui.label(
                            RichText::new(format!("📎 {}", att.name))
                                .size(11.0)
                                .color(Color32::from_rgb(150, 200, 255)),
                        );
                        if ui.button("✕").clicked() {
                            *action = Some(ChatAction::RemoveAttachment(i));
                        }
                    }
                });
            }

            // Text input
            ui.add_space(4.0);
            let hint = if state.agent_generating {
                "Waiting for response..."
            } else {
                "Type a message... (Enter to send, Shift+Enter for newline)"
            };
            let input = egui::TextEdit::multiline(&mut state.input_text)
                .font(FontId::proportional(14.0))
                .desired_rows(3)
                .desired_width(f32::INFINITY)
                .hint_text(hint)
                .interactive(!state.agent_generating);

            let response = ui.add(input);
            if response.has_focus()
                && !state.agent_generating
                && ui.input(|i| {
                    i.key_pressed(egui::Key::Enter) && !i.modifiers.shift && !i.modifiers.ctrl
                })
            {
                let text = state.input_text.trim().to_string();
                if !text.is_empty() {
                    *action = Some(ChatAction::SendMessage(text.clone()));
                    state.messages.push(ChatMessage::user(text));
                    state.input_text.clear();
                }
            }
        });
    }

    fn show_capabilities_panel(
        ui: &mut Ui,
        state: &mut ChatWidgetState,
        action: &mut Option<ChatAction>,
    ) {
        ui.label(RichText::new("Active Capabilities").strong().size(12.0));
        ui.horizontal_wrapped(|ui| {
            for cap in Capability::ALL {
                let is_active = state.active_capabilities.contains(&cap);
                if ui
                    .selectable_label(is_active, cap.label())
                    .on_hover_text(cap.description())
                    .clicked()
                {
                    if is_active {
                        state.active_capabilities.retain(|c| *c != cap);
                    } else {
                        state.active_capabilities.push(cap);
                    }
                    *action = Some(ChatAction::ToggleCapability(cap));
                }
            }
        });
        if state.active_capabilities.is_empty() {
            ui.label(
                RichText::new("No capabilities selected — general chat mode")
                    .size(11.0)
                    .color(Color32::GRAY),
            );
        }
    }

    fn show_context_panel(ui: &mut Ui, state: &mut ChatWidgetState) {
        ui.label(RichText::new("Context").strong().size(12.0));
        if let Some(ref doc) = state.context.document {
            ui.label(RichText::new(format!("📄 Document: {}", doc)).size(11.0));
        }
        if let Some(ref section) = state.context.section {
            ui.label(RichText::new(format!("📑 Section: {}", section)).size(11.0));
        }
        if let Some(ref sel) = state.context.selection_summary {
            ui.label(RichText::new(format!("🔍 Selection: {}", sel)).size(11.0));
        }
        ui.label(
            RichText::new(format!(
                "⚡ ~{} tokens in context",
                state.context.estimated_tokens
            ))
            .size(11.0)
            .color(Color32::GRAY),
        );
    }

    fn show_attachments_bar(
        ui: &mut Ui,
        _state: &mut ChatWidgetState,
        action: &mut Option<ChatAction>,
    ) {
        ui.label(RichText::new("Add Attachment").strong().size(12.0));
        ui.horizontal(|ui| {
            if ui.button("📄 File").clicked() {
                *action = Some(ChatAction::AddAttachment(Attachment {
                    name: "document.md".to_string(),
                    kind: AttachmentKind::File,
                    content: String::new(),
                }));
            }
            if ui.button("📝 Snippet").clicked() {
                *action = Some(ChatAction::AddAttachment(Attachment {
                    name: "snippet.txt".to_string(),
                    kind: AttachmentKind::Snippet,
                    content: String::new(),
                }));
            }
        });
    }

    pub fn show_compact(ui: &mut Ui, state: &mut ChatWidgetState) -> Option<ChatAction> {
        let mut action = None;

        ScrollArea::vertical()
            .auto_shrink([false, false])
            .stick_to_bottom(true)
            .show(ui, |ui| {
                for message in &state.messages {
                    let role_color = message.role.color();
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new(format!("[{}]", message.role.label()))
                                .size(10.0)
                                .color(role_color),
                        );
                        ui.label(
                            RichText::new(&message.content)
                                .size(12.0)
                                .color(ui.visuals().text_color()),
                        );
                    });
                    ui.add_space(2.0);
                }
            });

        ui.separator();
        ui.horizontal(|ui| {
            let input = egui::TextEdit::singleline(&mut state.input_text)
                .hint_text("Message...")
                .desired_width(ui.available_width() - 50.0);
            let response = ui.add(input);
            let send_enabled = !state.input_text.is_empty() && !state.agent_generating;
            if ui
                .add_enabled(send_enabled, egui::Button::new("Send"))
                .clicked()
                || (response.has_focus()
                    && ui.input(|i| i.key_pressed(egui::Key::Enter))
                    && send_enabled)
            {
                let text = state.input_text.trim().to_string();
                if !text.is_empty() {
                    let a = ChatAction::SendMessage(text.clone());
                    state.messages.push(ChatMessage::user(text));
                    state.input_text.clear();
                    action = Some(a);
                }
            }
        });

        action
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chat_message_new() {
        let msg = ChatMessage::new(MessageRole::User, "Hello".to_string());
        assert_eq!(msg.role, MessageRole::User);
        assert_eq!(msg.content, "Hello");
        assert!(!msg.streaming);
        assert!(msg.accepted);
    }

    #[test]
    fn test_chat_message_constructors() {
        let sys = ChatMessage::system("System msg".to_string());
        assert_eq!(sys.role, MessageRole::System);
        let user = ChatMessage::user("User msg".to_string());
        assert_eq!(user.role, MessageRole::User);
        let asst = ChatMessage::assistant("AI msg".to_string());
        assert_eq!(asst.role, MessageRole::Assistant);
        let think = ChatMessage::think("Hmm...".to_string());
        assert_eq!(think.role, MessageRole::Think);
        let tc = ChatMessage::tool_call("search", r#"{"q":"test"}"#.to_string());
        assert_eq!(tc.role, MessageRole::ToolCall);
        assert_eq!(tc.tool_name, Some("search".to_string()));
        let tr = ChatMessage::tool_result("Result data".to_string(), false);
        assert_eq!(tr.role, MessageRole::ToolResult);
        assert!(!tr.is_error);
        let err = ChatMessage::tool_result("Error!".to_string(), true);
        assert!(err.is_error);
        let ev = ChatMessage::event("Something happened".to_string());
        assert_eq!(ev.role, MessageRole::Event);
    }

    #[test]
    fn test_message_role_label() {
        assert_eq!(MessageRole::System.label(), "System");
        assert_eq!(MessageRole::User.label(), "You");
        assert_eq!(MessageRole::Assistant.label(), "AI");
        assert_eq!(MessageRole::Think.label(), "AI (thinking)");
        assert_eq!(MessageRole::ToolCall.label(), "Tool Call");
        assert_eq!(MessageRole::ToolResult.label(), "Tool Result");
        assert_eq!(MessageRole::Event.label(), "Event");
    }

    #[test]
    fn test_message_role_color() {
        for &role in &[
            MessageRole::System,
            MessageRole::User,
            MessageRole::Assistant,
            MessageRole::Think,
            MessageRole::ToolCall,
            MessageRole::ToolResult,
            MessageRole::Event,
        ] {
            let c = role.color();
            assert!(c.r() > 0 || c.g() > 0 || c.b() > 0);
        }
    }

    #[test]
    fn test_chat_widget_state_new() {
        let state = ChatWidgetState::new();
        assert!(state.messages.is_empty());
        assert!(state.input_text.is_empty());
        assert!(state.active_capabilities.is_empty());
        assert!(!state.agent_generating);
    }

    #[test]
    fn test_chat_widget_state_with_greeting() {
        let state = ChatWidgetState::new().with_greeting("Welcome!");
        assert_eq!(state.messages.len(), 1);
        assert_eq!(state.messages[0].role, MessageRole::System);
        assert_eq!(state.messages[0].content, "Welcome!");
    }

    #[test]
    fn test_chat_widget_state_add_message() {
        let mut state = ChatWidgetState::new();
        let id = state.add_message(ChatMessage::user("Test".to_string()));
        assert_eq!(state.messages.len(), 1);
        assert!(!id.is_empty());
    }

    #[test]
    fn test_chat_widget_state_clear() {
        let mut state = ChatWidgetState::new()
            .with_greeting("Hi")
            .with_greeting("Hello");
        state.input_text = "some text".to_string();
        state.clear();
        assert!(state.messages.is_empty());
        assert!(state.input_text.is_empty());
    }

    #[test]
    fn test_chat_widget_state_last_assistant_message() {
        let mut state = ChatWidgetState::new();
        state.add_message(ChatMessage::user("Hi".to_string()));
        state.add_message(ChatMessage::assistant("Hello there!".to_string()));
        state.add_message(ChatMessage::user("Tell me more".to_string()));
        state.add_message(ChatMessage::assistant("Sure!".to_string()));
        let last = state.last_assistant_message();
        assert!(last.is_some());
        assert_eq!(last.unwrap().content, "Sure!");
    }

    #[test]
    fn test_capability_variants() {
        assert_eq!(Capability::ALL.len(), 6);
        assert_eq!(Capability::Generate.label(), "Generate");
        assert_eq!(Capability::Research.label(), "Research");
        assert_eq!(Capability::Edit.label(), "Edit");
        assert_eq!(Capability::Critique.label(), "Critique");
        assert_eq!(Capability::Outline.label(), "Outline");
        assert_eq!(Capability::Brainstorm.label(), "Brainstorm");
    }

    #[test]
    fn test_capability_descriptions() {
        for cap in Capability::ALL {
            assert!(!cap.description().is_empty());
        }
    }

    #[test]
    fn test_chat_action_variants() {
        let actions = vec![
            ChatAction::SendMessage("hello".to_string()),
            ChatAction::Accept,
            ChatAction::AcceptAll,
            ChatAction::Revise("make it better".to_string()),
            ChatAction::Skip,
            ChatAction::Stop,
            ChatAction::ToggleCapability(Capability::Generate),
            ChatAction::AddAttachment(Attachment {
                name: "file.md".to_string(),
                kind: AttachmentKind::File,
                content: String::new(),
            }),
            ChatAction::RemoveAttachment(0),
            ChatAction::SetPacingMode(InteractionMode::FullControl),
            ChatAction::Undo,
            ChatAction::Redo,
            ChatAction::CopyMessage("content".to_string()),
            ChatAction::Retry,
            ChatAction::Clear,
        ];
        assert_eq!(actions.len(), 15);
    }

    #[test]
    fn test_streaming_message_property() {
        let mut msg = ChatMessage::assistant("Hello".to_string());
        assert!(!msg.streaming);
        msg.streaming = true;
        assert!(msg.streaming);
    }

    #[test]
    fn test_chat_message_ids_are_unique() {
        let msg1 = ChatMessage::assistant("First".to_string());
        let msg2 = ChatMessage::assistant("Second".to_string());
        assert_ne!(msg1.id, msg2.id);
    }

    #[test]
    fn test_auto_scroll_default() {
        let state = ChatWidgetState::new();
        assert!(state.auto_scroll);
    }
    #[test]
    fn split_response_plain_text_passes_through_as_assistant() {
        let chunks = split_response_with_thinking("Hello there.");
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].role, MessageRole::Assistant);
        assert_eq!(chunks[0].content, "Hello there.");
    }

    #[test]
    fn split_response_empty_returns_empty_vec() {
        assert!(split_response_with_thinking("").is_empty());
        assert!(split_response_with_thinking("   \n\t  ").is_empty());
    }

    #[test]
    fn split_response_think_only_no_assistant() {
        let chunks = split_response_with_thinking("<think>just reasoning, no body</think>");
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].role, MessageRole::Think);
        assert_eq!(chunks[0].content, "just reasoning, no body");
    }

    #[test]
    fn split_response_think_then_answer_demotes_thinking() {
        let chunks = split_response_with_thinking("<think>Ahem, plot</think>The quick brown fox.");
        assert_eq!(chunks.len(), 2);
        assert_eq!(chunks[0].role, MessageRole::Think);
        assert_eq!(chunks[0].content, "Ahem, plot");
        assert_eq!(chunks[1].role, MessageRole::Assistant);
        assert_eq!(chunks[1].content, "The quick brown fox.");
    }

    #[test]
    fn split_response_pre_text_then_think_then_answer() {
        let chunks =
            split_response_with_thinking("Lead-in prose.<think>reasoning</think>Tail answer.");
        assert_eq!(chunks.len(), 3);
        assert_eq!(chunks[0].role, MessageRole::Assistant);
        assert_eq!(chunks[0].content, "Lead-in prose.");
        assert_eq!(chunks[1].role, MessageRole::Think);
        assert_eq!(chunks[1].content, "reasoning");
        assert_eq!(chunks[2].role, MessageRole::Assistant);
        assert_eq!(chunks[2].content, "Tail answer.");
    }

    #[test]
    fn split_response_multiple_thinks_coalesce_into_one() {
        let chunks = split_response_with_thinking(
            "<think>first thought</think>mid prose<think>second thought</think>end.",
        );
        // Two thinks coalesce into a single `Think` with newline-join;
        // "mid prose" and "end." are the two assistants.
        assert_eq!(chunks.len(), 3);
        assert_eq!(chunks[0].role, MessageRole::Think);
        assert!(chunks[0].content.contains("first thought"));
        assert!(chunks[0].content.contains("second thought"));
        assert!(chunks[0].content.contains("\n\n"));
        assert_eq!(chunks[1].role, MessageRole::Assistant);
        assert_eq!(chunks[1].content, "mid prose");
        assert_eq!(chunks[2].role, MessageRole::Assistant);
        assert_eq!(chunks[2].content, "end.");
    }

    #[test]
    fn split_response_truncated_unclosed_think_drops_to_assistant() {
        // No matching close tag -> the whole input becomes one Assistant
        // bubble so the user sees the partial answer mid-stream.
        let chunks = split_response_with_thinking("<think>opened but not closed... mid answer");
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].role, MessageRole::Assistant);
        assert_eq!(
            chunks[0].content,
            "<think>opened but not closed... mid answer"
        );
    }

    #[test]
    fn chat_widget_add_assistant_response_demotes() {
        // End-to-end on ChatWidgetState: a think-then-answer response
        // should land as two messages, the first collapsed/Think.
        let mut state = ChatWidgetState::new();
        state.add_assistant_response("<think>plotting</think>Final line.");
        assert_eq!(state.messages.len(), 2);
        assert_eq!(state.messages[0].role, MessageRole::Think);
        assert_eq!(state.messages[0].content, "plotting");
        assert_eq!(state.messages[1].role, MessageRole::Assistant);
        assert_eq!(state.messages[1].content, "Final line.");
    }
}
