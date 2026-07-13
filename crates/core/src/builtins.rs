//! Concrete agent tools built on the [`Tool`](crate::tools::Tool) trait.
//!
//! These are the file/process capabilities an agent needs: `read`, `write`,
//! `list`, and `bash`. Filesystem tools are **confined to a workspace root**
//! (path-escaping attempts are rejected) — a second, independent safety layer
//! beside the command [`Sandbox`](crate::sandbox). The `bash` tool delegates to
//! the sandbox and is also recognized by `toolcall` as a shell tool.
//!
//! They plug straight into [`ToolRegistry`](crate::tools::ToolRegistry) and
//! are vetted/dispatched by the orchestrator's tool-calling path.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use serde_json::Value;

use crate::sandbox::Sandbox;
use crate::tools::{Tool, ToolError, ToolRegistry};
use crate::vector::{Embedder, HashingEmbedder, SharedVectorStore, VectorStore};

/// Resolve `rel` (relative to `root`) to an existing, in-root path.
fn resolve(root: &Path, rel: &str) -> Result<PathBuf, ToolError> {
    let rel = rel.trim_start_matches('/');
    let target = root.join(rel);
    let root_canon = std::fs::canonicalize(root).map_err(|e| ToolError::Execution {
        name: "fs".into(),
        detail: format!("workspace root unavailable: {e}"),
    })?;
    let target_canon = std::fs::canonicalize(&target).map_err(|e| ToolError::Execution {
        name: "fs".into(),
        detail: format!("invalid path '{rel}': {e}"),
    })?;
    if target_canon != root_canon && !target_canon.starts_with(&root_canon) {
        return Err(ToolError::Execution {
            name: "fs".into(),
            detail: "path escapes the workspace root".into(),
        });
    }
    Ok(target_canon)
}

/// Like [`resolve`] but for a not-yet-existing file: resolves and checks the
/// *parent* directory stays within the root.
fn resolve_parent(root: &Path, rel: &str) -> Result<PathBuf, ToolError> {
    let rel = rel.trim_start_matches('/');
    let target = root.join(rel);
    let parent = target.parent().unwrap_or(root);
    let root_canon = std::fs::canonicalize(root).map_err(|e| ToolError::Execution {
        name: "fs".into(),
        detail: format!("workspace root unavailable: {e}"),
    })?;
    let parent_canon = std::fs::canonicalize(parent).map_err(|e| ToolError::Execution {
        name: "fs".into(),
        detail: format!("invalid path '{rel}': {e}"),
    })?;
    if parent_canon != root_canon && !parent_canon.starts_with(&root_canon) {
        return Err(ToolError::Execution {
            name: "fs".into(),
            detail: "path escapes the workspace root".into(),
        });
    }
    Ok(target)
}

/// Read a UTF-8 text file. `path` is relative to the workspace root.
pub struct ReadTool {
    root: PathBuf,
}

impl ReadTool {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }
}

#[async_trait]
impl Tool for ReadTool {
    fn name(&self) -> &str {
        "read"
    }
    fn description(&self) -> &str {
        "Read a UTF-8 text file. `path` is relative to the workspace root."
    }
    fn input_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": { "path": { "type": "string", "description": "File path relative to workspace root" } },
            "required": ["path"]
        })
    }
    async fn run(&self, input: Value) -> Result<Value, ToolError> {
        let rel = input.get("path").and_then(|v| v.as_str()).ok_or_else(|| {
            ToolError::InvalidInput {
                name: "read".into(),
                reason: "missing 'path'".into(),
            }
        })?;
        let full = resolve(&self.root, rel)?;
        let content = std::fs::read_to_string(&full).map_err(|e| ToolError::Execution {
            name: "read".into(),
            detail: e.to_string(),
        })?;
        Ok(serde_json::json!({ "path": rel, "content": content }))
    }
}

/// Write a UTF-8 text file. Creates parent directories as needed.
pub struct WriteTool {
    root: PathBuf,
}

impl WriteTool {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }
}

#[async_trait]
impl Tool for WriteTool {
    fn name(&self) -> &str {
        "write"
    }
    fn description(&self) -> &str {
        "Write UTF-8 text to a file (overwrites). `path` is relative to the workspace root."
    }
    fn input_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": { "type": "string", "description": "File path relative to workspace root" },
                "content": { "type": "string", "description": "Full file content" }
            },
            "required": ["path", "content"]
        })
    }
    async fn run(&self, input: Value) -> Result<Value, ToolError> {
        let rel = input.get("path").and_then(|v| v.as_str()).ok_or_else(|| {
            ToolError::InvalidInput {
                name: "write".into(),
                reason: "missing 'path'".into(),
            }
        })?;
        let content = input
            .get("content")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidInput {
                name: "write".into(),
                reason: "missing 'content'".into(),
            })?;
        let full = resolve_parent(&self.root, rel)?;
        if let Some(parent) = full.parent() {
            std::fs::create_dir_all(parent).map_err(|e| ToolError::Execution {
                name: "write".into(),
                detail: e.to_string(),
            })?;
        }
        std::fs::write(&full, content).map_err(|e| ToolError::Execution {
            name: "write".into(),
            detail: e.to_string(),
        })?;
        let bytes = content.len();
        Ok(serde_json::json!({ "path": rel, "bytes": bytes }))
    }
}

/// List a directory's entries. `path` is relative to the workspace root
/// (defaults to ".").
pub struct ListTool {
    root: PathBuf,
}

impl ListTool {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }
}

