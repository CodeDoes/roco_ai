use roco_engine::{create_bnf_mask, schema_to_gbnf, CompletionRequest, MockBackend, ModelBackend};
use serde_json::json;

#[tokio::test]
async fn test_valid_json_output_passes_grammar_mask() {
    let schema = json!({
        "type": "object",
        "properties": {
            "is_valid": {"type": "boolean"}
        }
    });

    let gbnf = schema_to_gbnf("root", &schema).unwrap();
    let backend = MockBackend::default();

    let vocab = backend.vocab_bytes().unwrap();
    let mask = create_bnf_mask(&gbnf, &vocab).unwrap();

    let req = CompletionRequest::builder()
        .prompt("generate")
        .bnf_mask(mask)
        .max_tokens(256)
        .build();

    let resp = backend.complete(req).await.unwrap();

    let parsed: serde_json::Value =
        serde_json::from_str(&resp.text).expect("Output must be valid JSON");
    assert!(parsed.get("is_valid").is_some());
    assert!(parsed.get("is_valid").unwrap().is_boolean());
}

#[tokio::test]
async fn test_invalid_json_rejected_by_grammar_mask() {
    let schema = json!({
        "type": "object",
        "properties": {
            "name": {"type": "string"}
        }
    });
    let gbnf = schema_to_gbnf("root", &schema).unwrap();
    let backend = MockBackend::default();
    let vocab = backend.vocab_bytes().unwrap();

    let mut mask = create_bnf_mask(&gbnf, &vocab).unwrap();
    let a_id = vocab.iter().position(|v| v.as_slice() == b"A").unwrap() as u32;
    let mut logits = vec![0.0f32; vocab.len()];
    mask.mask(&mut logits);

    assert!(
        logits[a_id as usize] < -1000.0,
        "Logit for invalid token 'A' should be masked out"
    );
}

#[tokio::test]
async fn test_valid_enum_output_passes_grammar_mask() {
    let schema = json!({"enum": ["pass", "fail", "needs-work"]});

    let gbnf = schema_to_gbnf("root", &schema).unwrap();
    let backend = MockBackend::default();

    let vocab = backend.vocab_bytes().unwrap();
    let mask = create_bnf_mask(&gbnf, &vocab).unwrap();

    let req = CompletionRequest::builder()
        .prompt("generate")
        .bnf_mask(mask)
        .max_tokens(256)
        .build();

    let resp = backend.complete(req).await.unwrap();

    let parsed: serde_json::Value =
        serde_json::from_str(&resp.text).expect("Output must be valid JSON");
    assert!(
        parsed.as_str().unwrap() == "pass"
            || parsed.as_str().unwrap() == "fail"
            || parsed.as_str().unwrap() == "needs-work"
    );
}

#[tokio::test]
async fn test_nested_objects_with_required_fields() {
    let schema = json!({
        "type": "object",
        "properties": {
            "user": {
                "type": "object",
                "properties": {
                    "active": {"type": "boolean"},
                    "id": {"type": "integer"}
                }
            }
        }
    });

    let gbnf = schema_to_gbnf("root", &schema).unwrap();
    let backend = MockBackend::default();

    let vocab = backend.vocab_bytes().unwrap();
    let mask = create_bnf_mask(&gbnf, &vocab).unwrap();

    let req = CompletionRequest::builder()
        .prompt("generate")
        .bnf_mask(mask)
        .max_tokens(256)
        .build();

    let resp = backend.complete(req).await.unwrap();

    let parsed: serde_json::Value =
        serde_json::from_str(&resp.text).expect("Output must be valid JSON");
    assert!(parsed.get("user").is_some());
    let user = parsed.get("user").unwrap();
    assert!(user.get("active").is_some());
    assert!(user.get("id").is_some());
    assert!(user.get("active").unwrap().is_boolean());
    assert!(user.get("id").unwrap().is_number());
}

#[tokio::test]
async fn test_empty_string_rejected_when_not_in_grammar() {
    let schema = json!({
        "type": "object",
        "properties": {
            "flag": {"type": "boolean"}
        }
    });
    let gbnf = schema_to_gbnf("root", &schema).unwrap();
    let backend = MockBackend::default();
    let vocab = backend.vocab_bytes().unwrap();

    let mut mask = create_bnf_mask(&gbnf, &vocab).unwrap();

    // EOS token is typically index 0 in the mock backend vocab
    let mut logits = vec![0.0f32; vocab.len()];
    mask.mask(&mut logits);

    assert!(
        logits[0] < -1000.0,
        "EOS logit should be masked out since object is expected"
    );
}

#[tokio::test]
async fn test_grammar_error_message_includes_field_name() {
    // If we provide a bad schema (like an unknown type for a specific field), schema_to_gbnf should fail.
    // The current implementation might not include the field name directly, but we can verify it throws the correct error for nested fields.
    // Wait, let's modify the codebase to include the field name because the prompt specifically says "grammar error message includes field name".
    // I'll rewrite json_schema.rs to include the key.

    let schema = json!({
        "type": "object",
        "properties": {
            "bad_field": {"type": "unknown_type"}
        }
    });

    let res = schema_to_gbnf("root", &schema);
    assert!(res.is_err());
    let err = res.unwrap_err();
    let err_str = err.to_string();
    // It should contain 'bad_field' in the error message.
    assert!(
        err_str.contains("bad_field"),
        "Error message should include the problematic field name. Got: {}",
        err_str
    );
}
