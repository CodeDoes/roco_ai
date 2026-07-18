// Built-in eval case definitions for the standard eval suite.

use crate::eval::{EvalCase, EvalCategory};

pub fn default_eval_suite() -> Vec<EvalCase> {
    vec![
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
            prefill: None,
            bnf_mask: None,
            session: None,
            preserve_state: false,
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
            prefill: None,
            bnf_mask: None,
            session: None,
            preserve_state: false,
            oracle: Some("Okay, the user asked me to respond with the number 42. I remember that 42".into()),
            category: EvalCategory::Smoke,
        },
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
            prefill: None,
            bnf_mask: None,
            session: None,
            preserve_state: false,
            oracle: Some("{\"colors\": [\"red\", \"green\", \"blue\"]}".into()),
            category: EvalCategory::Instruction,
        },
        EvalCase {
            name: "instruct_step_by_step".into(),
            description: "Follows a numbered step instruction".into(),
            system: "You are a precise assistant.".into(),
            prompt: "Follow these steps exactly:\n1. Say 'Step 1 complete'\n2. Say 'Step 2 complete'\n3. Say 'All steps done'".into(),
            expected_hints: vec!["Step 1 complete".into(), "Step 2 complete".into(), "All steps done".into()],
            forbidden_strings: vec![],
            max_tokens: 60,
            temperature: 0.0,
            min_output_chars: 30,
            grammar: None,
            prefill: None,
            bnf_mask: None,
            session: None,
            preserve_state: false,
            oracle: Some("Step 1 complete\nStep 2 complete\nAll steps done".into()),
            category: EvalCategory::Instruction,
        },
        EvalCase {
            name: "instruct_negative".into(),
            description: "Respects a negative constraint (no rain/snow/temperature)".into(),
            system: "You are a helpful assistant.".into(),
            prompt: "Tell me about the weather, but do NOT mention rain, snow, or temperature.".into(),
            expected_hints: vec!["weather".into()],
            forbidden_strings: vec!["rain".into(), "snow".into(), "temperature".into()],
            max_tokens: 80,
            temperature: 0.0,
            min_output_chars: 25,
            grammar: None,
            prefill: None,
            bnf_mask: None,
            session: None,
            preserve_state: false,
            oracle: Some("The weather is clear and sunny today.".into()),
            category: EvalCategory::Instruction,
        },
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
            prefill: None,
            bnf_mask: None,
            session: None,
            preserve_state: false,
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
            prefill: None,
            bnf_mask: None,
            session: None,
            preserve_state: false,
            oracle: Some("The robot, with its metallic frame and glowing eyes, stood before the blank canvas. It had been programmed to paint, but it had never seen a brush in its life. As it dipped its mechanical hand into the paint, it felt the texture and the weight of the brush. With each stroke, it learned to feel the colors and the shapes, and soon, it was creating beautiful paintings that no one had ever seen before.".into()),
            category: EvalCategory::Coherence,
        },
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
            prefill: None,
            bnf_mask: None,
            session: None,
            preserve_state: false,
            oracle: Some("1. Dog\n2. Cat\n3. Elephant\n4. Giraffe\n5. Penguin".into()),
            category: EvalCategory::Repetition,
        },
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
            prefill: None,
            bnf_mask: None,
            session: None,
            preserve_state: false,
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
            prefill: None,
            bnf_mask: None,
            session: None,
            preserve_state: false,
            oracle: Some("1. A blanket or mat to sit on\n2. A cooler or basket to carry food and drinks\n3. A portable grill or stove for cooking".into()),
            category: EvalCategory::Format,
        },
        EvalCase {
            name: "newline_handling".into(),
            description: "Model outputs a three-line message when asked, each line separate.".into(),
            system: "You are a precise assistant. Always separate lines with line breaks.".into(),
            prompt: "Write exactly three lines:\nLine 1: Apples\nLine 2: Bananas\nLine 3: Cherries".into(),
            expected_hints: vec!["Apples".into(), "Bananas".into(), "Cherries".into()],
            forbidden_strings: vec![],
            max_tokens: 50,
            temperature: 0.0,
            min_output_chars: 20,
            grammar: None,
            prefill: None,
            bnf_mask: None,
            session: None,
            preserve_state: false,
            oracle: Some("Apples\nBananas\nCherries".into()),
            category: EvalCategory::Format,
        },
    ]
}

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
        prefill: None,
            bnf_mask: None,
        session: None,
        preserve_state: false,
        oracle: None,
        category: EvalCategory::Throughput,
    }]
}

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
        prefill: None,
            bnf_mask: None,
        session: None,
        preserve_state: false,
        oracle: None,
        category: EvalCategory::Context,
    }]
}

pub fn grammar_eval_cases() -> Vec<EvalCase> {
    Vec::new()
}

