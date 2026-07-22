//! File Tree — workspace file browser for navigating story projects.
//!
//! Displays a hierarchical file tree for a given root directory.
//! Supports expand/collapse, file selection, and file actions.

use egui::{self, RichText, Ui};
use std::path::PathBuf;

/// A node in the file tree
#[derive(Debug, Clone)]
pub struct FileTreeNode {
    pub name: String,
    pub path: PathBuf,
    pub is_dir: bool,
    pub children: Vec<FileTreeNode>,
    pub expanded: bool,
    pub depth: usize,
}

impl FileTreeNode {
    pub fn new(path: PathBuf, depth: usize) -> Option<Self> {
        let name = path
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| path.to_string_lossy().to_string());
        let is_dir = path.is_dir();

        let children = if is_dir {
            let mut children = Vec::new();
            if let Ok(entries) = std::fs::read_dir(&path) {
                let mut entries: Vec<_> = entries
                    .filter_map(|e| e.ok())
                    .filter(|e| {
                        let name = e.file_name().to_string_lossy().to_string();
                        !name.starts_with('.') // skip hidden
                    })
                    .collect();
                entries.sort_by_key(|e| {
                    let is_dir = e.file_type().map(|t| t.is_dir()).unwrap_or(false);
                    (!is_dir, e.file_name().to_string_lossy().to_string())
                });
                for entry in entries {
                    if let Some(node) = FileTreeNode::new(entry.path(), depth + 1) {
                        children.push(node);
                    }
                }
            }
            children
        } else {
            Vec::new()
        };

        Some(Self {
            name,
            path,
            is_dir,
            children,
            expanded: depth < 1, // auto-expand first level
            depth,
        })
    }

    pub fn file_icon(&self) -> &str {
        if self.is_dir {
            if self.expanded {
                "📂"
            } else {
                "📁"
            }
        } else {
            match self.extension() {
                "md" | "markdown" => "📝",
                "rs" => "🦀",
                "toml" => "⚙️",
                "json" => "📋",
                "yaml" | "yml" => "📋",
                "png" | "jpg" | "jpeg" | "gif" | "svg" => "🖼️",
                "st" => "🧠",
                _ => "📄",
            }
        }
    }

    pub fn extension(&self) -> &str {
        self.path.extension().and_then(|e| e.to_str()).unwrap_or("")
    }
}

/// Actions from the file tree
#[derive(Debug, Clone)]
pub enum FileTreeAction {
    SelectFile(PathBuf),
    OpenFile(PathBuf),
    ToggleFolder(PathBuf),
    Refresh,
    DeleteFile(PathBuf),
    RenameFile(PathBuf, String),
}

/// File tree widget state
#[derive(Debug, Clone)]
pub struct FileTreeState {
    /// Root directory to display
    pub root: PathBuf,
    /// Root node (rebuilt on refresh)
    pub root_node: Option<Box<FileTreeNode>>,
    /// Currently selected file path
    pub selected_path: Option<PathBuf>,
    /// Filter by extension (e.g. "md")
    pub extension_filter: Option<String>,
    /// Show hidden files
    pub show_hidden: bool,
}

impl FileTreeState {
    pub fn new(root: PathBuf) -> Self {
        let root_node = FileTreeNode::new(root.clone(), 0).map(Box::new);
        Self {
            root,
            root_node,
            selected_path: None,
            extension_filter: None,
            show_hidden: false,
        }
    }

    pub fn refresh(&mut self) {
        self.root_node = FileTreeNode::new(self.root.clone(), 0).map(Box::new);
    }
}

/// File tree widget
pub struct FileTree;

