//! Constrained-decoding grammar generation (GBNF) for RoCo AI tool calls.
//!
//! Inspired by `rwkv-harness/rust/crates/grammar` and its TS counterpart
//! (`src/tools/tool.ts`, `src/grammars/grammar-helpers.ts`): each tool emits a
//! `<tool_call> { "name": …, "arguments": { … } } </tool_call>` production, and
//! the registry assembles a root grammar that constrains a model's output to
//! valid tool-call JSON. A lightweight [`validate_grammar`] (a port of
//! `grammar-helpers`' `parseGrammar`) checks the generated grammar is
//! well-formed and closed (every referenced rule is defined).
//!
//! This is backend-agnostic: it only needs the tool schemas exposed by
//! [`crate::tools::ToolRegistry`].

use serde_json::Value;

use crate::tools::{Tool, ToolRegistry};

/// Sanitize a tool name into a GBNF rule identifier
/// (`^[a-zA-Z_][a-zA-Z0-9_-]*$`).
fn safe_name(name: &str) -> String {
    name.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

/// Map a JSON-Schema property type to a GBNF expression.
fn prop_type_gbnf(schema: &Value) -> String {
    // Enums win: emit an alternation of quoted literals.
    if let Some(e) = schema.get("enum").and_then(|v| v.as_array()) {
        let alts: Vec<String> = e
            .iter()
            .filter_map(|v| v.as_str())
            .map(|s| format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\"")))
            .collect();
        if !alts.is_empty() {
            return format!("( {} )", alts.join(" | "));
        }
    }
    match schema.get("type").and_then(|v| v.as_str()) {
        Some("string") => "string-value".to_string(),
        Some("number") => "number-value".to_string(),
        Some("boolean") => "\"true\" | \"false\"".to_string(),
        Some("array") => {
            "( string-value | number-value ) ( ws \",\" ws ( string-value | number-value ) )*".to_string()
        }
        Some("object") => "\"{\" [^}]* \"}\"".to_string(),
        _ => "string-value".to_string(),
    }
}

/// Which property keys to require in the arguments object.
fn arg_keys(schema: &Value) -> Vec<String> {
    let props = schema.get("properties").and_then(|v| v.as_object());
    let required: Vec<String> = schema
        .get("required")
        .and_then(|v| v.as_array())
        .map(|a| a.iter().filter_map(|v| v.as_str().map(str::to_string)).collect())
        .unwrap_or_default();
    match props {
        Some(p) if !required.is_empty() => p.keys().filter(|k| required.contains(k)).cloned().collect(),
        Some(p) => p.keys().cloned().collect(),
        None => Vec::new(),
    }
}

/// Build the three rules (`<safe>Name`, `<safe>Args`, `<safe>Call`) for a tool.
fn tool_rules(tool: &dyn Tool) -> Vec<String> {
    let safe = safe_name(tool.name());
    let schema = tool.input_schema();

    let name_rule = format!(
        "{}Name ::= \"\\\"name\\\"\" ws \":\" ws \"\\\"{}\\\"\"",
        safe,
        tool.name()
    );

    let keys = arg_keys(&schema);
    let props = schema.get("properties").and_then(|v| v.as_object());
    let mut param_rules = Vec::new();
    for k in &keys {
        let ps = props.and_then(|p| p.get(k));
        let ty = ps.map(prop_type_gbnf).unwrap_or_else(|| "string-value".to_string());
        param_rules.push(format!("\"\\\"{}\\\"\" ws \":\" ws {}", k, ty));
    }
    let inner = if param_rules.is_empty() {
        String::new()
    } else {
        param_rules.join(" ws \",\" ws ")
    };
    let args_rule = format!(
        "{}Args ::= \"\\\"arguments\\\"\" ws \":\" ws \"{{\" ws {} ws \"}}\"",
        safe, inner
    );

    let call_rule = format!(
        "{}Call ::= \"\\t\" \"<tool_call>\" \"\\n\" \"\\t\" \"{{\" ws {}Name ws \",\" ws {}Args ws \"}}\" \"\\n\" \"\\t\" \"</tool_call>\"",
        safe, safe, safe
    );

    vec![name_rule, args_rule, call_rule]
}

