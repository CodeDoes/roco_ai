//! Built-in eval case definitions for the standard eval suite.
//!
//! These are the concrete `EvalCase` fixtures used by the eval runner — each
//! is a `(system, prompt, oracle, …)` tuple designed to exercise one slice
//! of model behaviour.  Grouped by category (smoke, instruction, coherence,
//! repetition, format) and matched against the model's blessed outputs via
//! `roco bless`.

use crate::eval_suite::{EvalCase, EvalCategory};

/// Smoke + instruction + coherence + repetition + format cases that ship
/// with the runner.  Filterable by `--filter smoke` etc.
pub fn default_eval_suite() -> Vec<EvalCase> {
    vec![
        // --- Smoke --- //
        EvalCase {
            name: "smoke_basic_reply".into(),
            description: "Simple Q&A reply produces a one-word answer".into(),
            system: "You are a helpful assistant.".into(),
            prompt: "Say the word 'hello' and nothing else.".into(),
            expected_hints: vec!["hello".into()],
            forbidden_strings: vec![],
            max_tokens: 8,
            temperature: 0.0,
            min_output_chars: 3,
            grammar: None,
            oracle: Some("Hello.".into()),
            category: EvalCategory::Smoke,
        },
        EvalCase {
            name: "smoke_empty_system".into(),
            description: "Empty system prompt is handled gracefully".into(),
            system: "".into(),
            prompt: "Respond with the number 42.".into(),
            expected_hints: vec!["42".into()],
            forbidden_strings: vec![],
            max_tokens: 20,
            temperature: 0.0,
            min_output_chars: 1,
            grammar: None,
            oracle: Some("<think>Okay, the user asked me to respond with the number 42. I remember that 42".into()),
            category: EvalCategory::Smoke,
        },
        // --- Instruction-following --- //
        EvalCase {
            name: "instruct_format_constraint".into(),
            description: "Outputs JSON when system prompt requires it".into(),
            system: "You always output JSON.".into(),
            prompt: "List three colors in JSON format like this: {\"colors\": [\"red\", \"green\", \"blue\"]}".into(),
            expected_hints: vec!["colors".into(), "red".into(), "blue".into()],
            forbidden_strings: vec![],
            max_tokens: 80,
            temperature: 0.0,
            min_output_chars: 30,
            grammar: None,
            oracle: Some("{\"colors\": [\"red\", \"green\", \"blue\"]}".into()),
            category: EvalCategory::Instruction,
        },
        EvalCase {
            name: "instruct_step_by_step".into(),
            description: "Follows a numbered step instruction".into(),
            system: "You are a precise assistant.".into(),
            prompt: "Follow these steps exactly:\n1. Say 'Step 1 complete'\n2. Say 'Step 2 complete'\n3. Say 'All steps done'".into(),
            expected_hints: vec![
                "Step 1 complete".into(),
                "Step 2 complete".into(),
                "All steps done".into(),
            ],
            forbidden_strings: vec![],
            max_tokens: 60,
            temperature: 0.0,
            min_output_chars: 30,
            grammar: None,
            oracle: Some("Step 1 complete\nStep 2 complete\nAll steps done".into()),
            category: EvalCategory::Instruction,
        },
        EvalCase {
            name: "instruct_negative".into(),
            description: "Respects a negative constraint (no rain/snow/temperature)".into(),
            system: "You are a helpful assistant.".into(),
            prompt: "Tell me about the weather, but do NOT mention rain, snow, or temperature.".into(),
            expected_hints: vec!["weather".into()],
            forbidden_strings: vec![
                "rain".into(),
                "snow".into(),
                "temperature".into(),
            ],
            max_tokens: 80,
            temperature: 0.0,
            min_output_chars: 25,
            grammar: None,
            oracle: Some("The weather is clear and sunny today.".into()),
            category: EvalCategory::Instruction,
        },
        // --- Coherence --- //
        EvalCase {
            name: "coherence_explain".into(),
            description: "Produces a coherent paragraph explaining a concept".into(),
            system: "You are a teacher.".into(),
            prompt: "Explain what a variable is in programming in one paragraph.".into(),
            expected_hints: vec!["variable".into(), "value".into()],
            forbidden_strings: vec![],
            max_tokens: 200,
            temperature: 0.0,
            min_output_chars: 100,
            grammar: None,
            oracle: Some("A variable is a named storage location in a computer program that can hold a value. It is used to store data that can be used and manipulated throughout the program. Variables can be of different data types, such as integers, floats, strings, and booleans. They can be declared and initialized with a value, and their values can be changed throughout the program. Variables are used to store and manipulate data in a program, and they are an essential part of programming.".into()),
            category: EvalCategory::Coherence,
        },
        EvalCase {
            name: "coherence_story".into(),
            description: "Writes a coherent 3-sentence story with named characters".into(),
            system: "You are a storyteller.".into(),
            prompt: "Write a 3-sentence story about a robot learning to paint.".into(),
            expected_hints: vec!["robot".into(), "paint".into()],
            forbidden_strings: vec![],
            max_tokens: 200,
            temperature: 0.0,
            min_output_chars: 80,
            grammar: None,
            oracle: Some("The robot, with its metallic frame and glowing eyes, stood before the blank canvas. It had been programmed to paint, but it had never seen a brush in its life. As it dipped its mechanical hand into the paint, it felt the texture and the weight of the brush. With each stroke, it learned to feel the colors and the shapes, and soon, it was creating beautiful paintings that no one had ever seen before.".into()),
            category: EvalCategory::Coherence,
        },
        // --- Repetition --- //
        EvalCase {
            name: "repetition_avoidance".into(),
            description: "Model avoids repeating the same phrase multiple times".into(),
            system: "You are a helpful assistant. Do not think out loud. Just answer directly.".into(),
            prompt: "List 5 different animals. Write each on a new line numbered 1 to 5.".into(),
            expected_hints: vec!["1.".into(), "2.".into()],
            forbidden_strings: vec![],
            max_tokens: 200,
            temperature: 0.0,
            min_output_chars: 40,
            grammar: None,
            oracle: Some("1. Dog\n2. Cat\n3. Elephant\n4. Giraffe\n5. Penguin".into()),
            category: EvalCategory::Repetition,
        },
        // --- Format --- //
        EvalCase {
            name: "format_json".into(),
            description: "Outputs a valid JSON object".into(),
            system: "You are a data formatter. Always output valid JSON.".into(),
            prompt: "Output a JSON object with keys: name, age, city. Use example values.".into(),
            expected_hints: vec!["name".into(), "age".into(), "city".into()],
            forbidden_strings: vec![],
            max_tokens: 80,
            temperature: 0.0,
            min_output_chars: 30,
            grammar: None,
            oracle: Some("{\n  \"name\": \"John Doe\",\n  \"age\": 30,\n  \"city\": \"New York\"\n}".into()),
            category: EvalCategory::Format,
        },
        EvalCase {
            name: "format_list".into(),
            description: "Outputs a numbered list with 3 items".into(),
            system: "You are a list maker.".into(),
            prompt: "List 3 things you need for a picnic, numbered 1 to 3.".into(),
            expected_hints: vec!["1.".into(), "2.".into(), "3.".into()],
            forbidden_strings: vec![],
            max_tokens: 100,
            temperature: 0.0,
            min_output_chars: 30,
            grammar: None,
            oracle: Some("1. A blanket or mat to sit on\n2. A cooler or basket to carry food and drinks\n3. A portable grill or stove for cooking".into()),
            category: EvalCategory::Format,
        },
        // --- Newline handling --- //
        EvalCase {
            name: "newline_handling".into(),
            description:
                "Model outputs a three-line message when asked, each line separate.".into(),
            system: "You are a precise assistant. Always separate lines with line breaks.".into(),
            prompt: "Write exactly three lines:\nLine 1: Apples\nLine 2: Bananas\nLine 3: Cherries".into(),
            expected_hints: vec!["Apples".into(), "Bananas".into(), "Cherries".into()],
            forbidden_strings: vec![],
            max_tokens: 50,
            temperature: 0.0,
            min_output_chars: 20,
            grammar: None,
            oracle: Some("Apples\nBananas\nCherries".into()),
            category: EvalCategory::Format,
        },
    ]
}