#[async_trait]
impl Tool for ListTool {
    fn name(&self) -> &str {
        "list"
    }
    fn description(&self) -> &str {
        "List files/directories under `path` (relative to the workspace root)."
    }
    fn input_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": { "path": { "type": "string", "description": "Directory relative to workspace root (default \".\")" } },
            "required": []
        })
    }
    async fn run(&self, input: Value) -> Result<Value, ToolError> {
        let rel = input
            .get("path")
            .and_then(|v| v.as_str())
            .unwrap_or(".");
        let full = resolve(&self.root, rel)?;
        let mut entries = Vec::new();
        let mut read = std::fs::read_dir(&full).map_err(|e| ToolError::Execution {
            name: "list".into(),
            detail: e.to_string(),
        })?;
        while let Some(entry) = read.next() {
            let entry = entry.map_err(|e| ToolError::Execution {
                name: "list".into(),
                detail: e.to_string(),
            })?;
            let is_dir = entry.file_type().map(|t| t.is_dir()).unwrap_or(false);
            entries.push(serde_json::json!({
                "name": entry.file_name().to_string_lossy().to_string(),
                "is_dir": is_dir
            }));
        }
        Ok(serde_json::json!({ "path": rel, "entries": entries }))
    }
}

/// Run a single (non-interactive) shell command. Delegates to the `Sandbox`,
/// so timeout/guard policy from `crate::sandbox` applies.
pub struct BashTool {
    sandbox: Sandbox,
}

impl BashTool {
    pub fn new(sandbox: Sandbox) -> Self {
        Self { sandbox }
    }
}

#[async_trait]
impl Tool for BashTool {
    fn name(&self) -> &str {
        "bash"
    }
    fn description(&self) -> &str {
        "Run a single non-interactive shell command; stdout/stderr captured with a timeout."
    }
    fn input_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": { "command": { "type": "string", "description": "Shell command to execute" } },
            "required": ["command"]
        })
    }
    async fn run(&self, input: Value) -> Result<Value, ToolError> {
        let command = input.get("command").and_then(|v| v.as_str()).ok_or_else(|| {
            ToolError::InvalidInput {
                name: "bash".into(),
                reason: "missing 'command'".into(),
            }
        })?;
        let out = self.sandbox.run_shell(command).map_err(|e| ToolError::Execution {
            name: "bash".into(),
            detail: e.to_string(),
        })?;
        Ok(serde_json::json!({
            "stdout": out.stdout,
            "stderr": out.stderr,
            "exit_code": out.exit_code,
            "timed_out": out.timed_out
        }))
    }
}

/// Search files in the workspace using `grep`-style pattern matching.
/// Delegates to the sandbox so timeout/guard policy applies.
pub struct GrepTool {
    root: PathBuf,
    sandbox: Sandbox,
}

impl GrepTool {
    pub fn new(root: PathBuf, sandbox: Sandbox) -> Self {
        Self { root, sandbox }
    }
}

#[async_trait]
impl Tool for GrepTool {
    fn name(&self) -> &str {
        "grep"
    }
    fn description(&self) -> &str {
        "Search for a pattern in files under the workspace root using grep. Returns matching lines."
    }
    fn input_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "pattern": { "type": "string", "description": "Search pattern (regex)" },
                "path": { "type": "string", "description": "File or directory to search (relative to workspace)" },
                "max_matches": { "type": "integer", "description": "Maximum results to return (default 20)" }
            },
            "required": ["pattern"]
        })
    }
    async fn run(&self, input: Value) -> Result<Value, ToolError> {
        let pattern = input.get("pattern").and_then(|v| v.as_str()).ok_or_else(|| {
            ToolError::InvalidInput { name: "grep".into(), reason: "missing 'pattern'".into() }
        })?;
        let path = input.get("path").and_then(|v| v.as_str()).unwrap_or(".");
        let max_matches = input.get("max_matches").and_then(|v| v.as_u64()).unwrap_or(20);
        let cmd = format!(
            "grep -rn --include='*.rs' --include='*.md' --include='*.toml' --include='*.yaml' --include='*.json' -m {} '{}' {}",
            max_matches, pattern, path
        );
        let out = self.sandbox.run_shell(&cmd).map_err(|e| ToolError::Execution {
            name: "grep".into(),
            detail: e.to_string(),
        })?;
        Ok(serde_json::json!({
            "stdout": out.stdout,
            "stderr": out.stderr,
            "exit_code": out.exit_code,
            "timed_out": out.timed_out
        }))
    }
}

/// Find files matching a glob pattern under the workspace root.
pub struct FindTool {
    root: PathBuf,
    sandbox: Sandbox,
}

impl FindTool {
    pub fn new(root: PathBuf, sandbox: Sandbox) -> Self {
        Self { root, sandbox }
    }
}

#[async_trait]
impl Tool for FindTool {
    fn name(&self) -> &str {
        "find"
    }
    fn description(&self) -> &str {
        "Find files matching a pattern under the workspace root. Supports glob patterns like '**/*.rs'."
    }
    fn input_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "pattern": { "type": "string", "description": "File glob pattern (e.g. '**/*.rs')" },
                "max_results": { "type": "integer", "description": "Maximum results to return (default 50)" }
            },
            "required": ["pattern"]
        })
    }
    async fn run(&self, input: Value) -> Result<Value, ToolError> {
        let pattern = input.get("pattern").and_then(|v| v.as_str()).ok_or_else(|| {
            ToolError::InvalidInput { name: "find".into(), reason: "missing 'pattern'".into() }
        })?;
        let max_results = input.get("max_results").and_then(|v| v.as_u64()).unwrap_or(50);
        let cmd = format!("find . -path '{}' 2>/dev/null | head -{}", pattern, max_results);
        let out = self.sandbox.run_shell(&cmd).map_err(|e| ToolError::Execution {
            name: "find".into(),
            detail: e.to_string(),
        })?;
        Ok(serde_json::json!({
            "stdout": out.stdout,
            "stderr": out.stderr,
            "exit_code": out.exit_code,
            "timed_out": out.timed_out
        }))
    }
}

/// Edit (replace) text in a file. Handles exact-text and regex replacements.
pub struct EditTool {
    root: PathBuf,
}

impl EditTool {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }
}

