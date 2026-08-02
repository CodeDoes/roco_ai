use roco_cli::test_harness::MockCliRunner;
use std::fs;

#[test]
fn test_story_continue_command() {
    let runner = MockCliRunner::new();

    // First, start a story to create a workspace
    let start_res = runner.run_binary(["story", "A time traveler goes back to the dinosaur era"]);
    start_res.assert_success();
    start_res.assert_stdout_contains("Chapter 3");

    // Now issue the continue command
    let continue_res = runner.run_binary(["story", "--continue"]);
    continue_res.assert_success();

    // The output should indicate it's writing Chapter 4
    continue_res.assert_stdout_contains("Chapter 4");

    // Check if the workspace actually has Chapter 4
    let workspaces_dir = continue_res.workspaces_dir();
    let ws_entries = fs::read_dir(&workspaces_dir).unwrap();
    let mut found_ws = None;
    for entry in ws_entries.flatten() {
        if entry.path().is_dir() && entry.file_name() != "active" {
            found_ws = Some(entry.path());
            break;
        }
    }

    let ws_dir = found_ws.unwrap();
    assert!(
        ws_dir.join("03-CHAPTER_4.md").exists(),
        "Chapter 4 should have been created in the workspace"
    );

    // Assert that a backup was created
    let backups_dir = continue_res.roco_dir().join("backups");
    assert!(backups_dir.exists(), "Backups directory should exist");
    let backups_entries = fs::read_dir(&backups_dir).unwrap();
    let mut has_backup = false;
    for entry in backups_entries.flatten() {
        if entry.path().is_dir() {
            has_backup = true;
            break;
        }
    }
    assert!(
        has_backup,
        "A backup should have been created before continuing the story"
    );
}
