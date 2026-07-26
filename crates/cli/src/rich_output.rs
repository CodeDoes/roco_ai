//! Rich CLI output — colorful, formatted terminal output.
//!
//! Makes the CLI experience pleasant with:
//! - Color-coded output
//! - Progress bars
//! - Spinners
//! - Tables
//! - Panels
//! - Syntax highlighting

// ═════════════════════════════════════════════════════════════════════════════
// Colors
// ═════════════════════════════════════════════════════════════════════════════

/// ANSI color codes
pub struct Colors;

impl Colors {
    pub const RESET: &'static str = "\x1b[0m";
    pub const BOLD: &'static str = "\x1b[1m";
    pub const DIM: &'static str = "\x1b[2m";

    pub const RED: &'static str = "\x1b[31m";
    pub const GREEN: &'static str = "\x1b[32m";
    pub const YELLOW: &'static str = "\x1b[33m";
    pub const BLUE: &'static str = "\x1b[34m";
    pub const MAGENTA: &'static str = "\x1b[35m";
    pub const CYAN: &'static str = "\x1b[36m";
}

// ═════════════════════════════════════════════════════════════════════════════
// Rich Output
// ═════════════════════════════════════════════════════════════════════════════

/// Print a header
pub fn header(text: &str) {
    // Use char count so multi-byte chars (emoji, box-drawing) don't over-repeat.
    let char_width = text.chars().count();
    println!();
    println!("{}{}{}{}", Colors::BOLD, Colors::CYAN, text, Colors::RESET);
    println!("{}{}{}", Colors::DIM, "─".repeat(char_width), Colors::RESET);
}

/// Print a success message
pub fn success(text: &str) {
    println!("{}✓ {}{}", Colors::GREEN, text, Colors::RESET);
}

/// Print an error message
pub fn error(text: &str) {
    eprintln!("{}✗ {}{}", Colors::RED, text, Colors::RESET);
}

/// Print a warning message
pub fn warning(text: &str) {
    println!("{}⚠ {}{}", Colors::YELLOW, text, Colors::RESET);
}

/// Print an info message
pub fn info(text: &str) {
    println!("{}ℹ {}{}", Colors::BLUE, text, Colors::RESET);
}

/// Print a dimmed message
pub fn dim(text: &str) {
    println!("{}{}{}", Colors::DIM, text, Colors::RESET);
}

/// Strip mock JSON wrapper and noise from a completion response.
///
/// The `MockBackend` wraps its output as `{"result": "..."}`. When running
/// with a real model this is a no-op — the text is returned as-is.
pub fn clean_response(text: &str) -> String {
    let trimmed = text.trim();
    // Unwrap {"result": "..."} emitted by MockBackend
    if trimmed.starts_with('{') {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(trimmed) {
            if let Some(s) = v.get("result").and_then(|r| r.as_str()) {
                // Strip "[mock-3b] " prefix and return only the first non-context line.
                // Context lines look like "User: ...", "Assistant: ..." — skip them.
                let payload = s.trim_start_matches("[mock-3b] ");
                let first_real = payload
                    .lines()
                    .find(|l| {
                        let l = l.trim();
                        !l.is_empty()
                            && !l.starts_with("User:")
                            && !l.starts_with("Assistant:")
                    })
                    .unwrap_or("")
                    .trim_start_matches("[mock-3b] ")
                    .trim();
                return if first_real.is_empty() {
                    // The mock echoed back only context markers — return a neutral stand-in.
                    "[generating...]".to_string()
                } else {
                    first_real.to_string()
                };
            }
        }
    }
    trimmed.to_string()
}

// ═════════════════════════════════════════════════════════════════════════════
// Panels
// ═════════════════════════════════════════════════════════════════════════════

/// Print a panel with a title
pub fn panel(title: &str, content: &str) {
    let width = 60;
    let border = "─".repeat(width);

    println!("{}┌{}┐{}", Colors::DIM, border, Colors::RESET);
    println!(
        "{}│ {}{}{}{}│",
        Colors::DIM,
        Colors::RESET,
        Colors::BOLD,
        title,
        Colors::RESET
    );

    for line in content.lines() {
        let padding = width.saturating_sub(line.len() + 2);
        println!(
            "{}│ {}{}{}{}│",
            Colors::DIM,
            Colors::RESET,
            line,
            " ".repeat(padding),
            Colors::RESET
        );
    }

    println!("{}└{}┘{}", Colors::DIM, border, Colors::RESET);
}

// ═════════════════════════════════════════════════════════════════════════════
// Tables
// ═════════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_colors() {
        // Just make sure they compile
        let _ = Colors::RED;
        let _ = Colors::GREEN;
        let _ = Colors::BLUE;
    }
}
