//! Built-in tool implementations.
//!
//! Provides concrete [`Tool`] impls for common agent actions:
//! - `read` — read file contents
//! - `write` — write to a file
//! - `search` — grep/search within a workspace
//! - `list` — list directory contents
//! - `bash` — execute a shell command
//! - `now` — get current date/time
//! - `find_long_files` — find files exceeding a line-count threshold

use std::sync::Arc;
use std::time::SystemTime;

use crate::tool::{Tool, ToolError};

/// Register all built-in tools into a shared registry.
pub fn all_tools() -> Vec<Arc<dyn Tool>> {
    vec![
        Arc::new(ReadTool),
        Arc::new(WriteTool),
        Arc::new(SearchTool),
        Arc::new(ListDirTool),
        Arc::new(BashTool),
        Arc::new(NowTool),
        Arc::new(VectorSearchTool),
    ]
}

// ── ReadTool ─────────────────────────────────────────────────────

pub struct ReadTool;

impl Tool for ReadTool {
    fn name(&self) -> &str {
        "read"
    }
    fn description(&self) -> &str {
        "Read the contents of a file. Pass `path` as the absolute or relative file path."
    }
    fn schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": {"type": "string", "description": "Path to the file to read"}
            },
            "required": ["path"]
        })
    }
    fn call(&self, args: serde_json::Value) -> Result<serde_json::Value, ToolError> {
        let path = args
            .get("path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError("missing 'path' argument".into()))?;
        let content =
            std::fs::read_to_string(path).map_err(|e| ToolError(format!("read error: {e}")))?;
        Ok(serde_json::json!({"content": content}))
    }
}

// ── WriteTool ────────────────────────────────────────────────────

pub struct WriteTool;

impl Tool for WriteTool {
    fn name(&self) -> &str {
        "write"
    }
    fn description(&self) -> &str {
        "Write content to a file. Creates parent directories if needed."
    }
    fn schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": {"type": "string", "description": "Path to the file"},
                "content": {"type": "string", "description": "Content to write"}
            },
            "required": ["path", "content"]
        })
    }
    fn call(&self, args: serde_json::Value) -> Result<serde_json::Value, ToolError> {
        let path = args
            .get("path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError("missing 'path' argument".into()))?;
        let content = args
            .get("content")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError("missing 'content' argument".into()))?;
        if let Some(parent) = std::path::Path::new(path).parent() {
            std::fs::create_dir_all(parent).map_err(|e| ToolError(format!("mkdir error: {e}")))?;
        }
        std::fs::write(path, content).map_err(|e| ToolError(format!("write error: {e}")))?;
        Ok(serde_json::json!({"ok": true, "bytes": content.len()}))
    }
}

// ── SearchTool ───────────────────────────────────────────────────

pub struct SearchTool;

impl Tool for SearchTool {
    fn name(&self) -> &str {
        "search"
    }
    fn description(&self) -> &str {
        "Search for a pattern in files within a directory using grep."
    }
    fn schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "pattern": {"type": "string", "description": "Search pattern (regex)"},
                "path": {"type": "string", "description": "Directory or file to search (default: .)"},
                "max_results": {"type": "integer", "description": "Max matches to return"}
            },
            "required": ["pattern"]
        })
    }
    fn call(&self, args: serde_json::Value) -> Result<serde_json::Value, ToolError> {
        let pattern = args
            .get("pattern")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError("missing 'pattern' argument".into()))?;
        let path = args.get("path").and_then(|v| v.as_str()).unwrap_or(".");
        let max_results = args
            .get("max_results")
            .and_then(|v| v.as_u64())
            .unwrap_or(20) as usize;

        let mut results = Vec::new();
        let walker = walkdir::WalkDir::new(path);
        for entry in walker.into_iter().filter_map(|e| e.ok()) {
            if !entry.file_type().is_file() {
                continue;
            }
            if results.len() >= max_results {
                break;
            }
            if let Ok(contents) = std::fs::read_to_string(entry.path()) {
                for (lineno, line) in contents.lines().enumerate() {
                    if results.len() >= max_results {
                        break;
                    }
                    if line.contains(pattern) {
                        results.push(serde_json::json!({
                            "file": entry.path().to_string_lossy(),
                            "line": lineno + 1,
                            "text": line
                        }));
                    }
                }
            }
        }

        Ok(serde_json::json!({"matches": results, "count": results.len()}))
    }
}

// ── ListDirTool ──────────────────────────────────────────────────

pub struct ListDirTool;