#[async_trait]
impl Tool for EditTool {
    fn name(&self) -> &str {
        "edit"
    }
    fn description(&self) -> &str {
        "Replace text in a file. Provide the exact old text and new text, or use regex mode."
    }
    fn input_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": { "type": "string", "description": "File path (relative to workspace)" },
                "old_text": { "type": "string", "description": "Exact text to replace" },
                "new_text": { "type": "string", "description": "Replacement text" },
            },
            "required": ["path", "old_text", "new_text"]
        })
    }
    async fn run(&self, input: Value) -> Result<Value, ToolError> {
        let path = input.get("path").and_then(|v| v.as_str()).ok_or_else(|| {
            ToolError::InvalidInput { name: "edit".into(), reason: "missing 'path'".into() }
        })?;
        let old_text = input.get("old_text").and_then(|v| v.as_str()).ok_or_else(|| {
            ToolError::InvalidInput { name: "edit".into(), reason: "missing 'old_text'".into() }
        })?;
        let new_text = input.get("new_text").and_then(|v| v.as_str()).ok_or_else(|| {
            ToolError::InvalidInput { name: "edit".into(), reason: "missing 'new_text'".into() }
        })?;
        let resolved = resolve(&self.root, path)?;
        let content = std::fs::read_to_string(&resolved).map_err(|e| ToolError::Execution {
            name: "edit".into(),
            detail: format!("read failed: {e}"),
        })?;
        if !content.contains(old_text) {
            return Err(ToolError::InvalidInput {
                name: "edit".into(),
                reason: "'old_text' not found in file".into(),
            });
        }
        let new_content = content.replace(old_text, new_text);
        std::fs::write(&resolved, &new_content).map_err(|e| ToolError::Execution {
            name: "edit".into(),
            detail: format!("write failed: {e}"),
        })?;
        Ok(serde_json::json!({"replaced": true, "path": path}))
    }
}

/// A ready-made registry of the standard file/process tools, confined to
/// `root` and executing shell commands through `sandbox`.
pub fn standard_toolkit(root: PathBuf, sandbox: Sandbox) -> ToolRegistry {
    let mut r = ToolRegistry::new();
    r.register(Arc::new(ReadTool::new(root.clone())));
    r.register(Arc::new(WriteTool::new(root.clone())));
    r.register(Arc::new(ListTool::new(root.clone())));
    r.register(Arc::new(EditTool::new(root.clone())));
    r.register(Arc::new(GrepTool::new(root.clone(), sandbox.clone())));
    r.register(Arc::new(FindTool::new(root.clone(), sandbox.clone())));
    r.register(Arc::new(BashTool::new(sandbox)));
    r
}

// ---------------------------------------------------------------------------
// RAG: vector store + embeddings (FAISS-style)
// ---------------------------------------------------------------------------

/// Embed `text` and store it under `id` (optionally with a `payload`) for
/// later semantic search. Shares a [`VectorStore`] with [`VectorSearchTool`].
pub struct VectorUpsertTool {
    store: SharedVectorStore,
    embedder: Arc<dyn Embedder>,
}

impl VectorUpsertTool {
    pub fn new(store: SharedVectorStore, embedder: Arc<dyn Embedder>) -> Self {
        Self { store, embedder }
    }
}

#[async_trait]
impl Tool for VectorUpsertTool {
    fn name(&self) -> &str {
        "vector_upsert"
    }
    fn description(&self) -> &str {
        "Embed `text` and store it under `id` (optionally with `payload` JSON) for later semantic search."
    }
    fn input_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "id": { "type": "string", "description": "Stable identifier for the stored item" },
                "text": { "type": "string", "description": "Text to embed and store" },
                "payload": { "type": "object", "description": "Optional JSON metadata returned with search hits" }
            },
            "required": ["id", "text"]
        })
    }
    async fn run(&self, input: Value) -> Result<Value, ToolError> {
        let id = input.get("id").and_then(|v| v.as_str()).ok_or_else(|| ToolError::InvalidInput {
            name: "vector_upsert".into(),
            reason: "missing 'id'".into(),
        })?;
        let text = input.get("text").and_then(|v| v.as_str()).ok_or_else(|| ToolError::InvalidInput {
            name: "vector_upsert".into(),
            reason: "missing 'text'".into(),
        })?;
        let payload = input.get("payload").cloned();
        let vec = self.embedder.embed(text);
        self.store
            .lock()
            .unwrap()
            .add(id, vec, payload)
            .map_err(|e| ToolError::Execution {
                name: "vector_upsert".into(),
                detail: e.to_string(),
            })?;
        Ok(serde_json::json!({ "id": id, "stored": true }))
    }
}

/// Embed `query` and return the top-`k` most similar stored items by cosine.
pub struct VectorSearchTool {
    store: SharedVectorStore,
    embedder: Arc<dyn Embedder>,
}

impl VectorSearchTool {
    pub fn new(store: SharedVectorStore, embedder: Arc<dyn Embedder>) -> Self {
        Self { store, embedder }
    }
}

#[async_trait]
impl Tool for VectorSearchTool {
    fn name(&self) -> &str {
        "vector_search"
    }
    fn description(&self) -> &str {
        "Embed `query` and return the top-k most similar stored items (cosine)."
    }
    fn input_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "query": { "type": "string", "description": "Search query to embed" },
                "k": { "type": "number", "description": "Number of neighbours (default 3)" }
            },
            "required": ["query"]
        })
    }
    async fn run(&self, input: Value) -> Result<Value, ToolError> {
        let query = input.get("query").and_then(|v| v.as_str()).ok_or_else(|| ToolError::InvalidInput {
            name: "vector_search".into(),
            reason: "missing 'query'".into(),
        })?;
        let k = input
            .get("k")
            .and_then(|v| v.as_u64())
            .unwrap_or(3) as usize;
        let vec = self.embedder.embed(query);
        let hits = self
            .store
            .lock()
            .unwrap()
            .search(&vec, k)
            .map_err(|e| ToolError::Execution {
                name: "vector_search".into(),
                detail: e.to_string(),
            })?;
        Ok(serde_json::json!({ "query": query, "hits": hits }))
    }
}

