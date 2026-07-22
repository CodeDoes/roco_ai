//! Session Browser — browse, load, and delete saved sessions.
//!
//! Reads session files from `.roco/sessions/` and displays them in a table.
//! Supports load (emit action), delete, and refresh.

use egui::{self, Color32, RichText, Ui};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// A single session entry shown in the browser
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionEntry {
    pub id: String,
    pub pacing: String,
    pub created_at: String,
    pub updated_at: String,
    pub message_count: usize,
    pub last_prompt: Option<String>,
    pub path: PathBuf,
}

/// Actions the human takes from the session browser
#[derive(Debug, Clone)]
pub enum SessionBrowserAction {
    /// Load the selected session
    Load(PathBuf),
    /// Delete the selected session
    Delete(PathBuf),
    /// Refresh the session list
    Refresh,
}

/// Session browser widget state
#[derive(Debug, Clone)]
pub struct SessionBrowserState {
    /// All found sessions
    pub sessions: Vec<SessionEntry>,
    /// Currently selected session index
    pub selected_index: Option<usize>,
    /// Search/filter text
    pub filter_text: String,
    /// Session directory
    pub session_dir: PathBuf,
    /// Whether we're showing the browser
    pub visible: bool,
}

impl Default for SessionBrowserState {
    fn default() -> Self {
        let dir = PathBuf::from(".roco/sessions");
        Self {
            sessions: Vec::new(),
            selected_index: None,
            filter_text: String::new(),
            session_dir: dir,
            visible: false,
        }
    }
}

impl SessionBrowserState {
    pub fn new(session_dir: PathBuf) -> Self {
        let mut state = Self {
            session_dir,
            ..Default::default()
        };
        state.refresh();
        state
    }

    /// Refresh the session list from disk
    pub fn refresh(&mut self) {
        self.sessions.clear();
        let dir = &self.session_dir;
        if !dir.exists() {
            return;
        }
        let mut entries: Vec<_> = match std::fs::read_dir(dir) {
            Ok(entries) => entries
                .filter_map(|e| e.ok())
                .filter(|e| e.path().extension().is_some_and(|ext| ext == "json"))
                .collect(),
            Err(_) => return,
        };
        entries.sort_by_key(|e| e.file_name());

        for entry in &entries {
            let path = entry.path();
            if let Ok(json) = std::fs::read_to_string(&path) {
                if let Ok(value) = serde_json::from_str::<serde_json::Value>(&json) {
                    let messages = value["messages"].as_array().map(|a| a.len()).unwrap_or(0);
                    let pacing = value["pacing"].as_str().unwrap_or("?").to_string();
                    let created = value["created_at"].as_str().unwrap_or("?").to_string();
                    let updated = value["updated_at"].as_str().unwrap_or("?").to_string();
                    let id = path
                        .file_stem()
                        .map(|s| s.to_string_lossy().to_string())
                        .unwrap_or_default();
                    let last_prompt = value["messages"].as_array().and_then(|msgs| {
                        msgs.iter()
                            .rev()
                            .find(|m| m["role"].as_str() == Some("user"))
                            .and_then(|m| {
                                m["content"]
                                    .as_str()
                                    .map(|s| s.chars().take(80).collect::<String>())
                            })
                    });
                    self.sessions.push(SessionEntry {
                        id,
                        pacing,
                        created_at: created,
                        updated_at: updated,
                        message_count: messages,
                        last_prompt,
                        path: path.clone(),
                    });
                }
            }
        }
    }

    /// Get sessions matching the current filter
    pub fn filtered_sessions(&self) -> Vec<&SessionEntry> {
        if self.filter_text.is_empty() {
            self.sessions.iter().collect()
        } else {
            let lower = self.filter_text.to_lowercase();
            self.sessions
                .iter()
                .filter(|s| {
                    s.id.to_lowercase().contains(&lower)
                        || s.pacing.to_lowercase().contains(&lower)
                        || s.last_prompt
                            .as_deref()
                            .unwrap_or("")
                            .to_lowercase()
                            .contains(&lower)
                })
                .collect()
        }
    }
}

/// Session browser widget
pub struct SessionBrowser;