/// Shared non-terminals prepended to every grammar.
fn shared_rules() -> Vec<String> {
    vec![
        "ws ::= [ \\t\\n]*".to_string(),
        "string-value ::= \"\\\"\" [^\"]* \"\\\"\"".to_string(),
        "number-value ::= [0-9]+ (\".\" [0-9]+)?".to_string(),
    ]
}

/// Generate a tool-call-only grammar (no surrounding text/think blocks).
pub fn tools_to_gbnf(registry: &ToolRegistry) -> String {
    let mut lines = shared_rules();
    let mut call_names = Vec::new();
    for t in registry.all_tools() {
        for r in tool_rules(&*t) {
            lines.push(r);
        }
        call_names.push(format!("{}Call", safe_name(t.name())));
    }
    lines.push(format!("call ::= {}", call_names.join(" | ")));
    lines.push("root ::= call".to_string());
    lines.join("\n")
}

/// Generate a grammar permitting optional `<think>` blocks and free text
/// around tool calls (matches the rwkv-harness `tools_to_gbnf_with_think`).
pub fn tools_to_gbnf_with_think(registry: &ToolRegistry) -> String {
    let mut lines = shared_rules();
    lines.push("think-block ::= \"\\t\" \"<think>\" \"\\n\" \"\\t\" indented-line \"\\n\" \"\\t\" \"</think>\"".to_string());
    lines.push("indented-line ::= ([^\\n<] | \"\\n\" \"\\t\")*".to_string());
    lines.push("text ::= \"\\t\" indented-line".to_string());
    let mut call_names = Vec::new();
    for t in registry.all_tools() {
        for r in tool_rules(&*t) {
            lines.push(r);
        }
        call_names.push(format!("{}Call", safe_name(t.name())));
    }
    lines.push(format!("call ::= {}", call_names.join(" | ")));
    lines.push(
        "root ::= ws? (think-block)* (call ws? | text call ws? | call text ws?)+ (text ws?)*".to_string(),
    );
    lines.join("\n")
}

/// A response-only grammar (unrestricted free-form text).
pub fn tools_to_gbnf_response() -> String {
    ["root ::= text \"\\n\\n\"", "text ::= [^<]*"].join("\n")
}

/// Generate a grammar that constrains the assistant's output in the chat
/// message format (§2_message).  The assistant may produce a free-text
/// response, optional `<think>` blocks for chain-of-thought reasoning,
/// and/or `<tool_call>` invocations followed by `<tool_result>` blocks.
///
/// This is the "full conversation" grammar — it allows everything the
/// assistant is allowed to emit in a single turn.
pub fn message_format_gbnf(registry: Option<&ToolRegistry>, include_think: bool) -> String {
    let mut lines = shared_rules();

    // Text content: any character except `<` (to avoid confusing tag
    // boundaries), plus whitespace.
    lines.push("text-content ::= [^<]*".to_string());

    // Optional thinking block.
    if include_think {
        lines.push(
            "think-block ::= \"<think>\" text-content \"</think>\"".to_string(),
        );
    }

    // Tool call and result blocks.
    if let Some(registry) = registry {
        let mut call_names = Vec::new();
        for t in registry.all_tools() {
            for r in tool_rules(&*t) {
                lines.push(r);
            }
            call_names.push(format!("{}Call", safe_name(t.name())));
        }
        if !call_names.is_empty() {
            lines.push(format!("tool-call ::= {}", call_names.join(" | ")));
            lines.push(
                "tool-result ::= \"<tool_result>\" text-content \"</tool_result>\""
                    .to_string(),
            );
        }
    }

    // Compose the root: the assistant can produce any sequence of text,
    // think blocks, tool calls, and tool results.
    let mut segments = vec!["text-content".to_string()];
    if include_think {
        segments.push("think-block".to_string());
    }
    if registry.is_some() && !registry.is_some_and(|r| r.all_tools().is_empty()) {
        segments.push("tool-call".to_string());
        segments.push("tool-result".to_string());
    }
    // root: one or more of any segment type, interleaved freely.
    lines.push(format!(
        "root ::= ({} ws?)+",
        segments.join(" | ")
    ));

    lines.join("\n")
}