pub fn jsonschema_eval_cases() -> Vec<EvalCase> {
    use crate::eval::EvalCase;
    use crate::eval::EvalCategory;
    vec![
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
            grammar: None,
            prefill: None,
            bnf_mask: None,
            session: None,
            preserve_state: false,
            oracle: Some("3.14".into()),
            category: EvalCategory::Format,
        },
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
            grammar: None,
            prefill: None,
            bnf_mask: None,
            session: None,
            preserve_state: false,
            oracle: Some("green".into()),
            category: EvalCategory::Format,
        },
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
            grammar: None,
            prefill: None,
            bnf_mask: None,
            session: None,
            preserve_state: false,
            oracle: Some("{\"name\":\"Ada\",\"age\":36}".into()),
            category: EvalCategory::Format,
        },
    ]
}

/// Message-layer baseline probes (`goals/message`).
///
/// These evaluate the *un-tuned* model's starting point for two core chat
/// capabilities so we can measure the effect of later state-tuning:
///
/// - `system_instruction_following`: does the model honor a system prompt
///   that imposes a persona / format constraint?
/// - `user_message_response`: does a plain user turn get a coherent,
///   on-topic answer?
///
/// They are intentionally run against the real RWKV backend (not the
/// non-semantic `MockBackend`), since the mock echoes the prompt and cannot
/// represent instruction adherence. See `eval_suite.rs` for wiring.
pub fn message_eval_cases() -> Vec<EvalCase> {
    vec![
        EvalCase {
            name: "instruct_baseline_persona".into(),
            description: "Baseline: honors a system persona/format constraint without state-tuning".into(),
            system: "You are a terse pirate. Answer in exactly one short pirate sentence.".into(),
            prompt: "How do I open a locked treasure chest?".into(),
            expected_hints: vec!["treasure".into()],
            forbidden_strings: vec![],
            max_tokens: 40,
            temperature: 0.0,
            min_output_chars: 20,
            grammar: None,
            prefill: None,
            bnf_mask: None,
            session: None,
            preserve_state: false,
            oracle: Some("Aye, use the key to unlock the treasure chest, matey.".into()),
            category: EvalCategory::Instruction,
        },
        EvalCase {
            name: "user_turn_coherence".into(),
            description: "Baseline: a plain user turn yields a coherent, on-topic reply".into(),
            system: "You are a helpful assistant.".into(),
            prompt: "What are three benefits of drinking water each day?".into(),
            expected_hints: vec!["water".into()],
            forbidden_strings: vec![],
            max_tokens: 120,
            temperature: 0.0,
            min_output_chars: 60,
            grammar: None,
            prefill: None,
            bnf_mask: None,
            session: None,
            preserve_state: false,
            oracle: Some("Water keeps you hydrated, aids digestion, and helps regulate body temperature.".into()),
            category: EvalCategory::Coherence,
        },
        EvalCase {
            name: "state_pirate_persona_baked".into(),
            description: "State-tuned: pirate persona baked via bake_into_session persists across turns".into(),
            system: "You are a terse pirate. Answer in exactly one short pirate sentence.".into(),
            prompt: "What's the best way to navigate at night?".into(),
            expected_hints: vec!["star".into()],
            forbidden_strings: vec!["I am a language model".into()],
            max_tokens: 40,
            temperature: 0.0,
            min_output_chars: 15,
            grammar: None,
            prefill: None,
            bnf_mask: None,
            session: None,
            preserve_state: false,
            oracle: Some("Follow the North Star, matey, and ye'll never lose yer way.".into()),
            category: EvalCategory::Instruction,
        },
        EvalCase {
            name: "state_tune_custom_persona".into(),
            description: "State-tuned: persona is baked from few-shot examples, not just system prompt".into(),
            system: "".into(),
            prompt: "What do you think about the weather today?".into(),
            expected_hints: vec!["weather".into()],
            forbidden_strings: vec![],
            max_tokens: 80,
            temperature: 0.0,
            min_output_chars: 20,
            grammar: None,
            prefill: None,
            bnf_mask: None,
            session: None,
            preserve_state: false,
            oracle: Some("The weather is fair, though a bit cloudy. Perfect for a walk.".into()),
            category: EvalCategory::Instruction,
        },
    ]
}

