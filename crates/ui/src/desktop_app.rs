//! RoCo AI Desktop — Full GUI application built on egui/eframe.
//!
//! Wires together all widgets (PacingWidget, ChatWidget, MarkdownEditor,
//! FileTree, WikiBrowser, LinkGraph, SessionBrowser, ChangeTimeline) into a
//! complete desktop experience with model-backed generation, session
//! management, and the full human-paced interaction flow.

use crate::{
    change_timeline::{ChangeTimeline, ChangeTimelineState, TimelineAction, TimelineEntry, TimelineEntryKind},
    chat::{ChatAction, ChatMessage, ChatWidget, ChatWidgetState, MessageRole},
    file_tree::{FileTree, FileTreeAction, FileTreeState},
    link_graph::{LinkGraph, LinkGraphAction, LinkGraphState, NodeKind},
    markdown_editor::{MarkdownEditor, MarkdownEditorState},
    pacing::{PacingAction, PacingMode, PacingWidget, PacingWidgetState},
    session_browser::{SessionBrowser, SessionBrowserAction, SessionBrowserState},
    wiki_browser::{WikiBrowser, WikiBrowserAction, WikiBrowserState},
};
use eframe::egui;
use egui::{CentralPanel, Context, Layout, RichText, SidePanel, TopBottomPanel};
use roco_engine::{CompletionRequest, ModelBackend};
use std::path::PathBuf;
use std::sync::Arc;

/// Which tool is shown in the right/browser panel
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RightPanelTool {
    Editor,
    FileTree,
    Wiki,
    LinkGraph,
    Sessions,
    Timeline,
}

impl RightPanelTool {
    pub fn label(self) -> &'static str {
        match self {
            RightPanelTool::Editor => "Editor",
            RightPanelTool::FileTree => "Files",
            RightPanelTool::Wiki => "Wiki",
            RightPanelTool::LinkGraph => "Graph",
            RightPanelTool::Sessions => "Sessions",
            RightPanelTool::Timeline => "Timeline",
        }
    }

    pub fn icon(self) -> &'static str {
        match self {
            RightPanelTool::Editor => "\u{1f4dd}",
            RightPanelTool::FileTree => "\u{1f4c1}",
            RightPanelTool::Wiki => "\u{1f4d6}",
            RightPanelTool::LinkGraph => "\u{1f517}",
            RightPanelTool::Sessions => "\u{1f4ac}",
            RightPanelTool::Timeline => "\u{23f1}\u{fe0f}",
        }
    }
}

/// The main desktop application
pub struct RocoDesktopApp {
    // Core state
    backend: Option<Arc<dyn ModelBackend>>,
    model_loaded: bool,

    // Widget states — all owned here, each widget borrows mutably
    pacing_state: PacingWidgetState,
    chat_state: ChatWidgetState,
    editor_state: MarkdownEditorState,

    // Browser widget states
    file_tree_state: FileTreeState,
    wiki_state: WikiBrowserState,
    link_graph_state: LinkGraphState,
    session_browser_state: SessionBrowserState,
    timeline_state: ChangeTimelineState,

    // Session
    session_dir: PathBuf,
    session_path: Option<PathBuf>,

    // Layout
    left_panel_open: bool,
    right_panel_tool: Option<RightPanelTool>,

    // Status
    status_message: String,
}

fn now_rfc3339() -> String {
    chrono::Utc::now().to_rfc3339()
}

fn timeline_entry(id: &str, desc: &str, kind: TimelineEntryKind) -> TimelineEntry {
    TimelineEntry {
        id: id.to_string(),
        description: desc.to_string(),
        kind,
        timestamp: now_rfc3339(),
        is_current: true,
    }
}

impl RocoDesktopApp {
    pub fn new(backend: Option<Arc<dyn ModelBackend>>) -> Self {
        let session_dir = PathBuf::from(".roco/sessions");
        std::fs::create_dir_all(&session_dir).ok();

        // Set up a minimal example link graph for demo
        let mut link_graph_state = LinkGraphState::new();
        link_graph_state.add_node("protagonist", "Hero", NodeKind::Character);
        link_graph_state.add_node("antagonist", "Villain", NodeKind::Character);
        link_graph_state.add_node("forest", "Dark Forest", NodeKind::Location);
        link_graph_state.add_node("quest", "Main Quest", NodeKind::PlotThread);
        link_graph_state.add_edge("protagonist", "antagonist", "conflict");
        link_graph_state.add_edge("protagonist", "forest", "explores");
        link_graph_state.add_edge("protagonist", "quest", "drives");

        Self {
            model_loaded: backend.is_some(),
            backend,
            pacing_state: PacingWidgetState::new(PacingMode::Careful, 0),
            chat_state: ChatWidgetState::new().with_greeting(
                "Welcome to RoCo AI! Start by typing a message or browsing sessions.",
            ),
            editor_state: MarkdownEditorState::default(),
            file_tree_state: FileTreeState::new(
                std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
            ),
            wiki_state: WikiBrowserState::new(),
            link_graph_state,
            session_browser_state: SessionBrowserState::new(session_dir.clone()),
            timeline_state: ChangeTimelineState::new(),
            session_dir,
            session_path: None,
            left_panel_open: true,
            right_panel_tool: None,
            status_message: String::new(),
        }
    }

