//! Change Timeline — version control history viewer.
//!
//! Uses the existing `roco_agent::reversibility::VersionControl` engine API.
//! Displays a timeline of snapshots and actions with undo/redo controls.

use egui::{self, Color32, RichText, Ui, Vec2};

/// A timeline entry type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimelineEntryKind {
    Snapshot,
    Action,
    Undo,
    Redo,
    Rollback,
    Checkpoint,
}

impl TimelineEntryKind {
    pub fn label(self) -> &'static str {
        match self {
            TimelineEntryKind::Snapshot => "Snapshot",
            TimelineEntryKind::Action => "Action",
            TimelineEntryKind::Undo => "Undo",
            TimelineEntryKind::Redo => "Redo",
            TimelineEntryKind::Rollback => "Rollback",
            TimelineEntryKind::Checkpoint => "Checkpoint",
        }
    }

    pub fn icon(self) -> &'static str {
        match self {
            TimelineEntryKind::Snapshot => "📸",
            TimelineEntryKind::Action => "✏️",
            TimelineEntryKind::Undo => "↩️",
            TimelineEntryKind::Redo => "↪️",
            TimelineEntryKind::Rollback => "⏪",
            TimelineEntryKind::Checkpoint => "🏁",
        }
    }

    pub fn color(self) -> Color32 {
        match self {
            TimelineEntryKind::Snapshot => Color32::from_rgb(100, 200, 255),
            TimelineEntryKind::Action => Color32::from_rgb(100, 255, 100),
            TimelineEntryKind::Undo => Color32::from_rgb(255, 180, 50),
            TimelineEntryKind::Redo => Color32::from_rgb(50, 200, 255),
            TimelineEntryKind::Rollback => Color32::from_rgb(255, 100, 100),
            TimelineEntryKind::Checkpoint => Color32::from_rgb(200, 100, 255),
        }
    }
}

/// A single entry in the timeline
#[derive(Debug, Clone)]
pub struct TimelineEntry {
    pub id: String,
    pub description: String,
    pub kind: TimelineEntryKind,
    pub timestamp: String,
    pub is_current: bool,
}

/// Actions from the timeline
#[derive(Debug, Clone)]
pub enum TimelineAction {
    Undo,
    Redo,
    Rollback(String),
    CreateSnapshot(String),
    SelectEntry(usize),
}

/// Timeline widget state
#[derive(Debug, Clone)]
pub struct ChangeTimelineState {
    pub entries: Vec<TimelineEntry>,
    pub selected_index: Option<usize>,
    pub current_position: usize,
    pub max_undo: usize,
    pub snapshot_description: String,
    pub show_snapshot_input: bool,
}

impl Default for ChangeTimelineState {
    fn default() -> Self {
        Self {
            entries: Vec::new(),
            selected_index: None,
            current_position: 0,
            max_undo: 50,
            snapshot_description: String::new(),
            show_snapshot_input: false,
        }
    }
}

impl ChangeTimelineState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_entry(&mut self, entry: TimelineEntry) {
        self.entries.push(entry);
        self.current_position = self.entries.len();
    }

    pub fn clear(&mut self) {
        self.entries.clear();
        self.selected_index = None;
        self.current_position = 0;
    }

    pub fn can_undo(&self) -> bool {
        self.current_position > 0
    }

    pub fn can_redo(&self) -> bool {
        self.current_position < self.entries.len()
    }
}

/// Change timeline widget
pub struct ChangeTimeline;

