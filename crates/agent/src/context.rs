//! Context management for MechanisticAgent.
//!
//! Small local models struggle when prompts are bloated with irrelevant data or
//! too long. This module provides a **pull-based context protocol**: before each
//! inference call the controller queries available sources (workspace files,
//! past sessions, memory), ranks them by relevance, and assembles only what's
//! needed.
//!
//! # Architecture
//!
//! ```text
//! +--------------------------+
//! | MechanisticAgent         |
//! +------------+-------------+
//!              | controls what goes in
//!              v
//! +------------------+    +------------------+
//! | ContextManager   |--->| SessionStore     | (past conversations)
//! | - query_sources()|--->| Workspace        | (current files)
//! | - budget_tokens()|--->| MemoryStore      | (structured recall)
//! | - truncate_plan()|    +------------------+
//! +------------------+
//! ```
//!
//! ## Design decisions
//!
//! - **Pull not push**: the agent decides what context to include; no blanket
//!   injection of everything available.
//! - **Token-aware**: tracks estimated tokens per source and respects budgets.
//! - **Summarizable**: every SourceEntry can be summarized by a subsequent model
//!   call (future Phase 2).

use serde::{Deserialize, Serialize};

/// A single piece of context contributed by a source.
///
/// Each snippet carries its own relevance score so the manager can sort and
/// prune before assembling the final prompt fragment.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextSnippet {
    /// Human-readable label describing what this snippet is (e.g. "session:s2", "file:chapter.md").
    pub label: String,
    /// Free-text content drawn from the source.
    pub content: String,
    /// Relevance score 0.0–1.0 relative to the current task.
    pub relevance: f32,
    /// Estimated token count of this snippet (rough heuristic; used for budgeting).
    pub est_tokens: usize,
}

impl ContextSnippet {
    pub fn new(label: impl Into<String>, content: impl Into<String>) -> Self {
        let label = label.into();
        let content = content.into();
        // Rough token estimation: ~4 chars per token for English text.
        let est_tokens = if content.is_empty() {
            0
        } else {
            content.len() / 4
        };
        Self {
            label,
            content,
            relevance: 0.5, // neutral default; callers should tune
            est_tokens,
        }
    }

    /// Set the relevance score explicitly.
    pub fn with_relevance(mut self, relevance: f32) -> Self {
        self.relevance = relevance.clamp(0.0, 1.0);
        self
    }
}

/// Trait for anything the context manager can query for snippets.
///
/// Implementors know how to translate a natural-language query into zero or
/// more relevant `ContextSnippet`s from their own data source.
pub trait ContextQuery: Send + Sync {
    /// Query this source with the given description.
    ///
    /// Returns snippets ranked by relevance descending.
    fn query(&self, query_text: &str, limit: usize) -> Vec<ContextSnippet>;
}

/// Wraps [`SessionStore`](crate::SessionStore) to provide ContextQuery semantics over past
/// conversation transcripts.
///
/// Queries are matched against transcript tasks (the session description) which serve
/// as coarse relevance signals without doing an embedding pass.
pub struct SessionContextSource {
    store: std::sync::Arc<crate::SessionStore>,
    max_results: usize,
}

impl SessionContextSource {
    pub fn new(store: std::sync::Arc<crate::SessionStore>, max_results: usize) -> Self {
        Self { store, max_results }
    }
}

impl ContextQuery for SessionContextSource {
    fn query(&self, query_text: &str, _limit: usize) -> Vec<ContextSnippet> {
        let results = self.store.search(query_text, self.max_results);
        results
            .iter()
            .take(self.max_results)
            .map(|t| {
                // Use task/title-equivalent as the searchable content.
                let search_content = &t.task;
                // Heuristic: match common words between query and content.
                let relevance = compute_relevance(query_text, search_content);
                ContextSnippet::new(
                    format!("session:{}", t.id),
                    format!(
                        "Past session '{}': {}\n",
                        t.id,
                        t.task.as_str()
                    ),
                )
                .with_relevance(relevance)
            })
            .collect()
    }
}

/// Wraps the workspace directory to provide file-level context querying.
///
/// Lists all workspace files and extracts content from those whose names or
/// paths match keywords from the query. Used to surface relevant workspace
/// artifacts during planning.
#[derive(Debug)]
pub struct WorkspaceContextSource {
    root: std::path::PathBuf,
}

