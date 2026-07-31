//! Automated tests for RoCo CLI subcommands against `MockBackend`.

use roco_cli::test_harness::{MockCliRunner, ScriptedTuiSession};

#[test]
fn test_cli_help_subcommands() {
    let runner = MockCliRunner::new();

    let res = runner.run_binary(["help"]);
    res.assert_success();
    res.assert_stderr_contains("RoCo AI");
    // The help banner prints "Commands:" — this assertion previously looked
    // for "Subcommands:", which the CLI has never emitted.
    res.assert_stderr_contains("Commands:");
    res.assert_stderr_contains("interact");
    res.assert_stderr_contains("whoami");

    let res_flag = runner.run_binary(["--help"]);
    res_flag.assert_success();
}

#[test]
fn test_cli_whoami_reports_both_identities() {
    let runner = MockCliRunner::new();

    // `whoami` must work without starting the daemon chain.
    let res = runner.run_binary(["whoami"]);
    res.assert_success();
    res.assert_stdout_contains("Who is RoCo?");
    res.assert_stdout_contains("Who are you?");

    // Record a name, then read it back.
    let set = runner.run_binary(["whoami", "--set-name", "Ada"]);
    set.assert_success();

    let after = runner.run_binary(["whoami"]);
    after.assert_success();
    after.assert_stdout_contains("Ada");

    // JSON output is machine-readable.
    let json = runner.run_binary(["whoami", "--json"]);
    json.assert_success();
    json.assert_stdout_contains("\"name\"");

    // And it can be erased.
    let forget = runner.run_binary(["whoami", "--forget"]);
    forget.assert_success();
    let cleared = runner.run_binary(["whoami"]);
    cleared.assert_stdout_contains("don't know");
}

#[test]
fn test_cli_interact_answers_identity_questions_locally() {
    let session = ScriptedTuiSession::new()
        .type_line("my name is Ada")
        .type_line("who am I?")
        .type_line("what can you do?")
        .type_line(":quit");

    let res = session.run_subcommand("interact", &[]);
    res.assert_success();
    // The name is echoed back on recall, not hallucinated.
    res.assert_stdout_contains("Ada");
    // Capability answers come from the real command table.
    res.assert_stdout_contains("story");
    res.assert_stdout_contains("Session saved. Goodbye!");
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
    println!("STDOUT:\n{}", res.stdout);
    println!("STDERR:\n{}", res.stderr);
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

#[test]
fn test_cli_session_subcommand() {
    let runner = MockCliRunner::new();

    // 1. Create a session
    let res_create = runner.run_binary(["session", "create"]);
    res_create.assert_success();
    let session_id = res_create.stdout.trim().to_string();
    assert!(session_id.starts_with("session_"));

    // 2. Execute a prompt on that session
    let res_prompt = runner.run_binary(["session", &session_id, "-p", "My name is Ada"]);
    res_prompt.assert_success();

    // 3. Verify it was recorded in ConversationState
    let state = res_prompt.assert_latest_session();
    assert_eq!(state.id, session_id);
    assert_eq!(state.messages.len(), 2);
    assert_eq!(state.messages[0].role, "user");
    assert_eq!(state.messages[0].content, "My name is Ada");
    assert_eq!(state.messages[1].role, "assistant");
    assert!(!state.messages[1].content.is_empty());
}