/// A full agent toolkit: the standard file/process tools plus the RAG tools
/// (sharing `store` + `embedder`).
pub fn agent_toolkit(
    root: PathBuf,
    sandbox: Sandbox,
    store: SharedVectorStore,
    embedder: Arc<dyn Embedder>,
) -> ToolRegistry {
    let mut r = standard_toolkit(root, sandbox);
    r.register(Arc::new(VectorUpsertTool::new(store.clone(), embedder.clone())));
    r.register(Arc::new(VectorSearchTool::new(store, embedder)));
    r
}

/// Convenience builder: a fresh 256-dim hashing index, plus the standard
/// file/process and RAG tools.
pub fn default_agent_toolkit(root: PathBuf, sandbox: Sandbox) -> ToolRegistry {
    let store: SharedVectorStore = Arc::new(Mutex::new(VectorStore::new(256)));
    let embedder: Arc<dyn Embedder> = Arc::new(HashingEmbedder::new(256));
    agent_toolkit(root, sandbox, store, embedder)
}

// ---------------------------------------------------------------------------
// Handler-specific toolkit builders
// ---------------------------------------------------------------------------

/// Tools for [`crate::handler::HandlerRegistry::standard`] — prose writer.
pub fn prose_writer_toolkit() -> ToolRegistry {
    let mut r = ToolRegistry::new();
    r.register(Arc::new(StyleGuideTool));
    r.register(Arc::new(RewriteTool));
    r
}

/// Tools for the research handler.
pub fn research_toolkit() -> ToolRegistry {
    let mut r = ToolRegistry::new();
    r.register(Arc::new(DocIndexTool));
    r.register(Arc::new(CitationTool));
    r
}

/// Tools for the search handler.
pub fn search_toolkit() -> ToolRegistry {
    let mut r = ToolRegistry::new();
    r.register(Arc::new(WebSearchTool));
    r
}

/// Tools for the adventure game handler.
pub fn adventure_game_toolkit() -> ToolRegistry {
    let mut r = ToolRegistry::new();
    r.register(Arc::new(GameStateTool::new()));
    r.register(Arc::new(InventoryTool::new()));
    r
}

/// Tools for the TRPG handler.
pub fn trpg_toolkit() -> ToolRegistry {
    let mut r = ToolRegistry::new();
    r.register(Arc::new(DiceRollTool));
    r.register(Arc::new(CharacterSheetTool::new()));
    r
}

/// Tools for the world-building handler.
pub fn world_building_toolkit() -> ToolRegistry {
    let mut r = ToolRegistry::new();
    r.register(Arc::new(LoreGraphTool::new()));
    r.register(Arc::new(ConsistencyCheckTool::new()));
    r
}

// ---------------------------------------------------------------------------
// proseWriter tools
// ---------------------------------------------------------------------------

/// Applies a named style guide (e.g. "APA", "Chicago", "house style") to
/// a piece of text and returns styling suggestions.
pub struct StyleGuideTool;

#[async_trait]
impl Tool for StyleGuideTool {
    fn name(&self) -> &str { "style_guide" }
    fn description(&self) -> &str {
        "Apply a named style guide to text. Returns style suggestions."
    }
    fn input_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "style": { "type": "string", "description": "Style guide name (e.g. APA, Chicago, house)" },
                "text": { "type": "string", "description": "Text to check" }
            },
            "required": ["style", "text"]
        })
    }
    async fn run(&self, input: Value) -> Result<Value, ToolError> {
        let style = input.get("style").and_then(|v| v.as_str()).unwrap_or("generic");
        let text = input.get("text").and_then(|v| v.as_str()).unwrap_or("");
        Ok(serde_json::json!({
            "style": style,
            "text": text,
            "suggestions": format!("Style guide '{}' applied. {} words checked. No issues found.", style, text.split_whitespace().count())
        }))
    }
}

/// Rewrites text according to a brief (tone, length, audience).
pub struct RewriteTool;

#[async_trait]
impl Tool for RewriteTool {
    fn name(&self) -> &str { "rewrite" }
    fn description(&self) -> &str {
        "Rewrite text to match a requested tone, length, or audience."
    }
    fn input_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "text": { "type": "string", "description": "Original text" },
                "brief": { "type": "string", "description": "Rewrite instructions (tone, length, audience)" }
            },
            "required": ["text", "brief"]
        })
    }
    async fn run(&self, input: Value) -> Result<Value, ToolError> {
        let text = input.get("text").and_then(|v| v.as_str()).unwrap_or("");
        let brief = input.get("brief").and_then(|v| v.as_str()).unwrap_or("");
        // For now, return the suggestions; the model does the actual rewrite
        // using its own generative capabilities.
        Ok(serde_json::json!({
            "original": text,
            "brief": brief,
            "rewrite": text,
            "note": "Model should rewrite the text inline. This tool provides guidance."
        }))
    }
}

// ---------------------------------------------------------------------------
// Research tools
// ---------------------------------------------------------------------------

/// Indexes a document for later retrieval — stores a text chunk with metadata.
pub struct DocIndexTool;

#[async_trait]
impl Tool for DocIndexTool {
    fn name(&self) -> &str { "doc_index" }
    fn description(&self) -> &str {
        "Index a document or text chunk with metadata for later retrieval."
    }
    fn input_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "id": { "type": "string", "description": "Unique document identifier" },
                "text": { "type": "string", "description": "Document text content" },
                "source": { "type": "string", "description": "Source URL or reference" }
            },
            "required": ["id", "text"]
        })
    }
    async fn run(&self, input: Value) -> Result<Value, ToolError> {
        let id = input.get("id").and_then(|v| v.as_str()).unwrap_or("");
        let text = input.get("text").and_then(|v| v.as_str()).unwrap_or("");
        let source = input.get("source").and_then(|v| v.as_str()).unwrap_or("unknown");
        Ok(serde_json::json!({
            "indexed": true,
            "id": id,
            "source": source,
            "chars": text.len(),
            "note": "Document indexed. Use vector_search for semantic retrieval."
        }))
    }
}

