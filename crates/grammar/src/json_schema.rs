//! JSON Schema -> GBNF grammar conversion (compact).
//!
//! Ships primitives (`string`, `integer`, `number`, `boolean`, `null`),
//! `enum`, and `object`/`array` (recursively). Every listed property is
//! required and emitted in order. The output is schoolmarm-compatible GBNF.

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

const PRIMITIVES: &str = "\n\
string ::= \"\\\"\" ([^\\\\\"\\\\] | \"\\\\\" [\"/bfnrt\\\\])* \"\\\"\"\n\
integer ::= \"-\"? (\"0\" | [1-9] [0-9]*)\n\
number ::= integer (\"\" \".\" [0-9]+)? ([eE] [+-]? [0-9]+)?\n\
boolean ::= \"true\" | \"false\"\n\
null ::= \"null\"\n\
";

pub fn schema_to_gbnf(root_name: &str, schema: &Value) -> Result<String, GbnfError> {
    let mut rules: Vec<String> = Vec::new();
    let body = gen_rule(root_name, schema, &mut rules)?;
    let mut out = String::new();
    out.push_str(PRIMITIVES);
    out.push('\n');
    for r in &rules {
        out.push_str(r);
        out.push('\n');
    }
    out.push_str(&format!("root ::= {body}\n"));
    Ok(out)
}

fn gen_rule(name: &str, schema: &Value, rules: &mut Vec<String>) -> Result<String, GbnfError> {
    if !schema.is_object() {
        return Err(GbnfError::BadSchema { detail: "schema must be an object".into() });
    }

    if let Some(arr) = schema.get("enum").and_then(|v| v.as_array()) {
        let alts: Vec<String> = arr.iter().map(encode_json_value).collect::<Result<_, _>>()?;
        if alts.is_empty() {
            return Err(GbnfError::BadSchema { detail: "enum array is empty".into() });
        }
        return Ok(alts.join(" | "));
    }

    let ty = schema.get("type")
        .and_then(|v| v.as_str())
        .ok_or_else(|| GbnfError::BadSchema { detail: "missing 'type' and not an enum".into() })?;

    Ok(match ty {
        "string" => "string".to_string(),
        "integer" => "integer".to_string(),
        "number" => "number".to_string(),
        "boolean" => "boolean".to_string(),
        "null" => "null".to_string(),
        "object" => {
            let props = schema.get("properties")
                .and_then(|v| v.as_object())
                .ok_or_else(|| GbnfError::BadSchema { detail: "object schema needs 'properties'".into() })?;
            if props.is_empty() {
                rules.push(format!("{name}-obj ::= \"{{\" \"}}\""));
                return Ok(format!("{name}-obj"));
            }
            let mut members: Vec<String> = Vec::with_capacity(props.len());
            for (key, sub) in props {
                let sub_name = format!("{}-{}", name, sanitize(key));
                let sub_body = gen_rule(&sub_name, sub, rules)?;
                members.push(format!("{} \":\" {}", quote(key), sub_body));
            }
            let body = members.join(" \",\" ");
            rules.push(format!("{name}-obj ::= \"{{\" {body} \"}}\""));
            format!("{name}-obj")
        }
        "array" => {
            let items = schema.get("items")
                .ok_or_else(|| GbnfError::BadSchema { detail: "array schema needs 'items'".into() })?;
            let item_name = format!("{}-item", name);
            let item_body = gen_rule(&item_name, items, rules)?;
            rules.push(format!("{name}-arr ::= \"[\" ( {item_body} ( \",\" {item_body} )* )? \"]\""));
            format!("{name}-arr")
        }
        other => return Err(GbnfError::BadSchema { detail: format!("unknown type {other:?}") }),
    })
}

fn quote(s: &str) -> String {
    format!("\"{s}\"")
}

fn sanitize(s: &str) -> String {
    s.chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '_' }).collect()
}

fn encode_json_value(v: &Value) -> Result<String, GbnfError> {
    match v {
        Value::String(s) => Ok(format!("\"{}\"", escape_string(s))),
        Value::Number(n) => Ok(n.to_string()),
        Value::Bool(b) => Ok(format!("\"{b}\"")),
        Value::Null => Ok("\"null\"".to_string()),
        _ => Err(GbnfError::BadSchema { detail: format!("enum branch not a JSON primitive: {v}") }),
    }
}

fn escape_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '/' => out.push_str("\\/"),
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
        assert_eq!(
            schema_to_gbnf("root", &json!({"type":"string"})).unwrap(),
            (PRIMITIVES.to_string() + "\nroot ::= string\n")
        );
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
        assert!(g.contains("root ::= null"));
    }

    #[test]
    fn object_support() {
        let g = schema_to_gbnf("root", &json!({
            "type": "object",
            "properties": {
                "name": {"type": "string"},
                "age": {"type": "integer"}
            }
        })).unwrap();
        assert!(g.contains("root ::= root-obj"));
        assert!(g.contains("root-obj ::= \"{\""));
        assert!(g.contains("\"name\""));
        assert!(g.contains("\"age\""));
        assert!(g.contains("integer"));
    }

    #[test]
    fn array_support() {
        let g = schema_to_gbnf("root", &json!({
            "type": "array",
            "items": {"type": "integer"}
        })).unwrap();
        assert!(g.contains("root ::= root-arr"));
        assert!(g.contains("root-arr ::= \"[\""));
    }

    #[test]
    fn nested_object_and_array() {
        let g = schema_to_gbnf("root", &json!({
            "type": "object",
            "properties": {
                "id": {"type": "integer"},
                "tags": {"type": "array", "items": {"type": "string"}}
            }
        })).unwrap();
        assert!(g.contains("root-tags-arr ::= \"[\" ( string"));
        assert!(g.contains("root-obj ::= \"{\""));
    }

    #[test]
    fn primitives_parse_through_schoolmarm() {
        use schoolmarm::Grammar;
        for (label, schema) in [
            ("string", json!({"type":"string"})),
            ("integer", json!({"type":"integer"})),
            ("number", json!({"type":"number"})),
            ("boolean", json!({"type":"boolean"})),
            ("null", json!({"type":"null"})),
            ("enum", json!({"enum":["x","y","z"]})),
            ("object", json!({"type":"object","properties":{"a":{"type":"string"},"b":{"type":"integer"}}})),
            ("array", json!({"type":"array","items":{"type":"integer"}})),
        ] {
            let g = schema_to_gbnf("root", &schema).unwrap_or_else(|e| panic!("{label}: convert error: {e}"));
            Grammar::new(&g).unwrap_or_else(|e| panic!("{label}: schoolmarm rejected: {e:?}"));
        }
    }
}