impl Tool for ListDirTool {
    fn name(&self) -> &str {
        "list"
    }
    fn description(&self) -> &str {
        "List files and directories at a path."
    }
    fn schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": {"type": "string", "description": "Directory path (default: .)"}
            }
        })
    }
    fn call(&self, args: serde_json::Value) -> Result<serde_json::Value, ToolError> {
        let path = args.get("path").and_then(|v| v.as_str()).unwrap_or(".");
        let dir = std::fs::read_dir(path).map_err(|e| ToolError(format!("read_dir error: {e}")))?;
        let mut entries = Vec::new();
        for entry in dir {
            let entry = entry.map_err(|e| ToolError(format!("entry error: {e}")))?;
            let ft = entry.file_type().ok();
            entries.push(serde_json::json!({
                "name": entry.file_name().to_string_lossy(),
                "dir": ft.is_some_and(|f| f.is_dir()),
                "size": entry.metadata().ok().map(|m| m.len())
            }));
        }
        Ok(serde_json::json!({"entries": entries}))
    }
}

// ── BashTool ─────────────────────────────────────────────────────

pub struct BashTool;

impl Tool for BashTool {
    fn name(&self) -> &str {
        "bash"
    }
    fn description(&self) -> &str {
        "Execute a shell command and return its stdout/stderr."
    }
    fn schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "command": {"type": "string", "description": "Shell command to run"},
                "timeout": {"type": "integer", "description": "Timeout in seconds"}
            },
            "required": ["command"]
        })
    }
    fn call(&self, args: serde_json::Value) -> Result<serde_json::Value, ToolError> {
        let cmd = args
            .get("command")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError("missing 'command' argument".into()))?;
        let output = std::process::Command::new("sh")
            .arg("-c")
            .arg(cmd)
            .output()
            .map_err(|e| ToolError(format!("exec error: {e}")))?;
        Ok(serde_json::json!({
            "stdout": String::from_utf8_lossy(&output.stdout),
            "stderr": String::from_utf8_lossy(&output.stderr),
            "exit_code": output.status.code()
        }))
    }
}

// ── NowTool ──────────────────────────────────────────────────────

pub struct NowTool;

impl Tool for NowTool {
    fn name(&self) -> &str {
        "now"
    }
    fn description(&self) -> &str {
        "Get the current date and time."
    }
    fn schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {}
        })
    }
    fn call(&self, _args: serde_json::Value) -> Result<serde_json::Value, ToolError> {
        let now = SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        Ok(serde_json::json!({"timestamp": now}))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tool::Tool;

    #[test]
    fn read_tool_rejects_missing_path() {
        let tool = ReadTool;
        let result = tool.call(serde_json::json!({}));
        assert!(result.is_err());
    }

    #[test]
    fn now_tool_returns_timestamp() {
        let tool = NowTool;
        let result = tool.call(serde_json::json!({})).unwrap();
        assert!(result.get("timestamp").and_then(|v| v.as_u64()).is_some());
    }

    #[test]
    fn all_tools_are_registrable() {
        let mut reg = crate::ToolRegistry::new();
        for tool in all_tools() {
            reg.register(tool);
        }
        assert_eq!(reg.len(), 7);
        assert!(reg.get("read").is_some());
        assert!(reg.get("write").is_some());
        assert!(reg.get("search").is_some());
        assert!(reg.get("list").is_some());
        assert!(reg.get("bash").is_some());
        assert!(reg.get("now").is_some());
        assert!(reg.get("vector_search").is_some());
    }

    #[test]
    fn bash_tool_rejects_missing_command() {
        let tool = BashTool;
        let result = tool.call(serde_json::json!({}));
        assert!(result.is_err());
    }
}

// ── VectorSearchTool ─────────────────────────────────────────────

pub struct VectorSearchTool;

