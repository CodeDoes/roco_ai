//! Pacing Control Widget — Human controls the agent's pace.
//!
//! Maps UX-friendly pacing modes to the tested `InteractionMode` enum:
//! - **Planning** → `NoControl` (agent runs to completion, review at end)
//! - **Careful** → `FullControl` (one task at a time, human reviews each)
//! - **Rolling** → `ModerateControl` (batch of tasks, human reviews batch)
//! - **Auto-Accept** → `GoHam` (agent runs without stopping)

use egui::{ComboBox, RichText, Ui};
use roco_agent::interaction::{InteractionMode, InteractionState};

/// Pacing mode with UX-friendly names
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PacingMode {
    /// Agent runs to completion, human reviews at end
    #[default]
    Planning,
    /// One task at a time, human reviews each
    Careful,
    /// Batch of tasks, human reviews batch
    Rolling,
    /// Agent runs without stopping
    AutoAccept,
}

impl PacingMode {
    /// All available pacing modes in display order
    pub const ALL: [PacingMode; 4] = [
        PacingMode::Planning,
        PacingMode::Careful,
        PacingMode::Rolling,
        PacingMode::AutoAccept,
    ];

    /// Display name for the UI
    pub fn label(self) -> &'static str {
        match self {
            PacingMode::Planning => "Planning",
            PacingMode::Careful => "Careful",
            PacingMode::Rolling => "Rolling",
            PacingMode::AutoAccept => "Auto-Accept",
        }
    }

    /// Short description for tooltips
    pub fn description(self) -> &'static str {
        match self {
            PacingMode::Planning => "Agent runs to completion, you review at the end",
            PacingMode::Careful => "One task at a time, you review each one",
            PacingMode::Rolling => "Batch of tasks, you review each batch",
            PacingMode::AutoAccept => "Agent runs without stopping, maximum speed",
        }
    }

    /// Convert to the engine's InteractionMode
    pub fn to_interaction_mode(self) -> InteractionMode {
        match self {
            PacingMode::Planning => InteractionMode::NoControl,
            PacingMode::Careful => InteractionMode::FullControl,
            PacingMode::Rolling => InteractionMode::ModerateControl { batch_size: 3 },
            PacingMode::AutoAccept => InteractionMode::GoHam,
        }
    }

    /// Convert from the engine's InteractionMode
    pub fn from_interaction_mode(mode: &InteractionMode) -> Self {
        match mode {
            InteractionMode::NoControl => PacingMode::Planning,
            InteractionMode::FullControl => PacingMode::Careful,
            InteractionMode::ModerateControl { batch_size: _ } => PacingMode::Rolling,
            InteractionMode::GoHam => PacingMode::AutoAccept,
        }
    }
}

/// Pacing control widget state
#[derive(Debug, Clone, Default)]
pub struct PacingWidgetState {
    /// Current pacing mode
    pub mode: PacingMode,
    /// Whether the human has chosen to pause
    pub paused: bool,
    /// Progress (0.0 to 1.0)
    pub progress: f32,
    /// Current task number (1-indexed for display)
    pub current_task: usize,
    /// Total tasks
    pub total_tasks: usize,
    /// Optional feedback text for revision
    pub revision_feedback: String,
    /// Whether we're currently waiting for human input
    pub waiting_for_human: bool,
}

impl PacingWidgetState {
    /// Create a new state with the given pacing mode and total tasks
    pub fn new(mode: PacingMode, total_tasks: usize) -> Self {
        Self {
            mode,
            total_tasks,
            ..Default::default()
        }
    }

    /// Update from an InteractionState
    pub fn update_from_interaction_state(&mut self, state: &InteractionState) {
        self.mode = PacingMode::from_interaction_mode(&state.mode);
        self.progress = state.progress();
        self.current_task = state.tasks_completed + 1;
        self.total_tasks = state.total_tasks;
        self.waiting_for_human = state.waiting_for_human;
    }