impl WorkspaceContextSource {
    pub fn new(root: &std::path::Path) -> Self {
        Self {
            root: root.to_path_buf(),
        }
    }
}

impl ContextQuery for WorkspaceContextSource {
    fn query(&self, query_text: &str, limit: usize) -> Vec<ContextSnippet> {
        let mut snippets = Vec::new();
        let Ok(entries) = std::fs::read_dir(&self.root) else {
            return snippets;
        };
        for entry in entries.flatten().take(limit * 2) {
            if !entry.file_type().map(|t| t.is_file()).unwrap_or(false) {
                continue;
            }
            let name = entry.file_name().to_string_lossy().to_string();
            if let Ok(content) = std::fs::read_to_string(entry.path()) {
                // Only surface small files (>50KB is noise for context).
                if content.len() > 50_000 {
                    continue;
                }
                let relevance = compute_relevance(query_text, &name);
                // Truncate very large files to head/tail to bound output size.
                let truncated = truncate_for_context(&content, 2000);
                snippets.push(
                    ContextSnippet::new(format!("file:{}", name), truncated)
                        .with_relevance(relevance),
                );
            }
        }
        snippets.sort_by(|a, b| b.relevance.partial_cmp(&a.relevance).unwrap());
        snippets.truncate(limit);
        snippets
    }
}

/// Wraps [`MemoryStore`](crate::MemoryStore) for past-activity recall.
///
/// Uses the store's built-in `retrieve()` which does relevance-based filtering.
#[derive(Debug)]
pub struct MemoryContextSource {
    store: std::sync::Arc<crate::MemoryStore>,
    max_results: usize,
}

impl MemoryContextSource {
    pub fn new(store: std::sync::Arc<crate::MemoryStore>, max_results: usize) -> Self {
        Self {
            store,
            max_results,
        }
    }
}

impl ContextQuery for MemoryContextSource {
    fn query(&self, query_text: &str, _limit: usize) -> Vec<ContextSnippet> {
        // MemoryStore::retrieve already does relevance-based filtering.
        let entries = self.store.retrieve(query_text, self.max_results);
        entries
            .into_iter()
            .map(|m| {
                ContextSnippet::new(
                    format!("memory:{}", m.id),
                    format!("[{}] ({}) {}", m.id, m.kind, m.text),
                )
                .with_relevance(0.7) // retrieve() already filtered by relevance
            })
            .collect()
    }
}

// ── ContextBudget ──────────────────────────────────────────────────────

/// Tracks a token budget across multiple context-snippet additions.
///
/// Callers request snippets from various sources; the manager adds them in
/// relevance order until the budget is exhausted. Snippets exceeding the
/// remaining budget are silently dropped (or summarized — future work).
#[derive(Debug, Default)]
pub struct ContextBudget {
    max_tokens: usize,
    spent_tokens: usize,
}

impl ContextBudget {
    pub fn new(max_tokens: usize) -> Self {
        Self {
            max_tokens,
            spent_tokens: 0,
        }
    }

    pub fn remaining(&self) -> usize {
        self.max_tokens - self.spent_tokens
    }

    pub fn try_add(&mut self, snippet: &ContextSnippet) -> bool {
        if self.spent_tokens + snippet.est_tokens <= self.max_tokens {
            self.spent_tokens += snippet.est_tokens;
            true
        } else {
            false
        }
    }

    pub fn spent(&self) -> usize {
        self.spent_tokens
    }

    pub fn total_capacity(&self) -> usize {
        self.max_tokens
    }
}

// ── ContextManager ─────────────────────────────────────────────────────

/// Coordinates pulling context from multiple sources, budgeting tokens,
/// and assembling a combined context block.
///
/// This is what MechanisticAgent calls before `classify()`, `think_with_intent()`,
/// and `repair_derive()` to ensure the model sees only what's relevant.
pub struct ContextManager {
    sources: Vec<Box<dyn ContextQuery>>,
    budget: ContextBudget,
    /// Absolute maximum number of snippets regardless of budget.
    global_limit: usize,
}

impl ContextManager {
    pub fn new(budget_tokens: usize) -> Self {
        Self {
            sources: Vec::new(),
            budget: ContextBudget::new(budget_tokens),
            global_limit: 20,
        }
    }

    /// Register a context source.
    pub fn add_source(mut self, source: Box<dyn ContextQuery>) -> Self {
        self.sources.push(source);
        self
    }

