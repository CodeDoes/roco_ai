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

use crate::audio::{AudioBackend, StubAudioBackend, SttRequest, TtsRequest};
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

/// A ready-made registry of the standard file/process tools, confined to
/// `root` and executing shell commands through `sandbox`.
pub fn standard_toolkit(root: PathBuf, sandbox: Sandbox) -> ToolRegistry {
    let mut r = ToolRegistry::new();
    r.register(Arc::new(ReadTool::new(root.clone())));
    r.register(Arc::new(WriteTool::new(root.clone())));
    r.register(Arc::new(ListTool::new(root.clone())));
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

// ---------------------------------------------------------------------------
// Speech: STT / TTS
// ---------------------------------------------------------------------------

/// Transcribe speech audio at `audio_path` to text (speech-to-text).
pub struct SttTool {
    backend: Arc<dyn AudioBackend>,
}

impl SttTool {
    pub fn new(backend: Arc<dyn AudioBackend>) -> Self {
        Self { backend }
    }
}

#[async_trait]
impl Tool for SttTool {
    fn name(&self) -> &str {
        "stt"
    }
    fn description(&self) -> &str {
        "Transcribe speech audio at `audio_path` to text (speech-to-text)."
    }
    fn input_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "audio_path": { "type": "string", "description": "Path to the audio file" },
                "model": { "type": "string", "description": "Optional transcription model hint" }
            },
            "required": ["audio_path"]
        })
    }
    async fn run(&self, input: Value) -> Result<Value, ToolError> {
        let audio_path = input
            .get("audio_path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidInput {
                name: "stt".into(),
                reason: "missing 'audio_path'".into(),
            })?;
        let model = input
            .get("model")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let resp = self.backend.stt(&SttRequest {
            audio_path: std::path::PathBuf::from(audio_path),
            model,
        });
        match resp {
            Ok(r) => Ok(serde_json::json!({ "text": r.text })),
            Err(e) => Ok(serde_json::json!({ "ok": false, "error": e.to_string() })),
        }
    }
}

/// Synthesize `text` to speech audio (text-to-speech); writes to `out_path`.
pub struct TtsTool {
    backend: Arc<dyn AudioBackend>,
}

impl TtsTool {
    pub fn new(backend: Arc<dyn AudioBackend>) -> Self {
        Self { backend }
    }
}

#[async_trait]
impl Tool for TtsTool {
    fn name(&self) -> &str {
        "tts"
    }
    fn description(&self) -> &str {
        "Synthesize `text` to speech audio (text-to-speech); writes to `out_path` (model-dependent)."
    }
    fn input_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "text": { "type": "string", "description": "Text to speak" },
                "voice": { "type": "string", "description": "Optional voice/id" },
                "out_path": { "type": "string", "description": "Optional output audio path" }
            },
            "required": ["text"]
        })
    }
    async fn run(&self, input: Value) -> Result<Value, ToolError> {
        let text = input.get("text").and_then(|v| v.as_str()).ok_or_else(|| ToolError::InvalidInput {
            name: "tts".into(),
            reason: "missing 'text'".into(),
        })?;
        let voice = input
            .get("voice")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let out_path = input
            .get("out_path")
            .and_then(|v| v.as_str())
            .map(std::path::PathBuf::from);
        let resp = self.backend.tts(&TtsRequest {
            text: text.to_string(),
            voice,
            out_path,
        });
        match resp {
            Ok(r) => Ok(serde_json::json!({ "out_path": r.out_path.to_string_lossy(), "bytes": r.bytes })),
            Err(e) => Ok(serde_json::json!({ "ok": false, "error": e.to_string() })),
        }
    }
}

/// A full agent toolkit: the standard file/process tools plus the RAG tools
/// (sharing `store` + `embedder`) and the STT/TTS tools (sharing `audio`).
pub fn agent_toolkit(
    root: PathBuf,
    sandbox: Sandbox,
    store: SharedVectorStore,
    embedder: Arc<dyn Embedder>,
    audio: Arc<dyn AudioBackend>,
) -> ToolRegistry {
    let mut r = standard_toolkit(root, sandbox);
    r.register(Arc::new(VectorUpsertTool::new(store.clone(), embedder.clone())));
    r.register(Arc::new(VectorSearchTool::new(store, embedder)));
    r.register(Arc::new(SttTool::new(audio.clone())));
    r.register(Arc::new(TtsTool::new(audio)));
    r
}

/// Convenience builder: a fresh 256-dim hashing index, stub audio backend, and
/// the standard file/process tools. Swap `audio`/`embedder` for real backends.
pub fn default_agent_toolkit(root: PathBuf, sandbox: Sandbox) -> ToolRegistry {
    let store: SharedVectorStore = Arc::new(Mutex::new(VectorStore::new(256)));
    let embedder: Arc<dyn Embedder> = Arc::new(HashingEmbedder::new(256));
    let audio: Arc<dyn AudioBackend> = Arc::new(StubAudioBackend);
    agent_toolkit(root, sandbox, store, embedder, audio)
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
        let audio: Arc<dyn crate::audio::AudioBackend> =
            Arc::new(crate::audio::StubAudioBackend);
        let reg = agent_toolkit(temp_root(), Sandbox::new(), store, embedder, audio);

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
    async fn stt_tts_tools_report_when_audio_backend_unwired() {
        let reg = default_agent_toolkit(temp_root(), Sandbox::new());
        let stt = reg
            .dispatch("stt", serde_json::json!({ "audio_path": "x.wav" }))
            .await
            .unwrap();
        assert_eq!(stt["ok"], false);
        assert!(stt["error"].as_str().unwrap().contains("not wired"));

        let tts = reg
            .dispatch("tts", serde_json::json!({ "text": "hello" }))
            .await
            .unwrap();
        assert_eq!(tts["ok"], false);
    }
}