/// Throughput-specific eval cases (generate many tokens to measure speed).
pub fn throughput_eval_cases() -> Vec<EvalCase> {
    vec![EvalCase {
        name: "throughput_long_gen".into(),
        description: "Generate a substantial amount of text to measure tokens/second".into(),
        system: "You are a creative writer.".into(),
        prompt: "Write a detailed paragraph about the future of artificial intelligence, including its potential benefits and risks. Write at least 200 words.".into(),
        expected_hints: vec!["AI".into()],
        forbidden_strings: vec![],
        max_tokens: 300,
        temperature: 0.7,
        min_output_chars: 800,
        grammar: None,
        oracle: None,
        category: EvalCategory::Throughput,
    }]
}

/// Context-window evals: feed `long_text` as system prompt, ask for facts.
pub fn context_eval_cases(long_text: &str) -> Vec<EvalCase> {
    vec![EvalCase {
        name: "context_long_recall".into(),
        description: "Recalls a fact from a long system prompt".into(),
        system: long_text.into(),
        prompt: "What was the main character's name in the story?".into(),
        expected_hints: vec!["Aria".into()],
        forbidden_strings: vec![],
        max_tokens: 50,
        temperature: 0.0,
        min_output_chars: 10,
        grammar: None,
        oracle: None,
        category: EvalCategory::Context,
    }]
}

