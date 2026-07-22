//! Wiki Browser — rendered markdown viewer for story wiki content.
//!
//! Uses egui_markdown for rendering. Displays character bios, setting lore,
//! and other worldbuilding content from a markdown file or text source.

use egui::{self, RichText, Ui};

/// Wiki entry types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WikiSection {
    Characters,
    Setting,
    Lore,
    Timeline,
    Notes,
}

impl WikiSection {
    pub fn label(self) -> &'static str {
        match self {
            WikiSection::Characters => "Characters",
            WikiSection::Setting => "Setting",
            WikiSection::Lore => "Lore",
            WikiSection::Timeline => "Timeline",
            WikiSection::Notes => "Notes",
        }
    }

    pub fn icon(self) -> &'static str {
        match self {
            WikiSection::Characters => "👤",
            WikiSection::Setting => "🌍",
            WikiSection::Lore => "📜",
            WikiSection::Timeline => "⏳",
            WikiSection::Notes => "📌",
        }
    }
}

/// A single wiki page
#[derive(Debug, Clone)]
pub struct WikiPage {
    pub title: String,
    pub content: String,
    pub section: WikiSection,
    pub path: Option<String>,
}

/// Actions from the wiki browser
#[derive(Debug, Clone)]
pub enum WikiBrowserAction {
    SelectPage(usize),
    EditPage(usize),
    OpenInEditor(usize),
}

/// Wiki browser state
#[derive(Debug, Clone, Default)]
pub struct WikiBrowserState {
    pub pages: Vec<WikiPage>,
    pub selected_index: Option<usize>,
    pub search_text: String,
}

impl WikiBrowserState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_page(&mut self, page: WikiPage) {
        self.pages.push(page);
    }

    pub fn clear(&mut self) {
        self.pages.clear();
        self.selected_index = None;
    }

    pub fn filtered_pages(&self) -> Vec<usize> {
        if self.search_text.is_empty() {
            return (0..self.pages.len()).collect();
        }
        let lower = self.search_text.to_lowercase();
        self.pages
            .iter()
            .enumerate()
            .filter(|(_, p)| {
                p.title.to_lowercase().contains(&lower) || p.content.to_lowercase().contains(&lower)
            })
            .map(|(i, _)| i)
            .collect()
    }
}

/// Wiki browser widget
pub struct WikiBrowser;

