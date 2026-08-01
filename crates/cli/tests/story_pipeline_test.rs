//! End-to-end integration test for the full story pipeline (outline -> wiki -> chapter -> validate -> synopsis).

use roco_cli::test_harness::MockCliRunner;
use std::fs;

#[test]
fn test_cli_full_story_pipeline() {
    let runner = MockCliRunner::new();

    let res = runner.run_binary(["story", "A clockmaker builds a device that freezes time"]);
    println!("STDOUT:\n{}", res.stdout);
    println!("STDERR:\n{}", res.stderr);
    res.assert_success();

    // Check stdout for all phases
    res.assert_stdout_contains("Outline...");
    res.assert_stdout_contains("Worldbuilding...");
    res.assert_stdout_contains("Chapter 1");
    res.assert_stdout_contains("Chapter 2");
    res.assert_stdout_contains("Chapter 3");
    res.assert_stdout_contains("Synopsis...");
    res.assert_stdout_contains("Publishing...");
    res.assert_stdout_contains("Done! 8 files in workspace:");

    // Verify workspace files
    let workspaces_dir = res.workspaces_dir();
    assert!(workspaces_dir.exists(), "Workspaces directory should exist");

    let ws_entries = fs::read_dir(&workspaces_dir).unwrap();
    let mut found_ws = None;
    for entry in ws_entries.flatten() {
        if entry.path().is_dir() && entry.file_name() != "active" {
            found_ws = Some(entry.path());
            break;
        }
    }

    let ws_dir = found_ws.expect("A workspace directory should have been created");

    let expected_files = [
        "01-OUTLINE.md",
        "02-WIKI.md",
        "03-CHAPTER_1.md",
        "03-CHAPTER_2.md",
        "03-CHAPTER_3.md",
        "04-VALIDATION.md",
        "05-SYNOPSIS.md",
        "06-STORY.md",
    ];

    for file in &expected_files {
        let file_path = ws_dir.join(file);
        assert!(
            file_path.exists(),
            "Expected file {} to exist in workspace",
            file
        );
        let content = fs::read_to_string(&file_path).unwrap();
        assert!(!content.is_empty(), "File {} should not be empty", file);
    }

    // Verify stories directory
    let stories = res.list_stories();
    assert!(
        !stories.is_empty(),
        "Expected at least one story file published in .roco/stories/"
    );
}
