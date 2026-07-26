//! Integration and validation test suite for GUI Markdown Editor capabilities.

use roco_ui::{
    FileTreeAction, MarkdownDocument, RightPanelTool, RocoDesktopApp, Suggestion, SuggestionKind,
    TextRange,
};

#[test]
fn test_gui_editor_document_suggestion_lifecycle() {
    let mut doc = MarkdownDocument::new("Chapter 1: The journey begins.".to_string());

    let suggestion = Suggestion {
        id: "s1".to_string(),
        range: TextRange::new(11, 29),
        original_text: "The journey begins".to_string(),
        suggested_text: "The fog sets in".to_string(),
        kind: SuggestionKind::Replace,
        timestamp: chrono::Utc::now(),
        accepted: false,
        rejected: false,
    };

    doc.add_suggestion(suggestion);
    assert_eq!(doc.suggestions.len(), 1);

    // Accept suggestion
    assert!(doc.accept_suggestion("s1"));
    assert_eq!(doc.text, "Chapter 1: The fog sets in.");
    assert_eq!(doc.suggestions.len(), 0);
}

#[test]
fn test_gui_editor_file_tree_open_and_edit() {
    let tmp_dir = std::env::temp_dir().join(format!("roco_editor_gui_{}", std::process::id()));
    std::fs::create_dir_all(&tmp_dir).unwrap();
    let draft_file = tmp_dir.join("draft.md");
    std::fs::write(&draft_file, "# Draft Title\n\nInitial draft content.").unwrap();

    let mut app = RocoDesktopApp::new(None);
    app.handle_file_tree_action(FileTreeAction::OpenFile(draft_file.clone()));

    assert_eq!(app.right_panel_tool, Some(RightPanelTool::Editor));
    assert_eq!(
        app.editor_state.document.text,
        "# Draft Title\n\nInitial draft content."
    );

    // User edits the text in the editor
    app.editor_state
        .document
        .text
        .push_str("\n\nAdded chapter continuation.");
    assert!(app.editor_state.document.text.contains("continuation"));

    let _ = std::fs::remove_dir_all(tmp_dir);
}
