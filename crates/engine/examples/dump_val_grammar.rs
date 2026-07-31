//! Dump the GBNF grammar the crate generates for the StoryValidation schema.
//! Run: cargo run --release --example dump_val_grammar -p roco-engine
//!
//! Regression fixture for the enum-in-object scoping fix: `quality` is an
//! enum inside an object. The generated grammar must reference a named
//! `root_quality_enum` rule instead of inlining `|` alternation into the
//! object rule (GBNF `|` has lowest precedence and would split the rule).

fn main() {
    let schema = roco_engine::grammar::Schema::object()
        .prop(
            "quality",
            roco_engine::grammar::Schema::enum_values(vec![
                serde_json::json!("pass"),
                serde_json::json!("fail"),
                serde_json::json!("needs-work"),
            ]),
        )
        .prop("issues", roco_engine::grammar::Schema::string())
        .prop("suggestion", roco_engine::grammar::Schema::string())
        .build();
    let gbnf = roco_engine::grammar::json_schema::schema_to_gbnf("root", schema.to_json())
        .expect("schema is valid");
    println!("{gbnf}");
    assert!(
        gbnf.contains("root_quality_enum ::="),
        "enum must be a named rule"
    );
    let obj = gbnf.lines().find(|l| l.contains("root_obj ::=")).unwrap();
    assert!(
        obj.contains("root_quality_enum"),
        "object must reference the enum rule"
    );
    println!("\n✓ enum scoping OK");
}
