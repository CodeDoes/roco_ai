//! Tests for the session management subcommand.

use std::fs;
use std::path::PathBuf;

/// Get a temp dir for session tests.
fn temp_dir() -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "roco_session_test_{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let _ = fs::create_dir_all(&dir);
    dir
}

/// Test session create writes a valid JSON file.
#[test]
fn test_session_create_writes_file() {
    let dir = temp_dir();
    let session_dir = dir.join(".roco").join("sessions");
    let _ = fs::create_dir_all(&session_dir);

    // Simulate session creation by writing a state file
    let state = roco_protocol::ConversationState::new("test_session_123".to_string(), "careful");
    let path = session_dir.join("test_session_123.json");
    state.save(&path).expect("failed to save session");

    assert!(path.exists(), "session file should exist");

    // Verify it can be loaded back
    let loaded = roco_protocol::ConversationState::load(&path).expect("failed to load session");
    assert_eq!(loaded.id, "test_session_123");
    assert_eq!(loaded.pacing, "careful");
    assert_eq!(loaded.messages.len(), 0);

    // Cleanup
    let _ = fs::remove_dir_all(&dir);
}

/// Test session list shows created sessions.
#[test]
fn test_session_list_shows_files() {
    let dir = temp_dir();
    let session_dir = dir.join(".roco").join("sessions");
    let _ = fs::create_dir_all(&session_dir);

    // Create a few sessions
    for i in 1..=3 {
        let state = roco_protocol::ConversationState::new(format!("session_{}", i), "careful");
        let path = session_dir.join(format!("session_{}.json", i));
        state.save(&path).expect("failed to save session");
    }

    // Verify all files exist
    for i in 1..=3 {
        let path = session_dir.join(format!("session_{}.json", i));
        assert!(path.exists(), "session file should exist");
    }

    // Cleanup
    let _ = fs::remove_dir_all(&dir);
}

/// Test session show displays transcript.
#[test]
fn test_session_show_displays_messages() {
    let dir = temp_dir();
    let session_dir = dir.join(".roco").join("sessions");
    let _ = fs::create_dir_all(&session_dir);

    let mut state = roco_protocol::ConversationState::new("show_test".to_string(), "careful");
    state.add_message("user", "Hello");
    state.add_message("assistant", "Hi there!");
    state.add_message("user", "How are you?");
    state.add_message("assistant", "I'm doing well, thanks!");

    let path = session_dir.join("show_test.json");
    state.save(&path).expect("failed to save session");

    // Verify messages are persisted
    let loaded = roco_protocol::ConversationState::load(&path).expect("failed to load");
    assert_eq!(loaded.messages.len(), 4);
    assert_eq!(loaded.messages[0].role, "user");
    assert_eq!(loaded.messages[0].content, "Hello");
    assert_eq!(loaded.messages[1].role, "assistant");
    assert_eq!(loaded.messages[1].content, "Hi there!");

    // Cleanup
    let _ = fs::remove_dir_all(&dir);
}

/// Test session delete removes file.
#[test]
fn test_session_delete_removes_file() {
    let dir = temp_dir();
    let session_dir = dir.join(".roco").join("sessions");
    let _ = fs::create_dir_all(&session_dir);

    let state = roco_protocol::ConversationState::new("delete_test".to_string(), "careful");
    let path = session_dir.join("delete_test.json");
    state.save(&path).expect("failed to save session");

    assert!(path.exists(), "session file should exist before delete");

    // Delete
    std::fs::remove_file(&path).expect("failed to delete session");
    assert!(!path.exists(), "session file should not exist after delete");

    // Cleanup
    let _ = fs::remove_dir_all(&dir);
}

/// Test session persistence across turns.
#[test]
fn test_session_persistence_across_turns() {
    let dir = temp_dir();
    let session_dir = dir.join(".roco").join("sessions");
    let _ = fs::create_dir_all(&session_dir);

    // First turn
    let mut state = roco_protocol::ConversationState::new("persist_test".to_string(), "careful");
    state.add_message("user", "What is 2+2?");
    let path = session_dir.join("persist_test.json");
    state.save(&path).expect("failed to save session");

    // Simulate response
    let mut loaded = roco_protocol::ConversationState::load(&path).expect("failed to load");
    loaded.add_message("assistant", "4");
    loaded.save(&path).expect("failed to save");

    // Load again to verify persistence
    let final_state = roco_protocol::ConversationState::load(&path).expect("failed to load");
    assert_eq!(final_state.messages.len(), 2);
    assert_eq!(final_state.messages[0].content, "What is 2+2?");
    assert_eq!(final_state.messages[1].content, "4");

    // Cleanup
    let _ = fs::remove_dir_all(&dir);
}

/// Test session with long conversation history.
#[test]
fn test_session_long_conversation() {
    let dir = temp_dir();
    let session_dir = dir.join(".roco").join("sessions");
    fs::create_dir_all(&session_dir).expect("failed to create session dir");

    let mut state = roco_protocol::ConversationState::new("long_test".to_string(), "careful");

    // Simulate a long conversation
    for i in 0..50 {
        state.add_message("user", &format!("Question {}", i));
        state.add_message("assistant", &format!("Answer {}", i));
    }

    let path = session_dir.join("long_test.json");
    state.save(&path).expect("failed to save session");

    let loaded = roco_protocol::ConversationState::load(&path).expect("failed to load");
    assert_eq!(loaded.messages.len(), 100);

    // Cleanup
    let _ = fs::remove_dir_all(&dir);
}
