//! Grammar strategy types.
//!
//! Moved from `crates/grammar` into core so all crates can reference
//! strategy kinds without pulling in the full grammar implementation.

use serde::{Deserialize, Serialize};

/// The kind of decoding strategy to use.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum StrategyKind {
    /// State-tuned recurrent state (no grammar, pre-baked format examples).
    StateTuned,
    /// BNF grammar-constrained decoding.
    Grammar,
    /// XML template with think/response tags.
    Xml,
    /// JSON schema validation.
    Json,
    /// Raw text, no constraints.
    Raw,
}

impl StrategyKind {
    pub fn parse(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
            "state-tuned" | "state_tuned" | "st" => Some(Self::StateTuned),
            "grammar" | "bnf" | "gbnf" => Some(Self::Grammar),
            "xml" | "template" => Some(Self::Xml),
            "json" | "schema" => Some(Self::Json),
            "raw" | "none" | "plain" => Some(Self::Raw),
            _ => None,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::StateTuned => "state-tuned",
            Self::Grammar => "grammar",
            Self::Xml => "xml",
            Self::Json => "json",
            Self::Raw => "raw",
        }
    }
}

/// Selects a strategy and holds schema info for structured completion.
#[derive(Debug, Clone)]
pub struct StrategySelector {
    pub kind: StrategyKind,
    pub schema_json: String,
    pub system_hint: String,
}

impl StrategySelector {
    pub fn new(kind: StrategyKind, schema: roco_engine::grammar::Schema, system_hint: impl Into<String>) -> Self {
        Self {
            kind,
            schema_json: schema.to_json(),
            system_hint: system_hint.into(),
        }
    }

    pub fn grammar(&self) -> String {
        if self.kind == StrategyKind::Grammar {
            roco_engine::grammar::schema_to_gbnf("root", &self.schema_json)
                .unwrap_or_default()
        } else {
            String::new()
        }
    }

    pub fn parse<T: serde::de::DeserializeOwned>(&self, text: &str) -> Result<T, String> {
        serde_json::from_str(text).map_err(|e| format!("parse error: {e}"))
    }
}