    /// Convert to an InteractionMode
    pub fn to_interaction_mode(&self) -> InteractionMode {
        self.mode.to_interaction_mode()
    }

    /// Check if the current mode should pause for human input
    pub fn should_pause(&self, tasks_completed: usize) -> bool {
        self.mode.to_interaction_mode().should_pause(tasks_completed, self.total_tasks)
    }

    /// Get the current progress string
    pub fn progress_string(&self) -> String {
        format!("{:.0}%", self.progress * 100.0)
    }

    /// Get the current task display string
    pub fn task_string(&self) -> String {
        if self.total_tasks == 0 {
            "No tasks".to_string()
        } else {
            format!("Task {} / {}", self.current_task.min(self.total_tasks), self.total_tasks)
        }
    }
}

/// Actions the human can take from the pacing widget
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PacingAction {
    /// Accept current task and continue
    Accept,
    /// Accept all remaining tasks (switch to auto mode)
    AcceptAll,
    /// Request revision with feedback
    Revise,
    /// Skip current task
    Skip,
    /// Stop and publish
    Stop,
    /// Switch to Auto-Accept mode
    GoHam,
    /// Switch to Careful mode
    FullControl,
    /// Undo last action
    Undo,
    /// Redo last undone action
    Redo,
}

/// Pacing control widget
pub struct PacingWidget;

impl PacingWidget {
    /// Show the pacing control widget
    ///
    /// Returns the action the human selected, if any
    pub fn show(ui: &mut Ui, state: &mut PacingWidgetState) -> Option<PacingAction> {
        let mut action = None;

        ui.group(|ui| {
            ui.set_min_width(300.0);
            ui.vertical(|ui| {
                // Header
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Agent Pace").strong().size(16.0));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        // Pacing mode selector
                        ComboBox::from_label("")
                            .selected_text(state.mode.label())
                            .show_ui(ui, |ui| {
                                for mode in PacingMode::ALL {
                                    let selected = state.mode == mode;
                                    if ui
                                        .selectable_label(selected, mode.label())
                                        .on_hover_text(mode.description())
                                        .clicked()
                                    {
                                        state.mode = mode;
                                        state.paused = false;
                                    }
                                }
                            });
                    });
                });

                ui.separator();

                // Progress bar
                let progress_text = if state.total_tasks > 0 {
                    state.task_string()
                } else {
                    "No tasks".to_string()
                };
                ui.add(egui::ProgressBar::new(state.progress).text(progress_text));

                ui.add_space(4.0);

                // Current mode description
                ui.label(
                    RichText::new(state.mode.description())
                        .size(12.0)
                        .color(ui.visuals().weak_text_color()),
                );

                ui.add_space(8.0);

                // Human action buttons (only shown when waiting for human)
                if state.waiting_for_human {
                    ui.horizontal_wrapped(|ui| {
                        action_button(ui, "Accept", PacingAction::Accept, &mut action);
                        action_button(ui, "Accept All", PacingAction::AcceptAll, &mut action);
                        action_button(ui, "Revise", PacingAction::Revise, &mut action);
                        action_button(ui, "Skip", PacingAction::Skip, &mut action);
                        action_button(ui, "Stop", PacingAction::Stop, &mut action);
                    });

                    ui.horizontal_wrapped(|ui| {
                        action_button(ui, "Go Ham", PacingAction::GoHam, &mut action);
                        action_button(ui, "Careful", PacingAction::FullControl, &mut action);
                        action_button(ui, "Undo", PacingAction::Undo, &mut action);
                        action_button(ui, "Redo", PacingAction::Redo, &mut action);
                    });

                    // Revision feedback input
                    if matches!(action, Some(PacingAction::Revise)) {
                        ui.add_space(4.0);
                        ui.label("Revision feedback:");
                        ui.text_edit_multiline(&mut state.revision_feedback);
                    }
                } else if state.paused {
                    // Show resume button when paused but not waiting for human
                    ui.horizontal(|ui| {
                        if ui.button("Resume").clicked() {
                            state.paused = false;
                        }
                    });
                } else {
                    // Running state - show pause button
                    ui.horizontal(|ui| {
                        if ui.button("Pause").clicked() {
                            state.paused = true;
                        }
                        ui.label(
                            RichText::new("Running...")
                                .size(12.0)
                                .color(ui.visuals().weak_text_color()),
                        );
                    });
                }
            });
        });

        action
    }

    /// Show a compact version of the pacing widget (for toolbars)
    pub fn show_compact(ui: &mut Ui, state: &mut PacingWidgetState) -> Option<PacingAction> {
        let mut action = None;

        ui.horizontal(|ui| {
            ui.label(RichText::new("Pace:").size(12.0));

            ComboBox::from_label("")
                .selected_text(state.mode.label())
                .width(100.0)
                .show_ui(ui, |ui| {
                    for mode in PacingMode::ALL {
                        if ui
                            .selectable_label(state.mode == mode, mode.label())
                            .on_hover_text(mode.description())
                            .clicked()
                        {
                            state.mode = mode;
                            state.paused = false;
                        }
                    }
                });

            ui.add(egui::ProgressBar::new(state.progress).desired_width(100.0));

            if state.waiting_for_human {
                if ui.button("Accept").clicked() {
                    action = Some(PacingAction::Accept);
                }
                if ui.button("Skip").clicked() {
                    action = Some(PacingAction::Skip);
                }
            } else if state.paused {
                if ui.button("Resume").clicked() {
                    state.paused = false;
                }
            } else if ui.button("Pause").clicked() {
                state.paused = true;
            }
        });

        action
    }
}

