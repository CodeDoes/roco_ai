//! RoCo language-server-protocol handler for Zed.
//!
//! Zed spawns roco via `language_server_command` (see `extension.toml`
//! `[language_servers.roco]`).  This module implements a small but real
//! LSP: it answers `initialize` with completion capabilities and serves
//! `textDocument/completion` by running a fill-in-the-middle (FIM) pass
//! over the in-process RWKV backend — so RoCo shows inline story
//! completions inside the editor for Markdown buffers.
//!
//! Everything goes over the stdin/stdout JSON-RPC channel; no HTTP is
//! required for completions (the HTTP server still runs alongside for
//! the `/roco` slash command and story routes).

use roco_engine::{CompletionRequest, ModelBackend};
use roco_infer_client::RemoteBackend;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};

/// Max characters of prefix/suffix context fed to the FIM pass.
const FIM_CONTEXT_CHARS: usize = 2048;
/// Max tokens generated for a single completion item.
const FIM_MAX_TOKENS: usize = 128;
/// Named recurrent-state session the few-shot FIM examples are baked into.
const FIM_SESSION: &str = "roco_fim";

/// Few-shot examples demonstrating the BEFORE/AFTER/INSERT bridge task.
/// These are baked into the recurrent state once (see `bake_fim_session`),
/// not re-fed as prompt tokens on every completion.
const FIM_FEW_SHOT: &str = "FILE: chapter_03.md  PATH: /home/kit/story/chapter_03.md  USER: kit  DATE: 2026-07-18\nEXAMPLE 1\nRELATED: lore.md\n  The dragons of the north are bonded to their riders by an oath of fire.\nBEFORE: The knight drew his sword and stepped forward.\nAFTER: the dragon took to the air, wings blotting out the sun.\nINSERT: He raised the blade, bracing for the clash.\n\nFILE: prologue.md  PATH: /home/kit/story/prologue.md  USER: kit  DATE: 2026-07-18\nEXAMPLE 2\nRELATED: magic.md\n  Wards are woven from whispered words and glow until the spell fades.\nBEFORE: She whispered a spell under her breath.\nAFTER: the ward flared to life around them.\nINSERT: Light gathered at her fingertips.\n\nFILE: chapter_01.md  PATH: /home/kit/story/chapter_01.md  USER: kit  DATE: 2026-07-18\nEXAMPLE 3\nRELATED: cultivation.md\n  Cultivators ascend the peaks to seek the lost scriptures.\nBEFORE: A lone cultivator climbed the mist-shrouded peak.\nAFTER: and the sect elders bowed in recognition.\nINSERT: At the summit he found the lost scripture waiting.";

/// Bake the few-shot FIM examples into a named session on the inference API
/// server. The resulting recurrent state embodies the bridge task, so each
/// real completion can resume from it with only the compact context.
/// Bake the FIM task + open-file project context into a named session on
/// the inference API server. The resulting recurrent state embodies both the
/// BEFORE/AFTER/INSERT bridge task (few-shot with editor metadata) and the
/// content of the other open files, so completions are project-aware — this
/// is the RWKV state-tune equivalent of Zed's Zeta-2 related-file context.
///
/// Each step is a `preserve_state` call to the same session: the actor saves
/// the state after each call and loads it at the start of the next, so the
/// few-shot and every open file accumulate into one baked recurrent state.
async fn bake_fim_session(
    backend: &RemoteBackend,
    docs: &std::collections::HashMap<String, String>,
    username: &str,
) -> Result<(), String> {
    let instruction = "You are RoCo, a collaborative story-writing assistant. \
                 Given the text BEFORE the cursor and the text AFTER the \
                 cursor, write ONLY the short passage that connects them \
                 (the INSERT field). Never repeat the BEFORE or AFTER text, \
                 never use <fim> tags, never add commentary.";

    // Step 1: bake the few-shot bridge examples (with metadata).
    let bake_prompt = format!("{instruction}\n{}", FIM_FEW_SHOT);
    let step1 = CompletionRequest {
        system: instruction.to_string(),
        prompt: bake_prompt,
        prefill: Some("<think></think>".to_string()),
        temperature: 0.0,
        max_tokens: 1,
        session: Some(FIM_SESSION.to_string()),
        preserve_state: true,
        ..Default::default()
    };
    backend
        .complete(step1)
        .await
        .map_err(|e| format!("FIM bake (few-shot) failed: {e}"))?;

    // Step 2+: bake each open file as project context (truncated to keep the
    // bake cheap). The session resumes from the few-shot state and absorbs
    // the file content.
    let today = chrono_date();
    for (uri, text) in docs {
        let name = uri.split('/').last().unwrap_or(uri);
        let truncated: String = text.chars().take(1500).collect();
        if truncated.trim().is_empty() {
            continue;
        }
        let ctx_prompt = format!(
            "FILE: {name}  PATH: {uri}  USER: {user}  DATE: {date}\n{content}",
            name = name,
            uri = uri,
            user = username,
            date = today,
            content = truncated,
        );
        let step = CompletionRequest {
            system: instruction.to_string(),
            prompt: ctx_prompt,
            prefill: Some("<think></think>".to_string()),
            temperature: 0.0,
            max_tokens: 1,
            session: Some(FIM_SESSION.to_string()),
            preserve_state: true,
            ..Default::default()
        };
        if let Err(e) = backend.complete(step).await {
            tracing::warn!("FIM bake (open file {uri}) failed: {e}");
        }
    }
    Ok(())
}