    /// Configure absolute snippet cap.
    pub fn with_global_limit(mut self, limit: usize) -> Self {
        self.global_limit = limit;
        self
    }

    /// Run the full pull protocol: query each source, sort by relevance,
    /// budget, and return the assembled snippets.
    pub fn collect(&mut self, _query_text: &str) -> Vec<ContextSnippet> {
        self.budget = ContextBudget::new(self.budget.total_capacity());
        let mut all_snippets: Vec<ContextSnippet> = Vec::new();

        for source in &self.sources {
            // Ask each source for up to 5 snippets.
            let raw = source.query(_query_text, 5);
            all_snippets.extend(raw);
        }

        // Sort globally by relevance descending.
        all_snippets
            .sort_by(|a, b| b.relevance.partial_cmp(&a.relevance).unwrap());
        all_snippets.truncate(self.global_limit);

        // Budget-gated selection.
        let mut selected = Vec::new();
        for s in &all_snippets {
            if self.budget.try_add(s) {
                selected.push(s.clone());
            }
        }

        selected
    }

    /// Format collected snippets as a prompt-ready text block.
    pub fn to_prompt_block(snippets: &[ContextSnippet]) -> String {
        if snippets.is_empty() {
            return String::new();
        }
        let mut parts = Vec::new();
        for s in snippets.iter() {
            parts.push(format!(
                "[{}] (relevance {:.2})\n{}\n",
                s.label, s.relevance, s.content
            ));
        }
        format!(
            "## Context\nRelevant background:\n\n{}",
            parts.join("---\n")
        )
    }

    /// Reset budget for a fresh collection cycle.
    pub fn reset_budget(&mut self) {
        self.budget = ContextBudget::new(self.budget.total_capacity());
    }

    pub fn spent_tokens(&self) -> usize {
        self.budget.spent()
    }
}

// ── Utility helpers ────────────────────────────────────────────────────

/// Heuristic relevance scoring based on word overlap.
///
/// Split both texts into lowercase word tokens and compute Jaccard similarity.
/// Returns a value in [0.0, 1.0].
fn compute_relevance(a: &str, b: &str) -> f32 {
    let words_a: std::collections::HashSet<String> = tokenize_words(a);
    let words_b: std::collections::HashSet<String> = tokenize_words(b);
    if words_a.is_empty() || words_b.is_empty() {
        return 0.0;
    }
    let intersection = words_a.intersection(&words_b).count();
    let union = words_a.union(&words_b).count();
    intersection as f32 / union as f32
}

/// Lowercase and split on non-alpha characters. Returns owned strings to avoid
/// dangling-reference issues.
fn tokenize_words(text: &str) -> std::collections::HashSet<String> {
    text.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|s| !s.is_empty())
        .map(String::from)
        .collect()
}

/// Truncate very long text to head + tail sections, replacing the middle
/// with a marker. Keeps readability while bounding output size.
fn truncate_for_context(content: &str, max_chars: usize) -> String {
    if content.len() <= max_chars {
        content.to_string()
    } else {
        let head_end = max_chars / 2;
        let tail_start = content.len() - max_chars / 2;
        format!(
            "{}\n\n[... {} chars omitted ...]\n\n{}",
            &content[..head_end],
            content.len() - max_chars,
            &content[tail_start..]
        )
    }
}