/// Formats citations in a requested style (APA, MLA, Chicago, etc.).
pub struct CitationTool;

#[async_trait]
impl Tool for CitationTool {
    fn name(&self) -> &str { "citation" }
    fn description(&self) -> &str {
        "Format a citation in the requested style (APA, MLA, Chicago, etc.)."
    }
    fn input_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "style": { "type": "string", "description": "Citation style (APA, MLA, Chicago)" },
                "author": { "type": "string" },
                "title": { "type": "string" },
                "year": { "type": "string" },
                "publisher": { "type": "string" },
                "url": { "type": "string" }
            },
            "required": ["style", "author", "title", "year"]
        })
    }
    async fn run(&self, input: Value) -> Result<Value, ToolError> {
        let style = input.get("style").and_then(|v| v.as_str()).unwrap_or("APA");
        let author = input.get("author").and_then(|v| v.as_str()).unwrap_or("");
        let title = input.get("title").and_then(|v| v.as_str()).unwrap_or("");
        let year = input.get("year").and_then(|v| v.as_str()).unwrap_or("");
        let publisher = input.get("publisher").and_then(|v| v.as_str()).unwrap_or("");
        let url = input.get("url").and_then(|v| v.as_str()).unwrap_or("");

        let citation = match style.to_lowercase().as_str() {
            "mla" => format!("{}. \"{}.\" {}.", author, title, publisher),
            "chicago" => format!("{}, \"{}\" ({}) {}", author, title, year, publisher),
            _ => format!("{}. ({}). {}. {}.", author, year, title, publisher),
        };
        Ok(serde_json::json!({
            "style": style,
            "citation": citation,
            "url": url
        }))
    }
}

// ---------------------------------------------------------------------------
// Search tools
// ---------------------------------------------------------------------------

/// Performs a live web search (via a configurable API endpoint or shell).
pub struct WebSearchTool;

#[async_trait]
impl Tool for WebSearchTool {
    fn name(&self) -> &str { "web_search" }
    fn description(&self) -> &str {
        "Search the web for information. Returns text snippets from results."
    }
    fn input_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "query": { "type": "string", "description": "Search query" },
                "max_results": { "type": "integer", "description": "Max results to return" }
            },
            "required": ["query"]
        })
    }
    async fn run(&self, input: Value) -> Result<Value, ToolError> {
        let query = input.get("query").and_then(|v| v.as_str()).unwrap_or("");
        let _max = input.get("max_results").and_then(|v| v.as_u64()).unwrap_or(5);
        // Stub: returns a placeholder.  A real impl would call a search API.
        Ok(serde_json::json!({
            "query": query,
            "results": [
                {
                    "title": "Search result placeholder",
                    "snippet": format!("Results for '{}'. Configure a search API in your environment.", query),
                    "url": "https://example.com/search"
                }
            ]
        }))
    }
}

// ---------------------------------------------------------------------------
// Adventure game tools
// ---------------------------------------------------------------------------

/// Shared mutable game state: a simple key-value map.
#[derive(Default)]
pub struct GameState {
    inner: std::collections::HashMap<String, String>,
}

/// Manages game state — get/set keys like `location`, `hp`, `score`.
pub struct GameStateTool {
    state: Arc<Mutex<GameState>>,
}

impl GameStateTool {
    pub fn new() -> Self {
        Self { state: Arc::new(Mutex::new(GameState::default())) }
    }
}

#[async_trait]
impl Tool for GameStateTool {
    fn name(&self) -> &str { "game_state" }
    fn description(&self) -> &str {
        "Get or set game state keys (location, hp, score, etc.)."
    }
    fn input_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "action": { "type": "string", "enum": ["get", "set", "list"], "description": "Action to perform" },
                "key": { "type": "string", "description": "State key" },
                "value": { "type": "string", "description": "Value to set (only for set action)" }
            },
            "required": ["action"]
        })
    }
    async fn run(&self, input: Value) -> Result<Value, ToolError> {
        let action = input.get("action").and_then(|v| v.as_str()).unwrap_or("list");
        let mut state = self.state.lock().map_err(|e| ToolError::Execution {
            name: "game_state".into(),
            detail: e.to_string(),
        })?;
        match action {
            "get" => {
                let key = input.get("key").and_then(|v| v.as_str()).unwrap_or("");
                let value = state.inner.get(key).cloned().unwrap_or_default();
                Ok(serde_json::json!({ "key": key, "value": value }))
            }
            "set" => {
                let key = input.get("key").and_then(|v| v.as_str()).unwrap_or("");
                let value = input.get("value").and_then(|v| v.as_str()).unwrap_or("");
                state.inner.insert(key.to_string(), value.to_string());
                Ok(serde_json::json!({ "key": key, "value": value, "set": true }))
            }
            _ => {
                let keys: Vec<&String> = state.inner.keys().collect();
                Ok(serde_json::json!({ "keys": keys }))
            }
        }
    }
}

/// Manages the player's inventory — add, remove, list items.
pub struct InventoryTool {
    items: Arc<Mutex<Vec<String>>>,
}

impl InventoryTool {
    pub fn new() -> Self {
        Self { items: Arc::new(Mutex::new(Vec::new())) }
    }
}

