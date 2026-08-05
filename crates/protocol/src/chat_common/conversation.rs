/// A single serialized message in a chat session.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct ConversationMessage {
    pub role: String,
    pub content: String,
    pub timestamp: String,
}

/// Serialized session state for persistent conversations.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct ConversationState {
    pub id: String,
    pub messages: Vec<ConversationMessage>,
    pub pacing: String,
    pub created_at: String,
    pub updated_at: String,
}

impl ConversationState {
    pub fn new(id: String, pacing: &str) -> Self {
        let now = chrono::Utc::now().to_rfc3339();
        Self {
            id,
            messages: Vec::new(),
            pacing: pacing.to_string(),
            created_at: now.clone(),
            updated_at: now,
        }
    }

    pub fn add_message(&mut self, role: &str, content: &str) {
        self.messages.push(ConversationMessage {
            role: role.to_string(),
            content: content.to_string(),
            timestamp: chrono::Utc::now().to_rfc3339(),
        });
        self.updated_at = chrono::Utc::now().to_rfc3339();
    }

    pub fn save(&self, path: &std::path::Path) -> Result<(), String> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let json = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;

        // Atomic write: write to a temporary file in the same directory, then rename.
        // This avoids concurrent readers reading a truncated or partially written file.
        use std::collections::hash_map::RandomState;
        use std::hash::{BuildHasher, Hasher};
        use std::sync::atomic::{AtomicUsize, Ordering};

        static COMP_COUNTER: AtomicUsize = AtomicUsize::new(0);
        let count = COMP_COUNTER.fetch_add(1, Ordering::Relaxed);
        let pid = std::process::id();
        let mut hasher = RandomState::new().build_hasher();
        hasher.write_usize(count);
        hasher.write_u32(pid);

        let temp_name = format!(
            "{}.tmp-{:x}",
            path.file_name()
                .map(|name| name.to_string_lossy())
                .unwrap_or_else(|| std::borrow::Cow::Borrowed("")),
            hasher.finish()
        );
        let temp_path = path.with_file_name(temp_name);

        std::fs::write(&temp_path, &json).map_err(|e| e.to_string())?;
        if let Err(e) = std::fs::rename(&temp_path, path) {
            let _ = std::fs::remove_file(&temp_path);
            return Err(e.to_string());
        }
        Ok(())
    }

    pub fn load(path: &std::path::Path) -> Result<Self, String> {
        let json = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
        serde_json::from_str(&json).map_err(|e| e.to_string())
    }
}

/// Unique identifier for a conversation.
pub type ConversationId = String;

/// A single turn in a conversation.
#[derive(Debug, Clone)]
pub struct ConversationTurn {
    pub user: String,
    pub assistant: String,
}

/// State for an entire conversation session.
#[derive(Debug, Clone)]
pub struct Conversation {
    pub id: ConversationId,
    pub system: String,
    pub turns: Vec<ConversationTurn>,
    pub total_prompt_tokens: u64,
    pub total_completion_tokens: u64,
}

impl Conversation {
    pub fn new(id: ConversationId, system: String) -> Self {
        Self {
            id,
            system,
            turns: Vec::new(),
            total_prompt_tokens: 0,
            total_completion_tokens: 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_conversation() {
        let conv = Conversation::new("session-1".into(), "You are a helpful assistant.".into());
        assert_eq!(conv.id, "session-1");
        assert_eq!(conv.system, "You are a helpful assistant.");
        assert!(conv.turns.is_empty());
        assert_eq!(conv.total_prompt_tokens, 0);
        assert_eq!(conv.total_completion_tokens, 0);
    }

    #[test]
    fn test_conversation_turn_construction() {
        let turn = ConversationTurn {
            user: "Hello".into(),
            assistant: "Hi there!".into(),
        };
        assert_eq!(turn.user, "Hello");
        assert_eq!(turn.assistant, "Hi there!");
    }

    #[test]
    fn test_conversation_with_turns() {
        let mut conv = Conversation::new("test-1".into(), "system".into());
        conv.turns.push(ConversationTurn {
            user: "Q1".into(),
            assistant: "A1".into(),
        });
        conv.turns.push(ConversationTurn {
            user: "Q2".into(),
            assistant: "A2".into(),
        });
        assert_eq!(conv.turns.len(), 2);
        assert_eq!(conv.turns[0].user, "Q1");
        assert_eq!(conv.turns[1].assistant, "A2");
    }

    #[test]
    fn test_conversation_token_tracking() {
        let mut conv = Conversation::new("tokens".into(), "sys".into());
        conv.total_prompt_tokens = 150;
        conv.total_completion_tokens = 50;
        assert_eq!(conv.total_prompt_tokens, 150);
        assert_eq!(conv.total_completion_tokens, 50);
    }

    #[test]
    fn test_conversation_clone() {
        let conv = Conversation::new("clone-test".into(), "Be polite.".into());
        let cloned = conv.clone();
        assert_eq!(cloned.id, conv.id);
        assert_eq!(cloned.system, conv.system);
        assert_eq!(cloned.turns.len(), conv.turns.len());
        assert_eq!(cloned.total_prompt_tokens, conv.total_prompt_tokens);
    }

    #[test]
    fn test_conversation_debug() {
        let conv = Conversation::new("debug-test".into(), "debug sys".into());
        let debug_str = format!("{:?}", conv);
        assert!(debug_str.contains("debug-test"));
        assert!(debug_str.contains("debug sys"));
    }

    #[test]
    fn test_multiple_turns_preserve_order() {
        let mut conv = Conversation::new("order".into(), "sys".into());
        for i in 0..5 {
            conv.turns.push(ConversationTurn {
                user: format!("user_{}", i),
                assistant: format!("asst_{}", i),
            });
        }
        for (idx, turn) in conv.turns.iter().enumerate() {
            assert_eq!(turn.user, format!("user_{}", idx));
            assert_eq!(turn.assistant, format!("asst_{}", idx));
        }
    }
}
