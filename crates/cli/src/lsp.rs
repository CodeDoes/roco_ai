//! Minimal LSP initialize handshake for Zed's language server protocol.
//!
//! When Zed spawns roco via `language_server_command`, it sends an LSP
//! `initialize` message over stdin.  This module reads that message,
//! responds with an empty capabilities block, and returns — after that,
//! roco just serves HTTP on its port and ignores stdin.

use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};

/// Read one LSP message from stdin and respond to `initialize` if present.
///
/// Returns `Ok(())` after successfully (or silently) handling the message.
/// Returns `Err` only on I/O or framing errors (not on missing initialize).
pub async fn handle_lsp_initialize() -> Result<(), String> {
    let stdin = tokio::io::stdin();
    let mut reader = BufReader::new(stdin);
    let mut line = String::new();
    let mut content_length: Option<usize> = None;

    // Read LSP headers — each line is "Key: Value\r\n" ending with "\r\n"
    loop {
        line.clear();
        let bytes_read = reader
            .read_line(&mut line)
            .await
            .map_err(|e| format!("stdin read error: {e}"))?;
        if bytes_read == 0 {
            return Err("stdin closed before headers".to_string());
        }
        let trimmed = line.trim();
        if trimmed.is_empty() {
            break; // empty line marks end of headers
        }
        if let Some(len_str) = trimmed.strip_prefix("Content-Length: ") {
            content_length = Some(
                len_str
                    .parse::<usize>()
                    .map_err(|e| format!("invalid Content-Length: {e}"))?,
            );
        }
        // Content-Type is optional in LSP — ignore it
    }

    let len = content_length.ok_or("missing Content-Length header")?;

    // Read exactly `len` bytes of JSON body
    let mut buf = vec![0u8; len];
    reader
        .read_exact(&mut buf)
        .await
        .map_err(|e| format!("stdin body read error: {e}"))?;

    let body = String::from_utf8(buf).map_err(|e| format!("invalid UTF-8 in LSP message: {e}"))?;

    let msg: serde_json::Value =
        serde_json::from_str(&body).map_err(|e| format!("invalid JSON in LSP message: {e}"))?;

    // Only respond to `initialize` — ignore `initialized` and everything else
    if msg["method"].as_str() == Some("initialize") {
        let id = &msg["id"];
        let response = serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": {
                "capabilities": {}
            }
        });
        let resp_str = serde_json::to_string(&response)
            .map_err(|e| format!("failed to serialize response: {e}"))?;
        let resp_bytes = resp_str.as_bytes();

        // Write LSP-framed response to stdout
        let mut stdout = tokio::io::stdout();
        let header = format!("Content-Length: {}\r\n\r\n", resp_bytes.len());
        stdout
            .write_all(header.as_bytes())
            .await
            .map_err(|e| format!("stdout write error: {e}"))?;
        stdout
            .write_all(resp_bytes)
            .await
            .map_err(|e| format!("stdout write error: {e}"))?;
        stdout
            .flush()
            .await
            .map_err(|e| format!("stdout flush error: {e}"))?;
    }

    // Ignore all subsequent stdin — roco speaks HTTP, not LSP
    Ok(())
}