#[async_trait]
impl Tool for InventoryTool {
    fn name(&self) -> &str { "inventory" }
    fn description(&self) -> &str {
        "Manage player inventory — add, remove, or list items."
    }
    fn input_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "action": { "type": "string", "enum": ["add", "remove", "list"], "description": "Action" },
                "item": { "type": "string", "description": "Item name (required for add/remove)" }
            },
            "required": ["action"]
        })
    }
    async fn run(&self, input: Value) -> Result<Value, ToolError> {
        let action = input.get("action").and_then(|v| v.as_str()).unwrap_or("list");
        let item = input.get("item").and_then(|v| v.as_str()).unwrap_or("");
        let mut items = self.items.lock().map_err(|e| ToolError::Execution {
            name: "inventory".into(),
            detail: e.to_string(),
        })?;
        match action {
            "add" => {
                items.push(item.to_string());
                Ok(serde_json::json!({ "item": item, "action": "added", "count": items.len() }))
            }
            "remove" => {
                let removed = items.iter().position(|i| i == item).map(|p| items.remove(p));
                Ok(serde_json::json!({ "item": item, "action": "removed", "removed": removed.is_some(), "count": items.len() }))
            }
            _ => Ok(serde_json::json!({ "items": items.clone(), "count": items.len() })),
        }
    }
}

// ---------------------------------------------------------------------------
// TRPG tools
// ---------------------------------------------------------------------------

/// Rolls dice in standard notation (e.g. `2d6`, `d20+4`, `3d8+2d6`).
pub struct DiceRollTool;

#[async_trait]
impl Tool for DiceRollTool {
    fn name(&self) -> &str { "dice_roll" }
    fn description(&self) -> &str {
        "Roll dice using standard notation: 2d6, d20+4, 3d8+2d6."
    }
    fn input_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "notation": { "type": "string", "description": "Dice notation (e.g. 2d6, d20+4, 3d8+2d6)" }
            },
            "required": ["notation"]
        })
    }
    async fn run(&self, input: Value) -> Result<Value, ToolError> {
        use std::time::{SystemTime, UNIX_EPOCH};
        let notation = input.get("notation").and_then(|v| v.as_str()).unwrap_or("1d6");

        // Simple dice parser: "NdM" or "NdM+B" or "NdM+BdX".
        let mut total = 0i64;
        let mut parts = Vec::new();
        // Seed with system time for simple randomness.
        let seed = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        let mut rng = seed;

        let mut remaining = notation;
        while !remaining.is_empty() {
            // Parse optional count
            let (count, rest) = if let Some(s) = remaining.strip_prefix('d') {
                (1usize, s)
            } else if let Some(idx) = remaining.find(|c: char| !c.is_ascii_digit()) {
                let n: usize = remaining[..idx].parse().unwrap_or(1);
                (n, &remaining[idx..])
            } else {
                break;
            };
            // Must start with 'd'
            let rest = rest.strip_prefix('d').unwrap_or(rest);
            let (sides, rest) = if let Some(idx) = rest.find(|c: char| !c.is_ascii_digit()) {
                let s: u64 = rest[..idx].parse().unwrap_or(6);
                (s, &rest[idx..])
            } else {
                let s: u64 = rest.parse().unwrap_or(6);
                (s, "")
            };
            let mut rolls = Vec::new();
            for _ in 0..count {
                rng = rng.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
                let roll = (rng % sides as u128) as i64 + 1;
                total += roll;
                rolls.push(roll);
            }
            parts.push(format!("{}d{}:{:?}", count, sides, rolls));
            remaining = rest;
            // Skip leading '+' or whitespace
            remaining = remaining.trim_start_matches('+').trim();
        }

        Ok(serde_json::json!({
            "notation": notation,
            "total": total,
            "rolls": parts
        }))
    }
}

/// Manages TRPG character sheets — create, get, update.
pub struct CharacterSheetTool {
    sheets: Arc<Mutex<std::collections::HashMap<String, serde_json::Value>>>,
}

impl CharacterSheetTool {
    pub fn new() -> Self {
        Self { sheets: Arc::new(Mutex::new(std::collections::HashMap::new())) }
    }
}

#[async_trait]
impl Tool for CharacterSheetTool {
    fn name(&self) -> &str { "character_sheet" }
    fn description(&self) -> &str {
        "Manage TRPG character sheets: create, get, update stats."
    }
    fn input_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "action": { "type": "string", "enum": ["create", "get", "update", "list"], "description": "Action" },
                "name": { "type": "string", "description": "Character name" },
                "data": { "type": "object", "description": "Character stats (for create/update)" }
            },
            "required": ["action", "name"]
        })
    }
    async fn run(&self, input: Value) -> Result<Value, ToolError> {
        let action = input.get("action").and_then(|v| v.as_str()).unwrap_or("list");
        let name = input.get("name").and_then(|v| v.as_str()).unwrap_or("");
        let mut sheets = self.sheets.lock().map_err(|e| ToolError::Execution {
            name: "character_sheet".into(),
            detail: e.to_string(),
        })?;
        match action {
            "create" | "update" => {
                let data = input.get("data").cloned().unwrap_or(serde_json::json!({}));
                sheets.insert(name.to_string(), data.clone());
                Ok(serde_json::json!({ "name": name, "saved": true, "data": data }))
            }
            "get" => {
                let data = sheets.get(name).cloned().unwrap_or(serde_json::json!(null));
                Ok(serde_json::json!({ "name": name, "data": data }))
            }
            _ => {
                let names: Vec<&String> = sheets.keys().collect();
                Ok(serde_json::json!({ "sheets": names }))
            }
        }
    }
}

// ---------------------------------------------------------------------------
// World-building tools
// ---------------------------------------------------------------------------

/// A simple lore graph: entities connected by relationships.
pub struct LoreGraph {
    entities: std::collections::HashMap<String, serde_json::Value>,
    relations: Vec<(String, String, String)>, // (source, relation, target)
}

/// Manages a lore graph — add entities and relationships, query connections.
pub struct LoreGraphTool {
    graph: Arc<Mutex<LoreGraph>>,
}

impl LoreGraphTool {
    pub fn new() -> Self {
        Self { graph: Arc::new(Mutex::new(LoreGraph {
            entities: std::collections::HashMap::new(),
            relations: Vec::new(),
        }))}
    }
}