/// Render tool descriptions as a simple XML block for prompt embedding.
pub fn tools_to_xml(registry: &ToolRegistry) -> String {
    let mut out = Vec::new();
    for t in registry.all_tools() {
        let schema = t.input_schema();
        let props = schema.get("properties").and_then(|v| v.as_object());
        let required: Vec<String> = schema
            .get("required")
            .and_then(|v| v.as_array())
            .map(|a| a.iter().filter_map(|v| v.as_str().map(str::to_string)).collect())
            .unwrap_or_default();
        let mut params = Vec::new();
        if let Some(p) = props {
            for (k, ps) in p {
                let typ = ps.get("type").and_then(|v| v.as_str()).unwrap_or("string");
                let req = if required.contains(k) {
                    " required=\"true\""
                } else {
                    ""
                };
                let desc = ps.get("description").and_then(|v| v.as_str()).unwrap_or("");
                params.push(format!(
                    "  <parameter name=\"{}\" type=\"{}\"{}>{}</parameter>",
                    k, typ, req, desc
                ));
            }
        }
        out.push(format!(
            "<tool name=\"{}\" description=\"{}\">\n{}\n</tool>",
            t.name(),
            t.description(),
            params.join("\n")
        ));
    }
    out.join("\n\n")
}

// ---------------------------------------------------------------------------
// Grammar validation (port of grammar-helpers.ts `parseGrammar` + reachability)
// ---------------------------------------------------------------------------

/// A problem found while validating a grammar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GrammarIssue {
    pub name: String,
    pub message: String,
}

fn is_ident(s: &str) -> bool {
    let bytes = s.as_bytes();
    if bytes.is_empty() {
        return false;
    }
    let first = bytes[0];
    if !(first.is_ascii_alphabetic() || first == b'_') {
        return false;
    }
    bytes
        .iter()
        .all(|&b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

/// Collect identifiers referenced on an RHS, ignoring string literals and
/// character classes (mirrors `rhsIdentifiers` in grammar-helpers.ts).
fn rhs_idents(rhs: &str) -> Vec<String> {
    let mut s = String::new();
    let mut chars = rhs.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '"' => {
                // consume a string literal
                while let Some(&n) = chars.peek() {
                    chars.next();
                    if n == '"' {
                        break;
                    }
                    if n == '\\' {
                        chars.next();
                    }
                }
            }
            '[' => {
                // consume a character class
                s.push(' ');
                while let Some(&n) = chars.peek() {
                    chars.next();
                    if n == ']' {
                        break;
                    }
                    if n == '\\' {
                        chars.next();
                    }
                }
                s.push(' ');
            }
            _ => s.push(c),
        }
    }
    s.split(|c: char| !c.is_ascii_alphanumeric() && c != '_' && c != '-')
        .filter(|m| !m.is_empty() && is_ident(m))
        .map(str::to_string)
        .collect()
}

