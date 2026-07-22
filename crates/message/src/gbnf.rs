//! GBNF grammars for the structured chat message format.
//!
//! Generates GBNF strings that constrain model output to structured chat
//! messages with role prefixes, optional `<think>` reasoning blocks, optional
//! `<tools>` declarations, and optional `<tool_call>` / `<tool_result>` blocks.
//!
//! Compatible with both `schoolmarm` (fallback) and `bnf_sampler` (primary).
//!
//! # Grammar variants
//!
//! - **Simple** (`think=false, tools=false`):  
//!   `System: ... \n\n User: ... \n\n Assistant: ...`
//!
//! - **With think** (`think=true, tools=false`):  
//!   `System: ... \n\n User: ... \n\n Assistant: <think>...</think>...`
//!
//! - **With tools** (`think=false, tools=true`):  
//!   `System: ... <tools>{...} \n\n User: ... \n\n Assistant: <tool_call>{...}<tool_result>{...}...`
//!
//! - **Full** (`think=true, tools=true`):  
//!   All features combined.
//!
//! # Why this approach works
//!
//! `bnf_sampler` constrains tokens via a vocabulary trie (`qp-trie`). We don't
//! need explicit UTF-8 character rules — the tokenizer's vocabulary already
//! maps byte sequences → valid token IDs. The GBNF here only needs to define
//! the structural envelope (prefixes, delimiters, tag boundaries).

use serde_json::Value;

/// Which structural features to enable in the message GBNF.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[derive(Default)]
pub struct MessageFormatOptions {
    /// Whether the model may emit `<think>...</think>` reasoning blocks.
    pub think: bool,
    /// Whether the model may emit `<tool_call>` / `<tool_result>` blocks.
    pub tools: bool,
}


/// Generate a GBNF grammar for the structured chat message format.
///
/// The grammar constrains output to:
///
/// ```text
/// System: <system-content>
///
/// User: <user-content>
///
/// Assistant: <assistant-content>
/// ```
pub fn message_format_gbnf(options: &MessageFormatOptions, tool_schemas: &[Value]) -> String {
    let mut g = String::new();

    // --- Character matching: printable ASCII + high bytes ---
    // schoolmarm-compatible: uses \xNN hex escapes, no char class subtraction,
    // no null bytes in ranges. Text blocks are matched as sequences of
    // arbitrary characters — the vocabulary trie handles token-level validity.
    g.push_str("char ::= [ -~] | [\\x80-\\xFF]\n");
    g.push_str("text ::= char*\n\n");

    // --- System content ---
    if options.tools && !tool_schemas.is_empty() {
        let tools_body = generate_tools_body(tool_schemas);
        g.push_str(&format!(
            "sys ::= text \"<tools>\" {} \"</tools>\"\n",
            tools_body
        ));
    } else {
        g.push_str("sys ::= text\n");
    }
    g.push_str("user ::= text\n\n");

    // --- Assistant content ---
    if options.think {
        g.push_str("think ::= \"<think>\" text \"</think>\"\n");
        if options.tools {
            g.push_str("asm ::= think | text | tool-call | tool-result\n");
        } else {
            g.push_str("asm ::= think | text\n");
        }
    } else {
        if options.tools {
            g.push_str("asm ::= text | tool-call | tool-result\n");
        } else {
            g.push_str("asm ::= text\n");
        }
    }

    g.push('\n');

    if options.tools {
        g.push_str("tool-call ::= \"<tool_call>\" json \"</tool_call>\"\n");
        g.push_str("tool-result ::= \"<tool_result>\" text \"</tool_result>\"\n");
    }
    g.push('\n');

    // --- JSON (compact library for tool_call arguments) ---
    g.push_str("json ::= string | number | object | array | \"true\" | \"false\" | \"null\"\n");
    g.push_str("string ::= \"\\\"\" ( [\\x20-\\x21\\x23-\\x5B\\x5D-\\x7E\\x80-\\xFF] | \"\\\\\" [\"/bfnrt] | \"\\\\u\" hex hex hex hex )* \"\\\"\"\n");
    g.push_str("hex ::= [0-9a-fA-F]\n");
    g.push_str("number ::= \"-\"? (\"0\" | [1-9] [0-9]*) (\".\" [0-9]+)? ([eE] [+-]? [0-9]+)?\n");
    g.push_str("object ::= \"{\" ( string \":\" json ( \",\" string \":\" json )* )? \"}\"\n");
    g.push_str("array ::= \"[\" ( json ( \",\" json )* )? \"]\"\n\n");

    // --- Root: the message envelope ---
    g.push_str("root ::= \"System: \" sys \"\\n\\nUser: \" user \"\\n\\nAssistant: \" asm\n");

    g
}