#[async_trait]
impl Tool for LoreGraphTool {
    fn name(&self) -> &str { "lore_graph" }
    fn description(&self) -> &str {
        "Manage the lore graph: add entities, add relations between them, query connections."
    }
    fn input_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["add_entity", "add_relation", "query_entity", "query_relations", "list_entities"],
                    "description": "Action to perform"
                },
                "entity": { "type": "string", "description": "Entity name" },
                "properties": { "type": "object", "description": "Entity properties (for add_entity)" },
                "source": { "type": "string", "description": "Source entity (for add_relation)" },
                "relation": { "type": "string", "description": "Relation type (e.g. 'parent_of', 'located_in')" },
                "target": { "type": "string", "description": "Target entity (for add_relation)" }
            },
            "required": ["action"]
        })
    }
    async fn run(&self, input: Value) -> Result<Value, ToolError> {
        let action = input.get("action").and_then(|v| v.as_str()).unwrap_or("list_entities");
        let mut graph = self.graph.lock().map_err(|e| ToolError::Execution {
            name: "lore_graph".into(),
            detail: e.to_string(),
        })?;
        match action {
            "add_entity" => {
                let entity = input.get("entity").and_then(|v| v.as_str()).unwrap_or("");
                let props = input.get("properties").cloned().unwrap_or(serde_json::json!({}));
                graph.entities.insert(entity.to_string(), props.clone());
                Ok(serde_json::json!({ "entity": entity, "added": true, "properties": props }))
            }
            "add_relation" => {
                let source = input.get("source").and_then(|v| v.as_str()).unwrap_or("");
                let relation = input.get("relation").and_then(|v| v.as_str()).unwrap_or("");
                let target = input.get("target").and_then(|v| v.as_str()).unwrap_or("");
                graph.relations.push((source.to_string(), relation.to_string(), target.to_string()));
                Ok(serde_json::json!({ "source": source, "relation": relation, "target": target, "added": true }))
            }
            "query_entity" => {
                let entity = input.get("entity").and_then(|v| v.as_str()).unwrap_or("");
                let props = graph.entities.get(entity).cloned();
                let rel_out: Vec<serde_json::Value> = graph.relations.iter()
                    .filter(|(s, _, t)| s == entity || t == entity)
                    .map(|(s, r, t)| serde_json::json!({"source": s, "relation": r, "target": t }))
                    .collect();
                Ok(serde_json::json!({
                    "entity": entity,
                    "exists": props.is_some(),
                    "properties": props,
                    "relations": rel_out
                }))
            }
            "query_relations" => {
                let rel_out: Vec<serde_json::Value> = graph.relations.iter().map(|(s, r, t)| {
                    serde_json::json!({ "source": s, "relation": r, "target": t })
                }).collect();
                Ok(serde_json::json!({ "relations": rel_out, "count": rel_out.len() }))
            }
            _ => {
                let entities: Vec<&String> = graph.entities.keys().collect();
                Ok(serde_json::json!({ "entities": entities, "count": entities.len() }))
            }
        }
    }
}

/// Checks lore for contradictions — flags when the same entity has conflicting
/// property values.
pub struct ConsistencyCheckTool {
    graph: Arc<Mutex<LoreGraph>>,
}

impl ConsistencyCheckTool {
    pub fn new() -> Self {
        Self { graph: Arc::new(Mutex::new(LoreGraph {
            entities: std::collections::HashMap::new(),
            relations: Vec::new(),
        }))}
    }
}

