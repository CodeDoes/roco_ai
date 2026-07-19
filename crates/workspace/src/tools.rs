//! Workspace-scoped tool implementations.
//!
//! Each tool holds an [`Arc<Workspace>`](crate::Workspace) and resolves every
//! file path through [`Workspace::resolve`], so file operations can never
//! escape the workspace boundary. These satisfy `goals/workspace/file_tools.md`
//! (read / write / edit / search / list) and `goals/workspace/bash_like_tools.md`
//! (a shell tool confined to the workspace's working directory).

use std::sync::Arc;

use roco_tools::{Tool, ToolError};
use serde_json::Value;

use crate::workspace::Workspace;

fn arg_str(args: &Value, key: &str) -> Result<String, ToolError> {
    args.get(key)
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| ToolError(format!("missing string argument '{}'", key)))
}

fn arg_opt_str<'a>(args: &'a Value, key: &str) -> Option<&'a str> {
    args.get(key).and_then(|v| v.as_str())
}

// ── WorkspaceReadTool ────────────────────────────────────────────

pub struct WorkspaceReadTool {
    pub(crate) ws: Arc<Workspace>,
}

impl Tool for WorkspaceReadTool {
    fn name(&self) -> &str {
        "read"
    }
    fn description(&self) -> &str {
        "Read a file inside the workspace. `path` is relative to the workspace root."
    }
    fn schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": {"type": "string", "description": "Workspace-relative path to the file"}
            },
            "required": ["path"]
        })
    }
    fn call(&self, args: Value) -> Result<Value, ToolError> {
        let path = arg_str(&args, "path")?;
        let resolved = self
            .ws
            .resolve(&path)
            .map_err(|e| ToolError(e.to_string()))?;
        let content = std::fs::read_to_string(&resolved)
            .map_err(|e| ToolError(format!("read {}: {}", resolved.display(), e)))?;
        Ok(serde_json::json!({ "path": resolved.display().to_string(), "content": content }))
    }
}

// ── WorkspaceWriteTool ──────────────────────────────────────────

pub struct WorkspaceWriteTool {
    pub(crate) ws: Arc<Workspace>,
}

impl Tool for WorkspaceWriteTool {
    fn name(&self) -> &str {
        "write"
    }
    fn description(&self) -> &str {
        "Write content to a file inside the workspace, creating parent dirs as needed."
    }
    fn schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": {"type": "string", "description": "Workspace-relative path"},
                "content": {"type": "string", "description": "Content to write"}
            },
            "required": ["path", "content"]
        })
    }
    fn call(&self, args: Value) -> Result<Value, ToolError> {
        let path = arg_str(&args, "path")?;
        let content = arg_str(&args, "content")?;
        let resolved = self
            .ws
            .resolve(&path)
            .map_err(|e| ToolError(e.to_string()))?;
        if let Some(parent) = resolved.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| ToolError(format!("mkdir {}: {}", parent.display(), e)))?;
        }
        std::fs::write(&resolved, &content)
            .map_err(|e| ToolError(format!("write {}: {}", resolved.display(), e)))?;
        Ok(serde_json::json!({
            "path": resolved.display().to_string(),
            "ok": true,
            "bytes": content.len()
        }))
    }
}

// ── WorkspaceEditTool ───────────────────────────────────────────

pub struct WorkspaceEditTool {
    pub(crate) ws: Arc<Workspace>,
}

