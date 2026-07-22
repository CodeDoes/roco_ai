//! Interaction Modes — control how much human involvement is needed.
//!
//! The human chooses their level of involvement:
//! - **Full control** — one task at a time, human reviews each one
//! - **Moderate control** — batch of tasks, human reviews batch
//! - **No control** — agent runs to completion, human reviews at end
//! - **"Go ham" mode** — agent runs without stopping, local model so tokens are free

use serde::{Deserialize, Serialize};

// ═════════════════════════════════════════════════════════════════════════════
// Interaction Modes
// ═════════════════════════════════════════════════════════════════════════════

/// How much human involvement is needed
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub enum InteractionMode {
    /// One task at a time, human reviews each one
    #[default]
    FullControl,
    /// Batch of tasks, human reviews batch
    ModerateControl { batch_size: usize },
    /// Agent runs to completion, human reviews at end
    NoControl,
    /// Agent runs without stopping, local model so tokens are free
    GoHam,
}

impl InteractionMode {
    /// Should the agent pause after completing a task?
    pub fn should_pause(&self, tasks_completed: usize, total_tasks: usize) -> bool {
        match self {
            InteractionMode::FullControl => true,
            InteractionMode::ModerateControl { batch_size } => {
                tasks_completed.is_multiple_of(*batch_size) || tasks_completed >= total_tasks
            }
            InteractionMode::NoControl => tasks_completed >= total_tasks,
            InteractionMode::GoHam => false,
        }
    }

    /// Should the agent ask for feedback?
    pub fn should_ask_feedback(&self) -> bool {
        match self {
            InteractionMode::FullControl => true,
            InteractionMode::ModerateControl { .. } => true,
            InteractionMode::NoControl => false,
            InteractionMode::GoHam => false,
        }
    }

    /// Should the agent show previews?
    pub fn should_show_preview(&self) -> bool {
        match self {
            InteractionMode::FullControl => true,
            InteractionMode::ModerateControl { .. } => true,
            InteractionMode::NoControl => false,
            InteractionMode::GoHam => false,
        }
    }

    /// Should the agent run quality checks?
    pub fn should_check_quality(&self) -> bool {
        match self {
            InteractionMode::FullControl => true,
            InteractionMode::ModerateControl { .. } => true,
            InteractionMode::NoControl => true,
            InteractionMode::GoHam => false, // Speed over quality
        }
    }

    /// Should the agent auto-revise on quality failure?
    pub fn should_auto_revise(&self) -> bool {
        match self {
            InteractionMode::FullControl => false, // Human decides
            InteractionMode::ModerateControl { .. } => true,
            InteractionMode::NoControl => true,
            InteractionMode::GoHam => true,
        }
    }

    /// Get a description of this mode
    pub fn description(&self) -> String {
        match self {
            InteractionMode::FullControl => {
                "Full control: one task at a time, you review each one".to_string()
            }
            InteractionMode::ModerateControl { batch_size } => {
                format!(
                    "Moderate control: {} tasks at a time, you review each batch",
                    batch_size
                )
            }
            InteractionMode::NoControl => {
                "No control: agent runs to completion, you review at end".to_string()
            }
            InteractionMode::GoHam => {
                "Go ham: agent runs without stopping, maximum speed".to_string()
            }
        }
    }
}

// ═════════════════════════════════════════════════════════════════════════════
// Interaction State
// ═════════════════════════════════════════════════════════════════════════════

/// Current state of the interaction
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InteractionState {
    /// Current mode
    pub mode: InteractionMode,
    /// Tasks completed so far
    pub tasks_completed: usize,
    /// Total tasks in plan
    pub total_tasks: usize,
    /// Whether we're waiting for human input
    pub waiting_for_human: bool,
    /// Last human action
    pub last_human_action: Option<HumanAction>,
    /// Batch results (for moderate control)
    pub batch_results: Vec<String>,
}

/// Actions the human can take
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum HumanAction {
    /// Accept and continue
    Accept,
    /// Accept all remaining (skip reviews)
    AcceptAll,
    /// Give feedback for revision
    Revise(String),
    /// Skip to next task
    Skip,
    /// Jump to specific task
    JumpTo(usize),
    /// Stop and publish
    Stop,
    /// Switch to "go ham" mode
    GoHam,
    /// Switch to full control mode
    FullControl,
    /// Undo last action
    Undo,
    /// Redo last undone action
    Redo,
}

impl InteractionState {
    /// Create a new interaction state
    pub fn new(mode: InteractionMode, total_tasks: usize) -> Self {
        Self {
            mode,
            tasks_completed: 0,
            total_tasks,
            waiting_for_human: false,
            last_human_action: None,
            batch_results: Vec::new(),
        }
    }

    /// Record a task completion
    pub fn task_completed(&mut self, result: String) {
        self.tasks_completed += 1;
        self.batch_results.push(result);
    }

    /// Check if we should pause for human input
    pub fn should_pause(&self) -> bool {
        self.mode
            .should_pause(self.tasks_completed, self.total_tasks)
    }