impl SessionBrowser {
    /// Show the session browser panel
    pub fn show(ui: &mut Ui, state: &mut SessionBrowserState) -> Option<SessionBrowserAction> {
        let mut action = None;

        ui.vertical(|ui| {
            // Header
            ui.horizontal(|ui| {
                ui.label(RichText::new("Sessions").strong().size(14.0));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("↻").on_hover_text("Refresh").clicked() {
                        state.refresh();
                        action = Some(SessionBrowserAction::Refresh);
                    }
                });
            });

            // Filter
            ui.add(
                egui::TextEdit::singleline(&mut state.filter_text)
                    .hint_text("Filter sessions...")
                    .desired_width(f32::INFINITY),
            );

            ui.separator();
            ui.add_space(4.0);

            // Session list — collect indices first to avoid borrow issues
            let filtered_indices: Vec<usize> = {
                let filtered = state.filtered_sessions();
                filtered
                    .iter()
                    .map(|entry| {
                        state
                            .sessions
                            .iter()
                            .position(|s| s.id == entry.id)
                            .unwrap_or(0)
                    })
                    .collect()
            };
            if filtered_indices.is_empty() {
                ui.label(
                    RichText::new("No sessions found.")
                        .size(12.0)
                        .color(ui.visuals().weak_text_color()),
                );
                return;
            }

            let mut height = (filtered_indices.len() as f32 * 60.0).min(400.0);
            if filtered_indices.len() > 7 {
                height = 400.0;
            }

            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .max_height(height)
                .show(ui, |ui| {
                    for &real_idx in &filtered_indices {
                        let entry = &state.sessions[real_idx];
                        let selected = state.selected_index == Some(real_idx);
                        let selected = state.selected_index == Some(real_idx);
                        let bg = if selected {
                            ui.visuals().selection.bg_fill
                        } else {
                            ui.visuals().faint_bg_color
                        };

                        let id = ui.make_persistent_id(format!("session_{}", entry.id));
                        let response = egui::Frame::none()
                            .fill(bg)
                            .show(ui, |ui| {
                                ui.set_min_width(ui.available_width());
                                ui.horizontal(|ui| {
                                    // Session icon + info
                                    ui.label("💬");
                                    ui.vertical(|ui| {
                                        ui.label(
                                            RichText::new(&entry.id).size(12.0).strong().color(
                                                if selected {
                                                    ui.visuals().selection.stroke.color
                                                } else {
                                                    ui.visuals().text_color()
                                                },
                                            ),
                                        );
                                        ui.horizontal(|ui| {
                                            let pacing_color = match entry.pacing.as_str() {
                                                "planning" => Color32::from_rgb(100, 100, 255),
                                                "careful" => Color32::from_rgb(255, 180, 50),
                                                "rolling" => Color32::from_rgb(50, 200, 100),
                                                "auto-accept" => Color32::from_rgb(100, 255, 100),
                                                _ => Color32::GRAY,
                                            };
                                            ui.label(
                                                RichText::new(&entry.pacing)
                                                    .size(10.0)
                                                    .color(pacing_color),
                                            );
                                            ui.label(
                                                RichText::new(format!(
                                                    "{} msgs",
                                                    entry.message_count
                                                ))
                                                .size(10.0)
                                                .color(ui.visuals().weak_text_color()),
                                            );
                                        });
                                    });
                                });
                                if let Some(ref prompt) = entry.last_prompt {
                                    ui.label(
                                        RichText::new(format!("“{prompt}”"))
                                            .size(11.0)
                                            .color(ui.visuals().weak_text_color()),
                                    );
                                }
                            })
                            .response
                            .on_hover_text(format!(
                                "Created: {}\nUpdated: {}",
                                entry.created_at, entry.updated_at
                            ));

                        if response.clicked() {
                            state.selected_index = Some(real_idx);
                        }

                        // Double-click to load
                        if response.double_clicked() {
                            action = Some(SessionBrowserAction::Load(entry.path.clone()));
                        }

                        // Right-click menu
                        response.context_menu(|ui| {
                            if ui.button("Load").clicked() {
                                action = Some(SessionBrowserAction::Load(entry.path.clone()));
                                ui.close_menu();
                            }
                            if ui.button("Delete").clicked() {
                                action = Some(SessionBrowserAction::Delete(entry.path.clone()));
                                ui.close_menu();
                            }
                        });

                        ui.add_space(2.0);
                    }
                });
        });

        action
    }

    /// Compact version (for sidebar)
    pub fn show_compact(
        ui: &mut Ui,
        state: &mut SessionBrowserState,
    ) -> Option<SessionBrowserAction> {
        let mut action = None;

        ui.vertical(|ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new("Sessions").strong().size(12.0));
                if ui.button("↻").clicked() {
                    state.refresh();
                    action = Some(SessionBrowserAction::Refresh);
                }
            });

            let filtered = state.filtered_sessions();
            for entry in filtered.iter().take(10) {
                let label = format!(
                    "{} ({}msgs)",
                    entry.id.chars().take(30).collect::<String>(),
                    entry.message_count
                );
                if ui.selectable_label(false, &label).clicked() {
                    action = Some(SessionBrowserAction::Load(entry.path.clone()));
                }
            }
            if filtered.len() > 10 {
                ui.label(
                    RichText::new(format!("... and {} more", filtered.len() - 10))
                        .size(10.0)
                        .color(ui.visuals().weak_text_color()),
                );
            }
        });

        action
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_dir() -> PathBuf {
        // Use test name via backtrace — fallback to unique timestamp
        let dir = std::env::temp_dir().join(format!(
            "roco_sb_test_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = std::fs::create_dir_all(&dir);
        dir
    }

    fn write_test_session(dir: &PathBuf, id: &str, pacing: &str, messages: usize) {
        let mut msgs = Vec::new();
        for i in 0..messages {
            msgs.push(serde_json::json!({
                "role": if i == 0 { "user" } else { "assistant" },
                "content": format!("Message {i}"),
                "timestamp": "2026-07-19T12:00:00Z",
            }));
        }
        let value = serde_json::json!({
            "id": id,
            "pacing": pacing,
            "created_at": "2026-07-19T12:00:00Z",
            "updated_at": "2026-07-19T13:00:00Z",
            "messages": msgs,
        });
        let path = dir.join(format!("{id}.json"));
        std::fs::write(&path, serde_json::to_string_pretty(&value).unwrap()).unwrap();
    }

    #[test]
    fn test_session_browser_refresh() {
        let dir = test_dir();
        write_test_session(&dir, "test1", "careful", 2);
        write_test_session(&dir, "test2", "rolling", 4);

        let mut state = SessionBrowserState::new(dir.clone());
        assert_eq!(state.sessions.len(), 2);
        assert!(state.sessions.iter().any(|s| s.id == "test1"));
        assert!(state.sessions.iter().any(|s| s.id == "test2"));
        assert_eq!(
            state
                .sessions
                .iter()
                .find(|s| s.id == "test1")
                .unwrap()
                .pacing,
            "careful"
        );
        assert_eq!(
            state
                .sessions
                .iter()
                .find(|s| s.id == "test2")
                .unwrap()
                .message_count,
            4
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_session_browser_empty_filter() {
        let dir = test_dir();
        write_test_session(&dir, "alpha", "planning", 1);
        write_test_session(&dir, "beta", "auto-accept", 3);

        let mut state = SessionBrowserState::new(dir.clone());
        assert_eq!(state.filtered_sessions().len(), 2);

        state.filter_text = "alpha".to_string();
        assert_eq!(state.filtered_sessions().len(), 1);

        state.filter_text = "planning".to_string();
        assert_eq!(state.filtered_sessions().len(), 1);

        state.filter_text = "NONEXISTENT".to_string();
        assert_eq!(state.filtered_sessions().len(), 0);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_session_browser_no_directory() {
        let state = SessionBrowserState::new(PathBuf::from("/nonexistent/path"));
        assert!(state.sessions.is_empty());
    }

    #[test]
    fn test_session_browser_refresh_clears() {
        let dir = test_dir();
        write_test_session(&dir, "s1", "careful", 2);

        let mut state = SessionBrowserState::new(dir.clone());
        assert_eq!(state.sessions.len(), 1);

        // Refresh should still have 1
        state.refresh();
        assert_eq!(state.sessions.len(), 1);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_session_entry_fields() {
        let entry = SessionEntry {
            id: "test-session".into(),
            pacing: "rolling".into(),
            created_at: "2026-07-19".into(),
            updated_at: "2026-07-20".into(),
            message_count: 5,
            last_prompt: Some("Hello".into()),
            path: PathBuf::from("test.json"),
        };
        assert_eq!(entry.id, "test-session");
        assert_eq!(entry.pacing, "rolling");
        assert_eq!(entry.message_count, 5);
        assert_eq!(entry.last_prompt, Some("Hello".into()));
    }

    #[test]
    fn test_session_browser_action_variants() {
        let p = PathBuf::from("test.json");
        let actions = [
            SessionBrowserAction::Load(p.clone()),
            SessionBrowserAction::Delete(p.clone()),
            SessionBrowserAction::Refresh,
        ];
        assert_eq!(actions.len(), 3);
    }

    #[test]
    fn test_session_browser_filters_correctly() {
        let dir = test_dir();
        write_test_session(&dir, "story-dark-fantasy", "careful", 5);
        write_test_session(&dir, "story-sci-fi", "auto-accept", 2);
        write_test_session(&dir, "notes", "planning", 1);

        let mut state = SessionBrowserState::new(dir.clone());

        // Filter by pacing
        state.filter_text = "auto".to_string();
        let filtered = state.filtered_sessions();
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].id, "story-sci-fi");

        // Filter by ID
        state.filter_text = "fantasy".to_string();
        let filtered = state.filtered_sessions();
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].id, "story-dark-fantasy");

        let _ = std::fs::remove_dir_all(&dir);
    }
}
