//! Unified error type for the entire RoCo system.
//!
//! Replaces the proliferation of `String` errors and ad-hoc error enums
//! across crates. Every operation returns `Result<T, RoCoError>`.

use std::fmt;

/// The single error type used across all RoCo crates.
#[derive(Debug, thiserror::Error)]
pub enum RoCoError {
    /// Backend inference failed.
    #[error("backend error: {0}")]
    Backend(String),

    /// Workspace operation failed (path escape, file not found, etc.).
    #[error("workspace error: {0}")]
    Workspace(String),

    /// Grammar compilation or parsing failed.
    #[error("grammar error: {0}")]
    Grammar(String),

    /// Validation found issues with generated content.
    #[error("validation failed with {count} issue(s)")]
    Validation { count: usize, checks: Vec<ValidationCheck> },

    /// I/O error (file system, network, etc.).
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    /// Configuration error.
    #[error("config error: {0}")]
    Config(String),

    /// Serialization/deserialization error.
    #[error("serialization error: {0}")]
    Serde(#[from] serde_json::Error),

    /// Task scheduling error.
    #[error("scheduler error: {0}")]
    Scheduler(String),

    /// Generic catch-all for unexpected errors.
    #[error("{0}")]
    Other(String),
}

impl RoCoError {
    /// Create a backend error from any displayable type.
    pub fn backend<E: fmt::Display>(e: E) -> Self {
        Self::Backend(e.to_string())
    }

    /// Create a workspace error from any displayable type.
    pub fn workspace<E: fmt::Display>(e: E) -> Self {
        Self::Workspace(e.to_string())
    }

    /// Create a grammar error from any displayable type.
    pub fn grammar<E: fmt::Display>(e: E) -> Self {
        Self::Grammar(e.to_string())
    }

    /// Create a config error from any displayable type.
    pub fn config<E: fmt::Display>(e: E) -> Self {
        Self::Config(e.to_string())
    }

    /// Returns true if this error is retryable (e.g. transient backend failure).
    pub fn is_retryable(&self) -> bool {
        matches!(self, Self::Backend(_) | Self::Io(_))
    }

    /// Returns true if this is a validation error (not a system failure).
    pub fn is_validation(&self) -> bool {
        matches!(self, Self::Validation { .. })
    }
}

/// A single validation finding.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ValidationCheck {
    pub severity: ValidationSeverity,
    pub category: String,
    pub message: String,
    pub location: Option<String>,
}

/// Severity of a validation finding.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ValidationSeverity {
    Info,
    Warning,
    Error,
    Critical,
}

/// Result alias used throughout the codebase.
pub type RoCoResult<T> = Result<T, RoCoError>;