    /// Check if we're done
    pub fn is_done(&self) -> bool {
        self.tasks_completed >= self.total_tasks
    }

    /// Get progress as a fraction
    pub fn progress(&self) -> f32 {
        if self.total_tasks == 0 {
            return 1.0;
        }
        self.tasks_completed as f32 / self.total_tasks as f32
    }

    /// Get progress as a percentage string
    pub fn progress_string(&self) -> String {
        format!("{:.0}%", self.progress() * 100.0)
    }

    /// Process a human action
    pub fn process_action(&mut self, action: HumanAction) {
        self.last_human_action = Some(action.clone());
        self.waiting_for_human = false;

        match action {
            HumanAction::Accept => {
                // Continue to next task
            }
            HumanAction::AcceptAll => {
                // Switch to no control mode
                self.mode = InteractionMode::NoControl;
            }
            HumanAction::Revise(_) => {
                // Will be handled by caller
            }
            HumanAction::Skip => {
                // Skip to next task
            }
            HumanAction::JumpTo(n) => {
                // Jump to specific task
                self.tasks_completed = n;
            }
            HumanAction::Stop => {
                // Mark as done
                self.tasks_completed = self.total_tasks;
            }
            HumanAction::GoHam => {
                // Switch to go ham mode
                self.mode = InteractionMode::GoHam;
            }
            HumanAction::FullControl => {
                // Switch to full control mode
                self.mode = InteractionMode::FullControl;
            }
            HumanAction::Undo => {
                // Will be handled by caller
            }
            HumanAction::Redo => {
                // Will be handled by caller
            }
        }
    }

    /// Get the prompt to show the human
    pub fn human_prompt(&self) -> String {
        match &self.mode {
            InteractionMode::FullControl => {
                format!(
                    "Task {}/{} complete. What would you like to do?\n\
                     [a] Accept and continue\n\
                     [r] Revise with feedback\n\
                     [s] Skip to next\n\
                     [j] Jump to task N\n\
                     [x] Accept all remaining\n\
                     [g] Go ham (run without stopping)\n\
                     [q] Stop and publish",
                    self.tasks_completed + 1,
                    self.total_tasks
                )
            }
            InteractionMode::ModerateControl { batch_size } => {
                let batch_start = (self.tasks_completed / batch_size) * batch_size + 1;
                let batch_end = (batch_start + batch_size - 1).min(self.total_tasks);
                format!(
                    "Batch {}-{} complete. What would you like to do?\n\
                     [a] Accept batch and continue\n\
                     [r] Revise batch with feedback\n\
                     [x] Accept all remaining\n\
                     [g] Go ham (run without stopping)\n\
                     [q] Stop and publish",
                    batch_start, batch_end
                )
            }
            InteractionMode::NoControl => "Agent running... (press Ctrl+C to stop)".to_string(),
            InteractionMode::GoHam => {
                "Agent running at full speed... (press Ctrl+C to stop)".to_string()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_full_control_always_pauses() {
        let mode = InteractionMode::FullControl;
        assert!(mode.should_pause(0, 10));
        assert!(mode.should_pause(5, 10));
        assert!(mode.should_pause(10, 10));
    }

    #[test]
    fn test_moderate_control_pauses_at_batch() {
        let mode = InteractionMode::ModerateControl { batch_size: 3 };
        assert!(!mode.should_pause(1, 10));
        assert!(!mode.should_pause(2, 10));
        assert!(mode.should_pause(3, 10));
        assert!(!mode.should_pause(4, 10));
        assert!(!mode.should_pause(5, 10));
        assert!(mode.should_pause(6, 10));
    }

    #[test]
    fn test_no_control_only_pauses_at_end() {
        let mode = InteractionMode::NoControl;
        assert!(!mode.should_pause(0, 10));
        assert!(!mode.should_pause(5, 10));
        assert!(mode.should_pause(10, 10));
    }

    #[test]
    fn test_go_ham_never_pauses() {
        let mode = InteractionMode::GoHam;
        assert!(!mode.should_pause(0, 10));
        assert!(!mode.should_pause(5, 10));
        assert!(!mode.should_pause(10, 10));
    }

    #[test]
    fn test_interaction_state_progress() {
        let mut state = InteractionState::new(InteractionMode::FullControl, 10);
        assert_eq!(state.progress(), 0.0);
        assert_eq!(state.progress_string(), "0%");

        state.task_completed("test".to_string());
        assert_eq!(state.progress(), 0.1);
        assert_eq!(state.progress_string(), "10%");

        state.task_completed("test".to_string());
        assert_eq!(state.progress(), 0.2);
    }

    #[test]
    fn test_process_accept_all() {
        let mut state = InteractionState::new(InteractionMode::FullControl, 10);
        state.process_action(HumanAction::AcceptAll);
        assert_eq!(state.mode, InteractionMode::NoControl);
    }

    #[test]
    fn test_process_go_ham() {
        let mut state = InteractionState::new(InteractionMode::FullControl, 10);
        state.process_action(HumanAction::GoHam);
        assert_eq!(state.mode, InteractionMode::GoHam);
    }
}
