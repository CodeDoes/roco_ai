//! High-concurrency session persistence & state integrity integration test.

use roco_session::SessionStore;
use std::sync::Arc;

#[tokio::test]
async fn test_concurrent_session_reads_and_writes() {
    let tmp_dir = std::env::temp_dir().join(format!("roco-session-test-{}", std::process::id()));
    let store = Arc::new(SessionStore::new(&tmp_dir).expect("failed to create SessionStore"));

    // DRY spawn helper for concurrent workers
    let mut handles = Vec::new();
    for task_id in 0..6 {
        let store_clone = Arc::clone(&store);
        let handle = tokio::spawn(async move {
            let session_id = format!("session_{}", task_id % 3);
            let _ = store_clone.create_root(&session_id);
            for i in 0..5 {
                let msg = format!(
                    "User: Message {} from worker {}\nAssistant: OK\n",
                    i, task_id
                );
                let _ = store_clone.log_conversation(&session_id, &msg);
            }
        });
        handles.push(handle);
    }

    for h in handles {
        h.await.unwrap();
    }

    // Verify all 3 session files exist and can be loaded via store.open
    for i in 0..3 {
        let handle = store.open(&format!("session_{i}")).unwrap();
        let content = std::fs::read_to_string(handle.path().join("session.log")).unwrap();
        assert!(!content.is_empty());
    }

    let _ = std::fs::remove_dir_all(tmp_dir);
}
