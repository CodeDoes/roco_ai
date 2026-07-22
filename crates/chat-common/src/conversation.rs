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
