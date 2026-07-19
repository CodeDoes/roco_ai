/// A single message in a chat conversation.
#[derive(Debug, Clone)]
pub struct ChatMessage {
    pub role: Role,
    pub content: String,
}

/// The role of a message participant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    System,
    User,
    Assistant,
    Tool,
}

impl Role {
    pub fn as_str(&self) -> &'static str {
        match self {
            Role::System => "System",
            Role::User => "User",
            Role::Assistant => "Assistant",
            Role::Tool => "Tool",
        }
    }
}

/// How to structure the prompt across turns.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PromptStyle {
    /// `System: ...\n\nUser: {current}\n\nAssistant:`
    /// Only current turn sent; relies on RNN state for history.
    StateOnly,
    /// `System: ...\n\nUser: t1\n\nAssistant: r1\n\nUser: t2\n\n...`
    FullInterleaved,
    /// `{history}\n\nSystem: ...\n\nUser: {current}\n\nAssistant:`
    HistoryFirst,
    /// System prompt repeated before every user turn.
    RepeatedSystem,
}

impl PromptStyle {
    pub fn label(&self) -> &'static str {
        match self {
            PromptStyle::StateOnly => "state-only",
            PromptStyle::FullInterleaved => "interleaved",
            PromptStyle::HistoryFirst => "history-first",
            PromptStyle::RepeatedSystem => "repeated-system",
        }
    }
}

/// Build a formatted prompt string from messages.
pub fn build_prompt(
    style: PromptStyle,
    turns: &[ChatMessage],
    system: &str,
    current: &str,
) -> String {
    match style {
        PromptStyle::StateOnly => current.to_string(),
        PromptStyle::FullInterleaved => {
            let mut s = format!("System: {system}\n\n");
            for msg in turns {
                s.push_str(&format!("{}: {}\n\n", msg.role.as_str(), msg.content));
            }
            s.push_str(&format!("User: {current}"));
            s
        }
        PromptStyle::HistoryFirst => {
            let mut s = String::new();
            for msg in turns {
                s.push_str(&format!("{}: {}\n\n", msg.role.as_str(), msg.content));
            }
            s.push_str(&format!("System: {system}\n\nUser: {current}"));
            s
        }
        PromptStyle::RepeatedSystem => {
            let mut s = format!("System: {system}\n\n");
            for msg in turns {
                s.push_str(&format!(
                    "{}: {}\n\nSystem: {system}\n\n",
                    msg.role.as_str(),
                    msg.content
                ));
            }
            s.push_str(&format!("User: {current}"));
            s
        }
    }
}

/// Build the role-prefixed inference prompt from system + user text.
/// This is the form used by `rwkv_backend` for single-turn inference.
pub fn build_inference_prompt(system: &str, prompt: &str, preserve_state: bool) -> String {
    if system.is_empty() {
        format!("User: {prompt}\n\nAssistant:")
    } else if preserve_state {
        // Continuation turn: skip system prompt (already in state).
        format!("User: {prompt}\n\nAssistant:")
    } else {
        format!("System: {}\n\nUser: {prompt}\n\nAssistant:", system.trim())
    }
}
