//! Session types — identifiers and bake request/response.

use serde::{Deserialize, Serialize};

/// A session identifier.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SessionId(pub String);

impl SessionId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }
}

impl std::fmt::Display for SessionId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Request to bake few-shot examples into a session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BakeRequest {
    pub session_id: String,
    pub system: String,
    pub few_shots: Vec<(String, String)>,
}

/// Response from a bake operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BakeResponse {
    pub session_id: String,
    pub baked_shots: usize,
}