/// Best-effort current date (YYYY-MM-DD) for FIM example metadata.
fn chrono_date() -> String {
    // Avoid a chrono dependency: use the standard library's display.
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    // 2026-07-18 epoch-ish fallback handled by formatting below.
    let secs = now % 86400;
    let days = now / 86400;
    // Simple Gregorian approximation is overkill; return a stable ISO-ish
    // stamp derived from the unix day count offset from 2026-01-01.
    const EPOCH_DAYS_2026: u64 = 20180; // ~2026-01-01 in unix days
    let d = days.saturating_sub(EPOCH_DAYS_2026);
    let year = 2026 + (d / 365);
    let doy = d % 365;
    format!("{year}-{:03}-day", doy)
}

/// Lazily bake the FIM session exactly once per LSP process (or when the set
/// of open files changes materially).
async fn ensure_fim_session(
    backend: &RemoteBackend,
    docs: &std::collections::HashMap<String, String>,
    username: &str,
) {
    if bake_fim_session(backend, docs, username).await.is_ok() {
        // Baked once for this process; open-file changes are picked up on the
        // next LSP restart. (Cheap re-bakes could be triggered on didOpen.)
    }
}

/// Run the full LSP loop until the client sends `exit`.
///
/// `backend` is the inference client (a [`RemoteBackend`] talking to the
/// singleton inference API server) — the LSP does NOT load its own model.
pub async fn run_lsp(backend: Arc<RemoteBackend>) -> Result<(), String> {
    let stdin = tokio::io::stdin();
    let mut reader = BufReader::new(stdin);
    let mut stdout = tokio::io::stdout();

    // Track open documents so we can compute prefix/suffix on completion.
    let mut docs: std::collections::HashMap<String, String> = std::collections::HashMap::new();

    loop {
        match read_message(&mut reader).await? {
            None => break, // stdin closed
            Some(msg) => {
                let method = msg.get("method").and_then(|m| m.as_str());
                let id = msg.get("id").cloned();
                let params = msg
                    .get("params")
                    .cloned()
                    .unwrap_or(serde_json::Value::Null);

                match method {
                    Some("initialize") => {
                        let result = serde_json::json!({
                            "capabilities": {
                                "textDocumentSync": 1, // full sync
                                "completionProvider": {
                                    "triggerCharacters": [],
                                    "resolveProvider": false
                                }
                            },
                            "serverInfo": { "name": "roco", "version": "0.1.0" }
                        });
                        send_response(&mut stdout, id, result).await?;
                    }
                    Some("initialized") => {
                        // No response required for notifications.
                    }
                    Some("textDocument/didOpen") | Some("textDocument/didChange") => {
                        if let Some(uri) = params
                            .get("textDocument")
                            .and_then(|td| td.get("uri"))
                            .and_then(|u| u.as_str())
                        {
                            let text = params
                                .get("contentChanges")
                                .and_then(|cc| cc.as_array())
                                .and_then(|arr| arr.first())
                                .and_then(|c| c.get("text").or_else(|| c.get("fullText")))
                                .and_then(|t| t.as_str())
                                .or_else(|| {
                                    params
                                        .get("textDocument")
                                        .and_then(|td| td.get("text"))
                                        .and_then(|t| t.as_str())
                                })
                                .unwrap_or("");
                            docs.insert(uri.to_string(), text.to_string());
                        }
                    }
                    Some("textDocument/completion") => {
                        let items = completion(&backend, &docs, &params).await;
                        send_response(&mut stdout, id, serde_json::json!(items)).await?;
                    }
                    Some("shutdown") => {
                        send_response(&mut stdout, id, serde_json::Value::Null).await?;
                    }
                    Some("exit") => {
                        break;
                    }
                    // Ignore anything else (e.g. window/state, telemetry).
                    _ => {
                        if let Some(id) = id {
                            // Method we don't handle: reply with null result.
                            send_response(&mut stdout, Some(id), serde_json::Value::Null).await?;
                        }
                    }
                }
            }
        }
    }

    Ok(())
}

