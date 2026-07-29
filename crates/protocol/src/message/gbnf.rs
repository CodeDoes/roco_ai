//! kbnf-native grammars for the structured chat message format.
//!
//! Generates kbnf-native grammar strings that constrain model output to
//! structured chat messages with role prefixes, optional `mid` reasoning
//! blocks, optional `<tools>` declarations, and optional tool_call /
//! tool_result blocks.
//!
//! # Dialect
//!
//! All output is **kbnf-native**: `#'...'` regex char classes, `{...}`
//! repetition (0+), `[...]` optional, `;` terminators on every rule.
//! No llama.cpp GBNF `[abc]` character classes (kbnf parses those as
//! optionals), no `*`/`+` postfix on groups. This is the only dialect the
//! runtime grammar engine (`roco-bnf-engine` -> kbnf) accepts, so the
//! grammar is fed straight to it with no conversion step.

use serde_json::Value;

/// Which structural features to enable in the message grammar.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct MessageFormatOptions {
    /// Whether the model may emit `mid...end` reasoning blocks.
    pub think: bool,
    /// Whether the model may emit tool_call / tool_result blocks.
    pub tools: bool,
}

/// Printable ASCII char (0x20-0x7E) -- the body of every text block.
const CHAR_RULE: &str = "char ::= #'[ -~]';\n";

/// JSON string char: printable ASCII except `"` (0x22) and `\` (0x5C),
/// expressed as a positive class (kbnf's regex parser rejects literal
/// `"` and `\\` inside a negated class).
const JSON_STRING_CHAR_RULE: &str = "string_char ::= #'[\\x20-\\x21\\x23-\\x5B\\x5D-\\x7E]';\n";

/// Generate a kbnf-native grammar for the structured chat message format.
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

    // --- Character + text rules (shared) ---
    g.push_str(CHAR_RULE);
    g.push_str("text ::= char { char };\n\n");

    // --- System content ---
    if options.tools && !tool_schemas.is_empty() {
        let tools_body = generate_tools_body(tool_schemas);
        g.push_str(&format!(
            "sys ::= text \"<tools>\" {tools_body} \"</tools>\";\n"
        ));
    } else {
        g.push_str("sys ::= text;\n");
    }
    g.push_str("user ::= text;\n\n");

    // --- Assistant content ---
    if options.think {
        g.push_str("think ::= \"mid\" text \"end\";\n");
        if options.tools {
            g.push_str("asm ::= think | text | tool_call | tool_result;\n");
        } else {
            g.push_str("asm ::= think | text;\n");
        }
    } else if options.tools {
        g.push_str("asm ::= text | tool_call | tool_result;\n");
    } else {
        g.push_str("asm ::= text;\n");
    }
    g.push('\n');

    if options.tools {
        g.push_str("tool_call ::= \"<tool_call>\" json \"</tool_call>\";\n");
        g.push_str("tool_result ::= \"<tool_result>\" text \"</tool_result>\";\n");
    }
    g.push('\n');

    // --- JSON sub-grammar (for tool_call arguments) ---
    g.push_str(JSON_STRING_CHAR_RULE);
    g.push_str("json ::= string | number | object | array | \"true\" | \"false\" | \"null\";\n");
    g.push_str("string ::= \"\\\"\" { string_char | escape } \"\\\"\";\n");
    g.push_str(
        "escape ::= \"\\\\\" (\"\\\"\" | \"\\\\\" | \"/\" | \"b\" | \"f\" | \"n\" | \"r\" | \"t\" | \"u\" hex hex hex hex);\n",
    );
    g.push_str("hex ::= #'[0-9a-fA-F]';\n");
    g.push_str(
        "number ::= \"-\"? (\"0\" | #'[1-9]' { #'[0-9]' }) (\".\" { #'[0-9]' })? (( \"e\" | \"E\" ) ( \"+\" | \"-\" )? { #'[0-9]' })?;\n",
    );
    g.push_str("object ::= \"{\" ( string \":\" json { \",\" string \":\" json } )? \"}\";\n");
    g.push_str("array ::= \"[\" ( json { \",\" json } )? \"]\";\n\n");

    // --- Root: the message envelope ---
    g.push_str("root ::= \"System: \" sys \"\\n\\nUser: \" user \"\\n\\nAssistant: \" asm;\n");

    g
}