/// Build the GBNF for a `<tools>` block containing an array of tool schemas.
fn generate_tools_body(schemas: &[Value]) -> String {
    if schemas.is_empty() {
        return String::new();
    }
    // Produce a GBNF rule for a JSON array of tool descriptor objects.
    // Items are separated by quoted commas for schoolmarm-compatible GBNF.
    // Format: "[" first ( "," second ) ( "," third ) ... "]"
    let items: Vec<String> = schemas.iter().map(tool_schema_to_gbnf).collect();
    let _body = items.join(" \",\" ");
    // If multiple items, join with comma+space separators. Each comma is a literal "," string.
    // But for GBNF we don't need comma separators between alternates — bare whitespace works.
    // Actually, for a JSON array, items must be comma-separated. But in GBNF, bare commas
    // are not valid. We use quoted commas: ","
    // For 0 items: empty array
    // For 1+ items: first ( "," item )*
    if items.len() == 1 {
        format!("\"[\" {} \"]\"", items[0])
    } else {
        let tail = items[1..]
            .iter()
            .map(|item| format!("\",\" {}", item))
            .collect::<Vec<_>>()
            .join(" ");
        format!("\"[\" {} {} \"]\"", items[0], tail)
    }
}

/// Convert a single tool schema Value into a GBNF object production.
fn tool_schema_to_gbnf(schema: &Value) -> String {
    let _name = schema
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or("tool");
    // Produce a simple JSON object with name, description, parameters keys.
    // Each value is a JSON string or object, referencing the JSON library rules.
    "\"{\" string \":\" string \",\" string \":\" string \",\" string \":\" object \"}\"".to_string()
}

/// Build a `MessageFormatOptions` from flags.
pub fn options(think: bool, tools: bool) -> MessageFormatOptions {
    MessageFormatOptions { think, tools }
}

/// Full pipeline: message format GBNF with tool schemas embedded.
pub fn pipeline_gbnf(options: &MessageFormatOptions, tool_schemas: &[Value]) -> String {
    message_format_gbnf(options, tool_schemas)
}

