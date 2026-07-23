//! RoCo Session — persistent sessions and history under `.roco/`.
//!
//! **File layout**
//!
//! ```text
//! .roco/
//! ├── trace.log                      ← global timeline, ALL events
//! └── sessions/{id}/
//!     ├── session.log                ← conversation turns
//!     ├── trace.txt                  ← raw I/O transcript
//!     ├── meta.json                  ← parent_id, session_type, active_branch
//!     └── history-{branch}.jsonl     ← branch checkpoints
//! ```
//!
//! Each session's `(session.log, trace.txt)` contains only that level's own
//! input/output — no aggregation from sub-sessions. To reconstruct the full
//! picture you walk the tree or read `trace.log`.
//!
//! **Usage**
//!
//! ```rust,no_run
//! use roco_session::{SessionStore, GlobalTraceEvent};
//!
//! // Initialize once at agent startup
//! let store = SessionStore::new(".roco").unwrap();
//!
//! // Create a root session
//! store.create_root("abc123").unwrap();
//!
//! // Log a turn and stream a trace line
//! store.log_conversation("abc123", "User: Hello\nAssistant: Hi there!").unwrap();
//! store.log_trace("abc123", "System: You are helpful...\n\nUser: Hello\n\nAssistant: Hi there!").unwrap();
//!
//! // Spawn a sub-session (logs agent_switch in both traces)
//! let child = store.spawn_sub("abc123", "def456").unwrap();
//! assert_eq!(child.meta().parent_id.as_deref(), Some("abc123"));
//!
//! // Child writes to its own directory
//! child.log_conversation("Sub: analyzing data...").unwrap();
//!
//! // Join back into parent
//! store.join_back("def456", "abc123", "Found 2 anomalies, written to findings.md").unwrap();
//! ```

pub mod error;
pub mod pool;
pub mod store;
pub mod types;

pub use pool::{LruSessionPool, SessionPool};
pub use store::{SessionError, SessionHandle, SessionStore};
pub use types::*;

#[cfg(test)]
mod tests;