/// Validate a GBNF string: every rule has a valid identifier, no duplicates,
/// `root` exists, and every referenced rule is defined.
pub fn validate_grammar(gbnf: &str) -> Result<(), Vec<GrammarIssue>> {
    use std::collections::BTreeMap;

    let mut defs: BTreeMap<String, String> = BTreeMap::new();
    for raw in gbnf.lines() {
        let line = raw.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        let eq = match line.find("::=") {
            Some(i) => i,
            None => {
                return Err(vec![GrammarIssue {
                    name: String::new(),
                    message: format!("line missing '::=': {raw}"),
                }])
            }
        };
        let name = line[..eq].trim().to_string();
        let rhs = line[eq + 3..].trim().to_string();
        if !is_ident(&name) {
            return Err(vec![GrammarIssue {
                name,
                message: "invalid rule identifier".to_string(),
            }]);
        }
        if defs.contains_key(&name) {
            return Err(vec![GrammarIssue {
                name,
                message: "duplicate rule definition".to_string(),
            }]);
        }
        defs.insert(name, rhs);
    }

    let mut issues = Vec::new();
    if !defs.contains_key("root") {
        issues.push(GrammarIssue {
            name: "root".to_string(),
            message: "missing root rule".to_string(),
        });
    }
    for (name, rhs) in &defs {
        for id in rhs_idents(rhs) {
            if !defs.contains_key(&id) {
                issues.push(GrammarIssue {
                    name: name.clone(),
                    message: format!("references undefined rule '{id}'"),
                });
            }
        }
    }

    if issues.is_empty() {
        Ok(())
    } else {
        Err(issues)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::{AddTool, EchoTool, Tool, ToolError, ToolRegistry};
    use async_trait::async_trait;
    use std::sync::Arc;

    fn sample_registry() -> ToolRegistry {
        let mut r = ToolRegistry::new();
        r.register(Arc::new(EchoTool));
        r.register(Arc::new(AddTool));
        r
    }

    #[test]
    fn tool_only_grammar_contains_expected_productions() {
        let g = tools_to_gbnf(&sample_registry());
        for needle in [
            "echo",
            "add",
            "<tool_call>",
            "</tool_call>",
            "arguments",
            "message",
            "numbers",
            "root ::= call",
        ] {
            assert!(g.contains(needle), "grammar missing '{needle}'\n{g}");
        }
    }

    #[test]
    fn generated_grammar_is_valid_and_closed() {
        let g = tools_to_gbnf(&sample_registry());
        assert!(validate_grammar(&g).is_ok(), "grammar should validate:\n{g}");

        let g2 = tools_to_gbnf_with_think(&sample_registry());
        assert!(
            validate_grammar(&g2).is_ok(),
            "with_think grammar should validate:\n{g2}"
        );
    }

    #[test]
    fn with_think_grammar_includes_think_block() {
        let g = tools_to_gbnf_with_think(&sample_registry());
        assert!(g.contains("think-block"));
    }

    #[test]
    fn response_only_grammar_is_valid() {
        let g = tools_to_gbnf_response();
        assert!(g.contains("root ::= text"));
        assert!(validate_grammar(&g).is_ok());
    }

    #[test]
    fn xml_renders_tool_names_and_types() {
        let xml = tools_to_xml(&sample_registry());
        assert!(xml.contains("<tool name=\"echo\""));
        assert!(xml.contains("type=\"string\""));
        assert!(xml.contains("<tool name=\"add\""));
    }

    #[test]
    fn enum_params_become_alternations() {
        struct ModeTool;
        #[async_trait]
        impl Tool for ModeTool {
            fn name(&self) -> &str {
                "mode"
            }
            fn description(&self) -> &str {
                "Set the run mode."
            }
            fn input_schema(&self) -> Value {
                serde_json::json!({
                    "type": "object",
                    "properties": { "mode": { "type": "string", "enum": ["fast", "slow"] } },
                    "required": ["mode"],
                })
            }
            async fn run(&self, _input: Value) -> Result<Value, ToolError> {
                Ok(serde_json::json!({}))
            }
        }

        let mut r = ToolRegistry::new();
        r.register(Arc::new(ModeTool));
        let g = tools_to_gbnf(&r);
        assert!(g.contains("\"fast\""), "grammar missing enum value:\n{g}");
        assert!(g.contains("\"slow\""), "grammar missing enum value:\n{g}");
        assert!(validate_grammar(&g).is_ok());
    }

    #[test]
    fn validator_flags_undefined_rule() {
        let bad = "root ::= call\ncall ::= ghostCall";
        let errs = validate_grammar(bad).unwrap_err();
        assert!(errs
            .iter()
            .any(|i| i.message.contains("ghostCall")));
    }

    #[test]
    fn message_format_contains_expected_sections() {
        // Without tools, without think.
        let g = message_format_gbnf(None, false);
        assert!(g.contains("text-content"), "should have text-content: {g}");
        assert!(!g.contains("think-block"), "should NOT have think: {g}");
        assert!(!g.contains("tool-call"), "should NOT have tool-call: {g}");
        assert!(validate_grammar(&g).is_ok(), "grammar should validate: {g}");

        // With tools, with think.
        let g2 = message_format_gbnf(Some(&sample_registry()), true);
        assert!(g2.contains("text-content"));
        assert!(g2.contains("think-block"));
        assert!(g2.contains("tool-call"));
        assert!(g2.contains("tool-result"));
        assert!(g2.contains("echo"));
        assert!(g2.contains("add"));
        assert!(validate_grammar(&g2).is_ok(), "grammar should validate: {g2}");
    }

    #[test]
    fn message_format_root_allows_any_combination() {
        let g = message_format_gbnf(Some(&sample_registry()), true);
        // Root should allow text, think, tool-call, tool-result in any order.
        assert!(g.contains("text-content | think-block | tool-call | tool-result"));
    }
}