    /// Toggle a right-panel tool on/off
    fn toggle_tool(&mut self, tool: RightPanelTool) {
        self.right_panel_tool = if self.right_panel_tool == Some(tool) {
            None
        } else {
            Some(tool)
        };
    }

    /// Refresh browser states that depend on the filesystem
    fn refresh_browsers(&mut self) {
        self.file_tree_state.refresh();
        self.session_browser_state.refresh();
    }

    /// Handle a chat action by sending to the model
    fn handle_chat_action(&mut self, action: ChatAction, ctx: &Context) {
        match action {
            ChatAction::SendMessage(text) => {
                if let Some(ref backend) = self.backend {
                    self.status_message = "Generating...".to_string();
                    self.timeline_state.add_entry(timeline_entry(
                        "send", "Send message", TimelineEntryKind::Action,
                    ));

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
                            self.timeline_state.add_entry(timeline_entry(
                                "gen_done", "Generation complete", TimelineEntryKind::Checkpoint,
                            ));
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
                        "Model not loaded. Set RWKV_MODEL or restart with a backend.".to_string(),
                    ));
                }
            }
            ChatAction::Accept => {
                self.status_message = "Accepted. Continuing...".to_string();
                self.timeline_state.add_entry(timeline_entry(
                    "accept", "Accepted suggestion", TimelineEntryKind::Action,
                ));
                self.auto_save();
            }
            ChatAction::Skip => {
                self.status_message = "Skipped.".to_string();
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
                    if self.chat_state.messages.len() >= 2 {
                        self.chat_state.messages.pop();
                        self.chat_state.messages.pop();
                    }
                    self.handle_chat_action(ChatAction::SendMessage(content), ctx);
                }
            }
            ChatAction::CopyMessage(content) => {
                ctx.copy_text(content);
                self.status_message = "Copied to clipboard.".to_string();
            }
            _ => {}
        }
    }

    /// Handle a file tree action
    fn handle_file_tree_action(&mut self, action: FileTreeAction) {
        match action {
            FileTreeAction::OpenFile(path) => {
                self.status_message = format!("Opened: {}", path.display());
                if let Ok(content) = std::fs::read_to_string(&path) {
                    self.editor_state.document.text = content;
                    self.right_panel_tool = Some(RightPanelTool::Editor);
                }
            }
            FileTreeAction::SelectFile(path) => {
                self.status_message = format!("Selected: {}", path.display());
            }
            FileTreeAction::ToggleFolder(path) => {
                self.status_message = format!("Toggled folder: {}", path.display());
            }
            FileTreeAction::Refresh => {
                self.file_tree_state.refresh();
                self.status_message = "File tree refreshed.".to_string();
            }
            FileTreeAction::DeleteFile(path) => {
                self.status_message = format!("Delete: {}", path.display());
            }
            FileTreeAction::RenameFile(path, name) => {
                self.status_message = format!("Rename {} → {}", path.display(), name);
            }
        }
    }

    /// Handle a wiki browser action
    fn handle_wiki_action(&mut self, action: WikiBrowserAction) {
        match action {
            WikiBrowserAction::SelectPage(idx) => {
                if let Some(page) = self.wiki_state.pages.get(idx) {
                    self.status_message = format!("Wiki: {}", page.title);
                }
            }
            WikiBrowserAction::EditPage(idx) => {
                if let Some(page) = self.wiki_state.pages.get(idx) {
                    self.editor_state.document.text = page.content.clone();
                    self.right_panel_tool = Some(RightPanelTool::Editor);
                    self.status_message = format!("Editing wiki: {}", page.title);
                }
            }
            WikiBrowserAction::OpenInEditor(idx) => {
                if let Some(page) = self.wiki_state.pages.get(idx) {
                    self.editor_state.document.text = page.content.clone();
                    self.right_panel_tool = Some(RightPanelTool::Editor);
                    self.status_message = format!("Opened wiki: {}", page.title);
                }
            }
        }
    }

    /// Handle a session browser action
    fn handle_session_action(&mut self, action: SessionBrowserAction) {
        match action {
            SessionBrowserAction::Load(path) => {
                self.load_session(&path);
            }
            SessionBrowserAction::Delete(path) => {
                if std::fs::remove_file(&path).is_ok() {
                    self.status_message = format!("Deleted: {}", path.display());
                    self.session_browser_state.refresh();
                }
            }
            SessionBrowserAction::Refresh => {
                self.session_browser_state.refresh();
                self.status_message = "Sessions refreshed.".to_string();
            }
        }
    }

    /// Handle a link graph action
    fn handle_link_graph_action(&mut self, action: LinkGraphAction) {
        match action {
            LinkGraphAction::SelectNode(id) => {
                self.status_message = format!("Selected: {id}");
            }
            LinkGraphAction::OpenNode(id) => {
                self.status_message = format!("Open node: {id}");
            }
            LinkGraphAction::DragNode(..) => {
                // handled internally by the widget
            }
            LinkGraphAction::ZoomIn => {
                self.link_graph_state.zoom *= 1.2;
            }
            LinkGraphAction::ZoomOut => {
                self.link_graph_state.zoom /= 1.2;
            }
            LinkGraphAction::ResetView => {
                self.link_graph_state.zoom = 1.0;
                self.link_graph_state.pan = egui::Vec2::ZERO;
            }
            LinkGraphAction::AddNode(id, kind) => {
                let label = id.clone();
                self.link_graph_state.add_node(&id, &label, kind);
                self.status_message = format!("Added node: {label}");
            }
        }
    }

    /// Handle a timeline action
    fn handle_timeline_action(&mut self, action: TimelineAction) {
        match action {
            TimelineAction::Undo => {
                self.status_message = "Undo (wired via VersionControl in engine)".to_string();
                self.timeline_state.add_entry(timeline_entry(
                    "undo", "Undo action", TimelineEntryKind::Undo,
                ));
            }
            TimelineAction::Redo => {
                self.status_message = "Redo".to_string();
                self.timeline_state.add_entry(timeline_entry(
                    "redo", "Redo action", TimelineEntryKind::Redo,
                ));
            }
            TimelineAction::CreateSnapshot(label) => {
                let id = format!("snap_{}", chrono::Utc::now().timestamp());
                self.timeline_state.add_entry(TimelineEntry {
                    id: id.clone(),
                    description: format!("Snapshot: {label}"),
                    kind: TimelineEntryKind::Snapshot,
                    timestamp: now_rfc3339(),
                    is_current: true,
                });
                self.status_message = format!("Snapshot taken: {label}");
            }
            TimelineAction::SelectEntry(idx) => {
                if let Some(entry) = self.timeline_state.entries.get(idx) {
                    self.status_message = format!("Timeline: {}", entry.description);
                }
            }
            TimelineAction::Rollback(id) => {
                self.status_message = format!("Rollback to: {id}");
                self.timeline_state.add_entry(TimelineEntry {
                    id: format!("rollback_{}", chrono::Utc::now().timestamp()),
                    description: format!("Rollback to {id}"),
                    kind: TimelineEntryKind::Rollback,
                    timestamp: now_rfc3339(),
                    is_current: true,
                });
            }
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
        self.timeline_state.clear();
        self.timeline_state.add_entry(timeline_entry(
            "session_start", "Session started", TimelineEntryKind::Checkpoint,
        ));
        self.auto_save();
        self.refresh_browsers();
    }

    fn save_session(&mut self) {
        self.auto_save();
        if let Some(ref path) = self.session_path {
            self.status_message = format!("Saved: {}", path.display());
            self.timeline_state.add_entry(timeline_entry(
                "session_saved", "Session saved", TimelineEntryKind::Checkpoint,
            ));
        }
        self.refresh_browsers();
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
                self.timeline_state.clear();
                self.timeline_state.add_entry(timeline_entry(
                    "session_loaded", "Session loaded", TimelineEntryKind::Checkpoint,
                ));
            }
        }
    }

    /// Render the active right-panel tool
    fn show_right_panel(&mut self, ui: &mut egui::Ui, ctx: &Context) {
        match self.right_panel_tool {
            Some(RightPanelTool::Editor) => {
                ui.label(RichText::new("\u{1f4dd} Editor").strong().size(14.0));
                ui.separator();
                // Sync latest assistant message into editor
                if let Some(last) = self.chat_state.last_assistant_message() {
                    if self.editor_state.document.text != last.content {
                        self.editor_state.document.text = last.content.clone();
                    }
                }
                let mut editor = MarkdownEditor::new();
                editor.show(ui, &mut self.editor_state, ctx);
            }
            Some(RightPanelTool::FileTree) => {
                ui.label(RichText::new("\u{1f4c1} Files").strong().size(14.0));
                ui.separator();
                if let Some(action) = FileTree::show(ui, &mut self.file_tree_state) {
                    self.handle_file_tree_action(action);
                }
            }
            Some(RightPanelTool::Wiki) => {
                ui.label(RichText::new("\u{1f4d6} Wiki").strong().size(14.0));
                ui.separator();
                if let Some(action) = WikiBrowser::show(ui, &mut self.wiki_state) {
                    self.handle_wiki_action(action);
                }
            }
            Some(RightPanelTool::LinkGraph) => {
                ui.label(RichText::new("\u{1f517} Link Graph").strong().size(14.0));
                ui.separator();
                if let Some(action) = LinkGraph::show(ui, &mut self.link_graph_state) {
                    self.handle_link_graph_action(action);
                }
            }
            Some(RightPanelTool::Sessions) => {
                ui.label(RichText::new("\u{1f4ac} Sessions").strong().size(14.0));
                ui.separator();
                if let Some(action) = SessionBrowser::show(ui, &mut self.session_browser_state) {
                    self.handle_session_action(action);
                }
            }
            Some(RightPanelTool::Timeline) => {
                ui.label(RichText::new("\u{23f1}\u{fe0f} Timeline").strong().size(14.0));
                ui.separator();
                if let Some(action) = ChangeTimeline::show(ui, &mut self.timeline_state) {
                    self.handle_timeline_action(action);
                }
            }
            None => {
                // No tool selected — show a hint
                ui.vertical_centered(|ui| {
                    ui.add_space(60.0);
                    ui.label(
                        RichText::new("No tool selected")
                            .size(16.0)
                            .color(ui.visuals().weak_text_color()),
                    );
                    ui.add_space(8.0);
                    ui.label(
                        RichText::new("Use View \u{2192} Show \u{2026} to open a browser panel")
                            .size(12.0)
                            .color(ui.visuals().weak_text_color()),
                    );
                });
            }
        }
    }
}