impl Tool for WorkspaceEditTool {
    fn name(&self) -> &str {
        "edit"
    }
    fn description(&self) -> &str {
        "Replace occurrences of `old` with `new` in a workspace file. Set `all: false` to replace only the first match."
    }
    fn schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": {"type": "string", "description": "Workspace-relative path"},
                "old": {"type": "string", "description": "Text to find"},
                "new": {"type": "string", "description": "Replacement text"},
                "all": {"type": "boolean", "description": "Replace all matches (default true)"}
            },
            "required": ["path", "old"]
        })
    }
    fn call(&self, args: Value) -> Result<Value, ToolError> {
        let path = arg_str(&args, "path")?;
        let old = arg_str(&args, "old")?;
        let new = arg_opt_str(&args, "new").unwrap_or("").to_string();
        let all = args.get("all").and_then(|v| v.as_bool()).unwrap_or(true);
        let resolved = self
            .ws
            .resolve(&path)
            .map_err(|e| ToolError(e.to_string()))?;

        let content = std::fs::read_to_string(&resolved)
            .map_err(|e| ToolError(format!("read {}: {}", resolved.display(), e)))?;

        let (new_content, count) = if all {
            let count = content.matches(&old).count();
            (content.replace(&old, &new), count)
        } else {
            match content.find(&old) {
                Some(idx) => {
                    let mut c = content.clone();
                    c.replace_range(idx..idx + old.len(), &new);
                    (c, 1)
                }
                None => (content, 0),
            }
        };

        if count > 0 {
            std::fs::write(&resolved, &new_content)
                .map_err(|e| ToolError(format!("write {}: {}", resolved.display(), e)))?;
        }

        Ok(serde_json::json!({
            "path": resolved.display().to_string(),
            "ok": count > 0,
            "replacements": count
        }))
    }
}

// ── WorkspaceSearchTool ─────────────────────────────────────────

pub struct WorkspaceSearchTool {
    pub(crate) ws: Arc<Workspace>,
}

impl Tool for WorkspaceSearchTool {
    fn name(&self) -> &str {
        "search"
    }
    fn description(&self) -> &str {
        "Grep for a substring across files inside the workspace. `path` defaults to the workspace root."
    }
    fn schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "pattern": {"type": "string", "description": "Substring to search for"},
                "path": {"type": "string", "description": "Workspace-relative dir/file (default: workspace root)"},
                "max_results": {"type": "integer", "description": "Max matches to return"}
            },
            "required": ["pattern"]
        })
    }
    fn call(&self, args: Value) -> Result<Value, ToolError> {
        let pattern = arg_str(&args, "pattern")?;
        let max_results = args
            .get("max_results")
            .and_then(|v| v.as_u64())
            .unwrap_or(20) as usize;
        let base = match arg_opt_str(&args, "path") {
            Some(p) if !p.is_empty() => self.ws.resolve(p).map_err(|e| ToolError(e.to_string()))?,
            _ => self.ws.root().to_path_buf(),
        };

        let mut results = Vec::new();
        for entry in walkdir::WalkDir::new(&base)
            .into_iter()
            .filter_map(|e| e.ok())
        {
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
                    if line.contains(&pattern) {
                        results.push(serde_json::json!({
                            "file": entry.path().display().to_string(),
                            "line": lineno + 1,
                            "text": line
                        }));
                    }
                }
            }
        }

        Ok(serde_json::json!({ "matches": results, "count": results.len() }))
    }
}

// ── WorkspaceGrepTool ───────────────────────────────────────────

pub struct WorkspaceGrepTool {
    pub(crate) ws: Arc<Workspace>,
}

impl Tool for WorkspaceGrepTool {
    fn name(&self) -> &str {
        "grep"
    }
    fn description(&self) -> &str {
        "Perform a regex-based search/grep across files inside the workspace."
    }
    fn schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "pattern": {"type": "string", "description": "Regex pattern to search for"},
                "path": {"type": "string", "description": "Workspace-relative dir/file to scan (default: workspace root)"},
                "max_results": {"type": "integer", "description": "Max matches to return"}
            },
            "required": ["pattern"]
        })
    }
    fn call(&self, args: Value) -> Result<Value, ToolError> {
        let pattern_str = arg_str(&args, "pattern")?;
        let max_results = args
            .get("max_results")
            .and_then(|v| v.as_u64())
            .unwrap_or(20) as usize;
        let base = match arg_opt_str(&args, "path") {
            Some(p) if !p.is_empty() => self.ws.resolve(p).map_err(|e| ToolError(e.to_string()))?,
            _ => self.ws.root().to_path_buf(),
        };

        let re = regex::Regex::new(&pattern_str)
            .map_err(|e| ToolError(format!("Invalid regex pattern '{}': {}", pattern_str, e)))?;

        let mut results = Vec::new();
        for entry in walkdir::WalkDir::new(&base)
            .into_iter()
            .filter_map(|e| e.ok())
        {
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
                    if re.is_match(line) {
                        results.push(serde_json::json!({
                            "file": entry.path().display().to_string(),
                            "line": lineno + 1,
                            "text": line
                        }));
                    }
                }
            }
        }

        Ok(serde_json::json!({ "matches": results, "count": results.len() }))
    }
}

