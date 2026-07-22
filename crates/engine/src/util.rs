//! Utility functions for text processing used by evals and other engine modules.

/// Clean story text by stripping thinking blocks and fixing paragraph separation.
pub fn clean_story_text(text: &str) -> String {
    let text = strip_thinking(text);
    let text = fix_paragraphs(&text);
    text.trim().to_string()
}

/// Strip ` thinking...  ` and similar reasoning blocks from model output.
fn strip_thinking(text: &str) -> String {
    let mut result = String::new();
    let mut in_think = false;
    let mut i = 0;
    let chars: Vec<char> = text.chars().collect();

    while i < chars.len() {
        if i + 10 <= chars.len() {
            let window: String = chars[i..i + 10].iter().collect();
            if window.starts_with(" thinking") || window.starts_with(" \u{1f50d}") {
                in_think = true;
                i += 10;
                continue;
            }
        }
        if in_think {
            // Look for closing tag
            if i + 3 <= chars.len() {
                let close: String = chars[i..i + 3].iter().collect();
                if close == " response" || close == " \u{2728}" || close == " \u{2705}" {
                    in_think = false;
                    i += 3;
                    continue;
                }
            }
            if chars[i] == '\n' {
                // If we see a newline while in think mode and the next line
                // doesn't look like continuing thinking, end the block
                in_think = false;
                result.push('\n');
            }
            // Skip thinking character
            i += 1;
            continue;
        }
        // Check for closing tag when not in think mode (stray)
        if i + 9 <= chars.len() {
            let stray: String = chars[i..i + 9].iter().collect();
            if stray.starts_with(" response") {
                // Skip the closing tag (it's a stray)
                i += 9;
                continue;
            }
        }
        result.push(chars[i]);
        i += 1;
    }

    result
}

/// Fix paragraphs: ensure paragraphs are separated by double newlines.
fn fix_paragraphs(text: &str) -> String {
    let mut result = String::new();
    let lines: Vec<&str> = text.lines().collect();
    let mut i = 0;

    while i < lines.len() {
        let line = lines[i];

        if line.trim().is_empty() {
            result.push_str("\n\n");
            i += 1;
            continue;
        }

        // Check if this line is the end of a paragraph
        let is_para_break = if i + 1 < lines.len() {
            let next = lines[i + 1].trim();
            if next.is_empty() {
                false
            } else if line.ends_with('.')
                || line.ends_with('!')
                || line.ends_with('?')
                || line.ends_with('"')
                || line.ends_with('"')
                || line.ends_with('—')
            {
                next.starts_with(|c: char| c.is_uppercase() || c == '"' || c == '*' || c == '#')
                    && line.len() > 30
            } else {
                false
            }
        } else {
            false
        };

        if is_para_break || line.trim().starts_with('#') || line.trim().starts_with("---") {
            result.push_str(line.trim_end());
            result.push_str("\n\n");
        } else if !line.trim().is_empty() {
            if !result.is_empty() && !result.ends_with('\n') {
                result.push(' ');
            }
            result.push_str(line.trim_end());
            result.push('\n');
        }

        i += 1;
    }

    // Clean up multiple blank lines
    let mut cleaned = String::new();
    let mut prev_blank = false;
    for line in result.lines() {
        if line.trim().is_empty() {
            if !prev_blank {
                cleaned.push_str("\n\n");
                prev_blank = true;
            }
        } else {
            cleaned.push_str(line);
            cleaned.push('\n');
            prev_blank = false;
        }
    }

    let cleaned = cleaned.trim().to_string();
    cleaned.replace("\n\n\n", "\n\n")
}
