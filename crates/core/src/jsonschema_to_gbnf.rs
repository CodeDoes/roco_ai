//! JSON Schema -> GBNF grammar conversion (compact).
//!
//! Ships the *small* shape that's actually useful today: primitives
//! (`string`, `integer`, `number`, `boolean`, `null`) and `enum`.
//! Anything more complex stays out until a concrete eval case
//! demands it; we don't need a faithful JSON Schema implementation.
//!
//! The output is schoolmarm-compatible GBNF: rules are emitted in
//! topological order, rule names are bare identifiers, literals
//! are double-quoted, character ranges use `[a-z]` syntax, and
//! quantifiers (`*`, `+`, `?`) are inline as in llama.cpp's GBNF
//! dialect.
//!
//! Errors are bar-shaped: [`GbnfError::BadSchema`] for shapes we
//! can't comfortably express, [`GbnfError::Other`] as a passthrough
//! so callers don't have to enumerate.

use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GbnfError {
    BadSchema { detail: String },
}

impl std::fmt::Display for GbnfError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GbnfError::BadSchema { detail } => write!(f, "schema not supported: {detail}"),
        }
    }
}

impl std::error::Error for GbnfError {}

/// Library of GBNF rules referenced by the body of generated rules.
/// The school's name `string`, `integer`, etc. is what schoolmarm's
/// built-in grammar walker tokenizes cleanly against; we emit a
/// minimal but complete required-by-the-walker set here.
const PRIMITIVES: &str = "\n\
string ::= \"\\\"\" ([^\\\\\"\\\\] | \"\\\\\" [\"/bfnrt\\\\])* \"\\\"\"\n\
integer ::= \"-\"? (\"0\" | [1-9] [0-9]*)\n\
number ::= integer (\"\" \".\" [0-9]+)? ([eE] [+-]? [0-9]+)?\n\
boolean ::= \"true\" | \"false\"\n\
null ::= \"null\"\n\
";

/// Build a single-rule grammar from a JSON Schema value. The rule
/// is emitted under `root_name`. Primitive schemas return a rule
/// that references the library directly (eg `string`), enum schemas
/// return a rule that alternates between literal strings, and
/// everything else returns `BadSchema`.
///
/// The forward goal isn't to pass arbitrary JSON Schemas — it's
/// to give a small JSON object a small, useful GBNF representation
/// for testing constraint flow on the local RWKV model.
pub fn schema_to_gbnf(root_name: &str, schema: &Value) -> Result<String, GbnfError> {
    let body = body_for(schema)?;
    let mut out = String::new();
    out.push_str(PRIMITIVES);
    out.push('\n');
    out.push_str(&format!("{root_name} ::= {body}\n"));
    Ok(out)
}

fn body_for(schema: &Value) -> Result<String, GbnfError> {
    if !schema.is_object() {
        return Err(GbnfError::BadSchema {
            detail: "schema must be an object".into(),
        });
    }

    if let Some(arr) = schema.get("enum").and_then(|v| v.as_array()) {
        let alts: Vec<String> = arr
            .iter()
            .map(encode_json_value)
            .collect::<Result<_, _>>()?;
        if alts.is_empty() {
            return Err(GbnfError::BadSchema {
                detail: "enum array is empty".into(),
            });
        }
        return Ok(alts.join(" | "));
    }

    let ty = schema
        .get("type")
        .and_then(|v| v.as_str())
        .ok_or_else(|| GbnfError::BadSchema {
            detail: "missing 'type' and not an enum".into(),
        })?;

    Ok(match ty {
        "string" => "string".to_string(),
        "integer" => "integer".to_string(),
        "number" => "number".to_string(),
        "boolean" => "boolean".to_string(),
        "null" => "\"null\"".to_string(),
        "object" | "array" => {
            return Err(GbnfError::BadSchema {
                detail: format!(
                    "object/array schemas not in this compact converter \
                     (type={ty:?}). Use a hand-written GBNF for those."
                ),
            });
        }
        other => {
            return Err(GbnfError::BadSchema {
                detail: format!("unknown primitive type {other:?}"),
            });
        }
    })
}

/// Emit a single JSON value as a GBNF literal branch.
///
/// Strings get double-quoted with backslash escaping; numbers emit
/// the literal digit sequence; booleans and null get quoted; arrays
/// and objects are rejected because this converter doesn't handle
/// them.
fn encode_json_value(v: &Value) -> Result<String, GbnfError> {
    match v {
        Value::String(s) => Ok(format!("\"{}\"", escape_string(s))),
        Value::Number(n) => Ok(n.to_string()),
        Value::Bool(b) => Ok(format!("\"{b}\"")),
        Value::Null => Ok("\"null\"".to_string()),
        _ => Err(GbnfError::BadSchema {
            detail: format!("enum branch not a JSON primitive: {v}"),
        }),
    }
}

fn escape_string(s: &str) -> String {
    // GBNF string literal: backslash and double-quote are the only
    // extra characters that need escaping for the schoolmarm parser.
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"'  => out.push_str("\\\""),
            '/'  => out.push_str("\\/"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            other => out.push(other),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn primitives_reference_the_library() {
        assert_eq!(schema_to_gbnf("root", &json!({"type":"string"})).unwrap(), (PRIMITIVES.to_string() + "\nroot ::= string\n"));
        assert!(schema_to_gbnf("root", &json!({"type":"integer"})).unwrap().contains("root ::= integer"));
        assert!(schema_to_gbnf("root", &json!({"type":"boolean"})).unwrap().contains("root ::= boolean"));
    }

    #[test]
    fn enum_becomes_alternation() {
        let g = schema_to_gbnf("root", &json!({"enum":["ok","wait","stop"]})).unwrap();
        assert!(g.contains("root ::= \"ok\" | \"wait\" | \"stop\""));
    }

    #[test]
    fn null_qualifier_emitted() {
        let g = schema_to_gbnf("root", &json!({"type":"null"})).unwrap();
        assert!(g.contains("root ::= \"null\""));
    }

    #[test]
    fn object_rejection() {
        let res = schema_to_gbnf("root", &json!({
            "type":"object",
            "properties":{"a":{"type":"string"}}
        }));
        assert!(matches!(res, Err(GbnfError::BadSchema { .. })));
    }

    #[test]
    fn primitives_parse_through_schoolmarm() {
        // The whole point of the converter is to land in schoolmarm.
        // Run every primitive through it and assert acceptance.
        #[cfg(feature = "grammar-rwkv")]
        {
            use schoolmarm::Grammar;
            for (label, schema) in [
                ("string", json!({"type":"string"})),
                ("integer", json!({"type":"integer"})),
                ("number", json!({"type":"number"})),
                ("boolean", json!({"type":"boolean"})),
                ("null", json!({"type":"null"})),
                ("enum", json!({"enum":["x","y","z"]})),
            ] {
                let g = schema_to_gbnf("root", &schema).unwrap_or_else(|e| {
                    panic!("{label}: convert error: {e}");
                });
                Grammar::new(&g).unwrap_or_else(|e| {
                    panic!("{label}: schoolmarm rejected: {e:?}");
                });
            }
        }
        // Without the feature, just verify the strings look reasonable.
        #[cfg(not(feature = "grammar-rwkv"))]
        assert!(schema_to_gbnf("root", &json!({"type":"string"})).is_ok());
    }
}

