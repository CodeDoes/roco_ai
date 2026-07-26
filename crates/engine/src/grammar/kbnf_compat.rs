//! kbnf-compatible GBNF conversion utilities.
//!
//! kbnf's GBNF dialect differs from llama.cpp GBNF in these ways:
//!
//! | Feature | llama.cpp GBNF | kbnf GBNF |
//! |---|---|---|
//! | Rule terminator | newline or `\n` | `;` (semicolon) |
//! | Character class `[...]` | `[abc]` | `#[abc]` (regex string) |
//! | Excluded char `[^...]` | `[^abc]` | `#"[^abc]"` (regex string) |
//! | Optional | `(...)?` or `...?` | `[...]` or `...?` |
//! | Repeat | `(...)*` or `...*` | `{...}` or `...*` |
//! | Concatenation | implicit whitespace | implicit or explicit `,` |
//!
//! Since our `primitives_bnf()` already expands character classes into explicit
//! alternatives, the primary conversion needed is adding `;` terminators.

/// Convert a llama.cpp GBNF string to kbnf-compatible format.
///
/// This performs:
/// 1. Adds `;` at the end of each rule line (lines containing `::=`)
/// 2. Lines that already end with `;` are left unchanged
/// 3. Empty lines and comments are preserved
///
/// Note: this does NOT convert character classes (`[...]` → `#[...]`)
/// because our `primitives_bnf()` already expands them into explicit
/// alternatives. If you have grammars with character classes, convert
/// them separately.
pub fn gbnf_to_kbnf(gbnf: &str) -> String {
    let mut out = String::with_capacity(gbnf.len() + 16);
    for line in gbnf.lines() {
        let trimmed = line.trim();
        // If the line defines a rule and doesn't already end with ;
        if trimmed.contains("::=") && !trimmed.ends_with(';') {
            out.push_str(line);
            out.push_str(";\n");
        } else {
            out.push_str(line);
            out.push('\n');
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_adds_semicolons() {
        let input = concat!(
            "string ::= \"\\\"\" (char | escape)* \"\\\"\"\n",
            "char ::= \"a\" | \"b\"\n",
            "root ::= string\n",
        );
        let out = gbnf_to_kbnf(input);
        for line in out.lines() {
            let t = line.trim();
            if t.contains("::=") && !t.is_empty() {
                assert!(t.ends_with(';'), "rule should end with ;: {t:?}");
            }
        }
    }

    #[test]
    fn test_preserves_existing_semicolons() {
        let input = "root ::= \"yes\" | \"no\";\n";
        let out = gbnf_to_kbnf(input);
        assert_eq!(out, "root ::= \"yes\" | \"no\";\n");
    }

    #[test]
    fn test_preserves_comments() {
        let input = "# this is a comment\nroot ::= \"yes\"\n";
        let out = gbnf_to_kbnf(input);
        assert!(out.contains("# this is a comment"));
    }

    #[test]
    fn test_empty_input() {
        assert_eq!(gbnf_to_kbnf(""), "");
    }
}