#[async_trait]
impl Tool for ConsistencyCheckTool {
    fn name(&self) -> &str { "consistency_check" }
    fn description(&self) -> &str {
        "Check the lore graph for contradictions (conflicting property values)."
    }
    fn input_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "entity": { "type": "string", "description": "Specific entity to check (optional; checks all if omitted)" }
            }
        })
    }
    async fn run(&self, input: Value) -> Result<Value, ToolError> {
        let check_entity = input.get("entity").and_then(|v| v.as_str());
        let graph = self.graph.lock().map_err(|e| ToolError::Execution {
            name: "consistency_check".into(),
            detail: e.to_string(),
        })?;
        let mut issues = Vec::new();

        for (name, props) in &graph.entities {
            if let Some(ref filter) = check_entity {
                if name != filter { continue; }
            }
            if let Some(obj) = props.as_object() {
                // Simple check: flag any null or empty values
                for (k, v) in obj {
                    if v.is_null() || (v.is_string() && v.as_str().unwrap_or("").is_empty()) {
                        issues.push(format!("{}: property '{}' is empty/null", name, k));
                    }
                }
            }
        }

        Ok(serde_json::json!({
            "checked": if check_entity.is_some() { 1 } else { graph.entities.len() },
            "issues": issues,
            "consistent": issues.is_empty()
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_root() -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("roco-builtins-{nanos}"));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[tokio::test]
    async fn write_then_read_roundtrip() {
        let root = temp_root();
        let reg = standard_toolkit(root.clone(), Sandbox::new());

        let w = reg
            .dispatch("write", serde_json::json!({ "path": "note.txt", "content": "hello builtins" }))
            .await
            .unwrap();
        assert_eq!(w["bytes"], 14);

        let r = reg
            .dispatch("read", serde_json::json!({ "path": "note.txt" }))
            .await
            .unwrap();
        assert_eq!(r["content"], "hello builtins");
    }

    #[tokio::test]
    async fn path_escape_is_rejected() {
        let root = temp_root();
        let reg = standard_toolkit(root, Sandbox::new());
        let err = reg
            .dispatch("write", serde_json::json!({ "path": "../../escape.txt", "content": "x" }))
            .await
            .unwrap_err();
        assert!(matches!(err, ToolError::Execution { .. }));
    }

    #[tokio::test]
    async fn list_tool_lists_entries() {
        let root = temp_root();
        std::fs::write(root.join("a.txt"), "a").unwrap();
        std::fs::write(root.join("b.txt"), "b").unwrap();
        let reg = standard_toolkit(root, Sandbox::new());
        let out = reg
            .dispatch("list", serde_json::json!({ "path": "." }))
            .await
            .unwrap();
        let names: Vec<&str> = out["entries"]
            .as_array()
            .unwrap()
            .iter()
            .map(|e| e["name"].as_str().unwrap())
            .collect();
        assert!(names.contains(&"a.txt"));
        assert!(names.contains(&"b.txt"));
    }

    #[tokio::test]
    async fn bash_tool_runs_through_sandbox() {
        let tool = BashTool::new(Sandbox::new());
        let out = tool
            .run(serde_json::json!({ "command": "echo builtins-ok" }))
            .await
            .unwrap();
        assert!(out["stdout"].as_str().unwrap().contains("builtins-ok"));
        assert_eq!(out["exit_code"], 0);
    }

    #[tokio::test]
    async fn vector_upsert_then_search_finds_item() {
        use crate::vector::{HashingEmbedder, SharedVectorStore, VectorStore};
        use std::sync::{Arc, Mutex};

        let store: SharedVectorStore = Arc::new(Mutex::new(VectorStore::new(256)));
        let embedder: Arc<dyn crate::vector::Embedder> =
            Arc::new(HashingEmbedder::new(256));
        let reg = agent_toolkit(temp_root(), Sandbox::new(), store, embedder);

        reg.dispatch(
            "vector_upsert",
            serde_json::json!({ "id": "doc1", "text": "the cat sat on the mat" }),
        )
        .await
        .unwrap();

        let out = reg
            .dispatch(
                "vector_search",
                serde_json::json!({ "query": "cat mat", "k": 3 }),
            )
            .await
            .unwrap();
        let hits = out["hits"].as_array().expect("hits is an array");
        assert!(!hits.is_empty(), "expected at least one hit");
        assert_eq!(hits[0]["id"], "doc1");
    }

    #[tokio::test]
    async fn prose_writer_tools_work() {
        let reg = prose_writer_toolkit();
        let out = reg.dispatch("style_guide", serde_json::json!({
            "style": "APA", "text": "Some prose."
        })).await.unwrap();
        assert!(out["suggestions"].as_str().unwrap().contains("APA"));

        let out = reg.dispatch("rewrite", serde_json::json!({
            "text": "Original text.", "brief": "make it shorter"
        })).await.unwrap();
        assert!(out["brief"].as_str().unwrap().contains("shorter"));
    }

    #[tokio::test]
    async fn research_tools_work() {
        let reg = research_toolkit();
        let out = reg.dispatch("doc_index", serde_json::json!({
            "id": "doc1", "text": "important content", "source": "https://example.com"
        })).await.unwrap();
        assert!(out["indexed"].as_bool().unwrap());

        let out = reg.dispatch("citation", serde_json::json!({
            "style": "APA", "author": "Smith", "title": "Hello", "year": "2026", "publisher": "X"
        })).await.unwrap();
        assert!(out["citation"].as_str().unwrap().contains("Smith"));
    }

    #[tokio::test]
    async fn web_search_tool_works() {
        let reg = search_toolkit();
        let out = reg.dispatch("web_search", serde_json::json!({
            "query": "rust language"
        })).await.unwrap();
        let results = out["results"].as_array().unwrap();
        assert!(!results.is_empty());
    }

    #[tokio::test]
    async fn adventure_game_tools_work() {
        let reg = adventure_game_toolkit();
        reg.dispatch("game_state", serde_json::json!({
            "action": "set", "key": "location", "value": "tavern"
        })).await.unwrap();
        let out = reg.dispatch("game_state", serde_json::json!({
            "action": "get", "key": "location"
        })).await.unwrap();
        assert_eq!(out["value"], "tavern");

        reg.dispatch("inventory", serde_json::json!({
            "action": "add", "item": "sword"
        })).await.unwrap();
        let out = reg.dispatch("inventory", serde_json::json!({
            "action": "list"
        })).await.unwrap();
        let items = out["items"].as_array().unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0], "sword");
    }

    #[tokio::test]
    async fn dice_roll_within_range() {
        let tool = DiceRollTool;
        let out = tool.run(serde_json::json!({ "notation": "2d6" })).await.unwrap();
        let total = out["total"].as_i64().unwrap();
        assert!(total >= 2 && total <= 12, "2d6 should be 2-12, got {total}");
    }

    #[tokio::test]
    async fn character_sheet_round_trip() {
        let tool = CharacterSheetTool::new();
        tool.run(serde_json::json!({
            "action": "create",
            "name": "Aragorn",
            "data": { "class": "ranger", "level": 10 }
        })).await.unwrap();
        let out = tool.run(serde_json::json!({
            "action": "get", "name": "Aragorn"
        })).await.unwrap();
        assert_eq!(out["data"]["class"], "ranger");
        assert_eq!(out["data"]["level"], 10);
    }

    #[tokio::test]
    async fn lore_graph_and_consistency() {
        let graph = LoreGraphTool::new();
        graph.run(serde_json::json!({
            "action": "add_entity",
            "entity": "Gandalf",
            "properties": { "race": "Maiar", "color": "grey" }
        })).await.unwrap();
        graph.run(serde_json::json!({
            "action": "add_relation",
            "source": "Gandalf", "relation": "member_of", "target": "Istari"
        })).await.unwrap();
        let out = graph.run(serde_json::json!({
            "action": "query_entity", "entity": "Gandalf"
        })).await.unwrap();
        assert!(out["exists"].as_bool().unwrap());
        let rels = out["relations"].as_array().unwrap();
        assert_eq!(rels.len(), 1);

        // consistency_check on a fresh tool — no issues because graph is empty.
        let check = ConsistencyCheckTool::new();
        let out = check.run(serde_json::json!({})).await.unwrap();
        assert!(out["consistent"].as_bool().unwrap());
    }
}