/// Conversation state for serialization
#[derive(serde::Serialize, serde::Deserialize)]
pub struct ConversationState {
    pub id: String,
    pub messages: Vec<ConversationMessage>,
    pub pacing: String,
    pub created_at: String,
    pub updated_at: String,
}

impl ConversationState {
    pub fn save(&self, path: &std::path::Path) -> Result<(), String> {
        let json = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        std::fs::write(path, &json).map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn load(path: &std::path::Path) -> Result<Self, String> {
        let json = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
        serde_json::from_str(&json).map_err(|e| e.to_string())
    }
}

#[derive(serde::Serialize, serde::Deserialize)]
pub struct ConversationMessage {
    pub role: String,
    pub content: String,
    pub timestamp: String,
}

impl eframe::App for RocoDesktopApp {
    fn update(&mut self, ctx: &Context, _frame: &mut eframe::Frame) {
        // ── Menu bar ────────────────────────────────────────────────────
        TopBottomPanel::top("menu_bar").show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                ui.menu_button("File", |ui| {
                    if ui.button("\u{1f4c1} New Session").clicked() {
                        self.new_session();
                        ui.close_menu();
                    }
                    if ui.button("\u{1f4be} Save Session").clicked() {
                        self.save_session();
                        ui.close_menu();
                    }
                    if ui.button("\u{1f4c2} Open Session\u{2026}").clicked() {
                        self.right_panel_tool = Some(RightPanelTool::Sessions);
                        self.session_browser_state.refresh();
                        ui.close_menu();
                    }
                    ui.separator();
                    if ui.button("\u{1f6aa} Quit").clicked() {
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
                    ui.separator();
                    for tool in [
                        RightPanelTool::Editor,
                        RightPanelTool::FileTree,
                        RightPanelTool::Wiki,
                        RightPanelTool::LinkGraph,
                        RightPanelTool::Sessions,
                        RightPanelTool::Timeline,
                    ] {
                        let is_active = self.right_panel_tool == Some(tool);
                        let label = format!("{} {}", tool.icon(), tool.label());
                        if ui.selectable_label(is_active, &label).clicked() {
                            self.toggle_tool(tool);
                            // Refresh data when switching to a browser
                            match tool {
                                RightPanelTool::FileTree => self.file_tree_state.refresh(),
                                RightPanelTool::Sessions => self.session_browser_state.refresh(),
                                _ => {}
                            }
                            ui.close_menu();
                        }
                    }
                });

                ui.menu_button("Help", |ui| {
                    if ui.button("About").clicked() {
                        self.status_message =
                            "RoCo AI Desktop \u{2014} Collaborative Story Writing v0.1".into();
                        ui.close_menu();
                    }
                    if ui.button("Keyboard Shortcuts").clicked() {
                        self.status_message =
                            "Ctrl+Z: Undo | Ctrl+Y: Redo | Ctrl+S: Save | Enter: Send".into();
                        ui.close_menu();
                    }
                });

                // Right side: status bar
                ui.with_layout(Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(RichText::new(&self.status_message).size(11.0).color(
                        if self.status_message.starts_with("Error") {
                            egui::Color32::RED
                        } else if self.status_message.starts_with("Generating") {
                            egui::Color32::from_rgb(100, 200, 255)
                        } else {
                            ui.visuals().weak_text_color()
                        },
                    ));
                    if !self.model_loaded {
                        ui.label(
                            RichText::new("\u{26a0} No Model")
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
                        ui.label(RichText::new("\u{26a1} Pacing").strong().size(14.0));
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
                                PacingAction::FullControl => {
                                    self.pacing_state.mode = PacingMode::Careful;
                                    "Pacing: Careful".into()
                                }
                                _ => format!("{:?}", action),
                            };
                        }

                        ui.add_space(12.0);
                        ui.separator();
                        ui.add_space(8.0);

                        // Session info
                        ui.label(RichText::new("\u{1f4cb} Session").strong().size(14.0));
                        if let Some(ref path) = self.session_path {
                            if let Some(name) = path.file_stem() {
                                ui.label(
                                    RichText::new(name.to_string_lossy())
                                        .size(11.0)
                                        .color(ui.visuals().weak_text_color()),
                                );
                            }
                        } else {
                            ui.label(
                                RichText::new("No session loaded")
                                    .size(11.0)
                                    .color(ui.visuals().weak_text_color()),
                            );
                        }
                        ui.label(format!("\u{1f4ac} {} messages", self.chat_state.messages.len()));
                        let word_count: usize = self
                            .chat_state
                            .messages
                            .iter()
                            .map(|m| m.content.split_whitespace().count())
                            .sum();
                        ui.label(format!("\u{1f4dd} {word_count} words total"));

                        ui.add_space(8.0);
                        if ui.button("\u{1f4c1} New Session").clicked() {
                            self.new_session();
                        }
                        if ui.button("\u{1f4be} Save").clicked() {
                            self.save_session();
                        }

                        // Tool quick-launch
                        ui.add_space(12.0);
                        ui.separator();
                        ui.add_space(8.0);
                        ui.label(RichText::new("\u{1f527} Tools").strong().size(14.0));
                        for tool in [
                            RightPanelTool::Editor,
                            RightPanelTool::FileTree,
                            RightPanelTool::Wiki,
                            RightPanelTool::LinkGraph,
                            RightPanelTool::Sessions,
                            RightPanelTool::Timeline,
                        ] {
                            let label = format!("{} {}", tool.icon(), tool.label());
                            if ui.button(&label).clicked() {
                                self.toggle_tool(tool);
                                match tool {
                                    RightPanelTool::FileTree => self.file_tree_state.refresh(),
                                    RightPanelTool::Sessions => self.session_browser_state.refresh(),
                                    _ => {}
                                }
                            }
                        }
                    });
                });
        }

        // ── Right panel: browser/editor tools ──────────────────────────
        if self.right_panel_tool.is_some() {
            SidePanel::right("right_panel")
                .resizable(true)
                .default_width(380.0)
                .show(ctx, |ui| {
                    self.show_right_panel(ui, ctx);
                });
        }

        // ── Central panel: chat ─────────────────────────────────────────
        CentralPanel::default().show(ctx, |ui| {
            // Model not loaded warning
            if !self.model_loaded {
                ui.label(RichText::new(
                    "\u{26a0} Model not loaded. The model loads automatically when RWKV_MODEL is set.\n\
                     You can still browse sessions or use the editor offline."
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