impl Tool for VectorSearchTool {
    fn name(&self) -> &str {
        "vector_search"
    }
    fn description(&self) -> &str {
        "Manage and query a local vector embedding similarity search index. \
         Supports actions: 'add' (index text with optional metadata), 'query' (retrieve nearest neighbors), \
         and 'status' (get metadata about the index)."
    }
    fn schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["add", "query", "status"],
                    "description": "The action to perform: add, query, or status"
                },
                "text": {
                    "type": "string",
                    "description": "The text content to embed and index (required for 'add')"
                },
                "id": {
                    "type": "string",
                    "description": "Optional custom unique ID for the indexed entry (for 'add')"
                },
                "metadata": {
                    "type": "object",
                    "description": "Optional arbitrary JSON metadata to attach to the entry (for 'add')"
                },
                "query": {
                    "type": "string",
                    "description": "The search query string (required for 'query')"
                },
                "limit": {
                    "type": "integer",
                    "description": "Max number of nearest neighbors to return (default: 5, for 'query')"
                },
                "index_path": {
                    "type": "string",
                    "description": "Custom path to the index JSON file (optional, defaults to .roco/vector_store.json)"
                }
            },
            "required": ["action"]
        })
    }
    fn call(&self, args: serde_json::Value) -> Result<serde_json::Value, ToolError> {
        let action = args
            .get("action")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError("missing 'action' argument".into()))?;

        let index_path_str = args
            .get("index_path")
            .and_then(|v| v.as_str())
            .unwrap_or(".roco/vector_store.json");
        let index_path = std::path::Path::new(index_path_str);

        // Load the store
        let mut store = crate::embeddings::VectorStore::load_from_file(index_path)
            .map_err(|e| ToolError(format!("failed to load vector index: {e}")))?;

        match action {
            "add" => {
                let text = args
                    .get("text")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| ToolError("missing 'text' argument for 'add' action".into()))?;
                let id = args.get("id").and_then(|v| v.as_str()).map(|s| s.to_string());
                let metadata = args
                    .get("metadata")
                    .cloned()
                    .unwrap_or(serde_json::json!({}));

                let added_id = store.add(id, text, metadata);
                store.save_to_file(index_path)
                    .map_err(|e| ToolError(format!("failed to save vector index: {e}")))?;

                Ok(serde_json::json!({
                    "ok": true,
                    "id": added_id,
                    "message": "Text successfully embedded and indexed."
                }))
            }
            "query" => {
                let query = args
                    .get("query")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| ToolError("missing 'query' argument for 'query' action".into()))?;
                let limit = args.get("limit").and_then(|v| v.as_u64()).unwrap_or(5) as usize;

                let results = store.search(query, limit);
                let items: Vec<serde_json::Value> = results
                    .into_iter()
                    .map(|r| {
                        serde_json::json!({
                            "id": r.entry.id,
                            "text": r.entry.text,
                            "score": r.score,
                            "metadata": r.entry.metadata
                        })
                    })
                    .collect();

                Ok(serde_json::json!({
                    "results": items,
                    "count": items.len()
                }))
            }
            "status" => {
                Ok(serde_json::json!({
                    "entries_count": store.entries.len(),
                    "dimensions": store.dimensions,
                    "index_path": index_path_str
                }))
            }
            _ => Err(ToolError(format!("unknown action: {action}"))),
        }
    }
}

#[cfg(test)]
mod vector_tool_tests {
    use super::*;

    #[test]
    fn test_vector_search_tool_lifecycle() {
        let temp_dir = std::env::temp_dir();
        let path = temp_dir.join("test_vector_store_tool.json");
        let path_str = path.to_string_lossy().to_string();

        let tool = VectorSearchTool;

        // Check status on empty
        let r_status = tool.call(serde_json::json!({
            "action": "status",
            "index_path": path_str
        })).unwrap();
        assert_eq!(r_status["entries_count"], 0);

        // Add an entry
        let r_add = tool.call(serde_json::json!({
            "action": "add",
            "text": "The quick brown fox jumps over the lazy dog",
            "id": "fox-1",
            "metadata": {"type": "animal"},
            "index_path": path_str
        })).unwrap();
        assert_eq!(r_add["ok"], true);
        assert_eq!(r_add["id"], "fox-1");

        // Add another entry
        let r_add2 = tool.call(serde_json::json!({
            "action": "add",
            "text": "Rust is an extremely safe systems programming language",
            "id": "rust-1",
            "metadata": {"type": "tech"},
            "index_path": path_str
        })).unwrap();
        assert_eq!(r_add2["ok"], true);

        // Check status updated
        let r_status2 = tool.call(serde_json::json!({
            "action": "status",
            "index_path": path_str
        })).unwrap();
        assert_eq!(r_status2["entries_count"], 2);

        // Query the index
        let r_query = tool.call(serde_json::json!({
            "action": "query",
            "query": "safe programming language",
            "limit": 1,
            "index_path": path_str
        })).unwrap();

        assert_eq!(r_query["count"], 1);
        let first_match = &r_query["results"][0];
        assert_eq!(first_match["id"], "rust-1");
        assert!(first_match["score"].as_f64().unwrap() > 0.0);

        let _ = std::fs::remove_file(path);
    }
}
