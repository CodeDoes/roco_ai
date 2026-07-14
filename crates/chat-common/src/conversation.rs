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