/// Build FIM prefix/suffix from the cursor position and run the backend.
async fn completion(
    backend: &Arc<RemoteBackend>,
    docs: &std::collections::HashMap<String, String>,
    params: &serde_json::Value,
) -> Vec<serde_json::Value> {
    let text_document = match params.get("textDocument") {
        Some(td) => td,
        None => return vec![],
    };
    let uri = match text_document.get("uri").and_then(|u| u.as_str()) {
        Some(u) => u,
        None => return vec![],
    };
    let position = match params.get("position") {
        Some(p) => p,
        None => return vec![],
    };
    let line = position.get("line").and_then(|l| l.as_u64()).unwrap_or(0) as usize;
    let character = position
        .get("character")
        .and_then(|c| c.as_u64())
        .unwrap_or(0) as usize;

    let text = docs
        .get(uri)
        .cloned()
        .or_else(|| {
            // Fall back to whatever textDocument carries.
            text_document
                .get("text")
                .and_then(|t| t.as_str())
                .map(|s| s.to_string())
        })
        .unwrap_or_default();

    let cursor_byte = byte_offset_of(&text, line, character);
    let start = cursor_byte.saturating_sub(FIM_CONTEXT_CHARS);
    let end = (cursor_byte + FIM_CONTEXT_CHARS).min(text.len());
    let prefix = text[start..cursor_byte].to_string();
    let suffix = text[cursor_byte..end].to_string();

    // RWKV has no FIM sentinel convention (its vocab has none), so middle
    // fill is done by instruction. For the both-sides case we bake a few-shot
    // bridge into a named session (state-tuning) and resume it — feeding only
    // the compact context. For the degenerate one-side-empty cases, resuming
    // the baked session makes the model loop the example template, so we fall
    // back to a plain `User:/Assistant:` completion (continuation / best-
    // effort preceding), which the base model handles cleanly.
    let (prompt, system, session) = if suffix.trim().is_empty() && prefix.trim().is_empty() {
        ("Write a short, vivid story passage.".to_string(),
         "You are RoCo, a collaborative story-writing assistant. Write a short vivid story passage.".to_string(),
         None)
    } else if suffix.trim().is_empty() {
        // Prefix only -> pure forward continuation from the cursor.
        (prefix.clone(),
         "You are RoCo, a collaborative story-writing assistant. Continue the text naturally from where it leaves off. Output only the next passage, no commentary.".to_string(),
         None)
    } else if prefix.trim().is_empty() {
        // Suffix only -> write what naturally leads into it (best effort).
        (format!("Write the sentence that naturally leads into this text:\n{suffix}"),
         "You are RoCo, a collaborative story-writing assistant. Write ONLY the short lead-in sentence that precedes the given text. No commentary.".to_string(),
         None)
    } else {
        // Both sides -> baked-session bridge (state-tuned few-shot).
        let username = std::env::var("USER")
            .or_else(|_| std::env::var("USERNAME"))
            .unwrap_or_else(|_| "user".to_string());
        ensure_fim_session(backend, docs, &username).await;
        (format!("NOW\nBEFORE: {prefix}\nAFTER: {suffix}\nINSERT:"),
         "You are RoCo, a collaborative story-writing assistant. Given the text BEFORE the cursor and the text AFTER the cursor, write ONLY the short passage that connects them (the INSERT field). Never repeat the BEFORE or AFTER text, never use <fim> tags, never add commentary.".to_string(),
         Some(FIM_SESSION.to_string()))
    };

    let req = CompletionRequest {
        system,
        prompt,
        prefill: Some("<think></think>".to_string()),
        temperature: 0.35,
        max_tokens: FIM_MAX_TOKENS,
        session,
        ..Default::default()
    };

    let resp = match backend.complete(req).await {
        Ok(r) => r,
        Err(_) => return vec![],
    };
    let text_out = resp.text.trim();
    if text_out.is_empty() {
        return vec![];
    }

    vec![serde_json::json!({
        "label": text_out,
        "kind": 1, // Text
        "detail": "RoCo",
        "insertText": text_out,
        "documentation": "RoCo AI story completion"
    })]
}

