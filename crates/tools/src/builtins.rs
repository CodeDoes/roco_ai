//! Built-in tool implementations.
//!
//! Provides concrete [`Tool`] impls for common agent actions:
//! - `read` — read file contents
//! - `write` — write to a file
//! - `search` — grep/search within a workspace
//! - `list` — list directory contents
//! - `bash` — execute a shell command
//! - `now` — get current date/time

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
        assert_eq!(reg.len(), 6);
        assert!(reg.get("read").is_some());
        assert!(reg.get("write").is_some());
        assert!(reg.get("search").is_some());
        assert!(reg.get("list").is_some());
        assert!(reg.get("bash").is_some());
        assert!(reg.get("now").is_some());
    }

    #[test]
    fn bash_tool_rejects_missing_command() {
        let tool = BashTool;
        let result = tool.call(serde_json::json!({}));
        assert!(result.is_err());
    }
}