// ── WorkspaceListTool ───────────────────────────────────────────

pub struct WorkspaceListTool {
    pub(crate) ws: Arc<Workspace>,
}

impl Tool for WorkspaceListTool {
    fn name(&self) -> &str {
        "list"
    }
    fn description(&self) -> &str {
        "List files and directories inside the workspace. `path` defaults to the workspace root."
    }
    fn schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": {"type": "string", "description": "Workspace-relative dir (default: root)"}
            }
        })
    }
    fn call(&self, args: Value) -> Result<Value, ToolError> {
        let base = match arg_opt_str(&args, "path") {
            Some(p) if !p.is_empty() => self.ws.resolve(p).map_err(|e| ToolError(e.to_string()))?,
            _ => self.ws.root().to_path_buf(),
        };
        let dir = std::fs::read_dir(&base)
            .map_err(|e| ToolError(format!("read_dir {}: {}", base.display(), e)))?;
        let mut entries = Vec::new();
        for entry in dir {
            let entry = entry.map_err(|e| ToolError(format!("entry error: {e}")))?;
            let ft = entry.file_type().ok();
            entries.push(serde_json::json!({
                "name": entry.file_name().to_string_lossy(),
                "dir": ft.map_or(false, |f| f.is_dir()),
                "size": entry.metadata().ok().map(|m| m.len())
            }));
        }
        Ok(serde_json::json!({ "path": base.display().to_string(), "entries": entries }))
    }
}

// ── WorkspaceBashTool ───────────────────────────────────────────

pub struct WorkspaceBashTool {
    pub(crate) ws: Arc<Workspace>,
}

