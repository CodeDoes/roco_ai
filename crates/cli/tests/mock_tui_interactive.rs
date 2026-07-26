//! Automated tests for interactive TUI / REPL interfaces against `MockBackend`.

use roco_cli::test_harness::{MockCliRunner, ScriptedTuiSession};

#[test]
fn test_interactive_pacing_planning_mode() {
    let session = ScriptedTuiSession::new()
        .type_line("Once upon a time in a kingdom far away")
        .type_line("/save")
        .type_line("/quit");

    let res = session.run_subcommand("interact", &["--pace", "planning"]);
    res.assert_success();
    res.assert_stdout_contains("RoCo AI — Chat");
    res.assert_stdout_contains("Session saved. Goodbye!");

    let saved_session = res.assert_latest_session();
    assert_eq!(saved_session.pacing, "planning");
    assert!(saved_session.messages.len() >= 2);
}

#[test]
fn test_interactive_pacing_careful_mode_controls() {
    let session = ScriptedTuiSession::new()
        .type_line("A detective arrives at a dark alley")
        .type_line("/accept")
        .type_line("/pace rolling")
        .type_line("/pace auto")
        .type_line("/undo")
        .type_line("/save")
        .type_line("/quit");

    let res = session.run_subcommand("interact", &["--pace", "careful"]);
    res.assert_success();
    res.assert_stdout_contains("Accepted. Continuing...");
    res.assert_stdout_contains("Pacing: Rolling");
    res.assert_stdout_contains("Pacing: Auto-Accept");
    res.assert_stdout_contains("Session saved. Goodbye!");
}

#[test]
fn test_interactive_resume_mode() {
    let runner = MockCliRunner::new();

    // 1. Create initial session
    let res1 = runner.run_binary(["interact", "--prompt", "Initial chapter premise"]);
    res1.assert_success();
    let initial_session = res1.assert_latest_session();
    let session_id = initial_session.id;

    // 2. Resume session
    let session = ScriptedTuiSession::with_runner(runner)
        .type_line("What happens in chapter 2?")
        .type_line("/quit");

    let res2 = session.run_subcommand("interact", &["--resume", &session_id]);
    res2.assert_success();
    res2.assert_stdout_contains(&format!("Resuming Session: {session_id}"));
    res2.assert_stdout_contains("Reviewing past messages:");

    let resumed_session = res2.assert_latest_session();
    assert!(
        resumed_session.messages.len() >= 4,
        "Resumed session should contain history + new turns"
    );
}

#[test]
fn test_interactive_story_mode_workspace_lock() {
    let session = ScriptedTuiSession::new()
        .type_line("/lock test_workspace")
        .type_line("/status")
        .type_line("/unlock")
        .type_line("/quit");

    let res = session.run_subcommand("story-mode", &[]);
    res.assert_success();
    res.assert_stdout_contains("RoCo Story Mode");
    res.assert_stdout_contains("RoCo ready");
    res.assert_stdout_contains("Goodbye!");
}

#[test]
fn test_interactive_game_mode_colon_commands() {
    let session = ScriptedTuiSession::new()
        .type_line(":look")
        .type_line(":inventory")
        .type_line("I explore the mysterious door to the left")
        .type_line(":quit");

    let res = session.run_subcommand("game", &["mysterious island"]);
    res.assert_success();
    res.assert_stdout_contains("Scenario: mysterious island");
    res.assert_stdout_contains("Thanks for playing! Goodbye.");
}

#[test]
fn test_interactive_coder_mode_colon_commands() {
    let session = ScriptedTuiSession::new()
        .type_line("How do I parse JSON in Rust?")
        .type_line(":history")
        .type_line(":clear")
        .type_line(":quit");

    let res = session.run_subcommand("code", &[]);
    res.assert_success();
    res.assert_stdout_contains("RoCo AI — Coder Mode");
    res.assert_stdout_contains("Conversation");
    res.assert_stdout_contains("Conversation history cleared.");
    res.assert_stdout_contains("Happy coding! Goodbye.");
}

#[test]
fn test_interactive_router_mode_switching() {
    let session = ScriptedTuiSession::new()
        .type_line("Hello! I want to ask a general question.")
        .type_line(":mode")
        .type_line(":code")
        .type_line(":adventure")
        .type_line(":quit");

    let res = session.run_subcommand("router", &[]);
    res.assert_success();
    res.assert_stdout_contains("Current mode: 💬 Chat");
    res.assert_stdout_contains("Switched to coder mode.");
    res.assert_stdout_contains("Switched to adventure mode.");
    res.assert_stdout_contains("Goodbye!");
}

#[test]
fn test_interactive_router_hi_turn_and_quit() {
    let session = ScriptedTuiSession::new().type_line("hi").type_line(":q");

    let res = session.run_default_cli();
    res.assert_success();
    res.assert_stdout_contains("RoCo AI — Mode Router");
    res.assert_stdout_contains("💬 Chat >");
}