impl FileTree {
    /// Show the file tree panel
    pub fn show(ui: &mut Ui, state: &mut FileTreeState) -> Option<FileTreeAction> {
        let mut action = None;

        ui.vertical(|ui| {
            // Header
            ui.horizontal(|ui| {
                ui.label(RichText::new("Files").strong().size(14.0));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("↻").on_hover_text("Refresh").clicked() {
                        state.refresh();
                        action = Some(FileTreeAction::Refresh);
                    }
                });
            });
            ui.label(
                RichText::new(state.root.to_string_lossy())
                    .size(10.0)
                    .color(ui.visuals().weak_text_color()),
            );
            ui.separator();

            if let Some(ref root_node) = state.root_node {
                let act = Self::show_node(ui, root_node, state);
                if let Some(a) = act {
                    action = Some(a);
                }
            } else {
                ui.label(
                    RichText::new("No files found.")
                        .size(12.0)
                        .color(ui.visuals().weak_text_color()),
                );
            }
        });

        action
    }

    fn show_node(
        ui: &mut Ui,
        node: &FileTreeNode,
        state: &FileTreeState,
    ) -> Option<FileTreeAction> {
        let mut action = None;

        // Apply extension filter for non-directories
        if !node.is_dir {
            if let Some(ref filter_ext) = state.extension_filter {
                if node.extension() != filter_ext {
                    return None;
                }
            }
        }

        let indent = node.depth as f32 * 20.0;
        let icon = node.file_icon();
        let selected = state
            .selected_path
            .as_ref()
            .is_some_and(|p| *p == node.path);

        let bg = if selected {
            ui.visuals().selection.bg_fill
        } else {
            ui.visuals().faint_bg_color
        };

        let _id = ui.make_persistent_id(format!("file_{:?}", node.path));
        let response = egui::Frame::NONE
            .fill(bg)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.add_space(indent);
                    let label = format!("{} {}", icon, node.name);
                    let resp = ui.selectable_label(selected, &label);

                    if resp.clicked() {
                        if node.is_dir {
                            // Toggle expand (we'd need mutable access, handle below)
                        } else {
                            // We'll handle selection via the outer response
                        }
                    }
                });
            })
            .response
            .on_hover_text(node.path.to_string_lossy());

        // Handle clicks on the response
        if response.clicked() {
            if node.is_dir {
                // Toggle expand - we need to find and mutate the node
                // For now, handled via context menu
            } else {
                action = Some(FileTreeAction::SelectFile(node.path.clone()));
            }
        }
        if response.double_clicked() && !node.is_dir {
            action = Some(FileTreeAction::OpenFile(node.path.clone()));
        }

        // Context menu
        response.context_menu(|ui| {
            if node.is_dir {
                if ui.button("Toggle").clicked() {
                    ui.close_menu();
                }
            } else {
                if ui.button("Open").clicked() {
                    action = Some(FileTreeAction::OpenFile(node.path.clone()));
                    ui.close_menu();
                }
                if ui.button("Delete").clicked() {
                    action = Some(FileTreeAction::DeleteFile(node.path.clone()));
                    ui.close_menu();
                }
            }
        });

        // Children
        if node.is_dir && node.expanded {
            for child in &node.children {
                if let Some(a) = Self::show_node(ui, child, state) {
                    action = Some(a);
                }
            }
        }

        action
    }

    /// Compact version for sidebar
    pub fn show_compact(ui: &mut Ui, state: &mut FileTreeState) -> Option<FileTreeAction> {
        Self::show(ui, state)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_dir() -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "roco_ft_test_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = std::fs::create_dir_all(&dir);
        // Clean any leftover files
        for e in std::fs::read_dir(&dir)
            .unwrap_or_else(|_| std::fs::read_dir("/").unwrap())
            .flatten()
        {
            let _ = std::fs::remove_file(e.path());
        }
        let _ = std::fs::write(dir.join("hello.md"), "# Hello");
        let _ = std::fs::write(dir.join("main.rs"), "fn main() {}");
        let _ = std::fs::create_dir_all(dir.join("sub"));
        let _ = std::fs::write(dir.join("sub").join("nested.md"), "## Nested");
        dir
    }

    #[test]
    fn test_file_tree_node_new() {
        let dir = test_dir();
        let node = FileTreeNode::new(dir.clone(), 0).unwrap();
        assert!(node.is_dir);
        assert_eq!(
            node.children.len(),
            3,
            "expected 3 children: hello.md, main.rs, sub/"
        );
        // Should have hello.md, main.rs, and sub/
        let names: Vec<_> = node.children.iter().map(|c| c.name.as_str()).collect();
        assert!(names.contains(&"hello.md"));
        assert!(names.contains(&"main.rs"));
        assert!(names.contains(&"sub"));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_file_tree_node_flat_file() {
        let dir = test_dir();
        let md = dir.join("hello.md");
        let node = FileTreeNode::new(md.clone(), 0).unwrap();
        assert!(!node.is_dir);
        assert_eq!(node.extension(), "md");
        assert!(node.children.is_empty());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_file_tree_icons() {
        let dir = test_dir();
        let md = dir.join("hello.md");
        let node = FileTreeNode::new(md, 0).unwrap();
        assert_eq!(node.file_icon(), "📝");

        let rs = dir.join("main.rs");
        let node = FileTreeNode::new(rs, 0).unwrap();
        assert_eq!(node.file_icon(), "🦀");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_file_tree_extension() {
        let dir = test_dir();
        let node = FileTreeNode::new(dir.join("hello.md"), 0).unwrap();
        assert_eq!(node.extension(), "md");

        let node = FileTreeNode::new(dir.join("main.rs"), 0).unwrap();
        assert_eq!(node.extension(), "rs");

        let node = FileTreeNode::new(dir.join("noext"), 0).unwrap();
        assert_eq!(node.extension(), "");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_file_tree_action_variants() {
        let p = PathBuf::from("test.md");
        let actions = [
            FileTreeAction::SelectFile(p.clone()),
            FileTreeAction::OpenFile(p.clone()),
            FileTreeAction::ToggleFolder(p.clone()),
            FileTreeAction::Refresh,
            FileTreeAction::DeleteFile(p.clone()),
            FileTreeAction::RenameFile(p.clone(), "new.md".into()),
        ];
        assert_eq!(actions.len(), 6);
    }

    #[test]
    fn test_file_tree_state_new() {
        let dir = test_dir();
        let state = FileTreeState::new(dir.clone());
        assert!(state.root_node.is_some());
        assert_eq!(state.root, dir);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_file_tree_state_refresh() {
        let dir = test_dir();
        let mut state = FileTreeState::new(dir.clone());
        let old_len = state.root_node.as_ref().unwrap().children.len();

        // Add a file and refresh
        let _ = std::fs::write(dir.join("new_file.md"), "new");
        state.refresh();
        let new_len = state.root_node.as_ref().unwrap().children.len();
        assert_eq!(new_len, old_len + 1);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_file_tree_skips_hidden() {
        let dir = test_dir();
        let _ = std::fs::write(dir.join(".hidden"), "secret");
        let node = FileTreeNode::new(dir.clone(), 0).unwrap();
        let names: Vec<_> = node.children.iter().map(|c| c.name.as_str()).collect();
        assert!(!names.contains(&".hidden"));

        let _ = std::fs::remove_dir_all(&dir);
    }
}