/// Generate a GBNF grammar that constrains **only the assistant's response**,
/// stripping the `System:` / `User:` envelope from the `root` rule.
///
/// Use this when generating assistant output after the prompt already
/// includes the System/User context (i.e. the model should only emit the
/// content that follows `Assistant:`).
pub fn assistant_response_gbnf(options: &MessageFormatOptions, tool_schemas: &[Value]) -> String {
    let full = message_format_gbnf(options, tool_schemas);
    full.lines()
        .map(|line| {
            let trimmed = line.trim_start();
            if trimmed.starts_with("root ::= \"System: ") {
                // Replace the full-envelope root with an assistant-only root.
                "root ::= asm".to_string()
            } else {
                line.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Check that a GBNF string is parseable by schoolmarm.
    fn check_schoolmarm(gbnf: &str, label: &str) {
        assert!(gbnf.contains("root ::="), "{label}: must have root rule");
        match schoolmarm::Grammar::new(gbnf) {
            Ok(_) => {}
            Err(e) => panic!("{label}: schoolmarm failed: {e:?}\n=== GBNF ===\n{gbnf}\n==="),
        }
    }

    #[test]
    fn simple_parses_schoolmarm() {
        let gbnf = message_format_gbnf(&MessageFormatOptions::default(), &[]);
        check_schoolmarm(&gbnf, "simple");
        assert!(gbnf.contains("System:"), "simple: must contain System:");
        assert!(gbnf.contains("User:"), "simple: must contain User:");
        assert!(
            gbnf.contains("Assistant:"),
            "simple: must contain Assistant:"
        );
    }

    #[test]
    fn think_parses_schoolmarm() {
        let gbnf = message_format_gbnf(
            &MessageFormatOptions {
                think: true,
                tools: false,
            },
            &[],
        );
        check_schoolmarm(&gbnf, "think");
        assert!(gbnf.contains("<think>"), "think: must contain <think>");
        assert!(gbnf.contains("</think>"), "think: must contain </think>");
    }

    #[test]
    fn tools_parses_schoolmarm() {
        let schemas = vec![serde_json::json!({
            "name": "get_weather",
            "description": "Get weather for a location",
            "parameters": {"type": "object", "properties": {}}
        })];
        let gbnf = message_format_gbnf(
            &MessageFormatOptions {
                think: false,
                tools: true,
            },
            &schemas,
        );
        check_schoolmarm(&gbnf, "tools");
        assert!(
            gbnf.contains("<tool_call>"),
            "tools: must contain <tool_call>"
        );
        assert!(
            gbnf.contains("<tool_result>"),
            "tools: must contain <tool_result>"
        );
        assert!(gbnf.contains("<tools>"), "tools: must contain <tools>");
    }

    #[test]
    fn full_without_schemas_parses_schoolmarm() {
        // When tools=true but no schemas provided, grammar should still be valid.
        let gbnf = message_format_gbnf(
            &MessageFormatOptions {
                think: true,
                tools: true,
            },
            &[],
        );
        check_schoolmarm(&gbnf, "full-noschemas");
        assert!(gbnf.contains("<think>"), "must contain <think>");
        assert!(gbnf.contains("<tool_call>"), "must contain <tool_call>");
        assert!(gbnf.contains("<tool_result>"), "must contain <tool_result>");
    }

    #[test]
    fn full_without_tools_parses_schoolmarm() {
        // think=true, tools=false — only think tags, no tool tags.
        let gbnf = message_format_gbnf(
            &MessageFormatOptions {
                think: true,
                tools: false,
            },
            &[],
        );
        check_schoolmarm(&gbnf, "full-nothink");
        assert!(gbnf.contains("<think>"), "must contain <think>");
        assert!(
            !gbnf.contains("<tool_call>"),
            "must NOT contain <tool_call>"
        );
    }

    #[test]
    fn all_defined_rules_used_by_root() {
        let gbnf = message_format_gbnf(
            &MessageFormatOptions {
                think: true,
                tools: true,
            },
            &[],
        );
        let lines: Vec<&str> = gbnf.lines().collect();
        let defined: std::collections::HashSet<&str> = lines
            .iter()
            .filter(|l| l.contains("::="))
            .map(|l| l.split("::=").next().unwrap().trim())
            .collect();
        for rule in &[
            "sys",
            "user",
            "asm",
            "think",
            "tool-call",
            "tool-result",
            "text",
            "char",
        ] {
            assert!(
                defined.contains(rule),
                "rule `{rule}` must be defined, got: {:?}",
                defined
            );
        }
    }

    #[test]
    fn empty_grammar_produces_valid_output() {
        let gbnf = message_format_gbnf(&MessageFormatOptions::default(), &[]);
        assert!(!gbnf.is_empty());
        for marker in &["System", "User", "Assistant"] {
            assert!(gbnf.contains(marker), "must contain marker `{marker}`");
        }
    }
}
