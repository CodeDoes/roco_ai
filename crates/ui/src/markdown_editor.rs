//! MARKDOWN EDITOR WIDGET — The primary surface (prose is the product)
//!
//! The Markdown Editor is the PRIMARY SURFACE of the RoCo AI experience.
//! Prose is the product — the human authors stories, the AI assists.
//!
//! Features (per UX spec in roadmap/ux.md):
//! - Per-range comments, MS-Word style: margin annotations tied to specific text ranges
//! - Inline generate/replace with AI: select a range → generate or replace
//! - Diff view: show AI change against original at range granularity
//! - Accept-section and accept-selection: accept a whole section or specific range
//! - Built custom on `egui::TextEdit` + cursor/range mapping + `Painter` overlays
//! - Reuse `egui_markdown` for rendered/readonly preview of accepted prose
//!
//! Build principle: WIDGET STANDALONE-FIRST, THEN COMPOSE
//! This widget is built and tested in isolation before being composed into screens.

use egui::{self, Color32, Context, Layout, Rect, RichText, Stroke, TextEdit, Ui, Vec2};
use egui_extras::{Size, StripBuilder};
use serde::{Deserialize, Serialize};

/// A text range in the document (byte offsets, UTF-8)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TextRange {
    pub start: usize,
    pub end: usize,
}

impl TextRange {
    pub fn new(start: usize, end: usize) -> Self {
        Self { start, end }
    }

    pub fn len(&self) -> usize {
        self.end.saturating_sub(self.start)
    }

    pub fn is_empty(&self) -> bool {
        self.start >= self.end
    }

    pub fn contains(&self, offset: usize) -> bool {
        offset >= self.start && offset < self.end
    }
}

/// A comment anchored to a specific text range (MS-Word style margin comment)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RangeComment {
    pub id: String,
    pub range: TextRange,
    pub author: String,
    pub text: String,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub resolved: bool,
}

/// An AI-generated suggestion for a text range
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Suggestion {
    pub id: String,
    pub range: TextRange,
    pub original_text: String,
    pub suggested_text: String,
    pub kind: SuggestionKind,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub accepted: bool,
    pub rejected: bool,
}

/// Type of suggestion
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SuggestionKind {
    /// AI generated new content for an empty/selected range
    Generate,
    /// AI proposed a replacement for selected text
    Replace,
    /// AI proposed a rewrite/refinement
    Rewrite,
}

/// Action the human can take on a suggestion
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SuggestionAction {
    Accept,
    AcceptSection,
    AcceptSelection,
    Reject,
    Modify,
}

