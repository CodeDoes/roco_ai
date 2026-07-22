//! HTML GUI mode: `roco html` — live HTML canvas mode.
//!
//! The agent responds in HTML instead of plain text, served through a local
//! HTTP server with live browser rendering. Type in the terminal, see richly
//! styled HTML output in your browser.
//!
//! Usage:
//!   roco html                    Start interactive HTML session
//!   roco html "build a dashboard"  Start with an initial prompt
//!   roco html --port 9090        Custom port
//!
//! Open http://localhost:<port> in your browser to see the HTML-rendered
//! conversation. The page auto-refreshes after each agent response.

use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::thread;

use crate::daemon;
use crate::parse_opt;
use crate::rich_output as r;

/// A single conversation message
#[derive(Clone, Debug)]
struct HtmlMessage {
    role: String,
    content: String,
}

/// Shared server state — the conversation log and a dirty flag for refresh
struct ServerState {
    messages: Vec<HtmlMessage>,
    dirty: bool,
}

/// Run the HTML live-preview mode
pub fn cmd_html(extra: &[&str]) {
    let initial = extra.first().map(|s| *s).filter(|s| !s.starts_with('-'));
    let port: u16 = parse_opt("--port", extra)
        .and_then(|s| s.parse().ok())
        .unwrap_or(8080);
    let server_url = format!("http://127.0.0.1:{port}");

    let backend = daemon::ensure_sync_backend();

    // ── Shared state between server thread and main thread ──────────────
    let state = Arc::new(Mutex::new(ServerState {
        messages: Vec::new(),
        dirty: true,
    }));

    // ── Start HTTP server on a background thread ───────────────────────
    let server_state = state.clone();
    let server_port = port;
    thread::spawn(move || {
        let listener = match TcpListener::bind(("127.0.0.1", server_port)) {
            Ok(l) => l,
            Err(e) => {
                eprintln!(
                    "{}[HTML] Failed to bind port {server_port}: {e}{}",
                    r::Colors::RED,
                    r::Colors::RESET
                );
                return;
            }
        };
        // Mark the listener as non-blocking so accept() won't block forever
        // when we're trying to shut down.
        let _ = listener.set_nonblocking(true);

        for stream in listener.incoming() {
            match stream {
                Ok(stream) => {
                    let s = server_state.clone();
                    thread::spawn(move || handle_connection(stream, s));
                }
                Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    // No pending connection — spin a bit. This is fine for a
                    // dev-only HTTP server that serves one user.
                    thread::sleep(std::time::Duration::from_millis(50));
                }
                Err(_) => break,
            }
        }
    });

    // Give the server a moment to start
    thread::sleep(std::time::Duration::from_millis(100));

    // ── System prompt: agent must respond in HTML ──────────────────────
    let system_prompt = "\
        You are a creative AI that responds in **HTML only**.\n\n\
        RULES:\n\
        - Your ENTIRE response must be valid, complete HTML fragment.\n\
        - You may include <style> and <script> tags.\n\
        - Use inline CSS for all styling (no external files).\n\
        - Make it visually rich: colors, layouts, emoji, typography.\n\
        - You can create interactive elements with JavaScript.\n\
        - Charts, cards, tables, forms, galleries — all fair game.\n\
        - The HTML will be rendered in a browser in real time.\n\
        - Keep it self-contained — no external dependencies.\n\
        - Wrap multi-line content in appropriate HTML containers.\n\
        - If asked for text, still wrap it in styled HTML.\n\
        - NEVER output markdown or code fences — only raw HTML.\n\
        - If you need to show code, use <pre><code> inside your HTML.\n\n\
        Example responses:\n\
        <div style='padding:20px;font-family:sans-serif'>\n\
          <h1 style='color:#4a90d9'>Hello!</h1>\n\
          <p>This is an <strong>HTML</strong> response.</p>\n\
        </div>";

    // ── Open browser ───────────────────────────────────────────────────
    r::header("RoCo AI — HTML Mode");
    r::info(&format!("Server: {}", server_url));
    r::dim("  Open the URL in your browser to see HTML-rendered responses.");
    r::dim("  Type in the terminal.  :h for help, :q to quit.\n");

    // Open browser
    open_browser(&server_url);

    // Helper: add a message and mark dirty so the page refreshes
    let add_msg = |role: &str, content: &str| {
        let mut s = state.lock().unwrap();
        s.messages.push(HtmlMessage {
            role: role.to_string(),
            content: content.to_string(),
        });
        s.dirty = true;
    };

    // ── Generate initial greeting as HTML ──────────────────────────────
    let greeting_prompt = if let Some(p) = initial {
        p.to_string()
    } else {
        "Create a beautiful welcome page for RoCo AI HTML mode. Include the title, a brief intro, and instructions for the user.".to_string()
    };

    add_msg("user", &markdown_to_html(&greeting_prompt));
    r::header("You");

    let request = roco_engine::CompletionRequest {
        system: system_prompt.into(),
        prompt: greeting_prompt,
        temperature: 0.8,
        max_tokens: 2048,
        prefill: Some("<div style='font-family:sans-serif;padding:20px;'>\n".into()),
        ..Default::default()
    };

    println!(
        "{}Generating HTML response...{}",
        r::Colors::DIM,
        r::Colors::RESET
    );

    match futures::executor::block_on(backend.complete(request)) {
        Ok(resp) => {
            let html = sanitize_html_response(&resp.text);
            add_msg("assistant", &html);
            r::success("Rendered in browser.");
        }
        Err(e) => {
            let error_html = format!(
                "<div style='color:red;padding:20px;font-family:sans-serif'>\
                 <h2>Error</h2><p>{}</p></div>",
                e.to_string()
            );
            add_msg("assistant", &error_html);
            r::error(&format!("Generation failed: {e}"));
        }
    }

    // ── Main interactive loop ──────────────────────────────────────────
    let stdin = std::io::stdin();
    let mut input_buf = String::new();

    loop {
        print!("\n{}🖌 >{} ", r::Colors::DIM, r::Colors::RESET);
        std::io::stdout().flush().ok();

        input_buf.clear();
        stdin.read_line(&mut input_buf).ok();
        let input = input_buf.trim().to_string();

        if input.is_empty() {
            continue;
        }

        // Handle commands
        if input.starts_with('/') || input.starts_with(':') {
            let cmd = input
                .trim_start_matches('/')
                .trim_start_matches(':')
                .trim()
                .to_lowercase();
            match cmd.as_str() {
                "help" | "h" | "?" => {
                    r::panel(
                        "Commands",
                        &[
                            "  :help / :h    Show this help",
                            "  :open         Open/reopen browser",
                            "  :clear        Clear conversation",
                            "  :save <file>  Save latest HTML to file",
                            "  :url          Show server URL",
                            "  :quit / :q    Exit HTML mode",
                        ]
                        .join("\n"),
                    );
                    continue;
                }
                "open" | "o" => {
                    open_browser(&server_url);
                    r::info(&format!("Opened {server_url}"));
                    continue;
                }
                "clear" => {
                    let mut s = state.lock().unwrap();
                    s.messages.clear();
                    s.dirty = true;
                    r::success("Conversation cleared.");
                    continue;
                }
                "url" => {
                    r::info(&format!("Server: {server_url}"));
                    continue;
                }
                "save" | "s" => {
                    let s = state.lock().unwrap();
                    if let Some(last) = s.messages.last() {
                        if last.role == "assistant" {
                            let filename = format!(
                                "roco-html-{}.html",
                                chrono::Utc::now().format("%Y%m%d_%H%M%S")
                            );
                            let page = wrap_as_full_page(&s.messages);
                            if std::fs::write(&filename, &page).is_ok() {
                                r::success(&format!("Saved to {filename}"));
                            }
                        }
                    }
                    continue;
                }
                "quit" | "q" | "exit" => {
                    r::info("Goodbye!");
                    break;
                }
                _ => {
                    r::warning(&format!("Unknown: /{cmd}. Type :help."));
                    continue;
                }
            }
        }

        // ── User input → agent responds in HTML ────────────────────────
        add_msg("user", &markdown_to_html(&input));
        r::header("You");

        let request = roco_engine::CompletionRequest {
            system: system_prompt.into(),
            prompt: input,
            temperature: 0.8,
            max_tokens: 2048,
            prefill: Some("<div style='font-family:sans-serif;padding:20px;'>\n".into()),
            ..Default::default()
        };

        match futures::executor::block_on(backend.complete(request)) {
            Ok(resp) => {
                let html = sanitize_html_response(&resp.text);
                add_msg("assistant", &html);
                r::success("Rendered in browser.");
            }
            Err(e) => {
                let error_html = format!(
                    "<div style='color:red;padding:20px;font-family:sans-serif'>\
                     <h2>Error</h2><p>{}</p></div>",
                    e.to_string()
                );
                add_msg("assistant", &error_html);
                r::error(&format!("Generation failed: {e}"));
            }
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// HTTP Server
// ═══════════════════════════════════════════════════════════════════════════

fn handle_connection(stream: TcpStream, state: Arc<Mutex<ServerState>>) {
    let _peer = stream.peer_addr().ok();
    let mut reader = BufReader::new(&stream);
    let mut request_line = String::new();
    if reader.read_line(&mut request_line).is_err() {
        return;
    }
    // Drop reader so we can write to stream
    drop(reader);
    let mut stream = stream;

    let path = request_line.split_whitespace().nth(1).unwrap_or("/");

    let (status, content_type, body) = match path {
        "/" | "/index.html" => {
            let s = state.lock().unwrap();
            let body = wrap_as_full_page(&s.messages);
            ("200 OK", "text/html; charset=utf-8", body)
        }
        "/style.css" => ("200 OK", "text/css; charset=utf-8", CSS.to_string()),
        "/check" => {
            // SSE-lite: check if dirty, tell client to refresh
            let s = state.lock().unwrap();
            if s.dirty {
                ("200 OK", "text/plain", "refresh".to_string())
            } else {
                ("200 OK", "text/plain", "ok".to_string())
            }
        }
        _ => ("404 Not Found", "text/plain", "404".to_string()),
    };

    // Build response
    let response = format!(
        "HTTP/1.1 {status}\r\n\
         Content-Type: {content_type}\r\n\
         Content-Length: {}\r\n\
         Access-Control-Allow-Origin: *\r\n\
         Connection: close\r\n\
         \r\n\
         {body}",
        body.len()
    );

    let _ = stream.write_all(response.as_bytes());
    let _ = stream.flush();

    // Mark as not-dirty after serving the page
    if path == "/" || path == "/index.html" {
        if let Ok(mut s) = state.lock() {
            s.dirty = false;
        }
    }
}

/// Wrap all messages as a full HTML page with auto-refresh
fn wrap_as_full_page(messages: &[HtmlMessage]) -> String {
    let mut body_html = String::new();

    for msg in messages {
        let role_class = match msg.role.as_str() {
            "user" => "user-message",
            "assistant" => "assistant-message",
            _ => "system-message",
        };
        let role_label = match msg.role.as_str() {
            "user" => "You",
            "assistant" => "AI",
            _ => "System",
        };

        body_html.push_str(&format!(
            r#"<div class="message {role_class}">
                <div class="role-badge">{role_label}</div>
                <div class="content">{}</div>
               </div>"#,
            msg.content
        ));
    }

    format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="UTF-8">
<meta name="viewport" content="width=device-width, initial-scale=1.0">
<title>RoCo AI — HTML Mode</title>
<style>{CSS}</style>
<script>
// Poll for updates every 2 seconds
setInterval(function() {{
    fetch('/check').then(r => r.text()).then(t => {{
        if (t === 'refresh') location.reload();
    }}).catch(() => {{}});
}}, 2000);
</script>
</head>
<body>
<div class="container">
    <header class="header">
        <h1>🖌 RoCo AI — HTML Mode</h1>
        <p class="subtitle">Agent responses are rendered as HTML in real time</p>
    </header>
    <main class="messages">
        {body_html}
    </main>
    <footer class="footer">
        <p>Type in the terminal to continue the conversation</p>
    </footer>
</div>
</body>
</html>"#
    )
}

// ═══════════════════════════════════════════════════════════════════════════
// Helpers
// ═══════════════════════════════════════════════════════════════════════════

/// Extract clean HTML from a model response (strip markdown fences etc.)
fn sanitize_html_response(text: &str) -> String {
    let t = text.trim();

    // If it starts with a markdown code fence, extract content
    if t.starts_with("```html") {
        if let Some(start) = t.find('\n') {
            let after = &t[start + 1..];
            if let Some(end) = after.find("```") {
                return after[..end].trim().to_string();
            }
            return after.trim().to_string();
        }
    }
    if t.starts_with("```") {
        if let Some(start) = t.find('\n') {
            let after = &t[start + 1..];
            if let Some(end) = after.find("```") {
                return after[..end].trim().to_string();
            }
            return after.trim().to_string();
        }
    }

    // If it starts with <html or <div or any tag, it's already HTML
    if t.starts_with('<') {
        return t.to_string();
    }

    // Last resort: wrap plain text in a styled container
    format!("<div style='font-family:sans-serif;padding:12px;line-height:1.6;white-space:pre-wrap'>{}</div>", html_escape(t))
}

/// Convert a markdown-ish prompt to simple HTML for display
fn markdown_to_html(text: &str) -> String {
    let escaped = html_escape(text);
    format!(
        "<div style='font-family:sans-serif;padding:8px'>{}</div>",
        escaped
    )
}

/// Basic HTML escaping
fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// Open a URL in the default browser
fn open_browser(url: &str) {
    #[cfg(target_os = "linux")]
    let _ = std::process::Command::new("xdg-open").arg(url).spawn();
    #[cfg(target_os = "macos")]
    let _ = std::process::Command::new("open").arg(url).spawn();
    #[cfg(target_os = "windows")]
    let _ = std::process::Command::new("cmd")
        .args(["/c", "start", url])
        .spawn();
}

// ═══════════════════════════════════════════════════════════════════════════
// CSS (embedded)
// ═══════════════════════════════════════════════════════════════════════════

const CSS: &str = r#"
* { margin: 0; padding: 0; box-sizing: border-box; }

body {
    font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;
    background: #0d1117;
    color: #e6edf3;
    min-height: 100vh;
}

.container {
    max-width: 900px;
    margin: 0 auto;
    padding: 20px;
}

.header {
    text-align: center;
    padding: 30px 0 20px;
    border-bottom: 1px solid #30363d;
    margin-bottom: 24px;
}

.header h1 {
    font-size: 28px;
    background: linear-gradient(135deg, #58a6ff, #bc8cff);
    -webkit-background-clip: text;
    -webkit-text-fill-color: transparent;
    background-clip: text;
}

.subtitle {
    font-size: 14px;
    color: #8b949e;
    margin-top: 4px;
}

.messages {
    display: flex;
    flex-direction: column;
    gap: 16px;
}

.message {
    border-radius: 8px;
    overflow: hidden;
    border: 1px solid #30363d;
}

.user-message {
    background: #161b22;
    border-color: #30363d;
}

.assistant-message {
    background: #0d1117;
    border-color: #30363d;
}

.role-badge {
    display: inline-block;
    font-size: 11px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.5px;
    padding: 4px 10px;
    margin: 8px 8px 0;
    border-radius: 4px;
}

.user-message .role-badge {
    background: #1f6feb33;
    color: #58a6ff;
}

.assistant-message .role-badge {
    background: #3fb95033;
    color: #3fb950;
}

.content {
    padding: 12px 16px 16px;
    line-height: 1.6;
    overflow-x: auto;
}

/* Agent HTML content inherits these */
.content h1, .content h2, .content h3 { margin: 16px 0 8px; }
.content h1 { font-size: 24px; }
.content h2 { font-size: 20px; }
.content h3 { font-size: 16px; }
.content p { margin: 8px 0; }
.content pre {
    background: #161b22;
    border: 1px solid #30363d;
    border-radius: 6px;
    padding: 12px;
    overflow-x: auto;
    font-size: 13px;
}
.content code {
    background: #161b22;
    padding: 2px 6px;
    border-radius: 4px;
    font-size: 13px;
}
.content pre code { background: none; padding: 0; }
.content a { color: #58a6ff; }
.content img { max-width: 100%; border-radius: 6px; }
.content blockquote {
    border-left: 3px solid #30363d;
    padding-left: 12px;
    color: #8b949e;
    margin: 12px 0;
}
.content table {
    border-collapse: collapse;
    width: 100%;
    margin: 12px 0;
}
.content th, .content td {
    border: 1px solid #30363d;
    padding: 8px 12px;
    text-align: left;
}
.content th { background: #161b22; font-weight: 600; }
.content ul, .content ol { padding-left: 24px; margin: 8px 0; }

.footer {
    text-align: center;
    padding: 30px 0;
    color: #8b949e;
    font-size: 13px;
    border-top: 1px solid #30363d;
    margin-top: 24px;
}
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitize_html_already_html() {
        let html = "<div>hello</div>";
        assert_eq!(sanitize_html_response(html), "<div>hello</div>");
    }

    #[test]
    fn test_sanitize_html_fenced() {
        let input = "```html\n<div>hello</div>\n```";
        assert_eq!(sanitize_html_response(input), "<div>hello</div>");
    }

    #[test]
    fn test_sanitize_html_plain_text() {
        let result = sanitize_html_response("hello world");
        assert!(result.contains("hello world"));
        assert!(result.contains("<div"));
    }

    #[test]
    fn test_html_escape() {
        assert_eq!(html_escape("<script>"), "&lt;script&gt;");
        assert_eq!(html_escape("a & b"), "a &amp; b");
    }

    #[test]
    fn test_wrap_as_full_page() {
        let msgs = vec![
            HtmlMessage {
                role: "user".into(),
                content: "<p>hi</p>".into(),
            },
            HtmlMessage {
                role: "assistant".into(),
                content: "<div>hello</div>".into(),
            },
        ];
        let page = wrap_as_full_page(&msgs);
        assert!(page.contains("<!DOCTYPE html>"));
        assert!(page.contains("<p>hi</p>"));
        assert!(page.contains("<div>hello</div>"));
        assert!(page.contains("user-message"));
        assert!(page.contains("assistant-message"));
    }

    #[test]
    fn test_markdown_to_html_escapes() {
        let result = markdown_to_html("<script>alert('xss')</script>");
        assert!(!result.contains("<script>"));
        assert!(result.contains("&lt;script&gt;"));
    }
}