/// Helper to create an action button
fn action_button(ui: &mut Ui, label: &str, action: PacingAction, out: &mut Option<PacingAction>) {
    if ui.button(label).clicked() {
        *out = Some(action);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use roco_agent::interaction::{InteractionMode, InteractionState};

    #[test]
    fn test_pacing_mode_all_variants() {
        assert_eq!(PacingMode::Planning.label(), "Planning");
        assert_eq!(PacingMode::Careful.label(), "Careful");
        assert_eq!(PacingMode::Rolling.label(), "Rolling");
        assert_eq!(PacingMode::AutoAccept.label(), "Auto-Accept");
    }

    #[test]
    fn test_pacing_mode_to_interaction_mode() {
        assert_eq!(
            PacingMode::Planning.to_interaction_mode(),
            InteractionMode::NoControl
        );
        assert_eq!(
            PacingMode::Careful.to_interaction_mode(),
            InteractionMode::FullControl
        );
        assert_eq!(
            PacingMode::Rolling.to_interaction_mode(),
            InteractionMode::ModerateControl { batch_size: 3 }
        );
        assert_eq!(
            PacingMode::AutoAccept.to_interaction_mode(),
            InteractionMode::GoHam
        );
    }

    #[test]
    fn test_pacing_mode_from_interaction_mode() {
        assert_eq!(
            PacingMode::from_interaction_mode(&InteractionMode::NoControl),
            PacingMode::Planning
        );
        assert_eq!(
            PacingMode::from_interaction_mode(&InteractionMode::FullControl),
            PacingMode::Careful
        );
        assert_eq!(
            PacingMode::from_interaction_mode(&InteractionMode::ModerateControl { batch_size: 5 }),
            PacingMode::Rolling
        );
        assert_eq!(
            PacingMode::from_interaction_mode(&InteractionMode::GoHam),
            PacingMode::AutoAccept
        );
    }

    #[test]
    fn test_pacing_mode_descriptions() {
        assert!(!PacingMode::Planning.description().is_empty());
        assert!(!PacingMode::Careful.description().is_empty());
        assert!(!PacingMode::Rolling.description().is_empty());
        assert!(!PacingMode::AutoAccept.description().is_empty());
    }

    #[test]
    fn test_pacing_widget_state_new() {
        let state = PacingWidgetState::new(PacingMode::Careful, 10);
        assert_eq!(state.mode, PacingMode::Careful);
        assert_eq!(state.total_tasks, 10);
        assert_eq!(state.progress, 0.0);
        assert_eq!(state.current_task, 0);
        assert!(!state.paused);
        assert!(!state.waiting_for_human);
    }

    #[test]
    fn test_pacing_widget_state_update_from_interaction_state() {
        let mut state = PacingWidgetState::new(PacingMode::Rolling, 10);
        let interaction_state = InteractionState::new(InteractionMode::FullControl, 10);

        state.update_from_interaction_state(&interaction_state);

        assert_eq!(state.mode, PacingMode::Careful);
        assert_eq!(state.progress, 0.0);
        assert_eq!(state.current_task, 1);
        assert_eq!(state.total_tasks, 10);
        assert!(!state.waiting_for_human);
    }

    #[test]
    fn test_pacing_widget_state_to_interaction_mode() {
        let state = PacingWidgetState::new(PacingMode::AutoAccept, 5);
        assert_eq!(
            state.to_interaction_mode(),
            InteractionMode::GoHam
        );
    }

    #[test]
    fn test_pacing_widget_state_should_pause() {
        let state = PacingWidgetState::new(PacingMode::Careful, 10);
        // FullControl always pauses
        assert!(state.should_pause(0));
        assert!(state.should_pause(5));
        assert!(state.should_pause(10));

        let state = PacingWidgetState::new(PacingMode::Rolling, 10);
        // ModerateControl pauses at batch boundaries (batch_size=3)
        assert!(!state.should_pause(1));
        assert!(!state.should_pause(2));
        assert!(state.should_pause(3));
        assert!(!state.should_pause(4));
        assert!(!state.should_pause(5));
        assert!(state.should_pause(6));

        let state = PacingWidgetState::new(PacingMode::Planning, 10);
        // NoControl only pauses at end
        assert!(!state.should_pause(0));
        assert!(!state.should_pause(5));
        assert!(state.should_pause(10));

        let state = PacingWidgetState::new(PacingMode::AutoAccept, 10);
        // GoHam never pauses
        assert!(!state.should_pause(0));
        assert!(!state.should_pause(5));
        assert!(!state.should_pause(10));
    }

    #[test]
    fn test_pacing_widget_state_progress_string() {
        let mut state = PacingWidgetState::new(PacingMode::Careful, 10);
        state.progress = 0.0;
        assert_eq!(state.progress_string(), "0%");

        state.progress = 0.5;
        assert_eq!(state.progress_string(), "50%");

        state.progress = 1.0;
        assert_eq!(state.progress_string(), "100%");
    }

    #[test]
    fn test_pacing_widget_state_task_string() {
        let mut state = PacingWidgetState::new(PacingMode::Careful, 10);
        state.current_task = 1;
        assert_eq!(state.task_string(), "Task 1 / 10");

        state.current_task = 5;
        assert_eq!(state.task_string(), "Task 5 / 10");

        state.current_task = 10;
        assert_eq!(state.task_string(), "Task 10 / 10");

        state.total_tasks = 0;
        assert_eq!(state.task_string(), "No tasks");
    }

    #[test]
    fn test_pacing_action_variants() {
        // Just verify all variants exist and can be matched
        let actions = [
            PacingAction::Accept,
            PacingAction::AcceptAll,
            PacingAction::Revise,
            PacingAction::Skip,
            PacingAction::Stop,
            PacingAction::GoHam,
            PacingAction::FullControl,
            PacingAction::Undo,
            PacingAction::Redo,
        ];
        assert_eq!(actions.len(), 9);
    }
}