impl ChangeTimeline {
    /// Show the timeline panel
    pub fn show(ui: &mut Ui, state: &mut ChangeTimelineState) -> Option<TimelineAction> {
        let mut action = None;

        ui.vertical(|ui| {
            // Header
            ui.horizontal(|ui| {
                ui.label(RichText::new("History").strong().size(14.0));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(RichText::new(format!("{} entries", state.entries.len())).size(10.0).color(ui.visuals().weak_text_color()));
                });
            });

            // Undo/Redo buttons
            ui.horizontal(|ui| {
                let can_undo = state.can_undo();
                let can_redo = state.can_redo();
                if ui.add_enabled(can_undo, egui::Button::new("↩ Undo")).clicked() {
                    action = Some(TimelineAction::Undo);
                }
                if ui.add_enabled(can_redo, egui::Button::new("↪ Redo")).clicked() {
                    action = Some(TimelineAction::Redo);
                }
                if ui.button("📸 Snapshot").clicked() {
                    state.show_snapshot_input = !state.show_snapshot_input;
                }
            });

            // Snapshot input
            if state.show_snapshot_input {
                ui.horizontal(|ui| {
                    ui.add(egui::TextEdit::singleline(&mut state.snapshot_description)
                        .hint_text("Snapshot description...")
                        .desired_width(180.0));
                    if ui.button("Save").clicked() {
                        let desc = state.snapshot_description.trim().to_string();
                        let label = if desc.is_empty() { "Manual snapshot".into() } else { desc };
                        action = Some(TimelineAction::CreateSnapshot(label));
                        state.snapshot_description.clear();
                        state.show_snapshot_input = false;
                    }
                });
            }

            ui.separator();

            // Timeline entries
            if state.entries.is_empty() {
                ui.label(RichText::new("No history yet.").size(12.0).color(ui.visuals().weak_text_color()));
                return;
            }

            let entry_height = 36.0;
            let total_height = (state.entries.len() as f32 * entry_height).min(400.0);
            let timeline_left = 20.0;

            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .max_height(total_height)
                .show(ui, |ui| {
                    // Draw timeline line
                    let painter = ui.painter();
                    let line_x = ui.min_rect().left() + timeline_left;
                    let line_top = ui.min_rect().top();
                    let line_bottom = ui.min_rect().top() + total_height;
                    painter.vline(line_x, line_top..=line_bottom, egui::Stroke::new(1.5, ui.visuals().weak_text_color()));

                    for (i, entry) in state.entries.iter().enumerate() {
                        let selected = state.selected_index == Some(i);
                        let is_past = i < state.current_position;
                        let is_future = i >= state.current_position;

                        let bg = if selected {
                            ui.visuals().selection.bg_fill
                        } else if is_future {
                            Color32::from_rgba_premultiplied(100, 100, 100, 30)
                        } else {
                            ui.visuals().faint_bg_color
                        };

                        let response = egui::Frame::none()
                            .fill(bg)
                            .show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    ui.add_space(4.0);

                                    // Timeline dot — draw via painter
                                    let dot_color = if is_current(i, state.current_position) {
                                        Color32::from_rgb(255, 200, 50)
                                    } else if is_future {
                                        ui.visuals().weak_text_color()
                                    } else {
                                        entry.kind.color()
                                    };
                                    let (dot_response, dot_painter) = ui.allocate_painter(Vec2::new(12.0, 12.0), egui::Sense::click());
                                    let dot_center = dot_response.rect.center();
                                    dot_painter.circle_filled(dot_center, 5.0, dot_color);
                                    ui.add_space(8.0);

                                    // Icon + label
                                    ui.label(RichText::new(entry.kind.icon()).size(14.0));
                                    ui.vertical(|ui| {
                                        ui.label(RichText::new(&entry.description).size(12.0).strong().color(
                                            if is_current(i, state.current_position) {
                                                Color32::from_rgb(255, 200, 50)
                                            } else {
                                                ui.visuals().text_color()
                                            }
                                        ));
                                        ui.label(RichText::new(format!("{} — {}", entry.kind.label(), entry.timestamp))
                                            .size(10.0).color(ui.visuals().weak_text_color()));
                                    });
                                });
                            })
                            .response
                            .on_hover_text(format!("{}: {}", entry.kind.label(), entry.description));

                        if response.clicked() {
                            state.selected_index = Some(i);
                            action = Some(TimelineAction::SelectEntry(i));
                        }

                        // Context menu for rollback
                        if is_past {
                            response.context_menu(|ui| {
                                if ui.button("Rollback to here").clicked() {
                                    action = Some(TimelineAction::Rollback(entry.id.clone()));
                                    ui.close_menu();
                                }
                            });
                        }

                        ui.add_space(2.0);
                    }
                });
        });

        action
    }

    /// Compact version
    pub fn show_compact(ui: &mut Ui, state: &mut ChangeTimelineState) -> Option<TimelineAction> {
        let mut action = None;
        ui.horizontal(|ui| {
            if ui.add_enabled(state.can_undo(), egui::Button::new("↩")).on_hover_text("Undo").clicked() {
                action = Some(TimelineAction::Undo);
            }
            if ui.add_enabled(state.can_redo(), egui::Button::new("↪")).on_hover_text("Redo").clicked() {
                action = Some(TimelineAction::Redo);
            }
            ui.label(RichText::new(format!("{}/{}", state.current_position, state.entries.len())).size(11.0).color(ui.visuals().weak_text_color()));
        });
        action
    }
}