/// A diff hunk for showing changes at range granularity
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiffHunk {
    pub range: TextRange,
    pub old_text: String,
    pub new_text: String,
    pub kind: DiffKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DiffKind {
    Insert,
    Delete,
    Replace,
    Equal,
}

/// Document state for the markdown editor
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MarkdownDocument {
    /// The current text content (markdown source)
    pub text: String,
    /// Accepted/locked sections (ranges that are "published")
    #[serde(default)]
    pub accepted_ranges: Vec<TextRange>,
    /// Pending AI suggestions
    #[serde(default)]
    pub suggestions: Vec<Suggestion>,
    /// Human comments on ranges
    #[serde(default)]
    pub comments: Vec<RangeComment>,
    /// Version for undo/redo
    #[serde(default)]
    pub version: u64,
}

impl MarkdownDocument {
    pub fn new(text: String) -> Self {
        Self {
            text,
            accepted_ranges: Vec::new(),
            suggestions: Vec::new(),
            comments: Vec::new(),
            version: 0,
        }
    }

    pub fn empty() -> Self {
        Self::new(String::new())
    }

    /// Get text for a range
    pub fn range_text(&self, range: TextRange) -> &str {
        let start = range.start.min(self.text.len());
        let end = range.end.min(self.text.len());
        &self.text[start..end]
    }

    /// Find suggestions overlapping a range
    pub fn suggestions_overlapping(&self, range: TextRange) -> Vec<&Suggestion> {
        self.suggestions
            .iter()
            .filter(|s| s.range.start < range.end && s.range.end > range.start)
            .collect()
    }

    /// Find comments overlapping a range
    pub fn comments_overlapping(&self, range: TextRange) -> Vec<&RangeComment> {
        self.comments
            .iter()
            .filter(|c| c.range.start < range.end && c.range.end > range.start)
            .collect()
    }

    /// Add a suggestion
    pub fn add_suggestion(&mut self, suggestion: Suggestion) {
        self.suggestions.push(suggestion);
        self.version += 1;
    }

    /// Accept a suggestion
    pub fn accept_suggestion(&mut self, suggestion_id: &str) -> bool {
        if let Some(idx) = self.suggestions.iter().position(|s| s.id == suggestion_id) {
            let suggestion = self.suggestions.remove(idx);
            // Replace the text
            let start = suggestion.range.start.min(self.text.len());
            let end = suggestion.range.end.min(self.text.len());
            self.text
                .replace_range(start..end, &suggestion.suggested_text);
            // Mark the new range as accepted
            let new_range = TextRange::new(start, start + suggestion.suggested_text.len());
            self.accepted_ranges.push(new_range);
            self.version += 1;
            true
        } else {
            false
        }
    }

    /// Reject a suggestion
    pub fn reject_suggestion(&mut self, suggestion_id: &str) -> bool {
        if let Some(idx) = self.suggestions.iter().position(|s| s.id == suggestion_id) {
            self.suggestions.remove(idx);
            self.version += 1;
            true
        } else {
            false
        }
    }

    /// Add a comment
    pub fn add_comment(&mut self, comment: RangeComment) {
        self.comments.push(comment);
        self.version += 1;
    }

    /// Resolve a comment
    pub fn resolve_comment(&mut self, comment_id: &str) -> bool {
        if let Some(c) = self.comments.iter_mut().find(|c| c.id == comment_id) {
            c.resolved = true;
            self.version += 1;
            true
        } else {
            false
        }
    }
}

/// Editor mode
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum EditorMode {
    /// Editing markdown source
    #[default]
    Edit,
    /// Preview rendered markdown (readonly)
    Preview,
    /// Split view: edit + preview
    Split,
}

/// Selection state for the editor
#[derive(Debug, Clone, Default)]
pub struct SelectionState {
    /// Byte offset of cursor/selection start
    pub cursor: usize,
    /// Byte offset of selection end (if different from cursor)
    pub selection_end: Option<usize>,
    /// Whether we're in the middle of a drag selection
    pub selecting: bool,
}

impl SelectionState {
    pub fn range(&self) -> Option<TextRange> {
        self.selection_end.map(|end| {
            let start = self.cursor.min(end);
            let end = self.cursor.max(end);
            TextRange::new(start, end)
        })
    }

    pub fn has_selection(&self) -> bool {
        self.selection_end.is_some() && self.selection_end != Some(self.cursor)
    }
}

/// Markdown editor widget state
#[derive(Debug, Clone, Default)]
pub struct MarkdownEditorState {
    /// The document being edited
    pub document: MarkdownDocument,
    /// Current editor mode
    pub mode: EditorMode,
    /// Current selection
    pub selection: SelectionState,
    /// Scroll position for sync between edit/preview
    pub scroll_offset: f32,
    /// Whether the AI is currently generating
    pub ai_generating: bool,
    /// Pending AI action (generate/replace for selected range)
    pub pending_ai_action: Option<PendingAiAction>,
    /// Show diff view for suggestions
    pub show_diff: bool,
    /// Comment being composed
    pub composing_comment: Option<ComposingComment>,
    /// Undo stack (document versions)
    pub undo_stack: Vec<MarkdownDocument>,
    /// Redo stack
    pub redo_stack: Vec<MarkdownDocument>,
    /// Max undo history
    pub max_undo: usize,
    /// Path of the workspace file currently being edited (if any).
    /// When set, "Save" writes back to this path.
    pub file_path: Option<std::path::PathBuf>,
}

/// Pending AI action for a selected range
#[derive(Debug, Clone)]
pub struct PendingAiAction {
    pub range: TextRange,
    pub kind: SuggestionKind,
    pub prompt: String,
}

/// Comment being composed
#[derive(Debug, Clone)]
pub struct ComposingComment {
    pub range: TextRange,
    pub text: String,
}

/// Actions the markdown editor can emit
#[derive(Debug, Clone)]
pub enum MarkdownEditorAction {
    /// Text changed
    TextChanged(String),
    /// Selection changed
    SelectionChanged(SelectionState),
    /// Mode changed
    ModeChanged(EditorMode),
    /// Accept a suggestion
    AcceptSuggestion(String),
    /// Accept all suggestions in a section
    AcceptSection(TextRange),
    /// Accept specific selection
    AcceptSelection(TextRange),
    /// Reject a suggestion
    RejectSuggestion(String),
    /// Request AI generation for range
    RequestAiGenerate(TextRange, String),
    /// Request AI replacement for range
    RequestAiReplace(TextRange, String),
    /// Add a comment
    AddComment(TextRange, String),
    /// Resolve a comment
    ResolveComment(String),
    /// Toggle diff view
    ToggleDiff,
    /// Undo
    Undo,
    /// Redo
    Redo,
    /// Save version to history
    SaveVersion,
    /// Save the current document back to its workspace file (if file_path is set)
    SaveToFile,
}

/// The Markdown Editor Widget
pub struct MarkdownEditor;

impl MarkdownEditor {
    /// Create a new markdown editor widget
    pub fn new() -> Self {
        Self
    }

    /// Show the markdown editor
    ///
    /// Returns any action the human took
    pub fn show(
        &mut self,
        ui: &mut Ui,
        state: &mut MarkdownEditorState,
        ctx: &Context,
    ) -> Option<MarkdownEditorAction> {
        let mut action = None;

        // Toolbar
        self.show_toolbar(ui, state, &mut action);

        ui.separator();

        // Main editor area
        match state.mode {
            EditorMode::Edit => {
                self.show_edit_mode(ui, state, ctx, &mut action);
            }
            EditorMode::Preview => {
                self.show_preview_mode(ui, state, &mut action);
            }
            EditorMode::Split => {
                self.show_split_mode(ui, state, ctx, &mut action);
            }
        }

        action
    }

    /// Show the editor toolbar
    fn show_toolbar(
        &mut self,
        ui: &mut Ui,
        state: &mut MarkdownEditorState,
        action: &mut Option<MarkdownEditorAction>,
    ) {
        ui.horizontal(|ui| {
            // Mode selector
            ui.label("Mode:");
            egui::ComboBox::from_id_salt("editor_mode")
                .selected_text(match state.mode {
                    EditorMode::Edit => "Edit",
                    EditorMode::Preview => "Preview",
                    EditorMode::Split => "Split",
                })
                .show_ui(ui, |ui| {
                    for mode in [EditorMode::Edit, EditorMode::Preview, EditorMode::Split] {
                        if ui
                            .selectable_label(state.mode == mode, format!("{:?}", mode))
                            .clicked()
                        {
                            state.mode = mode;
                            *action = Some(MarkdownEditorAction::ModeChanged(mode));
                        }
                    }
                });

            ui.separator();

            // Diff toggle
            if ui
                .selectable_label(state.show_diff, "Show Diff")
                .on_hover_text("Toggle diff view for AI suggestions")
                .clicked()
            {
                state.show_diff = !state.show_diff;
                *action = Some(MarkdownEditorAction::ToggleDiff);
            }

            ui.separator();

            // AI actions (only when selection exists)
            if state.selection.has_selection() {
                ui.menu_button("AI Actions", |ui| {
                    if ui.button("Generate Here").clicked() {
                        if let Some(range) = state.selection.range() {
                            *action = Some(MarkdownEditorAction::RequestAiGenerate(
                                range,
                                "Continue writing from here".to_string(),
                            ));
                        }
                        ui.close_menu();
                    }
                    if ui.button("Replace Selection").clicked() {
                        if let Some(range) = state.selection.range() {
                            *action = Some(MarkdownEditorAction::RequestAiReplace(
                                range,
                                "Rewrite this section".to_string(),
                            ));
                        }
                        ui.close_menu();
                    }
                });
            }

            ui.separator();

            // Comment button
            if state.selection.has_selection()
                && ui
                    .button("💬 Comment")
                    .on_hover_text("Add comment to selection")
                    .clicked()
            {
                if let Some(range) = state.selection.range() {
                    state.composing_comment = Some(ComposingComment {
                        range,
                        text: String::new(),
                    });
                }
            }

            ui.separator();

            // Undo/Redo
            if ui.button("↶ Undo").on_hover_text("Undo (Ctrl+Z)").clicked() {
                *action = Some(MarkdownEditorAction::Undo);
            }
            if ui.button("↷ Redo").on_hover_text("Redo (Ctrl+Y)").clicked() {
                *action = Some(MarkdownEditorAction::Redo);
            }

            if state.file_path.is_some() {
                ui.separator();
                if ui
                    .button("💾 Save")
                    .on_hover_text("Save to workspace file")
                    .clicked()
                {
                    *action = Some(MarkdownEditorAction::SaveToFile);
                }
            }

            ui.with_layout(Layout::right_to_left(egui::Align::Center), |ui| {
                // Stats
                let word_count = state.document.text.split_whitespace().count();
                ui.label(RichText::new(format!("{} words", word_count)).small());
            });
        });
    }

    /// Show edit mode (markdown source editing)
    fn show_edit_mode(
        &mut self,
        ui: &mut Ui,
        state: &mut MarkdownEditorState,
        _ctx: &Context,
        action: &mut Option<MarkdownEditorAction>,
    ) {
        // Check for keyboard shortcuts
        self.handle_shortcuts(ui, state, action);

        // Main text edit area
        let text_edit = TextEdit::multiline(&mut state.document.text)
            .font(egui::FontId::monospace(14.0))
            .desired_rows(40)
            .desired_width(f32::INFINITY)
            .interactive(true)
            .code_editor();

        let response = ui.add(text_edit);

        // Handle text changes
        if response.changed() {
            *action = Some(MarkdownEditorAction::TextChanged(
                state.document.text.clone(),
            ));
            state.document.version += 1;
        }

        // Handle selection tracking (egui doesn't expose selection directly,
        // so we approximate from cursor position)
        if response.has_focus() {
            // Note: In a real implementation, we'd track cursor/selection more precisely
            // using egui's internal state or a custom text edit widget
        }

        // Overlay: suggestions, comments, diff highlights
        self.paint_overlays(ui, state, &response.rect);

        // Comment composer - take out to avoid borrow conflict
        let composing = state.composing_comment.take();
        if let Some(mut composing) = composing {
            self.show_comment_composer(ui, &mut composing, action, &mut state.document);
            // If text was cleared, it means Post or Cancel was clicked
            if !composing.text.is_empty() {
                state.composing_comment = Some(composing);
            }
        }
    }

    /// Show preview mode (rendered markdown)
    fn show_preview_mode(
        &mut self,
        ui: &mut Ui,
        state: &mut MarkdownEditorState,
        _action: &mut Option<MarkdownEditorAction>,
    ) {
        // Render markdown to rich egui widgets.
        // Uses a built-in lightweight renderer (no external dependency) that
        // handles headings, bold/italic, inline code, blockquotes, and lists.
        egui::ScrollArea::vertical().show(ui, |ui| {
            let text = &state.document.text;
            if text.trim().is_empty() {
                ui.label(
                    RichText::new("\u{1f4dd} No content to preview.")
                        .size(14.0)
                        .color(ui.visuals().weak_text_color()),
                );
            } else {
                render_markdown_preview(ui, text);
            }
        });
    }

    /// Show split mode (edit + preview side by side)
    fn show_split_mode(
        &mut self,
        ui: &mut Ui,
        state: &mut MarkdownEditorState,
        ctx: &Context,
        action: &mut Option<MarkdownEditorAction>,
    ) {
        StripBuilder::new(ui)
            .size(Size::remainder().at_least(300.0))
            .size(Size::remainder().at_least(300.0))
            .horizontal(|mut strip| {
                // Left: Edit
                strip.cell(|ui| {
                    ui.label(RichText::new("Edit").strong());
                    self.show_edit_mode(ui, state, ctx, action);
                });
                // Right: Preview
                strip.cell(|ui| {
                    ui.label(RichText::new("Preview").strong());
                    self.show_preview_mode(ui, state, action);
                });
            });
    }

    /// Paint overlay decorations (suggestions, comments, diff highlights)
    fn paint_overlays(&mut self, ui: &mut Ui, state: &mut MarkdownEditorState, text_rect: &Rect) {
        // Paint accepted ranges (subtle background)
        for range in &state.document.accepted_ranges {
            if let Some(range_rect) = self.range_to_rect(ui, state, *range) {
                ui.painter().rect_filled(
                    range_rect.expand(2.0),
                    2.0,
                    Color32::from_rgba_premultiplied(0, 200, 0, 30),
                );
            }
        }

        // Paint suggestions
        // Collect suggestions first to avoid borrow conflicts
        let suggestions: Vec<_> = state
            .document
            .suggestions
            .iter()
            .filter(|s| !s.accepted && !s.rejected)
            .cloned()
            .collect();
        for suggestion in suggestions {
            if let Some(range_rect) = self.range_to_rect(ui, state, suggestion.range) {
                let color = match suggestion.kind {
                    SuggestionKind::Generate => Color32::from_rgba_premultiplied(0, 150, 255, 180),
                    SuggestionKind::Replace => Color32::from_rgba_premultiplied(255, 150, 0, 180),
                    SuggestionKind::Rewrite => Color32::from_rgba_premultiplied(150, 0, 255, 180),
                };

                // Highlight the range
                ui.painter()
                    .rect_filled(range_rect.expand(2.0), 2.0, color.gamma_multiply(0.2));
                ui.painter().rect_stroke(
                    range_rect.expand(2.0),
                    2.0,
                    Stroke::new(2.0, color),
                    egui::StrokeKind::Inside,
                );

                // Show action buttons on hover
                if range_rect.contains(ui.ctx().pointer_interact_pos().unwrap_or_default()) {
                    self.paint_suggestion_actions(ui, range_rect, &suggestion);
                }

                // Diff view
                if state.show_diff {
                    self.paint_diff(ui, range_rect, &suggestion);
                }
            }
        }

        // Paint comments
        for comment in &state.document.comments {
            if let Some(range_rect) = self.range_to_rect(ui, state, comment.range) {
                // Comment indicator in margin
                let margin_rect = Rect::from_min_max(
                    text_rect.left_top() - Vec2::new(40.0, 0.0),
                    text_rect.left_top() + Vec2::new(30.0, range_rect.height()),
                );
                ui.painter()
                    .rect_filled(margin_rect, 2.0, Color32::from_rgb(255, 200, 0));
                ui.painter().text(
                    margin_rect.center(),
                    egui::Align2::CENTER_CENTER,
                    "💬",
                    egui::FontId::proportional(12.0),
                    Color32::BLACK,
                );

                // Comment bubble on hover
                if margin_rect.contains(ui.ctx().pointer_interact_pos().unwrap_or_default()) {
                    self.paint_comment_bubble(ui, margin_rect, comment);
                }
            }
        }
    }

    /// Convert a text range to a screen rectangle (approximate)
    fn range_to_rect(
        &self,
        ui: &mut Ui,
        state: &MarkdownEditorState,
        range: TextRange,
    ) -> Option<Rect> {
        // This is a simplified approximation. In a real implementation,
        // we'd need to measure text layout to get precise positions.
        // For now, we use a rough character-to-pixel mapping.
        let line_height = 20.0;
        let char_width = 8.5;

        // Count newlines before start/end to estimate line numbers
        let text = &state.document.text;
        let start_line = text[..range.start.min(text.len())].matches('\n').count();
        let end_line = text[..range.end.min(text.len())].matches('\n').count();

        let start_col = if start_line == 0 {
            range.start
        } else {
            let last_newline = text[..range.start.min(text.len())].rfind('\n').unwrap_or(0);
            range.start - last_newline - 1
        };

        let end_col = if end_line == 0 {
            range.end
        } else {
            let last_newline = text[..range.end.min(text.len())].rfind('\n').unwrap_or(0);
            range.end - last_newline - 1
        };

        // Get the text edit rect
        let text_rect = ui.min_rect();

        Some(Rect::from_min_max(
            text_rect.left_top()
                + Vec2::new(
                    start_col as f32 * char_width,
                    start_line as f32 * line_height,
                ),
            text_rect.left_top()
                + Vec2::new(
                    end_col as f32 * char_width,
                    (end_line + 1) as f32 * line_height,
                ),
        ))
    }

    /// Paint suggestion action buttons
    fn paint_suggestion_actions(
        &mut self,
        ui: &mut Ui,
        range_rect: Rect,
        _suggestion: &Suggestion,
    ) {
        let button_height = 24.0;
        let button_width = 70.0;
        let spacing = 4.0;
        let start_x = range_rect.right() + 8.0;
        let y = range_rect.top();

        // Accept button
        let accept_rect = Rect::from_min_size(
            egui::pos2(start_x, y),
            Vec2::new(button_width, button_height),
        );
        if ui.put(accept_rect, egui::Button::new("Accept")).clicked() {
            // Action will be handled by the main loop
        }
        ui.painter()
            .rect_filled(accept_rect, 4.0, Color32::from_rgb(0, 180, 0));
        ui.painter().text(
            accept_rect.center(),
            egui::Align2::CENTER_CENTER,
            "Accept",
            egui::FontId::proportional(11.0),
            Color32::WHITE,
        );

        // Reject button
        let reject_rect = Rect::from_min_size(
            egui::pos2(start_x, y + button_height + spacing),
            Vec2::new(button_width, button_height),
        );
        ui.painter()
            .rect_filled(reject_rect, 4.0, Color32::from_rgb(180, 0, 0));
        ui.painter().text(
            reject_rect.center(),
            egui::Align2::CENTER_CENTER,
            "Reject",
            egui::FontId::proportional(11.0),
            Color32::WHITE,
        );
    }

    /// Paint diff view for a suggestion
    fn paint_diff(&mut self, ui: &mut Ui, range_rect: Rect, suggestion: &Suggestion) {
        // Show original vs suggested side by side below the range
        let diff_y = range_rect.bottom() + 4.0;
        let diff_width = range_rect.width().max(300.0);

        // Original text (strikethrough)
        let old_rect = Rect::from_min_size(
            egui::pos2(range_rect.left(), diff_y),
            Vec2::new(diff_width, 40.0),
        );
        ui.painter().rect_filled(
            old_rect,
            4.0,
            Color32::from_rgba_premultiplied(255, 0, 0, 50),
        );
        ui.painter().text(
            old_rect.left_top() + Vec2::new(8.0, 8.0),
            egui::Align2::LEFT_TOP,
            format!(
                "− {}",
                suggestion
                    .original_text
                    .chars()
                    .take(80)
                    .collect::<String>()
            ),
            egui::FontId::monospace(11.0),
            Color32::from_rgb(200, 0, 0),
        );

        // New text (green)
        let new_rect = Rect::from_min_size(
            egui::pos2(range_rect.left(), diff_y + 44.0),
            Vec2::new(diff_width, 40.0),
        );
        ui.painter().rect_filled(
            new_rect,
            4.0,
            Color32::from_rgba_premultiplied(0, 255, 0, 50),
        );
        ui.painter().text(
            new_rect.left_top() + Vec2::new(8.0, 8.0),
            egui::Align2::LEFT_TOP,
            format!(
                "+ {}",
                suggestion
                    .suggested_text
                    .chars()
                    .take(80)
                    .collect::<String>()
            ),
            egui::FontId::monospace(11.0),
            Color32::from_rgb(0, 180, 0),
        );
    }

    /// Paint comment bubble
    fn paint_comment_bubble(&mut self, ui: &mut Ui, anchor_rect: Rect, comment: &RangeComment) {
        let bubble_width = 250.0;
        let bubble_rect = Rect::from_min_size(
            anchor_rect.right_top() + Vec2::new(8.0, -20.0),
            Vec2::new(bubble_width, 120.0),
        );

        // Bubble background
        ui.painter()
            .rect_filled(bubble_rect, 8.0, Color32::from_rgb(255, 250, 220));
        ui.painter().rect_stroke(
            bubble_rect,
            8.0,
            Stroke::new(1.0, Color32::from_rgb(200, 180, 0)),
            egui::StrokeKind::Inside,
        );

        // Comment text
        ui.painter().text(
            bubble_rect.left_top() + Vec2::new(12.0, 12.0),
            egui::Align2::LEFT_TOP,
            &comment.text,
            egui::FontId::proportional(12.0),
            Color32::BLACK,
        );

        // Author and time
        let time_str = comment.timestamp.format("%H:%M").to_string();
        ui.painter().text(
            bubble_rect.left_bottom() - Vec2::new(12.0, 12.0),
            egui::Align2::LEFT_BOTTOM,
            format!("— {} @ {}", comment.author, time_str),
            egui::FontId::proportional(10.0),
            Color32::GRAY,
        );
    }

    /// Show comment composer UI
    fn show_comment_composer(
        &mut self,
        ui: &mut Ui,
        composing: &mut ComposingComment,
        action: &mut Option<MarkdownEditorAction>,
        document: &mut MarkdownDocument,
    ) {
        ui.group(|ui| {
            ui.label(RichText::new("Add Comment").strong());
            ui.text_edit_multiline(&mut composing.text);

            ui.horizontal(|ui| {
                if ui.button("Post").clicked() && !composing.text.trim().is_empty() {
                    let comment = RangeComment {
                        id: uuid::Uuid::new_v4().to_string(),
                        range: composing.range,
                        author: "Human".to_string(), // Would come from auth
                        text: composing.text.clone(),
                        timestamp: chrono::Utc::now(),
                        resolved: false,
                    };
                    document.add_comment(comment);
                    *action = Some(MarkdownEditorAction::SaveVersion);
                    // Clear the composing comment by returning a flag
                    composing.text.clear();
                }
                if ui.button("Cancel").clicked() {
                    composing.text.clear();
                }
            });
        });
    }

    /// Handle keyboard shortcuts
    fn handle_shortcuts(
        &mut self,
        ui: &mut Ui,
        _state: &mut MarkdownEditorState,
        action: &mut Option<MarkdownEditorAction>,
    ) {
        let ctx = ui.ctx();

        // Ctrl+Z / Cmd+Z = Undo
        if ctx.input(|i| i.modifiers.ctrl && i.key_pressed(egui::Key::Z)) {
            if !ctx.input(|i| i.modifiers.shift) {
                *action = Some(MarkdownEditorAction::Undo);
            } else {
                *action = Some(MarkdownEditorAction::Redo);
            }
        }

        // Ctrl+Y / Cmd+Y = Redo
        if ctx.input(|i| i.modifiers.ctrl && i.key_pressed(egui::Key::Y)) {
            *action = Some(MarkdownEditorAction::Redo);
        }

        // Ctrl+S = Save version
        if ctx.input(|i| i.modifiers.ctrl && i.key_pressed(egui::Key::S)) {
            *action = Some(MarkdownEditorAction::SaveVersion);
        }
    }
}

/// Apply an action to the editor state
pub fn apply_editor_action(state: &mut MarkdownEditorState, action: MarkdownEditorAction) {
    match action {
        MarkdownEditorAction::TextChanged(new_text) => {
            // Push current state to undo stack before changing
            if state.document.text != new_text {
                state.undo_stack.push(state.document.clone());
                if state.undo_stack.len() > state.max_undo {
                    state.undo_stack.remove(0);
                }
                state.redo_stack.clear();
                state.document.text = new_text;
            }
        }
        MarkdownEditorAction::SelectionChanged(sel) => {
            state.selection = sel;
        }
        MarkdownEditorAction::ModeChanged(mode) => {
            state.mode = mode;
        }
        MarkdownEditorAction::AcceptSuggestion(id) => {
            state.document.accept_suggestion(&id);
        }
        MarkdownEditorAction::AcceptSection(range) => {
            // Accept all suggestions overlapping the range
            let suggestions: Vec<String> = state
                .document
                .suggestions_overlapping(range)
                .into_iter()
                .filter(|s| !s.accepted && !s.rejected)
                .map(|s| s.id.clone())
                .collect();
            for id in suggestions {
                state.document.accept_suggestion(&id);
            }
        }
        MarkdownEditorAction::AcceptSelection(range) => {
            // Accept suggestions exactly matching the selection
            let suggestions: Vec<String> = state
                .document
                .suggestions_overlapping(range)
                .into_iter()
                .filter(|s| !s.accepted && !s.rejected && s.range == range)
                .map(|s| s.id.clone())
                .collect();
            for id in suggestions {
                state.document.accept_suggestion(&id);
            }
        }
        MarkdownEditorAction::RejectSuggestion(id) => {
            state.document.reject_suggestion(&id);
        }
        MarkdownEditorAction::RequestAiGenerate(range, prompt) => {
            state.pending_ai_action = Some(PendingAiAction {
                range,
                kind: SuggestionKind::Generate,
                prompt,
            });
            state.ai_generating = true;
        }
        MarkdownEditorAction::RequestAiReplace(range, prompt) => {
            state.pending_ai_action = Some(PendingAiAction {
                range,
                kind: SuggestionKind::Replace,
                prompt,
            });
            state.ai_generating = true;
        }
        MarkdownEditorAction::AddComment(range, text) => {
            let comment = RangeComment {
                id: uuid::Uuid::new_v4().to_string(),
                range,
                author: "Human".to_string(),
                text,
                timestamp: chrono::Utc::now(),
                resolved: false,
            };
            state.document.add_comment(comment);
        }
        MarkdownEditorAction::ResolveComment(id) => {
            state.document.resolve_comment(&id);
        }
        MarkdownEditorAction::ToggleDiff => {
            state.show_diff = !state.show_diff;
        }
        MarkdownEditorAction::Undo => {
            if let Some(prev_doc) = state.undo_stack.pop() {
                state.redo_stack.push(state.document.clone());
                state.document = prev_doc;
            }
        }
        MarkdownEditorAction::Redo => {
            if let Some(next_doc) = state.redo_stack.pop() {
                state.undo_stack.push(state.document.clone());
                state.document = next_doc;
            }
        }
        MarkdownEditorAction::SaveVersion => {
            state.undo_stack.push(state.document.clone());
            if state.undo_stack.len() > state.max_undo {
                state.undo_stack.remove(0);
            }
            state.redo_stack.clear();
        }
        MarkdownEditorAction::SaveToFile => {
            if let Some(ref path) = state.file_path.clone() {
                if let Err(e) = std::fs::write(path, &state.document.text) {
                    // Log the error — the caller can check the status message.
                    eprintln!("Failed to save editor file {:?}: {e}", path);
                }
            }
        }
    }
}

impl Default for MarkdownEditor {
    fn default() -> Self {
        Self::new()
    }
}

/// Lightweight markdown → egui renderer for read-only preview.
///
/// Handles the most common markdown constructs: ATX headings (`#`..`######`),
/// bold (`**..**`), italic (`*..*` / `_.._`), inline code (`` `..` ``),
/// blockquotes (`>`), unordered lists (`-` / `*`), and paragraphs.
/// Tables and images are displayed as raw text + icon.
/// This keeps the dependency footprint small (no external markdown crate needed).
pub fn render_markdown_preview(ui: &mut Ui, text: &str) {
    for line in text.lines() {
        let trimmed = line.trim();

        if trimmed.is_empty() {
            ui.add_space(4.0);
            continue;
        }

        // Headings
        if let Some(content) = trimmed.strip_prefix("###### ") {
            ui.label(RichText::new(inline_markdown(content)).size(12.0).strong());
        } else if let Some(content) = trimmed.strip_prefix("##### ") {
            ui.label(RichText::new(inline_markdown(content)).size(12.5).strong());
        } else if let Some(content) = trimmed.strip_prefix("#### ") {
            ui.label(RichText::new(inline_markdown(content)).size(13.0).strong());
        } else if let Some(content) = trimmed.strip_prefix("### ") {
            ui.label(RichText::new(inline_markdown(content)).size(14.0).strong());
        } else if let Some(content) = trimmed.strip_prefix("## ") {
            ui.label(RichText::new(inline_markdown(content)).size(16.0).strong());
        } else if let Some(content) = trimmed.strip_prefix("# ") {
            ui.label(RichText::new(inline_markdown(content)).size(20.0).strong());
        }
        // Blockquote
        else if let Some(content) = trimmed.strip_prefix("> ") {
            ui.horizontal(|ui| {
                ui.add_space(8.0);
                ui.label(
                    RichText::new(inline_markdown(content))
                        .size(13.0)
                        .color(egui::Color32::GRAY),
                );
            });
        }
        // Unordered list
        else if trimmed.starts_with("- ") || trimmed.starts_with("* ") {
            let content = &trimmed[2..];
            ui.horizontal(|ui| {
                ui.label(RichText::new("  \u{2022}").size(13.0));
                ui.label(RichText::new(inline_markdown(content)).size(13.0));
            });
        }
        // Horizontal rule
        else if trimmed == "---" || trimmed == "***" || trimmed == "___" {
            ui.separator();
        }
        // Regular paragraph
        else {
            ui.label(RichText::new(inline_markdown(trimmed)).size(13.0));
        }
    }
}

/// Parse inline markdown formatting within a line.
/// Handles **bold**, *italic*, `code`, and [links](url).
fn inline_markdown(s: &str) -> String {
    // Simple strikethrough of markdown syntax — keeps the content but
    // removes the formatting markers. A full implementation would paint
    // styled spans; for now this is a parse-and-keep-readable approach.
    let mut result = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();

    while let Some(c) = chars.next() {
        match c {
            '*' => {
                // Check for **bold** or *italic*
                if chars.peek() == Some(&'*') {
                    chars.next(); // skip second *
                                  // Read until **
                    let mut content = String::new();
                    while let Some(nc) = chars.next() {
                        if nc == '*' && chars.peek() == Some(&'*') {
                            chars.next(); // skip closing **
                                          // Wrap in bold markers (simplified: use unicode bold hints)
                            content.insert(0, '\u{1f9b0}');
                            content.push('\u{1f9b0}');
                            result.push_str(&content);
                            break;
                        }
                        content.push(nc);
                    }
                } else {
                    // Single * = italic
                    let mut content = String::new();
                    for nc in chars.by_ref() {
                        if nc == '*' {
                            content.insert(0, '\u{1f44d}');
                            result.push_str(&content);
                            break;
                        }
                        content.push(nc);
                    }
                }
            }
            '`' => {
                // Inline code — just keep content
                let mut content = String::new();
                for nc in chars.by_ref() {
                    if nc == '`' {
                        result.push_str(&format!("\u{1f4bb}{}", content));
                        break;
                    }
                    content.push(nc);
                }
                if content.is_empty() {
                    result.push('`');
                }
            }
            '[' => {
                // Link: [text](url)
                let mut link_text = String::new();
                for nc in chars.by_ref() {
                    if nc == ']' {
                        break;
                    }
                    link_text.push(nc);
                }
                if chars.next() == Some('(') {
                    let mut url = String::new();
                    for nc in chars.by_ref() {
                        if nc == ')' {
                            break;
                        }
                        url.push(nc);
                    }
                    result.push_str(&format!("{link_text} [{url}]"));
                } else {
                    result.push_str(&link_text);
                }
            }
            _ => result.push(c),
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_text_range() {
        let range = TextRange::new(0, 10);
        assert_eq!(range.len(), 10);
        assert!(!range.is_empty());
        assert!(range.contains(5));
        assert!(!range.contains(10));
        assert!(!range.contains(15));
    }

    #[test]
    fn test_text_range_empty() {
        let range = TextRange::new(5, 5);
        assert_eq!(range.len(), 0);
        assert!(range.is_empty());
    }

    #[test]
    fn test_markdown_document_new() {
        let doc = MarkdownDocument::new("Hello world".to_string());
        assert_eq!(doc.text, "Hello world");
        assert_eq!(doc.version, 0);
    }

    #[test]
    fn test_markdown_document_range_text() {
        let doc = MarkdownDocument::new("Hello world".to_string());
        assert_eq!(doc.range_text(TextRange::new(0, 5)), "Hello");
        assert_eq!(doc.range_text(TextRange::new(6, 11)), "world");
    }

    #[test]
    fn test_markdown_document_add_suggestion() {
        let mut doc = MarkdownDocument::new("Hello world".to_string());
        let suggestion = Suggestion {
            id: "s1".to_string(),
            range: TextRange::new(6, 11),
            original_text: "world".to_string(),
            suggested_text: "there".to_string(),
            kind: SuggestionKind::Replace,
            timestamp: chrono::Utc::now(),
            accepted: false,
            rejected: false,
        };
        doc.add_suggestion(suggestion);
        assert_eq!(doc.suggestions.len(), 1);
        assert_eq!(doc.version, 1);
    }

    #[test]
    fn test_markdown_document_accept_suggestion() {
        let mut doc = MarkdownDocument::new("Hello world".to_string());
        let suggestion = Suggestion {
            id: "s1".to_string(),
            range: TextRange::new(6, 11),
            original_text: "world".to_string(),
            suggested_text: "there".to_string(),
            kind: SuggestionKind::Replace,
            timestamp: chrono::Utc::now(),
            accepted: false,
            rejected: false,
        };
        doc.add_suggestion(suggestion);

        assert!(doc.accept_suggestion("s1"));
        assert_eq!(doc.text, "Hello there");
        assert_eq!(doc.suggestions.len(), 0);
        assert_eq!(doc.accepted_ranges.len(), 1);
        assert_eq!(doc.accepted_ranges[0], TextRange::new(6, 11));
    }

    #[test]
    fn test_markdown_document_reject_suggestion() {
        let mut doc = MarkdownDocument::new("Hello world".to_string());
        let suggestion = Suggestion {
            id: "s1".to_string(),
            range: TextRange::new(6, 11),
            original_text: "world".to_string(),
            suggested_text: "there".to_string(),
            kind: SuggestionKind::Replace,
            timestamp: chrono::Utc::now(),
            accepted: false,
            rejected: false,
        };
        doc.add_suggestion(suggestion);

        assert!(doc.reject_suggestion("s1"));
        assert_eq!(doc.text, "Hello world"); // unchanged
        assert_eq!(doc.suggestions.len(), 0);
    }

    #[test]
    fn test_markdown_document_comments() {
        let mut doc = MarkdownDocument::new("Hello world".to_string());
        let comment = RangeComment {
            id: "c1".to_string(),
            range: TextRange::new(0, 5),
            author: "Alice".to_string(),
            text: "Great opening!".to_string(),
            timestamp: chrono::Utc::now(),
            resolved: false,
        };
        doc.add_comment(comment);
        assert_eq!(doc.comments.len(), 1);
        assert_eq!(doc.version, 1);

        let overlapping = doc.comments_overlapping(TextRange::new(2, 3));
        assert_eq!(overlapping.len(), 1);

        let non_overlapping = doc.comments_overlapping(TextRange::new(6, 11));
        assert_eq!(non_overlapping.len(), 0);

        assert!(doc.resolve_comment("c1"));
        assert!(doc.comments[0].resolved);
    }

    #[test]
    fn test_selection_state() {
        let mut sel = SelectionState::default();
        sel.cursor = 10;
        sel.selection_end = Some(20);
        assert!(sel.has_selection());
        assert_eq!(sel.range(), Some(TextRange::new(10, 20)));

        sel.selection_end = Some(10);
        assert!(!sel.has_selection());
    }

    #[test]
    fn test_apply_editor_action_text_changed() {
        let mut state = MarkdownEditorState {
            document: MarkdownDocument::new("Original".to_string()),
            max_undo: 10,
            ..Default::default()
        };

        apply_editor_action(
            &mut state,
            MarkdownEditorAction::TextChanged("Modified".to_string()),
        );
        assert_eq!(state.document.text, "Modified");
        assert_eq!(state.undo_stack.len(), 1);
        assert_eq!(state.undo_stack[0].text, "Original");
    }

    #[test]
    fn test_apply_editor_action_undo_redo() {
        let mut state = MarkdownEditorState {
            document: MarkdownDocument::new("Original".to_string()),
            max_undo: 10,
            ..Default::default()
        };

        apply_editor_action(
            &mut state,
            MarkdownEditorAction::TextChanged("First".to_string()),
        );
        apply_editor_action(
            &mut state,
            MarkdownEditorAction::TextChanged("Second".to_string()),
        );
        assert_eq!(state.document.text, "Second");
        assert_eq!(state.undo_stack.len(), 2);

        apply_editor_action(&mut state, MarkdownEditorAction::Undo);
        assert_eq!(state.document.text, "First");
        assert_eq!(state.redo_stack.len(), 1);

        apply_editor_action(&mut state, MarkdownEditorAction::Undo);
        assert_eq!(state.document.text, "Original");
        assert_eq!(state.undo_stack.len(), 0);

        apply_editor_action(&mut state, MarkdownEditorAction::Redo);
        assert_eq!(state.document.text, "First");
    }

    #[test]
    fn test_apply_editor_action_accept_suggestion() {
        let mut state = MarkdownEditorState {
            document: MarkdownDocument::new("Hello world".to_string()),
            ..Default::default()
        };
        state.document.add_suggestion(Suggestion {
            id: "s1".to_string(),
            range: TextRange::new(6, 11),
            original_text: "world".to_string(),
            suggested_text: "there".to_string(),
            kind: SuggestionKind::Replace,
            timestamp: chrono::Utc::now(),
            accepted: false,
            rejected: false,
        });

        apply_editor_action(
            &mut state,
            MarkdownEditorAction::AcceptSuggestion("s1".to_string()),
        );
        assert_eq!(state.document.text, "Hello there");
    }

    #[test]
    fn test_suggestion_kind() {
        assert_eq!(SuggestionKind::Generate as u8, 0);
        assert_eq!(SuggestionKind::Replace as u8, 1);
        assert_eq!(SuggestionKind::Rewrite as u8, 2);
    }
}
