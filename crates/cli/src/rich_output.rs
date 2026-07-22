//! Rich CLI output — colorful, formatted terminal output.
//!
//! Makes the CLI experience pleasant with:
//! - Color-coded output
//! - Progress bars
//! - Spinners
//! - Tables
//! - Panels
//! - Syntax highlighting

use std::io::{self, Write};

// ═════════════════════════════════════════════════════════════════════════════
// Colors
// ═════════════════════════════════════════════════════════════════════════════

/// ANSI color codes
pub struct Colors;

impl Colors {
    pub const RESET: &'static str = "\x1b[0m";
    pub const BOLD: &'static str = "\x1b[1m";
    pub const DIM: &'static str = "\x1b[2m";
    pub const ITALIC: &'static str = "\x1b[3m";
    pub const UNDERLINE: &'static str = "\x1b[4m";

    pub const RED: &'static str = "\x1b[31m";
    pub const GREEN: &'static str = "\x1b[32m";
    pub const YELLOW: &'static str = "\x1b[33m";
    pub const BLUE: &'static str = "\x1b[34m";
    pub const MAGENTA: &'static str = "\x1b[35m";
    pub const CYAN: &'static str = "\x1b[36m";
    pub const WHITE: &'static str = "\x1b[37m";

    pub const BG_RED: &'static str = "\x1b[41m";
    pub const BG_GREEN: &'static str = "\x1b[42m";
    pub const BG_YELLOW: &'static str = "\x1b[43m";
    pub const BG_BLUE: &'static str = "\x1b[44m";
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

/// Print a bold message
pub fn bold(text: &str) {
    println!("{}{}{}", Colors::BOLD, text, Colors::RESET);
}

/// Print a colored message
pub fn colored(text: &str, color: &str) {
    println!("{}{}{}", color, text, Colors::RESET);
}

// ═════════════════════════════════════════════════════════════════════════════
// Progress
// ═════════════════════════════════════════════════════════════════════════════

/// Print a progress bar
pub fn progress(current: usize, total: usize, label: &str) {
    let width = 40;
    let filled = (current as f64 / total as f64 * width as f64) as usize;
    let empty = width - filled;

    print!("\r{}[", Colors::DIM);
    print!("{}{}{}", Colors::GREEN, "█".repeat(filled), Colors::RESET);
    print!("{}{}{}", Colors::DIM, "░".repeat(empty), Colors::RESET);
    print!("{}] ", Colors::RESET);
    print!("{}{}{}", Colors::BOLD, label, Colors::RESET);
    print!(" ({}/{})", current, total);
    io::stdout().flush().ok();
}

/// Finish a progress bar
pub fn progress_done(label: &str) {
    println!("\r{}✓ {}{}", Colors::GREEN, label, Colors::RESET);
}

// ═════════════════════════════════════════════════════════════════════════════
// Spinner
// ═════════════════════════════════════════════════════════════════════════════

/// Print a spinner (call repeatedly)
pub fn spinner(frame: usize, text: &str) {
    let frames = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
    let frame = frames[frame % frames.len()];
    print!("\r{}{} {}{}", Colors::CYAN, frame, text, Colors::RESET);
    io::stdout().flush().ok();
}

/// Finish a spinner
pub fn spinner_done(text: &str) {
    println!("\r{}✓ {}{}", Colors::GREEN, text, Colors::RESET);
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

/// Print a table
pub fn table(headers: &[&str], rows: &[Vec<&str>]) {
    // Calculate column widths
    let mut widths: Vec<usize> = headers.iter().map(|h| h.len()).collect();
    for row in rows {
        for (i, cell) in row.iter().enumerate() {
            if i < widths.len() {
                widths[i] = widths[i].max(cell.len());
            }
        }
    }

    // Print header
    print!("{}┌", Colors::DIM);
    for (i, width) in widths.iter().enumerate() {
        print!(
            "{}{}",
            "─".repeat(width + 2),
            if i < widths.len() - 1 { "┬" } else { "┐" }
        );
    }
    println!("{}", Colors::RESET);

    print!("{}│", Colors::DIM);
    for (i, header) in headers.iter().enumerate() {
        let padding = widths[i].saturating_sub(header.len());
        print!(
            " {}{}{}{} │",
            Colors::BOLD,
            header,
            Colors::RESET,
            " ".repeat(padding)
        );
    }
    println!("{}", Colors::RESET);

    // Print separator
    print!("{}├", Colors::DIM);
    for (i, width) in widths.iter().enumerate() {
        print!(
            "{}{}",
            "─".repeat(width + 2),
            if i < widths.len() - 1 { "┼" } else { "┤" }
        );
    }
    println!("{}", Colors::RESET);

    // Print rows
    for row in rows {
        print!("{}│", Colors::DIM);
        for (i, cell) in row.iter().enumerate() {
            let padding = if i < widths.len() {
                widths[i].saturating_sub(cell.len())
            } else {
                0
            };
            print!(" {}{} │", cell, " ".repeat(padding));
        }
        println!("{}", Colors::RESET);
    }

    // Print footer
    print!("{}└", Colors::DIM);
    for (i, width) in widths.iter().enumerate() {
        print!(
            "{}{}",
            "─".repeat(width + 2),
            if i < widths.len() - 1 { "┴" } else { "┘" }
        );
    }
    println!("{}", Colors::RESET);
}

// ═════════════════════════════════════════════════════════════════════════════
// Syntax Highlighting
// ═════════════════════════════════════════════════════════════════════════════

/// Highlight markdown syntax
pub fn highlight_markdown(text: &str) {
    for line in text.lines() {
        if line.starts_with('#') {
            println!("{}{}{}", Colors::BOLD, Colors::CYAN, line);
        } else if line.starts_with('-') || line.starts_with('*') {
            println!("{}{}{}", Colors::GREEN, line, Colors::RESET);
        } else if line.starts_with('>') {
            println!("{}{}{}", Colors::ITALIC, Colors::DIM, line);
        } else if line.contains("**") {
            // Bold text
            let highlighted = line.replace("**", &format!("{}{}", Colors::BOLD, Colors::RESET));
            println!("{}", highlighted);
        } else {
            println!("{}", line);
        }
    }
}

// ═════════════════════════════════════════════════════════════════════════════
// Prompts
// ═════════════════════════════════════════════════════════════════════════════

/// Print a prompt and read input
pub fn prompt(message: &str) -> io::Result<String> {
    print!("{}{}{} ", Colors::BOLD, message, Colors::RESET);
    io::stdout().flush()?;
    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    Ok(input.trim().to_string())
}

/// Print a choice prompt
pub fn choice(message: &str, options: &[(&str, &str)]) -> io::Result<String> {
    println!("{}{}{}", Colors::BOLD, message, Colors::RESET);
    for (key, desc) in options {
        println!("  {}[{}]{} {}", Colors::CYAN, key, Colors::RESET, desc);
    }
    print!("{}Choice:{} ", Colors::BOLD, Colors::RESET);
    io::stdout().flush()?;
    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    Ok(input.trim().to_lowercase())
}

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
