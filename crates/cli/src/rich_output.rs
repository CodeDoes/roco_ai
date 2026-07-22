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
    println!();
    println!("{}{}{}{}", Colors::BOLD, Colors::CYAN, text, Colors::RESET);
    println!("{}{}{}", Colors::DIM, "─".repeat(text.len()), Colors::RESET);
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
