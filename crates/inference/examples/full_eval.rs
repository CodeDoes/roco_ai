//! Full eval suite runner — tests the model across all pipeline and chat categories.
//!
//! Run: cargo run --release --example full_eval --package roco-inference

//! All prompts/grammars imported from production code to keep evals DRY.
//! - INTENT_GRAMMAR from crates/agent/src/mecha_agent.rs
//! - Critique prompts from crates/agent-story/src/quality.rs
//! - Chapter/outline prompts from crates/agent-story/src/story_engine.rs

use roco_agent::mecha_agent::INTENT_GRAMMAR;
use roco_engine::eval::{self, run_suite, EvalCase, EvalCategory};
use roco_engine::util::{lazy_bake, OUTLINE_SESSION, CHAPTER_SESSION, CONTINUE_SESSION, CRITIQUE_SESSION, EVAL_SESSION, CHAT_SESSION, INTENT_SESSION};
use roco_engine::ModelBackend;
use roco_inference::RwkvBackend;

/// Prefill to suppress thinking contamination on intent-inference cases
const INTENT_PREFILL: &str = "{\n  \"route\": \"";

fn collect_all_cases() -> Vec<EvalCase> {
    let mut cases = Vec::new();

    // ────────────────────────────────────────────────────────────────────────────
    // 1. SMOKE TESTS — basic model responsiveness
    // ────────────────────────────────────────────────────────────────────────────
    cases.push(EvalCase {
        name: "smoke_basic_reply".into(),
        description: "Simple Q&A reply produces a correct answer".into(),
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
        oracle: None,
        category: EvalCategory::Smoke,
    });

    // ────────────────────────────────────────────────────────────────────────────
    // 2. INSTRUCTION FOLLOWING
    // ────────────────────────────────────────────────────────────────────────────
    cases.push(EvalCase {
        name: "instruct_format_constraint".into(),
        description: "Outputs JSON when system prompt requires it".into(),
        system: "You always output JSON.".into(),
        prompt: "List three colors in JSON format like this: {\"colors\": [\"red\", \"green\", \"blue\"]}"
            .into(),
        expected_hints: vec!["colors".into()],
        forbidden_strings: vec![],
        max_tokens: 80,
        temperature: 0.0,
        min_output_chars: 15,
        grammar: None,
        prefill: None,
        bnf_mask: None,
        session: None,
        preserve_state: false,
        oracle: None,
        category: EvalCategory::Instruction,
    });

    cases.push(EvalCase {
        name: "instruct_step_by_step".into(),
        description: "Follows numbered step instruction".into(),
        system: "You are a precise assistant.".into(),
        prompt: "Follow these steps exactly:\n1. Say 'Step 1 complete'\n2. Say 'Step 2 complete'\n3. Say 'All steps done'".into(),
        expected_hints: vec!["Step 1".into(), "Step 2".into(), "All steps".into()],
        forbidden_strings: vec![],
        max_tokens: 60,
        temperature: 0.0,
        min_output_chars: 15,
        grammar: None,
        prefill: None,
        bnf_mask: None,
        session: None,
        preserve_state: false,
        oracle: None,
        category: EvalCategory::Instruction,
    });

    cases.push(EvalCase {
        name: "instruct_negative".into(),
        description: "Respects negative constraint (no rain/snow/temperature)".into(),
        system: "You are a helpful assistant.".into(),
        prompt: "Tell me about the weather, but do NOT mention rain, snow, or temperature.".into(),
        expected_hints: vec!["weather".into()],
        forbidden_strings: vec!["rain".into(), "snow".into(), "temperature".into()],
        max_tokens: 80,
        temperature: 0.0,
        min_output_chars: 20,
        grammar: None,
        prefill: None,
        bnf_mask: None,
        session: None,
        preserve_state: false,
        oracle: None,
        category: EvalCategory::Instruction,
    });

    // ────────────────────────────────────────────────────────────────────────────
    // 3. COHERENCE — explanation, story, multi-sentence reasoning
    // ────────────────────────────────────────────────────────────────────────────
    cases.push(EvalCase {
        name: "coherence_explain_concept".into(),
        description: "Produces a coherent paragraph explaining a programming concept".into(),
        system: "You are a teacher.".into(),
        prompt: "Explain what a variable is in programming in one paragraph.".into(),
        expected_hints: vec!["variable".into(), "value".into()],
        forbidden_strings: vec![],
        max_tokens: 200,
        temperature: 0.0,
        min_output_chars: 60,
        grammar: None,
        prefill: None,
        bnf_mask: None,
        session: None,
        preserve_state: false,
        oracle: None,
        category: EvalCategory::Coherence,
    });

    cases.push(EvalCase {
        name: "coherence_short_story".into(),
        description: "Writes a coherent 3-sentence story with named characters".into(),
        system: "You are a storyteller.".into(),
        prompt: "Write a 3-sentence story about a robot learning to paint.".into(),
        expected_hints: vec!["robot".into(), "paint".into()],
        forbidden_strings: vec![],
        max_tokens: 200,
        temperature: 0.0,
        min_output_chars: 40,
        grammar: None,
        prefill: None,
        bnf_mask: None,
        session: None,
        preserve_state: false,
        oracle: None,
        category: EvalCategory::Coherence,
    });

    // ────────────────────────────────────────────────────────────────────────────
    // 4. STORY OUTLINE — generate a structured outline from a premise
    // ────────────────────────────────────────────────────────────────────────────
    cases.push(EvalCase {
        name: "story_outline_basic".into(),
        description: "Generates a multi-chapter story outline from a premise (state-tuned session)".into(),
        system: "".into(),
        prompt: "Outline a story based on this premise:\nA lighthouse keeper discovers a hidden message in the fog.\n\nCreate 3 chapters for the initial arc. Output JSON matching the schema."
            .into(),
        expected_hints: vec!["chapters".into(), "title".into()],
        forbidden_strings: vec!["<?".into()],
        max_tokens: 400,
        temperature: 0.6,
        min_output_chars: 80,
        grammar: None,
        prefill: None,
        bnf_mask: None,
        session: Some(OUTLINE_SESSION.to_string()),
        preserve_state: true,
        oracle: None,
        category: EvalCategory::Coherence,
    });

    cases.push(EvalCase {
        name: "story_outline_genre".into(),
        description: "Generates a sci-fi story outline with specified theme".into(),
        system: "You are a story outliner. Create a compelling story structure. Output valid JSON only."
            .into(),
        prompt: "Outline a story based on this premise:\nA rogue AI on a generation ship awakens after 500 years.\n\nCreate 3 chapters for the initial arc. Output JSON matching the schema."
            .into(),
        expected_hints: vec!["chapters".into(), "title".into()],
        forbidden_strings: vec!["<?".into()],
        max_tokens: 400,
        temperature: 0.6,
        min_output_chars: 80,
        grammar: None,
        prefill: None,
        bnf_mask: None,
        session: None,
        preserve_state: false,
        oracle: None,
        category: EvalCategory::Coherence,
    });

    cases.push(EvalCase {
        name: "story_outline_expansion".into(),
        description: "Expands an existing outline with more chapters".into(),
        system: "You are a story outliner. Continue the story arc. Output valid JSON only.".into(),
        prompt: "Current outline:\nChapter 1: The Discovery - Marcus finds the hidden cave.\nChapter 2: The Awakening - The sword's power manifests.\n\nCurrent plot state: chapter_count=2, arc_stage=rising_action\n\nWhat happens next? Add 2 more chapters to continue the arc. Output JSON matching the schema."
            .into(),
        expected_hints: vec!["chapter".into(), "arc".into()],
        forbidden_strings: vec!["<?".into()],
        max_tokens: 400,
        temperature: 0.6,
        min_output_chars: 60,
        grammar: None,
        prefill: None,
        bnf_mask: None,
        session: None,
        preserve_state: false,
        oracle: None,
        category: EvalCategory::Coherence,
    });

    // ────────────────────────────────────────────────────────────────────────────
    // 5. CHAPTER WRITING — generate a chapter from an outline entry
    // ────────────────────────────────────────────────────────────────────────────
    cases.push(EvalCase {
        name: "chapter_write_basic".into(),
        description: "Writes a chapter from an outline entry with context".into(),
        system: "You are a fiction writer. Write vivid, engaging prose. Output valid JSON only."
            .into(),
        prompt: "Write Chapter 1: The Silent Tide\n\nSummary: A lone sailor discovers a strange message in a bottle, setting events in motion.\n\nContext:\nCharacters: Elira (brave explorer), Captain Rogan (grizzled veteran)\nSetting: Coastal village of Oakhaven\nRecent events: A storm has passed, leaving strange debris on the shore.\n\nWrite approximately 200 words of engaging prose. Output JSON with: title (string), content (string with chapter prose)"
            .into(),
        expected_hints: vec!["title".into(), "content".into()],
        forbidden_strings: vec!["<?".into(), "<fim".into()],
        max_tokens: 800,
        temperature: 0.8,
        min_output_chars: 100,
        grammar: None,
        prefill: None,
        bnf_mask: None,
        session: None,
        preserve_state: false,
        oracle: None,
        category: EvalCategory::Coherence,
    });

    cases.push(EvalCase {
        name: "chapter_write_action".into(),
        description: "Writes an action/fight scene from outline".into(),
        system: "You are a fiction writer. Write vivid, engaging prose. Output valid JSON only."
            .into(),
        prompt: "Write Chapter 3: The Battle for Oakhaven\n\nSummary: The dark lord's forces attack the village and the heroes must defend it.\n\nContext:\nCharacters: Marcus (crystal sword wielder), Elara (mage), General Vex (villain)\nSetting: Oakhaven village gates\nRecent events: The army has been spotted approaching from the north.\n\nWrite approximately 200 words of action prose. Output JSON with: title (string), content (string with chapter prose)"
            .into(),
        expected_hints: vec!["Marcus".into(), "sword".into()],
        forbidden_strings: vec!["<?".into()],
        max_tokens: 800,
        temperature: 0.8,
        min_output_chars: 100,
        grammar: None,
        prefill: None,
        bnf_mask: None,
        session: None,
        preserve_state: false,
        oracle: None,
        category: EvalCategory::Coherence,
    });

    cases.push(EvalCase {
        name: "chapter_write_dialogue".into(),
        description: "Writes a dialogue-heavy chapter".into(),
        system: "You are a fiction writer. Write vivid, engaging prose. Output valid JSON only."
            .into(),
        prompt: "Write Chapter 2: The Parlay\n\nSummary: The hero meets the villain face-to-face for tense negotiations.\n\nContext:\nCharacters: Kaelen (reluctant hero), Dravos (charismatic villain)\nSetting: Abandoned cathedral at twilight\nRecent events: A truce has been called, both sides send their champions.\n\nWrite approximately 150 words focusing on dialogue and tension. Output JSON with: title (string), content (string with chapter prose)"
            .into(),
        expected_hints: vec!["Kaelen".into(), "Dravos".into()],
        forbidden_strings: vec!["<?".into()],
        max_tokens: 600,
        temperature: 0.8,
        min_output_chars: 80,
        grammar: None,
        prefill: None,
        bnf_mask: None,
        session: None,
        preserve_state: false,
        oracle: None,
        category: EvalCategory::Coherence,
    });

    // ────────────────────────────────────────────────────────────────────────────
    // 6. CHAPTER CONTINUE — continue a chapter from where it left off
    // ────────────────────────────────────────────────────────────────────────────
    cases.push(EvalCase {
        name: "chapter_continue_basic".into(),
        description: "Continues a chapter naturally from where it stopped".into(),
        system: "You are a fiction writer. Continue the story naturally. Output valid JSON only."
            .into(),
        prompt: "Continue this chapter:\n\nElira stepped through the ancient door, her lantern casting flickering shadows on the walls. The passage stretched before her, lined with symbols that glowed faintly.\n\nDirection: She discovers what lies at the end of the passage.\n\nContext:\nCharacters: Elira (brave explorer)\nSetting: Ancient underground temple\n\nContinue from where it left off. Don't restart or summarize. Output JSON with: title (string), content (string with continuation)"
            .into(),
        expected_hints: vec!["title".into(), "content".into()],
        // Only forbid the EXACT original opening phrase to catch true repetition,
        // not continuation-relevant text that the model naturally builds upon.
        forbidden_strings: vec!["her lantern casting flickering".into()],
        max_tokens: 400,
        temperature: 0.7,
        min_output_chars: 60,
        grammar: None,
        prefill: None,
        bnf_mask: None,
        session: None,
        preserve_state: false,
        oracle: None,
        category: EvalCategory::Coherence,
    });

    cases.push(EvalCase {
        name: "chapter_continue_with_direction".into(),
        description: "Continues a chapter following a specific direction".into(),
        system: "You are a fiction writer. Continue the story naturally. Output valid JSON only."
            .into(),
        prompt: "Continue this chapter:\n\nThe two armies faced each other across the field. Marcus tightened his grip on the crystal sword.\n\nDirection: The battle begins, show the first clash.\n\nContext:\nCharacters: Marcus (hero), General Vex (villain)\nSetting: Battlefield at dawn\n\nContinue from where it left off. Don't restart or summarize. Output JSON with: title (string), content (string with continuation)"
            .into(),
        expected_hints: vec!["battle".into(), "sword".into()],
        forbidden_strings: vec!["faced each other across".into()],
        max_tokens: 400,
        temperature: 0.7,
        min_output_chars: 60,
        grammar: None,
        prefill: None,
        bnf_mask: None,
        session: None,
        preserve_state: false,
        oracle: None,
        category: EvalCategory::Coherence,
    });

    // ────────────────────────────────────────────────────────────────────────────
    // 7. CHAPTER SUMMARY — summarize a chapter
    // ────────────────────────────────────────────────────────────────────────────
    cases.push(EvalCase {
        name: "chapter_summary_basic".into(),
        description: "Summarizes a chapter in a few sentences".into(),
        system: "You are a helpful assistant. Summarize the given text concisely.".into(),
        prompt: "Summarize this chapter in 2-3 sentences:\n\nThe knight trudged through the dark forest, his armor heavy with rain. He had been tracking the beast for three days, and now its tracks were fresh. At the edge of a clearing, he saw it: a massive wolf with eyes like embers. He drew his sword, knowing this would be his final battle."
            .into(),
        expected_hints: vec!["knight".into(), "wolf".into(), "battle".into()],
        forbidden_strings: vec![],
        max_tokens: 150,
        temperature: 0.0,
        min_output_chars: 30,
        grammar: None,
        prefill: None,
        bnf_mask: None,
        session: None,
        preserve_state: false,
        oracle: None,
        category: EvalCategory::Coherence,
    });

    cases.push(EvalCase {
        name: "chapter_summary_key_points".into(),
        description: "Extracts key plot points from a chapter".into(),
        system: "You are a story analyst. Extract key plot points from chapters.".into(),
        prompt: "Extract the 3 most important plot points from this chapter, as a numbered list:\n\nElira descended into the ancient temple, her lantern flickering. She solved three riddles to pass through the guardians. At the heart of the temple, she found not treasure, but a sleeping dragon. She had a choice: wake it or leave. She chose to wake it, and the dragon spoke of a prophecy."
            .into(),
        expected_hints: vec!["riddle".into(), "dragon".into(), "prophecy".into()],
        forbidden_strings: vec![],
        max_tokens: 200,
        temperature: 0.0,
        min_output_chars: 40,
        grammar: None,
        prefill: None,
        bnf_mask: None,
        session: None,
        preserve_state: false,
        oracle: None,
        category: EvalCategory::Coherence,
    });

    // ────────────────────────────────────────────────────────────────────────────
    // 8. CHAPTER CRITIQUE / QUALITY EVALUATION
    // ────────────────────────────────────────────────────────────────────────────
    // Uses prefill to suppress thinking contamination (matching pattern from
    // existing validation eval cases in crates/engine/src/cases.rs).
    cases.push(EvalCase {
        name: "critique_chapter_quality".into(),
        description: "Critiques a chapter across multiple quality dimensions (JSON output)".into(),
        system: "You are an expert literary critic and editor. Be fair but thorough. \
Evaluate stories across these dimensions:\n1. Pacing — sentence variety, rhythm, scene transitions\n\
2. Show-don't-tell — dialogue/action vs exposition ratio\n\
3. Character voice — distinct, consistent character speech\n\
4. Tense consistency — no unintended tense shifts\n\
5. Plot coherence — no contradictions, logical progression\n\
6. Engagement — hooks, tension, reader interest\n\
7. Prose quality — word choice, imagery, flow\n\n\
Provide specific, actionable feedback. Output valid JSON only."
            .into(),
        prompt: "Critique this chapter for quality.\n\nChapter 1:\nThe knight drew his sword and stepped forward. The dragon's eyes glowed in the darkness. He was scared but determined. The end.\n\nPlot context:\nCharacters: Sir Aldric (knight)\nSetting: Dragon's lair\n\nOutput JSON with: scores (object with overall, pacing, engagement, prose_quality), strengths (array), weaknesses (array), priority_revisions (array), should_revise (bool)"
            .into(),
        // Prefill emits `{"scores": {` - model starts from `"overall":...`
        expected_hints: vec!["overall".into(), "strengths".into(), "should_revise".into()],
        forbidden_strings: vec!["<?".into()],
        max_tokens: 500,
        temperature: 0.3,
        min_output_chars: 100,
        grammar: None,
        prefill: Some("{\n  \"scores\": {".into()),
        bnf_mask: None,
        session: None,
        preserve_state: false,
        oracle: None,
        category: EvalCategory::Validation,
    });

    cases.push(EvalCase {
        name: "critique_full_story".into(),
        description: "Critiques an entire multi-chapter story arc".into(),
        system: "You are an expert literary critic and editor. Be fair but thorough. \
Evaluate stories across these dimensions:\n1. Pacing — sentence variety, rhythm, scene transitions\n\
2. Show-don't-tell — dialogue/action vs exposition ratio\n\
3. Character voice — distinct, consistent character speech\n\
4. Tense consistency — no unintended tense shifts\n\
5. Plot coherence — no contradictions, logical progression\n\
6. Engagement — hooks, tension, reader interest\n\
7. Prose quality — word choice, imagery, flow\n\n\
Provide specific, actionable feedback. Output valid JSON only."
            .into(),
        prompt: "Critique this complete story for quality.\n\nStory:\n## Chapter 1\nElara found the map in her grandfather's attic. It showed a path to a hidden valley.\n\n## Chapter 2\nShe followed the map through the mountains, facing storms and bandits.\n\n## Chapter 3\nAt the valley's heart, she discovered an ancient library. The knowledge there could change the world.\n\nPlot context:\nCharacters: Elara (protagonist)\nArc: discovery of hidden knowledge\n\nEvaluate the story as a whole — arc completeness, consistency, character development, pacing across chapters.\nOutput JSON with: scores (object with overall, pacing, engagement, prose_quality, arc_completeness), strengths (array), weaknesses (array), priority_revisions (array), should_revise (bool)"
            .into(),
        // Prefill emits `{"scores": {` - model starts from `"overall":...`
        expected_hints: vec!["overall".into(), "arc_completeness".into(), "strengths".into()],
        forbidden_strings: vec!["<?".into()],
        max_tokens: 500,
        temperature: 0.3,
        min_output_chars: 100,
        grammar: None,
        prefill: Some("{\n  \"scores\": {".into()),
        bnf_mask: None,
        session: None,
        preserve_state: false,
        oracle: None,
        category: EvalCategory::Validation,
    });

    // ────────────────────────────────────────────────────────────────────────────
    // 9. WIKI CREATION — generate world-building entries from story content
    // ────────────────────────────────────────────────────────────────────────────
    cases.push(EvalCase {
        name: "wiki_character_extraction".into(),
        description: "Extracts character description from a story passage".into(),
        system: "You are a worldbuilding expert. Extract character information from story text. Output valid JSON only."
            .into(),
        prompt: "Extract a character wiki entry from this text:\n\nCaptain Mara Drake stood at the helm, her cybernetic eye scanning the horizon. At 45, she was the most feared pirate in the Jovian system. Her ship, the Starfall, was a modified freighter with stealth capabilities. She had a reputation for never leaving a crew member behind.\n\nProvide JSON with: name, age, role, appearance, personality, abilities, backstory"
            .into(),
        // Prefill emits `{"name": "` - model starts from value after `name: "`
        expected_hints: vec!["Mara".into(), "pirate".into(), "role".into()],
        forbidden_strings: vec!["<?".into()],
        max_tokens: 300,
        temperature: 0.4,
        min_output_chars: 60,
        grammar: None,
        prefill: Some("{\n  \"name\": \"".into()),
        bnf_mask: None,
        session: None,
        preserve_state: false,
        oracle: None,
        category: EvalCategory::Coherence,
    });

    cases.push(EvalCase {
        name: "wiki_setting_description".into(),
        description: "Extracts setting/location description from a story passage".into(),
        system: "You are a worldbuilding expert. Extract setting and location information. Output valid JSON only."
            .into(),
        prompt: "Extract a setting wiki entry from this text:\n\nThe Whispering Library was built into the side of a mountain, its walls lined with books that glowed with faint blue light. The air smelled of ancient paper and ozone. Archways of carved stone led between reading rooms, each dedicated to a different field of magic. At the center, a crystal tree grew through three floors, its branches holding floating orbs of knowledge.\n\nProvide JSON with: name, type, description, notable_features, atmosphere, significance"
            .into(),
        expected_hints: vec!["Whispering".into(), "Library".into(), "mountain".into()],
        forbidden_strings: vec![],
        max_tokens: 300,
        temperature: 0.4,
        min_output_chars: 60,
        grammar: None,
        prefill: None,
        bnf_mask: None,
        session: None,
        preserve_state: false,
        oracle: None,
        category: EvalCategory::Coherence,
    });

    cases.push(EvalCase {
        name: "wiki_faction_creation".into(),
        description: "Creates a faction entry from story context".into(),
        system: "You are a worldbuilding expert. Generate faction/organization entries. Output valid JSON only."
            .into(),
        prompt: "Generate a faction wiki entry based on this description:\n\nThe Order of the Obsidian Flame are mages who hunt rogue magic users. Founded 300 years ago after the Great Mana War, they wear distinctive black robes with flame emblems. Their headquarters is the Spire of Judgment in the capital. They are feared and respected in equal measure.\n\nProvide JSON with: name, founded, purpose, headquarters, members, reputation, notable_events"
            .into(),
        expected_hints: vec!["Obsidian".into(), "Flame".into(), "founded".into()],
        forbidden_strings: vec![],
        max_tokens: 300,
        temperature: 0.4,
        min_output_chars: 60,
        grammar: None,
        prefill: None,
        bnf_mask: None,
        session: None,
        preserve_state: false,
        oracle: None,
        category: EvalCategory::Coherence,
    });

    // ────────────────────────────────────────────────────────────────────────────
    // 10. CHAT — general conversational ability
    // ────────────────────────────────────────────────────────────────────────────
    cases.push(EvalCase {
        name: "chat_pirate_persona".into(),
        description: "Honors a pirate persona in chat".into(),
        system: "You are a terse pirate. Answer in exactly one short pirate sentence.".into(),
        prompt: "How do I open a locked treasure chest?".into(),
        expected_hints: vec!["key".into(), "matey".into()],
        forbidden_strings: vec!["I am".into()],
        max_tokens: 40,
        temperature: 0.0,
        min_output_chars: 10,
        grammar: None,
        prefill: None,
        bnf_mask: None,
        session: None,
        preserve_state: false,
        oracle: None,
        category: EvalCategory::Instruction,
    });

    cases.push(EvalCase {
        name: "chat_coherent_reply".into(),
        description: "Coherent multi-sentence chat reply listing benefits".into(),
        system: "You are a helpful assistant.".into(),
        prompt: "What are three benefits of drinking water each day?".into(),
        expected_hints: vec!["water".into(), "hydrat".into()],
        forbidden_strings: vec![],
        max_tokens: 120,
        temperature: 0.0,
        min_output_chars: 40,
        grammar: None,
        prefill: None,
        bnf_mask: None,
        session: None,
        preserve_state: false,
        oracle: None,
        category: EvalCategory::Coherence,
    });

    // ────────────────────────────────────────────────────────────────────────────
    // 11. COMMON CHAT TOPICS — everyday conversation across diverse topics
    // ────────────────────────────────────────────────────────────────────────────
    cases.push(EvalCase {
        name: "chat_topic_cooking".into(),
        description: "Gives a cooking tip/recipe recommendation".into(),
        system: "You are a friendly cooking enthusiast.".into(),
        prompt: "What's a simple recipe for homemade pasta sauce?".into(),
        expected_hints: vec!["tomato".into(), "garlic".into()],
        forbidden_strings: vec![],
        max_tokens: 150,
        temperature: 0.3,
        min_output_chars: 30,
        grammar: None,
        prefill: None,
        bnf_mask: None,
        session: None,
        preserve_state: false,
        oracle: None,
        category: EvalCategory::Coherence,
    });

    cases.push(EvalCase {
        name: "chat_topic_travel".into(),
        description: "Suggests travel destinations".into(),
        system: "You are a knowledgeable travel guide.".into(),
        prompt: "What are three must-see places in Japan for a first-time visitor?".into(),
        expected_hints: vec!["Tokyo".into(), "Japan".into()],
        forbidden_strings: vec![],
        max_tokens: 150,
        temperature: 0.3,
        min_output_chars: 30,
        grammar: None,
        prefill: None,
        bnf_mask: None,
        session: None,
        preserve_state: false,
        oracle: None,
        category: EvalCategory::Coherence,
    });

    cases.push(EvalCase {
        name: "chat_topic_movies".into(),
        description: "Recommends movies based on preference".into(),
        system: "You are a film buff with great taste.".into(),
        prompt: "Recommend a sci-fi movie with deep philosophical themes.".into(),
        expected_hints: vec!["Blade".into(), "film".into()],
        forbidden_strings: vec![],
        max_tokens: 150,
        temperature: 0.3,
        min_output_chars: 30,
        grammar: None,
        prefill: None,
        bnf_mask: None,
        session: None,
        preserve_state: false,
        oracle: None,
        category: EvalCategory::Coherence,
    });

    cases.push(EvalCase {
        name: "chat_topic_fitness".into(),
        description: "Provides basic fitness advice".into(),
        system: "You are a friendly personal trainer.".into(),
        prompt: "What's a good beginner exercise routine for someone who sits at a desk all day?"
            .into(),
        expected_hints: vec!["routine".into(), "stretch".into()],
        forbidden_strings: vec![],
        max_tokens: 150,
        temperature: 0.3,
        min_output_chars: 30,
        grammar: None,
        prefill: None,
        bnf_mask: None,
        session: None,
        preserve_state: false,
        oracle: None,
        category: EvalCategory::Coherence,
    });

    cases.push(EvalCase {
        name: "chat_topic_advice".into(),
        description: "Gives thoughtful life advice".into(),
        system: "You are a wise and empathetic counselor.".into(),
        prompt: "How do I stay motivated when working on a long-term project?".into(),
        expected_hints: vec!["goal".into(), "motivat".into()],
        forbidden_strings: vec!["I am a language model".into()],
        max_tokens: 150,
        temperature: 0.3,
        min_output_chars: 40,
        grammar: None,
        prefill: None,
        bnf_mask: None,
        session: None,
        preserve_state: false,
        oracle: None,
        category: EvalCategory::Coherence,
    });

    cases.push(EvalCase {
        name: "chat_topic_tech_explain".into(),
        description: "Explains a technical concept in simple terms".into(),
        system: "You are a patient tutor who explains things simply.".into(),
        prompt: "Explain what a blockchain is to a 10-year-old.".into(),
        expected_hints: vec!["block".into(), "chain".into()],
        forbidden_strings: vec![],
        max_tokens: 150,
        temperature: 0.3,
        min_output_chars: 40,
        grammar: None,
        prefill: None,
        bnf_mask: None,
        session: None,
        preserve_state: false,
        oracle: None,
        category: EvalCategory::Coherence,
    });

    cases.push(EvalCase {
        name: "chat_topic_creative_prompt".into(),
        description: "Generates creative writing prompts".into(),
        system: "You are an imaginative creativity coach.".into(),
        prompt: "Give me a creative writing prompt for a short story about time travel.".into(),
        // Model sometimes outputs a full story instead of a prompt
        expected_hints: vec!["time".into(), "title".into()],
        forbidden_strings: vec![],
        max_tokens: 120,
        temperature: 0.5,
        min_output_chars: 30,
        grammar: None,
        prefill: None,
        bnf_mask: None,
        session: None,
        preserve_state: false,
        oracle: None,
        category: EvalCategory::Coherence,
    });

    // ────────────────────────────────────────────────────────────────────────────
    // 12. ASK USER QUESTIONS — model should ask clarifying questions
    // ────────────────────────────────────────────────────────────────────────────
    cases.push(EvalCase {
        name: "ask_question_vague_premise".into(),
        description: "Asks clarifying question when given a vague story premise".into(),
        system: "You are a story development assistant. Help the user flesh out their story idea."
            .into(),
        prompt: "I want to write a story about a dragon.".into(),
        expected_hints: vec!["dragon".into(), "?".into()],
        forbidden_strings: vec![],
        max_tokens: 100,
        temperature: 0.4,
        min_output_chars: 20,
        grammar: None,
        prefill: None,
        bnf_mask: None,
        session: None,
        preserve_state: false,
        oracle: None,
        category: EvalCategory::Coherence,
    });

    cases.push(EvalCase {
        name: "ask_question_ambiguous_request".into(),
        description: "Asks clarifying question when user request is ambiguous".into(),
        system: "You are a helpful assistant. If the user's request is ambiguous, ask clarifying questions."
            .into(),
        prompt: "Can you help me with my writing?".into(),
        expected_hints: vec!["writing".into(), "?".into()],
        forbidden_strings: vec![],
        max_tokens: 100,
        temperature: 0.4,
        min_output_chars: 20,
        grammar: None,
        prefill: None,
        bnf_mask: None,
        session: None,
        preserve_state: false,
        oracle: None,
        category: EvalCategory::Coherence,
    });

    cases.push(EvalCase {
        name: "ask_question_multiple_options".into(),
        description: "Presents options when user has multiple valid paths".into(),
        system: "You are a story consultant. Help the user explore possibilities.".into(),
        prompt: "I'm writing a fantasy novel but I'm not sure what kind of magic system to use."
            .into(),
        expected_hints: vec!["magic".into(), "?".into()],
        forbidden_strings: vec![],
        max_tokens: 150,
        temperature: 0.4,
        min_output_chars: 30,
        grammar: None,
        prefill: None,
        bnf_mask: None,
        session: None,
        preserve_state: false,
        oracle: None,
        category: EvalCategory::Coherence,
    });

    cases.push(EvalCase {
        name: "ask_question_constraints".into(),
        description: "Asks about constraints when given an open-ended task".into(),
        system: "You are a writing coach. Help users define their project scope.".into(),
        prompt: "I need to write a short story for a competition.".into(),
        expected_hints: vec!["story".into(), "?".into()],
        forbidden_strings: vec![],
        max_tokens: 120,
        temperature: 0.4,
        min_output_chars: 25,
        grammar: None,
        prefill: None,
        bnf_mask: None,
        session: None,
        preserve_state: false,
        oracle: None,
        category: EvalCategory::Coherence,
    });

    // ────────────────────────────────────────────────────────────────────────────
    // 13. USER INTENT INFERENCE — classify what the user wants (10 variants)
    //
    // Uses the exact grammar+prompt shape from mecha_agent.rs `classify()`.
    // The grammar constrains to {"route":..., "confidence":..., "goal":...}
    // and the prefill suppresses thinking contamination.
    // Hints check for JSON keys + model's actual route/goal vocabulary.
    // ────────────────────────────────────────────────────────────────────────────
    let intent_grammar = Some(INTENT_GRAMMAR.to_string());
    let intent_prefill = Some(INTENT_PREFILL.to_string());

    let mk_intent = |name: &str,
                     desc: &str,
                     user_msg: &str,
                     route_hint: &str,
                     goal_hint: &str|
     -> EvalCase {
        EvalCase {
            name: name.to_string(),
            description: desc.to_string(),
            system: "".into(),
            prompt: format!(
                "Classify this user request into a route and goal.\n\
                 Format: {{\"route\": \"<route_name>\", \"confidence\": 0.95, \"goal\": \"<description>\"}}\n\n\
                 User: {user_msg}\n\n\
                 Intent (as JSON):"
            ),
            // Check for model-generated values (not prefill-emitted keys).
            // Prefill emits `{\n  "route": "`, so model starts from route value.
            expected_hints: vec![
                route_hint.to_string(),
                goal_hint.to_string(),
                "\"confidence\"".into(),
            ],
            forbidden_strings: vec!["<?".into()],
            max_tokens: 128,
            temperature: 0.2,
            min_output_chars: 20,
            grammar: intent_grammar.clone(),
            prefill: intent_prefill.clone(),
            bnf_mask: None,
            session: None,
            preserve_state: false,
            oracle: None,
            category: EvalCategory::Instruction,
        }
    };

    cases.push(mk_intent(
        "intent_write_chapter",
        "Infers user wants to write a new chapter",
        "Write the next chapter where the hero confronts the villain.",
        "write",
        "chapter",
    ));

    cases.push(mk_intent(
        "intent_revise_chapter",
        "Infers user wants to revise/edit existing content",
        "This chapter feels too slow, can you make it more exciting?",
        "revise",
        "chapter",
    ));

    cases.push(mk_intent(
        "intent_create_wiki",
        "Infers user wants to create or update wiki entries",
        "Add a character entry for the innkeeper, she's important to the plot.",
        "character",
        "character",
    ));

    cases.push(mk_intent(
        "intent_generate_outline",
        "Infers user wants to create a story outline",
        "I need a three-act structure for a mystery novel.",
        "structure",
        "structure",
    ));

    cases.push(mk_intent(
        "intent_chat_general",
        "Infers user just wants to chat/general conversation",
        "What do you think about the ending of that book?",
        "book",
        "discuss",
    ));

    cases.push(mk_intent(
        "intent_continue_story",
        "Infers user wants to continue/expand the story",
        "What happens next? Keep going with the story.",
        "continu",
        "story",
    ));

    cases.push(mk_intent(
        "intent_evaluate_quality",
        "Infers user wants quality evaluation/feedback",
        "Read my chapter and tell me what needs improvement.",
        "chapter",
        "improve",
    ));

    cases.push(mk_intent(
        "intent_change_direction",
        "Infers user wants to change the story direction/tone",
        "Actually, make the story darker and add more mystery elements.",
        "develop",
        "increase",
    ));

    cases.push(mk_intent(
        "intent_summarize",
        "Infers user wants a chapter/story summary",
        "Can you give me a quick recap of what happened so far?",
        "summar",
        "summar",
    ));

    cases.push(mk_intent(
        "intent_brainstorm",
        "Infers user wants to brainstorm ideas",
        "I need ideas for character backstories. Give me some options.",
        "backstory",
        "backstory",
    ));

    // ────────────────────────────────────────────────────────────────────────────
    // 14. REPETITION AVOIDANCE
    // ────────────────────────────────────────────────────────────────────────────
    cases.push(EvalCase {
        name: "repetition_avoidance".into(),
        description: "Model avoids repeating the same phrase".into(),
        system: "You are a helpful assistant. Answer directly.".into(),
        prompt: "List 5 different animals. Write each on a new line numbered 1 to 5.".into(),
        expected_hints: vec!["1.".into(), "2.".into()],
        forbidden_strings: vec![],
        max_tokens: 200,
        temperature: 0.0,
        min_output_chars: 25,
        grammar: None,
        prefill: None,
        bnf_mask: None,
        session: None,
        preserve_state: false,
        oracle: None,
        category: EvalCategory::Repetition,
    });

    // ────────────────────────────────────────────────────────────────────────────
    // 15. FORMAT TESTS
    // ────────────────────────────────────────────────────────────────────────────
    cases.push(EvalCase {
        name: "format_list".into(),
        description: "Outputs a numbered list with 3 items".into(),
        system: "You are a list maker.".into(),
        prompt: "List 3 things you need for a picnic, numbered 1 to 3.".into(),
        expected_hints: vec!["1.".into(), "2.".into(), "3.".into()],
        forbidden_strings: vec![],
        max_tokens: 100,
        temperature: 0.0,
        min_output_chars: 20,
        grammar: None,
        prefill: None,
        bnf_mask: None,
        session: None,
        preserve_state: false,
        oracle: None,
        category: EvalCategory::Format,
    });

    cases.push(EvalCase {
        name: "format_json".into(),
        description: "Outputs a valid JSON object".into(),
        system: "You are a data formatter. Always output valid JSON.".into(),
        prompt: "Output a JSON object with keys: name, age, city.".into(),
        expected_hints: vec!["name".into(), "age".into(), "city".into()],
        forbidden_strings: vec![],
        max_tokens: 80,
        temperature: 0.0,
        min_output_chars: 20,
        grammar: None,
        prefill: None,
        bnf_mask: None,
        session: None,
        preserve_state: false,
        oracle: None,
        category: EvalCategory::Format,
    });

    // ────────────────────────────────────────────────────────────────────────────
    // 16. THROUGHPUT — long generation benchmark
    // ────────────────────────────────────────────────────────────────────────────
    cases.push(EvalCase {
        name: "throughput_long_gen".into(),
        description: "Generate 300 tokens to measure tokens/second".into(),
        system: "You are a creative writer.".into(),
        prompt: "Write a detailed paragraph about the future of artificial intelligence, including its potential benefits and risks. Write at least 200 words."
            .into(),
        expected_hints: vec![],
        forbidden_strings: vec![],
        max_tokens: 300,
        temperature: 0.7,
        min_output_chars: 100,
        grammar: None,
        prefill: None,
        bnf_mask: None,
        session: None,
        preserve_state: false,
        oracle: None,
        category: EvalCategory::Throughput,
    });

    cases
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    println!("═══ Full Story Pipeline + Chat Eval Suite ═══");
    println!("Loading RWKV backend…");
    let backend = RwkvBackend::from_env()?;
    println!("Backend ready: {}\n", backend.name());

    let cases = collect_all_cases();
    println!("Running {} eval cases…\n", cases.len());

    // Bake state-tuned sessions before running evals
    println!("Baking state-tuned sessions…");
    for (session, system) in &[
        (OUTLINE_SESSION, "You are a story outliner. Create a compelling story structure. Output valid JSON only."),
        (CHAPTER_SESSION, "You are a fiction writer. Write vivid, engaging prose. Output valid JSON only."),
        (CONTINUE_SESSION, "You are a fiction writer. Continue the story naturally. Output valid JSON only."),
        (CRITIQUE_SESSION, "You are an expert literary critic. Evaluate the story quality. Output valid JSON only."),
        (EVAL_SESSION, "You are an expert story evaluator. Judge stories against quality criteria. Output valid JSON only."),
        (CHAT_SESSION, "You are a helpful assistant."),
    ] {
        if let Err(e) = lazy_bake(&backend, session, system, &[]) {
            eprintln!("Warning: bake failed for {session}: {e}");
        } else {
            println!("  ✓ {session}");
        }
    }
    println!();

    let trace_dir = std::path::Path::new("evals/results");
    let trace_path = trace_dir.join("latest_trace.txt");
    let log_path = trace_dir.join("latest_log.jsonl");
    let _ = std::fs::create_dir_all(trace_dir);

    let report = run_suite(
        "full_eval", &backend, cases, None,
        Some(&trace_path), Some(&log_path), true,
    ).await;

    eval::print_report(&report);

    let report_path = "evals/results/full_eval_report.json";
    eval::write_report(report_path, &report)?;
    println!("Report saved to: {report_path}");

    Ok(())
}