impl WikiBrowser {
    /// Show the wiki browser panel
    pub fn show(ui: &mut Ui, state: &mut WikiBrowserState) -> Option<WikiBrowserAction> {
        let mut action = None;

        ui.vertical(|ui| {
            // Header
            ui.horizontal(|ui| {
                ui.label(RichText::new("Wiki").strong().size(14.0));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(
                        RichText::new(format!("{} pages", state.pages.len()))
                            .size(10.0)
                            .color(ui.visuals().weak_text_color()),
                    );
                });
            });

            ui.add(
                egui::TextEdit::singleline(&mut state.search_text)
                    .hint_text("Search wiki...")
                    .desired_width(f32::INFINITY),
            );
            ui.separator();

            if state.pages.is_empty() {
                ui.label(
                    RichText::new("No wiki pages yet.")
                        .size(12.0)
                        .color(ui.visuals().weak_text_color()),
                );
                return;
            }

            // Section sidebar + content
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.set_min_width(120.0);
                    let filtered = state.filtered_pages();
                    let scroll_height = (filtered.len() as f32 * 28.0).min(300.0);
                    egui::ScrollArea::vertical()
                        .auto_shrink([false, false])
                        .max_height(scroll_height)
                        .show(ui, |ui| {
                            for &idx in &filtered {
                                if idx >= state.pages.len() {
                                    continue;
                                }
                                let page = &state.pages[idx];
                                let selected = state.selected_index == Some(idx);
                                let icon = page.section.icon();
                                let label = format!("{} {}", icon, page.title);

                                let response = ui.selectable_label(selected, &label);
                                if response.clicked() {
                                    state.selected_index = Some(idx);
                                    action = Some(WikiBrowserAction::SelectPage(idx));
                                }
                                if response.double_clicked() {
                                    action = Some(WikiBrowserAction::EditPage(idx));
                                }
                            }
                        });
                });

                // Content area
                if let Some(idx) = state.selected_index {
                    if idx < state.pages.len() {
                        let page = &state.pages[idx];
                        ui.separator();
                        ui.vertical(|ui| {
                            ui.label(RichText::new(&page.title).strong().size(16.0));
                            let section_label =
                                format!("{} {}", page.section.icon(), page.section.label());
                            ui.label(
                                RichText::new(section_label)
                                    .size(11.0)
                                    .color(ui.visuals().weak_text_color()),
                            );
                            ui.separator();

                            // Render markdown
                            egui::ScrollArea::vertical()
                                .auto_shrink([false, false])
                                .max_height(500.0)
                                .show(ui, |inner_ui| {
                                    // Basic markdown rendering (plain text with headers)
                                    for line in page.content.lines() {
                                        if let Some(rest) = line.strip_prefix("# ") {
                                            inner_ui.heading(rest);
                                        } else if let Some(rest) = line.strip_prefix("## ") {
                                            inner_ui.label(RichText::new(rest).size(18.0).strong());
                                        } else if let Some(rest) = line.strip_prefix("### ") {
                                            inner_ui.label(RichText::new(rest).size(16.0).strong());
                                        } else if line.starts_with("- ") || line.starts_with("* ") {
                                            inner_ui.label(format!("  • {}", &line[2..]));
                                        } else if line.trim().is_empty() {
                                            inner_ui.add_space(4.0);
                                        } else {
                                            inner_ui.label(line);
                                        }
                                    }
                                });

                            if ui.button("✏️ Open in Editor").clicked() {
                                action = Some(WikiBrowserAction::OpenInEditor(idx));
                            }
                        });
                    }
                }
            });
        });

        action
    }

    /// Compact version
    pub fn show_compact(ui: &mut Ui, state: &mut WikiBrowserState) -> Option<WikiBrowserAction> {
        let mut action = None;
        ui.vertical(|ui| {
            ui.label(RichText::new("Wiki").strong().size(12.0));
            let filtered = state.filtered_pages();
            for &idx in filtered.iter().take(5) {
                if idx < state.pages.len() {
                    let page = &state.pages[idx];
                    let label = format!("{} {}", page.section.icon(), page.title);
                    if ui
                        .selectable_label(state.selected_index == Some(idx), &label)
                        .clicked()
                    {
                        state.selected_index = Some(idx);
                        action = Some(WikiBrowserAction::SelectPage(idx));
                    }
                }
            }
            if filtered.len() > 5 {
                ui.label(
                    RichText::new(format!("+{} more", filtered.len() - 5))
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

    #[test]
    fn test_wiki_section_variants() {
        assert_eq!(WikiSection::Characters.label(), "Characters");
        assert_eq!(WikiSection::Setting.label(), "Setting");
        assert_eq!(WikiSection::Lore.label(), "Lore");
        assert_eq!(WikiSection::Timeline.label(), "Timeline");
        assert_eq!(WikiSection::Notes.label(), "Notes");
        assert!(!WikiSection::Characters.icon().is_empty());
    }

    #[test]
    fn test_wiki_browser_state_new() {
        let state = WikiBrowserState::new();
        assert!(state.pages.is_empty());
        assert!(state.search_text.is_empty());
        assert!(state.selected_index.is_none());
    }

    #[test]
    fn test_wiki_browser_state_add_page() {
        let mut state = WikiBrowserState::new();
        state.add_page(WikiPage {
            title: "Hero".into(),
            content: "The main character.".into(),
            section: WikiSection::Characters,
            path: None,
        });
        assert_eq!(state.pages.len(), 1);
        assert_eq!(state.pages[0].title, "Hero");
    }

    #[test]
    fn test_wiki_browser_state_clear() {
        let mut state = WikiBrowserState::new();
        state.add_page(WikiPage {
            title: "Hero".into(),
            content: "Main character.".into(),
            section: WikiSection::Characters,
            path: None,
        });
        state.selected_index = Some(0);
        state.clear();
        assert!(state.pages.is_empty());
        assert!(state.selected_index.is_none());
    }

    #[test]
    fn test_wiki_browser_filtered_pages() {
        let mut state = WikiBrowserState::new();
        state.add_page(WikiPage {
            title: "King Arthur".into(),
            content: "The once and future king.".into(),
            section: WikiSection::Characters,
            path: None,
        });
        state.add_page(WikiPage {
            title: "Camelot".into(),
            content: "The legendary castle and court.".into(),
            section: WikiSection::Setting,
            path: None,
        });
        state.add_page(WikiPage {
            title: "Merlin".into(),
            content: "The wizard who guides Arthur.".into(),
            section: WikiSection::Characters,
            path: None,
        });

        // No filter
        assert_eq!(state.filtered_pages().len(), 3);

        // Filter by title
        state.search_text = "arthur".into();
        assert_eq!(state.filtered_pages().len(), 2);

        // Filter by content
        state.search_text = "wizard".into();
        assert_eq!(state.filtered_pages().len(), 1);
        assert_eq!(state.pages[state.filtered_pages()[0]].title, "Merlin");

        // No match
        state.search_text = "zzzzz".into();
        assert!(state.filtered_pages().is_empty());
    }

    #[test]
    fn test_wiki_browser_action_variants() {
        let actions = [
            WikiBrowserAction::SelectPage(0),
            WikiBrowserAction::EditPage(1),
            WikiBrowserAction::OpenInEditor(2),
        ];
        assert_eq!(actions.len(), 3);
    }

    #[test]
    fn test_wiki_page_construction() {
        let page = WikiPage {
            title: "Test".into(),
            content: "Content".into(),
            section: WikiSection::Notes,
            path: Some("notes/test.md".into()),
        };
        assert_eq!(page.title, "Test");
        assert_eq!(page.path, Some("notes/test.md".into()));
    }
}