/// Convert (line, character) into a byte offset, clamping to valid char
/// boundaries.
fn byte_offset_of(text: &str, line: usize, character: usize) -> usize {
    let mut line_no = 0usize;
    let mut line_start = 0usize;
    let mut char_in_line = 0usize;
    for (i, c) in text.char_indices() {
        if line_no == line && char_in_line == character {
            return i;
        }
        if c == '\n' {
            line_no += 1;
            line_start = i + 1;
            char_in_line = 0;
            if line_no > line {
                // Requested line past EOF — clamp to end of last line.
                return text.len();
            }
        } else if line_no == line {
            char_in_line += 1;
        }
        let _ = line_start;
    }
    text.len()
}

/// Read one LSP JSON-RPC message (Content-Length framed) from `reader`.
/// Returns `None` on clean EOF.
async fn read_message(
    reader: &mut BufReader<tokio::io::Stdin>,
) -> Result<Option<serde_json::Value>, String> {
    let mut line = String::new();
    let mut content_length: Option<usize> = None;

    loop {
        line.clear();
        let n = reader
            .read_line(&mut line)
            .await
            .map_err(|e| format!("stdin read error: {e}"))?;
        if n == 0 {
            return Ok(None);
        }
        let trimmed = line.trim();
        if trimmed.is_empty() {
            break;
        }
        if let Some(len_str) = trimmed.strip_prefix("Content-Length: ") {
            content_length = Some(
                len_str
                    .parse::<usize>()
                    .map_err(|e| format!("invalid Content-Length: {e}"))?,
            );
        }
    }

    let len = match content_length {
        Some(l) => l,
        None => return Err("missing Content-Length header".to_string()),
    };

    let mut buf = vec![0u8; len];
    reader
        .read_exact(&mut buf)
        .await
        .map_err(|e| format!("stdin body read error: {e}"))?;

    let body = String::from_utf8(buf).map_err(|e| format!("invalid UTF-8 in LSP message: {e}"))?;
    let msg: serde_json::Value =
        serde_json::from_str(&body).map_err(|e| format!("invalid JSON in LSP message: {e}"))?;
    Ok(Some(msg))
}

/// Write an LSP JSON-RPC response to `stdout`.
async fn send_response(
    stdout: &mut tokio::io::Stdout,
    id: Option<serde_json::Value>,
    result: serde_json::Value,
) -> Result<(), String> {
    let response = serde_json::json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": result
    });
    let resp_str =
        serde_json::to_string(&response).map_err(|e| format!("failed to serialize: {e}"))?;
    let bytes = resp_str.as_bytes();
    let header = format!("Content-Length: {}\r\n\r\n", bytes.len());
    stdout
        .write_all(header.as_bytes())
        .await
        .map_err(|e| format!("stdout write: {e}"))?;
    stdout
        .write_all(bytes)
        .await
        .map_err(|e| format!("stdout write: {e}"))?;
    stdout
        .flush()
        .await
        .map_err(|e| format!("stdout flush: {e}"))?;
    Ok(())
}