// ── Tests ──────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compute_relevance_high_overlap() {
        let a = "the quick brown fox jumps over the lazy dog";
        let b = "brown fox quick jumps";
        let r = compute_relevance(a, b);
        assert!(r > 0.3, "overlap should yield high relevance, got {:.2}", r);
    }

    #[test]
    fn test_compute_relevance_no_overlap() {
        let a = "rust programming language features";
        let b = "baking sourdough bread recipe";
        let r = compute_relevance(a, b);
        assert_eq!(r, 0.0, "disjoint vocabularies should score zero");
    }

    #[test]
    fn test_tokenization_normalizes_case() {
        let a = "Rust Programming Language";
        let b = "rust programming features";
        let r = compute_relevance(a, b);
        assert!(r > 0.2, "case should not affect overlap, got {:.2}", r);
    }

    #[test]
    fn test_truncate_preserves_head_tail() {
        let content = "A".repeat(1000);
        let truncated = truncate_for_context(&content, 200);
        assert!(truncated.starts_with('A'));
        assert!(truncated.contains("[..."));
        assert!(truncated.ends_with('A'));
        assert!(truncated.len() < content.len());
    }

    #[test]
    fn test_truncate_returns_full_when_short() {
        let content = "short string";
        let truncated = truncate_for_context(&content, 200);
        assert_eq!(truncated, content);
    }

    #[test]
    fn test_context_budget_respects_max() {
        let mut budget = ContextBudget::new(100);
        let s1 = ContextSnippet::new("a", "x".repeat(100)); // ~25 tokens
        let s2 = ContextSnippet::new("b", "y".repeat(400)); // ~100 tokens
        let s3 = ContextSnippet::new("c", "z".repeat(800)); // ~200 tokens

        assert!(budget.try_add(&s1)); // ~25 tokens added
        assert_eq!(budget.remaining(), 75);
        assert!(!budget.try_add(&s2)); // ~100 > 75 remaining
        assert_eq!(budget.remaining(), 75);
        // After s2 rejected, s3 also exceeds 75.
        assert!(!budget.try_add(&s3));
        assert_eq!(budget.spent(), 25);
    }

    #[test]
    fn test_context_manager_assembles_and_sorts() {
        // Register one source manually using a simple stub.
        struct StubSource(Vec<ContextSnippet>);
        impl ContextQuery for StubSource {
            fn query(&self, _query_text: &str, _limit: usize) -> Vec<ContextSnippet> {
                self.0.clone()
            }
        }

        let low = ContextSnippet::new("l", "low-relevance content").with_relevance(0.1);
        let med = ContextSnippet::new("m", "medium-relevance content").with_relevance(0.6);
        let high = ContextSnippet::new("h", "high-relevance content").with_relevance(0.9);

        let mut manager = ContextManager::new(500)
            .with_global_limit(50)
            .add_source(Box::new(StubSource(vec![low, med, high])));

        let collected = manager.collect("something important");
        // Should be sorted descending by relevance.
        assert!(!collected.is_empty());
        if collected.len() >= 2 {
            assert!(
                collected[0].relevance >= collected[1].relevance,
                "first should be at least as relevant as second"
            );
        }
    }

    #[test]
    fn test_context_manager_budget_gates_selection() {
        struct AllHighSource;
        impl ContextQuery for AllHighSource {
            fn query(&self, _q: &str, _limit: usize) -> Vec<ContextSnippet> {
                vec![
                    ContextSnippet::new("s1", "x".repeat(400)),  // ~100 tokens
                    ContextSnippet::new("s2", "y".repeat(200)),  // ~50 tokens
                    ContextSnippet::new("s3", "z".repeat(100)),  // ~25 tokens
                ]
            }
        }

        // Budget of 30 lets only s3 (~25 tokens) through.
        let mut manager = ContextManager::new(30)
            .with_global_limit(50)
            .add_source(Box::new(AllHighSource));
        let collected = manager.collect("anything");

        // s1 (100) and s2 (50) both exceed 30; only s3 (25) fits.
        assert_eq!(collected.len(), 1);
        assert_eq!(collected[0].label, "s3");
    }

    #[test]
    fn test_context_manager_to_prompt_block() {
        let s1 = ContextSnippet::new("src:a", "alpha content");
        let s2 = ContextSnippet::new("src:b", "beta content");
        let snippets = vec![s1, s2];
        let block = ContextManager::to_prompt_block(&snippets);
        assert!(block.contains("## Context")); // check formatting marker
        assert!(block.contains("[src:a]"));
        assert!(block.contains("alpha content"));
        assert!(block.contains("beta content"));
    }

    #[test]
    fn test_context_manager_empty_block() {
        let block = ContextManager::to_prompt_block(&[]);
        assert!(block.is_empty());
    }

    #[test]
    fn test_context_snippet_with_relevance() {
        let snippet = ContextSnippet::new("test", "content")
            .with_relevance(0.95);
        assert_eq!(snippet.relevance, 0.95);

        // Clamping: values > 1.0 capped to 1.0.
        let too_high = ContextSnippet::new("test", "content")
            .with_relevance(2.0);
        assert_eq!(too_high.relevance, 1.0);

        // Clamping: values < 0.0 capped to 0.0.
        let too_low = ContextSnippet::new("test", "content")
            .with_relevance(-0.5);
        assert_eq!(too_low.relevance, 0.0);
    }
}
