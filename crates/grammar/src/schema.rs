//! Programmatic JSON Schema builder.
//!
//! Provides a type-safe API to build JSON schemas programmatically,
//! which are then converted to GBNF grammars via `schema_to_gbnf()`.

use serde_json::{json, Value};

/// A JSON Schema builder that can be composed and reused.
#[derive(Debug, Clone)]
pub struct Schema {
    value: Value,
}

impl Schema {
    /// Create a string schema.
    pub fn string() -> Self {
        Self {
            value: json!({"type": "string"}),
        }
    }

    /// Create an integer schema.
    pub fn integer() -> Self {
        Self {
            value: json!({"type": "integer"}),
        }
    }

    /// Create a number schema.
    pub fn number() -> Self {
        Self {
            value: json!({"type": "number"}),
        }
    }

    /// Create a boolean schema.
    pub fn boolean() -> Self {
        Self {
            value: json!({"type": "boolean"}),
        }
    }

    /// Create a null schema.
    pub fn null() -> Self {
        Self {
            value: json!({"type": "null"}),
        }
    }

    /// Create an enum schema from a list of allowed values.
    pub fn enum_values(values: Vec<Value>) -> Self {
        Self {
            value: json!({"enum": values}),
        }
    }

    /// Create an array schema with the given item type.
    pub fn array(items: Schema) -> Self {
        Self {
            value: json!({
                "type": "array",
                "items": items.value
            }),
        }
    }

    /// Create an object schema builder.
    pub fn object() -> ObjectBuilder {
        ObjectBuilder {
            properties: serde_json::Map::new(),
        }
    }

    /// Get the underlying JSON Schema value.
    pub fn to_json(&self) -> &Value {
        &self.value
    }

    /// Convert this schema to a GBNF grammar.
    pub fn to_gbnf(&self, root_name: &str) -> Result<String, super::json_schema::GbnfError> {
        super::json_schema::schema_to_gbnf(root_name, &self.value)
    }
}

/// Builder for object schemas.
#[derive(Debug, Clone)]
pub struct ObjectBuilder {
    properties: serde_json::Map<String, Value>,
}

impl ObjectBuilder {
    /// Add a property to the object.
    pub fn prop<S: Into<String>>(mut self, name: S, schema: Schema) -> Self {
        self.properties.insert(name.into(), schema.value);
        self
    }

    /// Build the object schema.
    pub fn build(self) -> Schema {
        Schema {
            value: json!({
                "type": "object",
                "properties": self.properties
            }),
        }
    }
}

/// Reusable schema definitions for common types.
/// Centralized here to avoid repetition across the codebase.
pub mod common {
    use super::Schema;
    use serde_json::json;

    /// A simple user object with name and age.
    pub fn user() -> Schema {
        Schema::object()
            .prop("name", Schema::string())
            .prop("age", Schema::integer())
            .build()
    }

    /// A tag/label enum.
    pub fn tag() -> Schema {
        Schema::enum_values(vec![
            json!("bug"),
            json!("feature"),
            json!("enhancement"),
            json!("documentation"),
        ])
    }

    /// A list of strings.
    pub fn string_list() -> Schema {
        Schema::array(Schema::string())
    }