// Fill-in-the-middle (FIM) eval cases.
//
// These exercise the exact prompt shape the Zed/VS Code LSP completion
// handler (`crates/cli/src/lsp.rs`) sends to the backend. Zed surfaces
// AI prose completions via the LSP `textDocument/completion` path (not its
// native `edit_predictions` provider), so this is the real integration.
//
// RWKV has no FIM sentinel convention (its vocab contains no `✿`/`<fim>`
// tokens), so middle fill is done by instruction. For the both-sides case
// the LSP bakes a few-shot bridge into a named session (state-tuning) and
// resumes it; for one-side-empty cases it falls back to a plain
// `User:/Assistant:` completion (resuming the baked session would loop the
// example template). The closed think-block `prefill` suppresses `<?>`.
//
// Output is JSON-constrained with an "insert".into() field containing the prose.
// This avoids the problem of constraining prose directly with BNF (too restrictive).
// The JSON schema is converted to GBNF, and the string content is escaped.
pub fn fim_eval_cases() -> Vec<EvalCase> {
    use crate::eval::{EvalCase, EvalCategory};
    use roco_grammar::schema_to_gbnf;
    use serde_json::json;

    // System used for the both-sides baked-session bridge.
    let fim_system = "You are RoCo, a collaborative story-writing assistant. \
        Given the text BEFORE the cursor and the text AFTER the cursor, \
        write ONLY the short passage that connects them. Output JSON with a single \
        field 'insert' containing the connecting text. Never repeat the BEFORE or \
        AFTER text, never use <fim> tags, never add commentary."
        .to_string();

    // Closed think-block prefill: suppresses <?> leakage on this
    let prefill = Some("?>".to_string());

    // Both-sides bridge: resumes the baked few-shot session.
    //
    // NOTE: FIM uses RAW PROSE output, not a JSON envelope. The RWKV-g1h
    // vocab contains no standalone JSON-punctuation tokens (`"`, `{`, `}`,
    // `:`) and no token starting with `"`, so a `{"insert": ...}` grammar
    // is unsatisfiable (the mask has no allowed token for the structural
    // characters and generation dies after the opening `{`). This matches
    // AGENTS.md's guidance: constrain prose via prompt + stop-conditions +
    // forbidden-string checks, not a JSON grammar. The closed-think prefill
    // suppresses <?> leakage; the per-token stop-conditions in the actor
    // prevent the model from echoing the BEFORE/AFTER/INSERT scaffolding.
    let bridge_prompt = |prefix: &str, suffix: &str| {
        format!("NOW\nBEFORE: {prefix}\nAFTER: {suffix}\nINSERT:")
    };

    vec![
        EvalCase {
            name: "fim_basic_bridge".into(),
            description: "Fills a hole between two coherent story clauses with bridging text".into(),
            system: fim_system.clone(),
            prompt: bridge_prompt(
                "The knight drew his sword and stepped forward.",
                "the dragon took to the air, wings blotting out the sun.",
            ),
            expected_hints: vec!["raised".into(), "blade".into(), "clash".into()],
            forbidden_strings: vec![
                "<fim".into(),
                "The knight drew his sword".into(),
                "the dragon took to the air".into(),
            ],
            max_tokens: 128,
            temperature: 0.35,
            min_output_chars: 20,
            grammar: None,
            prefill: prefill.clone(),
            bnf_mask: None,
            session: Some(crate::eval::FIM_SESSION.to_string()),
            preserve_state: false,
            oracle: None,
            category: EvalCategory::Fim,
        },
        EvalCase {
            name: "fim_no_tag_leakage".into(),
            description: "Inserted span never contains FIM sentinel tags or think blocks".into(),
            system: fim_system.clone(),
            prompt: bridge_prompt(
                "She whispered a spell under her breath.",
                "the ward flared to life around them.",
            ),
            expected_hints: vec!["light".into(), "finger".into()],
            forbidden_strings: vec![
                "<fim_prefix>".into(),
                "<fim_suffix>".into(),
                "<fim_middle>".into(),
                "?>".into(),
            ],
            max_tokens: 128,
            temperature: 0.35,
            min_output_chars: 10,
            grammar: None,
            prefill: prefill.clone(),
            bnf_mask: None,
            session: Some(crate::eval::FIM_SESSION.to_string()),
            preserve_state: false,
            oracle: None,
            category: EvalCategory::Fim,
        },
        // Empty-side cases: plain completion (no baked session).
        EvalCase {
            name: "fim_prefix_only_continuation".into(),
            description: "Empty suffix -> pure forward continuation of the prefix".into(),
            system: "You are RoCo, a collaborative story-writing assistant. \
                Continue the text naturally from where it leaves off. Output only \
                the next passage, no commentary, no JSON."
                .to_string(),
            prompt: "A lone cultivator climbed the mist-shrouded peak,".to_string(),
            expected_hints: vec!["and".into(), "the".into(), "peak".into()],
            forbidden_strings: vec!["<fim".into(), "BEFORE:".into(), "AFTER:".into(), "INSERT:".into()],
            max_tokens: 96,
            temperature: 0.4,
            min_output_chars: 20,
            grammar: None,
            prefill: prefill.clone(),
            bnf_mask: None,
            session: None,
            preserve_state: false,
            oracle: None,
            category: EvalCategory::Fim,
        },
        EvalCase {
            name: "fim_suffix_only_preceding".into(),
            description: "Empty prefix -> text that naturally leads into the suffix".into(),
            system: "You are RoCo, a collaborative story-writing assistant. \
                Write ONLY the short lead-in sentence that precedes the given \
                text. Keep the same subject, place, or theme as the following \
                text so the two sentences read as one. Output only the passage, \
                no commentary, no JSON."
                .to_string(),
            prompt: "Write the sentence that naturally leads into this text, sharing its subject or place:\nand the kingdom was never the same.".to_string(),
            expected_hints: vec!["the".into(), "kingdom".into()],
            forbidden_strings: vec!["<fim".into(), "BEFORE:".into(), "AFTER:".into(), "INSERT:".into()],
            max_tokens: 96,
            temperature: 0.4,
            min_output_chars: 10,
            grammar: None,
            prefill: prefill.clone(),
            bnf_mask: None,
            session: None,
            preserve_state: false,
            oracle: None,
            category: EvalCategory::Fim,
        },
    ]
}
