use std::fs;
use std::thread;

use roco_cli::test_harness::MockCliRunner;
use roco_protocol::ConversationState;

#[test]
fn test_session_create_writes_file() {
    let runner = MockCliRunner::new();
    let res = runner.run_binary(["session", "create"]);
    res.assert_success();

    let session_id = res.stdout.trim();
    assert!(session_id.starts_with("session_"));

    let session_path = res.sessions_dir().join(format!("{}.json", session_id));
    assert!(session_path.exists());

    let state = ConversationState::load(&session_path).unwrap();
    assert_eq!(state.id, session_id);
}

#[test]
fn test_session_load_reads_transcript() {
    let runner = MockCliRunner::new();
    let res = runner.run_binary(["session", "create"]);
    res.assert_success();
    let session_id = res.stdout.trim();

    // Send first turn
    runner
        .run_binary(["session", session_id, "-p", "Hello"])
        .assert_success();

    // Verify transcript
    let session_path = res.sessions_dir().join(format!("{}.json", session_id));
    let state = ConversationState::load(&session_path).unwrap();
    assert_eq!(state.messages.len(), 2);
    assert_eq!(state.messages[0].role, "user");
    assert_eq!(state.messages[0].content, "Hello");
    assert_eq!(state.messages[1].role, "assistant");
}

#[test]
fn test_multi_turn_conversation_persists() {
    let runner = MockCliRunner::new();
    let res = runner.run_binary(["session", "create"]);
    res.assert_success();
    let session_id = res.stdout.trim();

    // First turn
    runner
        .run_binary(["session", session_id, "-p", "Hello"])
        .assert_success();

    // Second turn
    runner
        .run_binary(["session", session_id, "-p", "How are you?"])
        .assert_success();

    // Verify both turns persisted
    let session_path = res.sessions_dir().join(format!("{}.json", session_id));
    let state = ConversationState::load(&session_path).unwrap();
    assert_eq!(state.messages.len(), 4);
    assert_eq!(state.messages[0].content, "Hello");
    assert_eq!(state.messages[2].content, "How are you?");
}

#[test]
fn test_session_delete_removes_file() {
    let runner = MockCliRunner::new();
    let res = runner.run_binary(["session", "create"]);
    res.assert_success();
    let session_id = res.stdout.trim();

    let session_path = res.sessions_dir().join(format!("{}.json", session_id));
    assert!(session_path.exists());

    runner
        .run_binary(["session", "delete", session_id])
        .assert_success();
    assert!(!session_path.exists());
}

#[test]
fn test_session_list_shows_all() {
    let runner = MockCliRunner::new();

    let res1 = runner.run_binary(["session", "create"]);
    let id1 = res1.stdout.trim().to_string();

    let res2 = runner.run_binary(["session", "create"]);
    let id2 = res2.stdout.trim().to_string();

    let list_res = runner.run_binary(["session", "list"]);
    list_res.assert_success();
    list_res.assert_stdout_contains(&id1);
    list_res.assert_stdout_contains(&id2);
}

#[test]
fn test_session_show_displays_messages() {
    let runner = MockCliRunner::new();
    let res = runner.run_binary(["session", "create"]);
    let session_id = res.stdout.trim();

    runner
        .run_binary(["session", session_id, "-p", "Test message"])
        .assert_success();

    let show_res = runner.run_binary(["session", "show", session_id]);
    show_res.assert_success();
    show_res.assert_stdout_contains("Test message");
    show_res.assert_stdout_contains("You");
    show_res.assert_stdout_contains("RoCo");
}

#[test]
fn test_concurrent_session_access() {
    let runner = MockCliRunner::new();
    let res = runner.run_binary(["session", "create"]);
    let session_id = res.stdout.trim().to_string();

    let mut handles = vec![];
    for i in 0..5 {
        let runner_clone = runner.clone();
        let sid = session_id.clone();
        let handle = thread::spawn(move || {
            let msg = format!("Message {}", i);
            runner_clone
                .run_binary(["session", &sid, "-p", &msg])
                .assert_success();
        });
        handles.push(handle);
    }

    for handle in handles {
        handle.join().unwrap();
    }

    let session_path = res.sessions_dir().join(format!("{}.json", session_id));
    let state = ConversationState::load(&session_path).unwrap();
    assert!(!state.messages.is_empty());
}

#[test]
fn test_corrupted_json_file_handled_gracefully() {
    let runner = MockCliRunner::new();
    let res = runner.run_binary(["session", "list"]);
    let session_id = "corrupted_123";
    let session_dir = res.sessions_dir();
    fs::create_dir_all(&session_dir).unwrap();

    let path = session_dir.join(format!("{}.json", session_id));
    fs::write(&path, "{ invalid json format").unwrap();

    let res_show = runner.run_binary(["session", "show", session_id]);
    assert_eq!(res_show.exit_code, 1);
    res_show.assert_stderr_contains("Error: Failed to parse session");
}