impl Tool for WorkspaceBashTool {
    fn name(&self) -> &str {
        "bash"
    }
    fn description(&self) -> &str {
        "Run a shell command with the workspace working directory as the cwd. \
         Note: the shell is not fully sandboxed — it is scoped to the workspace \
         directory but can still invoke arbitrary programs."
    }
    fn schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "command": {"type": "string", "description": "Shell command to run"},
                "timeout": {"type": "integer", "description": "Timeout in seconds (unused, reserved)"}
            },
            "required": ["command"]
        })
    }
    fn call(&self, args: Value) -> Result<Value, ToolError> {
        let cmd = arg_str(&args, "command")?;
        if let Some(reason) = crate::workspace::blocked_command_reason(&cmd) {
            return Err(ToolError(format!(
                "command blocked by workspace policy (matched '{reason}'): refusing to run"
            )));
        }
        let cwd = self.ws.root().join(self.ws.cwd());
        let output = std::process::Command::new("sh")
            .arg("-c")
            .arg(&cmd)
            .current_dir(&cwd)
            .output()
            .map_err(|e| ToolError(format!("exec error: {e}")))?;
        Ok(serde_json::json!({
            "cwd": cwd.display().to_string(),
            "stdout": String::from_utf8_lossy(&output.stdout),
            "stderr": String::from_utf8_lossy(&output.stderr),
            "exit_code": output.status.code()
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::WorkspaceKind;

    fn make_ws() -> Arc<Workspace> {
        Arc::new(Workspace::temp(WorkspaceKind::Temp).unwrap())
    }

    #[test]
    fn read_write_roundtrip_stays_in_workspace() {
        let ws = make_ws();
        let tools = Workspace::scoped_tools(ws.clone());
        let write = tools.iter().find(|t| t.name() == "write").unwrap();
        let r = write
            .call(serde_json::json!({"path": "a/b.txt", "content": "hello"}))
            .unwrap();
        assert_eq!(r["ok"], true);
        assert_eq!(r["bytes"], 5);

        let read = tools.iter().find(|t| t.name() == "read").unwrap();
        let r = read.call(serde_json::json!({"path": "a/b.txt"})).unwrap();
        assert_eq!(r["content"], "hello");
    }

    #[test]
    fn read_outside_workspace_is_rejected() {
        let ws = make_ws();
        let tools = Workspace::scoped_tools(ws.clone());
        let read = tools.iter().find(|t| t.name() == "read").unwrap();
        let r = read.call(serde_json::json!({"path": "../../etc/passwd"}));
        assert!(r.is_err(), "escape attempt must be rejected");
    }

    #[test]
    fn edit_replaces_text() {
        let ws = make_ws();
        let tools = Workspace::scoped_tools(ws.clone());
        let write = tools.iter().find(|t| t.name() == "write").unwrap();
        write
            .call(serde_json::json!({"path": "f.txt", "content": "foo bar foo"}))
            .unwrap();
        let edit = tools.iter().find(|t| t.name() == "edit").unwrap();
        let r = edit
            .call(serde_json::json!({"path": "f.txt", "old": "foo", "new": "baz", "all": true}))
            .unwrap();
        assert_eq!(r["replacements"], 2);
        let read = tools.iter().find(|t| t.name() == "read").unwrap();
        let r = read.call(serde_json::json!({"path": "f.txt"})).unwrap();
        assert_eq!(r["content"], "baz bar baz");
    }

    #[test]
    fn search_finds_pattern() {
        let ws = make_ws();
        let tools = Workspace::scoped_tools(ws.clone());
        let write = tools.iter().find(|t| t.name() == "write").unwrap();
        write
            .call(serde_json::json!({"path": "x.txt", "content": "needle here\nsomething else"}))
            .unwrap();
        let search = tools.iter().find(|t| t.name() == "search").unwrap();
        let r = search
            .call(serde_json::json!({"pattern": "needle"}))
            .unwrap();
        assert_eq!(r["count"], 1);
        assert_eq!(r["matches"][0]["line"], 1);
    }

    #[test]
    fn grep_finds_pattern_with_regex() {
        let ws = make_ws();
        let tools = Workspace::scoped_tools(ws.clone());
        let write = tools.iter().find(|t| t.name() == "write").unwrap();
        write
            .call(
                serde_json::json!({"path": "y.txt", "content": "hello 123 world\nno digits here"}),
            )
            .unwrap();
        let grep = tools.iter().find(|t| t.name() == "grep").unwrap();
        let r = grep.call(serde_json::json!({"pattern": r"\d+"})).unwrap();
        assert_eq!(r["count"], 1);
        assert_eq!(r["matches"][0]["line"], 1);
        assert!(r["matches"][0]["text"].as_str().unwrap().contains("123"));
    }

    #[test]
    fn list_returns_entries() {
        let ws = make_ws();
        let tools = Workspace::scoped_tools(ws.clone());
        let write = tools.iter().find(|t| t.name() == "write").unwrap();
        write
            .call(serde_json::json!({"path": "one.txt", "content": "x"}))
            .unwrap();
        let list = tools.iter().find(|t| t.name() == "list").unwrap();
        let r = list.call(serde_json::json!({})).unwrap();
        assert!(r["entries"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e["name"] == "one.txt"));
    }

    #[test]
    fn bash_runs_in_workspace_cwd() {
        let ws = make_ws();
        let tools = Workspace::scoped_tools(ws.clone());
        let bash = tools.iter().find(|t| t.name() == "bash").unwrap();
        let r = bash.call(serde_json::json!({"command": "pwd"})).unwrap();
        assert!(r["stdout"].as_str().unwrap().contains("roco-ws"));
    }

    #[test]
    fn bash_rejects_destructive_command() {
        let ws = make_ws();
        let tools = Workspace::scoped_tools(ws.clone());
        let bash = tools.iter().find(|t| t.name() == "bash").unwrap();
        let r = bash.call(serde_json::json!({"command": "rm -rf /"}));
        assert!(r.is_err(), "destructive command must be blocked");
        assert!(r.unwrap_err().to_string().contains("blocked"));
    }
}