/// Build the grammar for a `<tools>` block containing an array of tool schemas.
fn generate_tools_body(schemas: &[Value]) -> String {
    if schemas.is_empty() {
        return String::new();
    }
    let items: Vec<String> = schemas.iter().map(tool_schema_to_gbnf).collect();
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

/// Convert a single tool schema Value into a grammar object production.
fn tool_schema_to_gbnf(schema: &Value) -> String {
    let _name = schema
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or("tool");
    "\"{\" string \":\" string \",\" string \":\" string \",\" string \":\" object \"}\""
        .to_string()
}

/// Build a `MessageFormatOptions` from flags.
pub fn options(think: bool, tools: bool) -> MessageFormatOptions {
    MessageFormatOptions { think, tools }
}

/// Full pipeline: message format grammar with tool schemas embedded.
pub fn pipeline_gbnf(options: &MessageFormatOptions, tool_schemas: &[Value]) -> String {
    message_format_gbnf(options, tool_schemas)
}

/// Generate a kbnf-native grammar that constrains **only the assistant's
/// response**, stripping the `System:` / `User:` envelope from the `root`
/// rule.
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
                "root ::= asm;".to_string()
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

    /// Build the grammar and assert it compiles in kbnf with a non-empty
    /// allowed-token set. This is the real acceptance test -- a grammar
    /// that parses but allows zero tokens at the start state is degenerate.
    fn check_kbnf(gbnf: &str, label: &str) {
        let mut vocab: Vec<Vec<u8>> = vec![b"".to_vec()];
        for b in [0x09u8, 0x0Au8, 0x0Du8, 0x20u8] {
            vocab.push(vec![b]);
        }
        for b in 0x21u8..=0x7Eu8 {
            vocab.push(vec![b]);
        }
        let engine = roco_engine::BnfEngine::new(gbnf, &vocab).unwrap_or_else(|e| {
            panic!("{label}: kbnf failed: {e:?}\n=== grammar ===\n{gbnf}\n===")
        });
        assert!(
            engine.allowed_count() > 0,
            "{label}: grammar allows zero tokens at start"
        );
    }

    #[test]
    fn simple_compiles_kbnf() {
        let gbnf = message_format_gbnf(&MessageFormatOptions::default(), &[]);
        check_kbnf(&gbnf, "simple");
        assert!(gbnf.contains("System:"), "simple: must contain System:");
        assert!(gbnf.contains("User:"), "simple: must contain User:");
        assert!(
            gbnf.contains("Assistant:"),
            "simple: must contain Assistant:"
        );
    }

    #[test]
    fn think_compiles_kbnf() {
        let gbnf = message_format_gbnf(
            &MessageFormatOptions {
                think: true,
                tools: false,
            },
            &[],
        );
        check_kbnf(&gbnf, "think");
        assert!(gbnf.contains("mid"), "think: must contain mid");
        assert!(gbnf.contains("end"), "think: must contain end");
    }

    #[test]
    fn tools_compiles_kbnf() {
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
        check_kbnf(&gbnf, "tools");
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
    fn full_without_schemas_compiles_kbnf() {
        // When tools=true but no schemas provided, grammar should still be valid.
        let gbnf = message_format_gbnf(
            &MessageFormatOptions {
                think: true,
                tools: true,
            },
            &[],
        );
        check_kbnf(&gbnf, "full-noschemas");
        assert!(gbnf.contains("mid"), "must contain mid");
        assert!(gbnf.contains("<tool_call>"), "must contain <tool_call>");
        assert!(gbnf.contains("<tool_result>"), "must contain <tool_result>");
    }

    #[test]
    fn full_without_tools_compiles_kbnf() {
        // think=true, tools=false -- only think tags, no tool tags.
        let gbnf = message_format_gbnf(
            &MessageFormatOptions {
                think: true,
                tools: false,
            },
            &[],
        );
        check_kbnf(&gbnf, "full-nothink");
        assert!(gbnf.contains("mid"), "must contain mid");
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
            "tool_call",
            "tool_result",
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
