//! Right panel tool enum and display helpers for the desktop app.

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