    /// A list of integers.
    pub fn integer_list() -> Schema {
        Schema::array(Schema::integer())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn build_primitive_schemas() {
        assert_eq!(Schema::string().to_json(), &json!({"type": "string"}));
        assert_eq!(Schema::integer().to_json(), &json!({"type": "integer"}));
        assert_eq!(Schema::number().to_json(), &json!({"type": "number"}));
        assert_eq!(Schema::boolean().to_json(), &json!({"type": "boolean"}));
        assert_eq!(Schema::null().to_json(), &json!({"type": "null"}));
    }

    #[test]
    fn build_enum_schema() {
        let schema = Schema::enum_values(vec![json!("a"), json!("b")]);
        assert_eq!(schema.to_json(), &json!({"enum": ["a", "b"]}));
    }

    #[test]
    fn build_array_schema() {
        let schema = Schema::array(Schema::integer());
        assert_eq!(
            schema.to_json(),
            &json!({"type": "array", "items": {"type": "integer"}})
        );
    }

    #[test]
    fn build_object_schema() {
        let schema = Schema::object()
            .prop("name", Schema::string())
            .prop("age", Schema::integer())
            .build();

        let expected = json!({
            "type": "object",
            "properties": {
                "name": {"type": "string"},
                "age": {"type": "integer"}
            }
        });
        assert_eq!(schema.to_json(), &expected);
    }

    #[test]
    fn build_nested_schema() {
        let schema = Schema::object()
            .prop("tags", Schema::array(Schema::string()))
            .prop("count", Schema::integer())
            .build();

        let expected = json!({
            "type": "object",
            "properties": {
                "tags": {"type": "array", "items": {"type": "string"}},
                "count": {"type": "integer"}
            }
        });
        assert_eq!(schema.to_json(), &expected);
    }

    #[test]
    fn schema_to_gbnf_conversion() {
        let schema = Schema::boolean();
        let gbnf = schema.to_gbnf("root").unwrap();
        assert!(gbnf.contains("root ::= boolean"));
    }

    #[test]
    fn reusable_schemas() {
        // Define once
        let user_schema = common::user();

        // Use multiple times
        let user_list = Schema::array(user_schema.clone());
        let user_object = Schema::object()
            .prop("creator", user_schema.clone())
            .prop("assignee", user_schema)
            .build();

        // Both should convert to valid GBNF
        assert!(user_list.to_gbnf("root").is_ok());
        assert!(user_object.to_gbnf("root").is_ok());
    }

    #[test]
    fn common_schemas_are_valid() {
        // All common schemas should produce valid GBNF
        assert!(common::user().to_gbnf("root").is_ok());
        assert!(common::tag().to_gbnf("root").is_ok());
        assert!(common::string_list().to_gbnf("root").is_ok());
        assert!(common::integer_list().to_gbnf("root").is_ok());
    }

    #[test]
    fn schemas_are_valid_json_schema() {
        use jsonschema::JSONSchema;

        // Test primitive schemas with valid data
        let string_schema = Schema::string();
        let compiled = JSONSchema::compile(string_schema.to_json()).unwrap();
        assert!(compiled.is_valid(&json!("hello")));
        assert!(!compiled.is_valid(&json!(42)));

        let integer_schema = Schema::integer();
        let compiled = JSONSchema::compile(integer_schema.to_json()).unwrap();
        assert!(compiled.is_valid(&json!(42)));
        assert!(!compiled.is_valid(&json!("hello")));

        let number_schema = Schema::number();
        let compiled = JSONSchema::compile(number_schema.to_json()).unwrap();
        assert!(compiled.is_valid(&json!(3.5)));
        assert!(compiled.is_valid(&json!(42)));

        let boolean_schema = Schema::boolean();
        let compiled = JSONSchema::compile(boolean_schema.to_json()).unwrap();
        assert!(compiled.is_valid(&json!(true)));
        assert!(compiled.is_valid(&json!(false)));

        let null_schema = Schema::null();
        let compiled = JSONSchema::compile(null_schema.to_json()).unwrap();
        assert!(compiled.is_valid(&json!(null)));
        assert!(!compiled.is_valid(&json!("hello")));

        // Test enum schema
        let enum_schema = Schema::enum_values(vec![json!("a"), json!("b")]);
        let compiled = JSONSchema::compile(enum_schema.to_json()).unwrap();
        assert!(compiled.is_valid(&json!("a")));
        assert!(compiled.is_valid(&json!("b")));
        assert!(!compiled.is_valid(&json!("c")));

        // Test array schema
        let array_schema = Schema::array(Schema::integer());
        let compiled = JSONSchema::compile(array_schema.to_json()).unwrap();
        assert!(compiled.is_valid(&json!([1, 2, 3])));
        assert!(!compiled.is_valid(&json!(["a", "b"])));

        // Test object schema
        let object_schema = Schema::object()
            .prop("name", Schema::string())
            .prop("age", Schema::integer())
            .build();
        let compiled = JSONSchema::compile(object_schema.to_json()).unwrap();
        assert!(compiled.is_valid(&json!({"name": "Alice", "age": 30})));
        assert!(!compiled.is_valid(&json!({"name": "Bob", "age": "thirty"})));
    }

    #[test]
    fn nested_schemas_are_valid_json_schema() {
        use jsonschema::JSONSchema;

        let schema = Schema::object()
            .prop("tags", Schema::array(Schema::string()))
            .prop("count", Schema::integer())
            .prop("active", Schema::boolean())
            .build();

        let json = schema.to_json();
        let compiled = JSONSchema::compile(json).expect("Schema should be valid");

        // Test that valid data validates
        assert!(compiled.is_valid(&json!({
            "tags": ["a", "b"],
            "count": 42,
            "active": true
        })));

        // Test that invalid data fails validation
        assert!(!compiled.is_valid(&json!({
            "tags": "not an array",
            "count": 42,
            "active": true
        })));
    }
}