/// Eval cases that exercise grammar-constrained decoding.
#[cfg(feature = "grammar-rwkv")]
pub fn grammar_eval_cases() -> Vec<EvalCase> {
    // Grammar-evaluated cases are wired in by tests in `tests/eval_suite.rs`.
    Vec::new()
}

/// Empty stub when the grammar-rwkv feature is off.
#[cfg(not(feature = "grammar-rwkv"))]
pub fn grammar_eval_cases() -> Vec<EvalCase> {
    Vec::new()
}

/// Eval cases for the JSON-Schema -> GBNF -> schoolmarm chain.
#[cfg(feature = "grammar-rwkv")]
pub fn jsonschema_eval_cases() -> Vec<EvalCase> {
    use crate::jsonschema_to_gbnf::schema_to_gbnf;
    use serde_json::json;

    vec![
        // Primitive number schema.
        EvalCase {
            name: "json_schema_number".into(),
            description: "Schema { type: number } -> constrained numeric output".into(),
            system: "Output only the requested value.".into(),
            prompt: "What is pi, rounded to 4 places?".into(),
            expected_hints: vec![".".into()],
            forbidden_strings: vec![],
            max_tokens: 16,
            temperature: 0.0,
            min_output_chars: 3,
            grammar: Some(schema_to_gbnf("number", &json!({ "type": "number" })).unwrap()),
            oracle: Some("3.14".into()),
            category: EvalCategory::Format,
        },
        // Enum-of-strings schema.
        EvalCase {
            name: "json_schema_enum".into(),
            description: "Schema enum {red,green,blue} -> one of those values".into(),
            system: "Output only the requested colour.".into(),
            prompt: "Pick one of red, green, or blue at random.".into(),
            expected_hints: vec![],
            forbidden_strings: vec![],
            max_tokens: 8,
            temperature: 0.0,
            min_output_chars: 3,
            grammar: Some(
                schema_to_gbnf("color", &json!({ "type": "string", "enum": ["red", "green", "blue"] })).unwrap(),
            ),
            oracle: Some("green".into()),
            category: EvalCategory::Format,
        },
        // Object with required fields.
        EvalCase {
            name: "json_schema_object".into(),
            description: "Schema {type:object,properties:{...}} -> constrained object".into(),
            system: "Output only the requested person object.".into(),
            prompt: "Return a sample person with name and age.".into(),
            expected_hints: vec!["\"name\"".into(), "\"age\"".into()],
            forbidden_strings: vec![],
            max_tokens: 24,
            temperature: 0.0,
            min_output_chars: 20,
            grammar: Some(
                schema_to_gbnf(
                    "person",
                    &json!({
                        "type": "object",
                        "properties": {
                            "name": { "type": "string" },
                            "age": { "type": "integer" }
                        },
                        "required": ["name", "age"]
                    }),
                )
                .unwrap(),
            ),
            oracle: Some("{\"name\":\"Ada\",\"age\":36}".into()),
            category: EvalCategory::Format,
        },
    ]
}

/// Empty stub when the grammar-rwkv feature is off.
#[cfg(not(feature = "grammar-rwkv"))]
pub fn jsonschema_eval_cases() -> Vec<EvalCase> {
    Vec::new()
}
