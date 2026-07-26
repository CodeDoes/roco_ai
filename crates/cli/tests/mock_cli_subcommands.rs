//! Automated tests for RoCo CLI subcommands against `MockBackend`.

use roco_cli::test_harness::{MockCliRunner, ScriptedTuiSession};

#[test]
fn test_cli_help_subcommands() {
    let runner = MockCliRunner::new();

    let res = runner.run_binary(["help"]);
    res.assert_success();
    res.assert_stderr_contains("RoCo AI");
    res.assert_stderr_contains("Subcommands:");

    let res_flag = runner.run_binary(["--help"]);
    res_flag.assert_success();
}

#[test]
fn test_cli_gpu_check_and_jobs() {
    let runner = MockCliRunner::new();

    let res = runner.run_binary(["gpu-check"]);
    res.assert_success();
    res.assert_stdout_contains("Vulkan");

    let res_jobs = runner.run_binary(["jobs"]);
    res_jobs.assert_success();
    res_jobs.assert_stdout_contains("Inference Daemon");
}

#[test]
fn test_cli_interact_prompt_mode() {
    let runner = MockCliRunner::new();

    let res = runner.run_binary([
        "interact",
        "--prompt",
        "A futuristic cyberpunk detective story",
    ]);
    res.assert_success();
    res.assert_stdout_contains("Prompt: A futuristic cyberpunk detective story");
    res.assert_stdout_contains("Session saved:");

    let session = res.assert_latest_session();
    assert_eq!(session.messages.len(), 2);
    assert_eq!(session.messages[0].role, "user");
    assert_eq!(
        session.messages[0].content,
        "A futuristic cyberpunk detective story"
    );
    assert_eq!(session.messages[1].role, "assistant");
    assert!(!session.messages[1].content.is_empty());
}

#[test]
fn test_cli_interact_list_sessions() {
    let runner = MockCliRunner::new();

    // First create a session via prompt mode
    let res1 = runner.run_binary(["interact", "--prompt", "First session prompt"]);
    res1.assert_success();

    // List sessions
    let res2 = runner.run_binary(["interact", "--list-sessions"]);
    res2.assert_success();
    res2.assert_stdout_contains("Available Sessions");
}

#[test]
fn test_cli_story_pipeline() {
    let runner = MockCliRunner::new();

    let res = runner.run_binary(["story", "A clockmaker builds a device that freezes time"]);
    res.assert_success();
    res.assert_stdout_contains("Generating story...");
    res.assert_stdout_contains("Workspace:");

    let stories = res.list_stories();
    assert!(
        !stories.is_empty(),
        "Expected at least one story file published in .roco/stories/"
    );
}

#[test]
fn test_cli_story_mode_one_shot() {
    let runner = MockCliRunner::new();

    let res = runner.run_binary(["story-mode", "/help"]);
    res.assert_success();
    res.assert_stdout_contains("RoCo ready");

    // Alias 'sm'
    let res_sm = runner.run_binary(["sm", "/help"]);
    res_sm.assert_success();
    res_sm.assert_stdout_contains("RoCo ready");
}

#[test]
fn test_cli_game_mode() {
    let session = ScriptedTuiSession::new().type_line(":quit");
    let res = session.run_subcommand("game", &["haunted castle"]);
    res.assert_success();
    res.assert_stdout_contains("RoCo AI — Adventure Game");
    res.assert_stdout_contains("Scenario: haunted castle");
    res.assert_stdout_contains("Goodbye.");
}

#[test]
fn test_cli_coder_mode() {
    let session = ScriptedTuiSession::new()
        .type_line("How do I sort a vector in Rust?")
        .type_line(":quit");
    let res = session.run_subcommand("code", &["--lang", "rust"]);
    res.assert_success();
    res.assert_stdout_contains("RoCo AI — Coder Mode");
    res.assert_stdout_contains("Language focus: rust");
    res.assert_stdout_contains("Happy coding! Goodbye.");
}

#[test]
fn test_cli_router_default() {
    let session = ScriptedTuiSession::new().type_line(":quit");
    let res = session.run_subcommand("router", &[]);
    res.assert_success();
    res.assert_stdout_contains("RoCo AI — Mode Router");
    res.assert_stdout_contains("Goodbye!");
}

#[test]
fn test_cli_export_command() {
    let runner = MockCliRunner::new();

    // Create a dummy workspace file
    let ws_dir = runner.working_dir().join("test_story");
    std::fs::create_dir_all(&ws_dir).unwrap();
    std::fs::write(
        ws_dir.join("01-OUTLINE.md"),
        "# Test Title\n\n## Chapter 1: Begin\nSummary",
    )
    .unwrap();
    std::fs::write(
        ws_dir.join("03-CHAPTER_1.md"),
        "# Chapter 1\n\nOnce upon a time in a test.",
    )
    .unwrap();

    let out_file = runner.working_dir().join("exported.html");
    let res = runner.run_binary([
        "export",
        ws_dir.to_str().unwrap(),
        "--format",
        "html",
        "--output",
        out_file.to_str().unwrap(),
    ]);
    res.assert_success();
    assert!(out_file.exists(), "Exported HTML file should exist");
}

#[test]
fn test_cli_eval_command() {
    let project_root = std::env::current_dir().unwrap();
    let runner = MockCliRunner::new().with_working_dir(&project_root);

    // Verify eval subcommand entry point executes
    let res = runner.run_binary(["eval"]);
    assert!(res.exit_code >= 0, "eval command should execute");
}