fn is_current(i: usize, current: usize) -> bool {
    // The current position points to the NEXT entry to be added
    // So entry at index `current - 1` is the most recent one
    if current == 0 { return false; }
    i == current - 1
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_timeline_entry_kind_variants() {
        for kind in &[
            TimelineEntryKind::Snapshot,
            TimelineEntryKind::Action,
            TimelineEntryKind::Undo,
            TimelineEntryKind::Redo,
            TimelineEntryKind::Rollback,
            TimelineEntryKind::Checkpoint,
        ] {
            assert!(!kind.label().is_empty());
            assert!(!kind.icon().is_empty());
        }
    }

    #[test]
    fn test_change_timeline_state_new() {
        let state = ChangeTimelineState::new();
        assert!(state.entries.is_empty());
        assert_eq!(state.current_position, 0);
        assert!(!state.can_undo());
        assert!(!state.can_redo());
    }

    #[test]
    fn test_change_timeline_state_add_entry() {
        let mut state = ChangeTimelineState::new();
        state.add_entry(TimelineEntry {
            id: "s1".into(),
            description: "Initial".into(),
            kind: TimelineEntryKind::Snapshot,
            timestamp: "12:00".into(),
            is_current: false,
        });
        assert_eq!(state.entries.len(), 1);
        assert_eq!(state.current_position, 1);
        assert!(state.can_undo());
        assert!(!state.can_redo());
    }

    #[test]
    fn test_change_timeline_state_clear() {
        let mut state = ChangeTimelineState::new();
        state.add_entry(TimelineEntry {
            id: "s1".into(), description: "Test".into(), kind: TimelineEntryKind::Action,
            timestamp: "12:00".into(), is_current: false,
        });
        state.clear();
        assert!(state.entries.is_empty());
        assert_eq!(state.current_position, 0);
    }

    #[test]
    fn test_is_current() {
        assert!(!is_current(0, 0));
        assert!(is_current(0, 1));
        assert!(is_current(4, 5));
        assert!(!is_current(3, 5));
        assert!(!is_current(5, 3));
    }

    #[test]
    fn test_timeline_action_variants() {
        let actions = [
            TimelineAction::Undo,
            TimelineAction::Redo,
            TimelineAction::Rollback("s1".into()),
            TimelineAction::CreateSnapshot("manual".into()),
            TimelineAction::SelectEntry(0),
        ];
        assert_eq!(actions.len(), 5);
    }

    #[test]
    fn test_can_undo_redo() {
        let mut state = ChangeTimelineState::new();
        assert!(!state.can_undo());
        assert!(!state.can_redo());

        state.add_entry(TimelineEntry {
            id: "a1".into(), description: "Action 1".into(), kind: TimelineEntryKind::Action,
            timestamp: "12:00".into(), is_current: false,
        });
        assert!(state.can_undo());
        assert!(!state.can_redo());

        state.add_entry(TimelineEntry {
            id: "a2".into(), description: "Action 2".into(), kind: TimelineEntryKind::Action,
            timestamp: "12:01".into(), is_current: false,
        });
        assert!(state.can_undo());
        assert!(!state.can_redo());
    }

    #[test]
    fn test_timeline_entry_colors() {
        // Just verify they return valid colors
        for kind in &[
            TimelineEntryKind::Snapshot,
            TimelineEntryKind::Action,
            TimelineEntryKind::Undo,
            TimelineEntryKind::Redo,
            TimelineEntryKind::Rollback,
            TimelineEntryKind::Checkpoint,
        ] {
            let c = kind.color();
            assert!(c.r() > 0 || c.g() > 0 || c.b() > 0);
        }
    }

    #[test]
    fn test_snapshot_input_toggle() {
        let mut state = ChangeTimelineState::new();
        assert!(!state.show_snapshot_input);
        state.show_snapshot_input = true;
        assert!(state.show_snapshot_input);
    }
}