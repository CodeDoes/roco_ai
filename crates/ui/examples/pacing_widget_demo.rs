use eframe::egui;
use roco_ui::{PacingMode, PacingWidgetState};

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([400.0, 500.0]),
        ..Default::default()
    };

    eframe::run_native(
        "Pacing Widget Demo",
        options,
        Box::new(|cc| Ok(Box::new(DemoApp::new(cc)))),
    )
}

struct DemoApp {
    state: PacingWidgetState,
    last_action: Option<roco_ui::PacingAction>,
    log: Vec<String>,
}

impl DemoApp {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        // Load fonts for egui
        cc.egui_ctx.set_visuals(egui::Visuals::dark());

        Self {
            state: PacingWidgetState::new(PacingMode::Careful, 10),
            last_action: None,
            log: vec!["Demo started. Try the pace modes and action buttons.".to_string()],
        }
    }
}

impl eframe::App for DemoApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("Pacing Widget Demo");
            ui.separator();

            // Show the pacing widget
            if let Some(action) = roco_ui::PacingWidget::show(ui, &mut self.state) {
                self.last_action = Some(action);
                self.log.push(format!("Action: {:?}", action));
            }

            ui.separator();

            // Compact version
            ui.label("Compact version (toolbar style):");
            if let Some(action) = roco_ui::PacingWidget::show_compact(ui, &mut self.state) {
                self.last_action = Some(action);
                self.log.push(format!("Compact Action: {:?}", action));
            }

            ui.separator();

            // Debug state
            ui.collapsing("Debug State", |ui| {
                ui.label(format!("Mode: {:?}", self.state.mode));
                ui.label(format!("Paused: {}", self.state.paused));
                ui.label(format!("Progress: {:.1}%", self.state.progress * 100.0));
                ui.label(format!("Current Task: {}", self.state.current_task));
                ui.label(format!("Total Tasks: {}", self.state.total_tasks));
                ui.label(format!(
                    "Waiting for Human: {}",
                    self.state.waiting_for_human
                ));
                ui.label(format!("Last Action: {:?}", self.last_action));
                ui.label(format!(
                    "Revision Feedback: {:?}",
                    self.state.revision_feedback
                ));

                ui.separator();
                ui.label("Interaction Mode Mapping:");
                let mode = self.state.to_interaction_mode();
                ui.label(format!("InteractionMode: {:?}", mode));
            });

            ui.separator();

            // Action log
            ui.collapsing("Action Log", |ui| {
                for entry in &self.log {
                    ui.label(entry);
                }
            });

            // Simulate some progress
            ui.horizontal(|ui| {
                if ui.button("Simulate Progress (+10%)").clicked() {
                    self.state.progress = (self.state.progress + 0.1).min(1.0);
                    self.state.current_task =
                        ((self.state.progress * self.state.total_tasks as f32) as usize)
                            .min(self.state.total_tasks)
                            .max(1);
                    self.log
                        .push(format!("Progress: {:.0}%", self.state.progress * 100.0));
                }

                if ui.button("Simulate Waiting for Human").clicked() {
                    self.state.waiting_for_human = true;
                    self.log.push("Waiting for human...".to_string());
                }

                if ui.button("Simulate Task Complete").clicked() {
                    self.state.current_task =
                        (self.state.current_task + 1).min(self.state.total_tasks);
                    self.state.progress =
                        self.state.current_task as f32 / self.state.total_tasks as f32;
                    self.state.waiting_for_human = self.state.should_pause(self.state.current_task);
                    self.log.push(format!(
                        "Task {} complete, waiting: {}",
                        self.state.current_task, self.state.waiting_for_human
                    ));
                }
            });
        });
    }
}
