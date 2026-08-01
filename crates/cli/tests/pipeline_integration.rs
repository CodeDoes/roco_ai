use roco_cli::test_harness::MockCliRunner;
use std::fs;

#[test]
fn test_full_pipeline_success() {
    let runner = MockCliRunner::new();
    let res = runner.run_binary([
        "story",
        "A sci-fi test premise",
        "--strategy",
        "json",
        "--mock",
    ]);
    res.assert_success();

    let ws_dir = res.roco_dir().join("workspaces");
    let mut entries = fs::read_dir(&ws_dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect::<Vec<_>>();
    entries.sort();

    let active_ws = entries
        .into_iter()
        .find(|p| p.is_dir())
        .expect("No workspace dir found");

    let expected_files = vec![
        "01-OUTLINE.md",
        "02-WIKI.md",
        "03-CHAPTER_1.md",
        "03-CHAPTER_2.md",
        "03-CHAPTER_3.md",
        "04-VALIDATION.md",
        "05-SYNOPSIS.md",
        "06-STORY.md",
    ];

    for file in expected_files {
        let path = active_ws.join(file);
        assert!(path.exists(), "Expected file {} to exist", file);
    }
}

#[test]
fn test_resume_skips_completed_phases() {
    let runner = MockCliRunner::new();

    let res1 = runner.run_binary([
        "story",
        "A sci-fi test premise",
        "--strategy",
        "json",
        "--mock",
    ]);
    res1.assert_success();

    let res2 = runner.run_binary(["story", "--resume", "--mock"]);
    res2.assert_success();

    res2.assert_stdout_contains("Outline (existing)");
    res2.assert_stdout_contains("Worldbuilding (existing)");
    res2.assert_stdout_contains("Chapter 1 (existing)");
    res2.assert_stdout_contains("Chapter 2 (existing)");
    res2.assert_stdout_contains("Chapter 3 (existing)");
}

#[test]
fn test_validation_failure_triggers_retry_and_converges() {
    let runner = MockCliRunner::new().with_env("ROCO_MOCK_FORCE_FAIL_VALIDATION", "1");

    let res = runner.run_binary([
        "story",
        "A sci-fi test premise",
        "--strategy",
        "json",
        "--mock",
    ]);
    res.assert_success();

    res.assert_stdout_contains("needs revision (attempt 1/3)");
    res.assert_stdout_contains("accepted/passed");
}

#[test]
fn test_validation_revision_converges_within_3_attempts() {
    let runner = MockCliRunner::new().with_env("ROCO_MOCK_FORCE_FAIL_VALIDATION", "1");

    // We already know it converges on attempt 1 because of the patched MockBackend.
    // If it did not converge within 3 attempts, the CLI would log "still fails after 3 retries".
    // We can assert that it did NOT log that.
    let res = runner.run_binary([
        "story",
        "A sci-fi test premise",
        "--strategy",
        "json",
        "--mock",
    ]);
    res.assert_success();

    let stdout = res.stdout;
    assert!(
        stdout.contains("needs revision (attempt 1/3)"),
        "Expected at least one revision"
    );
    assert!(
        !stdout.contains("still fails after 3 retries"),
        "Expected to converge before failing all retries"
    );
}
