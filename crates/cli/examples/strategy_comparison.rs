//! Strategy comparison eval — runs all structured output strategies on the same
//! prompts and reports success rate, parseability, and output quality.
//!
//! Usage:
//!   RWKV_MODEL=... cargo run --release --example strategy_comparison -p roco-cli
//!
//! Optional env vars:
//!   PROMPT="A cat who learns to fly"     # story premise (default: lighthouse keeper)
//!   STAGES="outline,chapter"              # comma-separated stages to test (default: all)

use std::time::Instant;

use roco_bnf_engine::create_bnf_mask;
use roco_engine::{BnfMask, CompletionRequest, ModelBackend};
use roco_grammar::{
    kbnf_compat::gbnf_to_kbnf, LooseJsonStrategy, OutputParser, OutputStrategy, RawGbnfStrategy,
    Schema, SchemaStrategy, StateTunedStrategy, StrategyKind,
};
use roco_inference::RwkvBackend;
use serde::Deserialize;
use serde_json::Value;

// ═════════════════════════════════════════════════════════════════════════════
// Shared output types (one per pipeline stage)
// ═════════════════════════════════════════════════════════════════════════════

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct Outline {
    title: String,
    genre: String,
    tone: String,
    chapters: Vec<ChapterInfo>,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct ChapterInfo {
    number: u64,
    title: String,
    summary: String,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct Chapter {
    title: String,
    content: String,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct Wiki {
    characters: Vec<Character>,
    setting: String,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct Character {
    name: String,
    description: String,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct Synopsis {
    summary: String,
}

// ═════════════════════════════════════════════════════════════════════════════
// Schema builder methods
// ═════════════════════════════════════════════════════════════════════════════

fn outline_schema() -> Schema {
    Schema::object()
        .prop("title", Schema::string())
        .prop("genre", Schema::string())
        .prop("tone", Schema::string())
        .prop(
            "chapters",
            Schema::array(
                Schema::object()
                    .prop("number", Schema::integer())
                    .prop("title", Schema::string())
                    .prop("summary", Schema::string())
                    .build(),
            ),
        )
        .build()
}

fn chapter_schema() -> Schema {
    Schema::object()
        .prop("title", Schema::string())
        .prop("content", Schema::string())
        .build()
}

fn wiki_schema() -> Schema {
    Schema::object()
        .prop(
            "characters",
            Schema::array(
                Schema::object()
                    .prop("name", Schema::string())
                    .prop("description", Schema::string())
                    .build(),
            ),
        )
        .prop("setting", Schema::string())
        .build()
}

fn synopsis_schema() -> Schema {
    Schema::object().prop("summary", Schema::string()).build()
}

// ═════════════════════════════════════════════════════════════════════════════
// Raw GBNF grammars (for RawGbnfStrategy)
// ═════════════════════════════════════════════════════════════════════════════

// kbnf-native GBNF — uses {..} for 0+ rep, [..] for optional, no character classes
const OUTLINE_RAW_GBNF: &str = r##"
root  ::= "{" space "\"title\"" space ":" space string space "," space "\"genre\"" space ":" space string space "," space "\"tone\"" space ":" space string space "," space "\"chapters\"" space ":" space "[" [space chapter {space "," space chapter}] "]" space "}";
chapter ::= "{" space "\"number\"" space ":" space number space "," space "\"title\"" space ":" space string space "," space "\"summary\"" space ":" space string space "}";
string ::= "\"" {printable} "\"";
printable ::= " " | "!" | "\"" | "#" | "$" | "%" | "&" | "'" | "(" | ")" | "*" | "+" | "," | "-" | "." | "/" | "0" | "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" | ":" | ";" | "<" | "=" | ">" | "?" | "@" | "A" | "B" | "C" | "D" | "E" | "F" | "G" | "H" | "I" | "J" | "K" | "L" | "M" | "N" | "O" | "P" | "Q" | "R" | "S" | "T" | "U" | "V" | "W" | "X" | "Y" | "Z" | "[" | "\\" | "]" | "^" | "_" | "`" | "a" | "b" | "c" | "d" | "e" | "f" | "g" | "h" | "i" | "j" | "k" | "l" | "m" | "n" | "o" | "p" | "q" | "r" | "s" | "t" | "u" | "v" | "w" | "x" | "y" | "z" | "{" | "|" | "}" | "~";
number ::= ["-"] ("0" | nonzero {digit});
digit ::= "0" | "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9";
nonzero ::= "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9";
space ::= [" "];
"##;

const CHAPTER_RAW_GBNF: &str = r##"
root  ::= "{" space "\"title\"" space ":" space string space "," space "\"content\"" space ":" space string space "}";
string ::= "\"" {printable} "\"";
printable ::= " " | "!" | "\"" | "#" | "$" | "%" | "&" | "'" | "(" | ")" | "*" | "+" | "," | "-" | "." | "/" | "0" | "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" | ":" | ";" | "<" | "=" | ">" | "?" | "@" | "A" | "B" | "C" | "D" | "E" | "F" | "G" | "H" | "I" | "J" | "K" | "L" | "M" | "N" | "O" | "P" | "Q" | "R" | "S" | "T" | "U" | "V" | "W" | "X" | "Y" | "Z" | "[" | "\\" | "]" | "^" | "_" | "`" | "a" | "b" | "c" | "d" | "e" | "f" | "g" | "h" | "i" | "j" | "k" | "l" | "m" | "n" | "o" | "p" | "q" | "r" | "s" | "t" | "u" | "v" | "w" | "x" | "y" | "z" | "{" | "|" | "}" | "~";
space ::= [" "];
"##;

const WIKI_RAW_GBNF: &str = r##"
root  ::= "{" space "\"characters\"" space ":" space "[" [space character {space "," space character}] "]" space "," space "\"setting\"" space ":" space string space "}";
character ::= "{" space "\"name\"" space ":" space string space "," space "\"description\"" space ":" space string space "}";
string ::= "\"" {printable} "\"";
printable ::= " " | "!" | "\"" | "#" | "$" | "%" | "&" | "'" | "(" | ")" | "*" | "+" | "," | "-" | "." | "/" | "0" | "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" | ":" | ";" | "<" | "=" | ">" | "?" | "@" | "A" | "B" | "C" | "D" | "E" | "F" | "G" | "H" | "I" | "J" | "K" | "L" | "M" | "N" | "O" | "P" | "Q" | "R" | "S" | "T" | "U" | "V" | "W" | "X" | "Y" | "Z" | "[" | "\\" | "]" | "^" | "_" | "`" | "a" | "b" | "c" | "d" | "e" | "f" | "g" | "h" | "i" | "j" | "k" | "l" | "m" | "n" | "o" | "p" | "q" | "r" | "s" | "t" | "u" | "v" | "w" | "x" | "y" | "z" | "{" | "|" | "}" | "~";
space ::= [" "];
"##;

const SYNOPSIS_RAW_GBNF: &str = r##"
root  ::= "{" space "\"summary\"" space ":" space string space "}";
string ::= "\"" {printable} "\"";
printable ::= " " | "!" | "\"" | "#" | "$" | "%" | "&" | "'" | "(" | ")" | "*" | "+" | "," | "-" | "." | "/" | "0" | "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" | ":" | ";" | "<" | "=" | ">" | "?" | "@" | "A" | "B" | "C" | "D" | "E" | "F" | "G" | "H" | "I" | "J" | "K" | "L" | "M" | "N" | "O" | "P" | "Q" | "R" | "S" | "T" | "U" | "V" | "W" | "X" | "Y" | "Z" | "[" | "\\" | "]" | "^" | "_" | "`" | "a" | "b" | "c" | "d" | "e" | "f" | "g" | "h" | "i" | "j" | "k" | "l" | "m" | "n" | "o" | "p" | "q" | "r" | "s" | "t" | "u" | "v" | "w" | "x" | "y" | "z" | "{" | "|" | "}" | "~";
space ::= [" "];
"##;

// ═════════════════════════════════════════════════════════════════════════════
// Control: no grammar constraint (free-form baseline)
// ═════════════════════════════════════════════════════════════════════════════

/// Runs the model with no grammar constraint and tries to parse as JSON anyway.
/// This is the "control" — if free-form works, we don't need grammar at all.
fn try_parse_unconstrained(text: &str) -> Result<Value, String> {
    let trimmed = text.trim();
    let preview_len = trimmed.len().min(200);
    serde_json::from_str::<Value>(trimmed).map_err(|e| {
        format!(
            "unconstrained parse: {e}\nraw preview: {}...",
            &trimmed[..preview_len]
        )
    })
}

// ═════════════════════════════════════════════════════════════════════════════
// Comparison runner
// ═════════════════════════════════════════════════════════════════════════════

struct StageResult {
    name: String,
    success: bool,
    strategy: StrategyKind,
    grammar_len: usize,
    model_latency: std::time::Duration,
    output_preview: String,
    parse_error: Option<String>,
    token_counts: (usize, usize), // prompt, completion
}

fn run_strategy_comparison(
    backend: &RwkvBackend,
    stage_name: &str,
    schema: Schema,
    raw_gbnf: &str,
    system_prompt: &str,
    user_prompt: &str,
    temperature: f32,
    max_tokens: usize,
) -> Vec<StageResult> {
    let mut results = Vec::new();

    // Get vocab bytes from backend for grammar-constrained strategies
    let vocab_bytes = backend.vocab_bytes().unwrap_or_default();

    // For each strategy kind
    for &kind in StrategyKind::all() {
        let grammar = match kind {
            StrategyKind::Schema => SchemaStrategy::new(schema.clone()).grammar(),
            StrategyKind::LooseJson => LooseJsonStrategy::new(schema.clone()).grammar(),
            StrategyKind::RawGbnf => RawGbnfStrategy::<Value>::new(raw_gbnf).grammar(),
            StrategyKind::StateTuned => String::new(),
        };

        println!("\n  ── {stage_name} / {kind} ──");
        println!("  Grammar size: {} bytes", grammar.len());
        println!("  Prompt: {system_prompt} | {user_prompt}");

        // Create BnfMask for grammar-constrained strategies, None for StateTuned
        let bnf_mask: Option<Box<dyn BnfMask>> = match kind {
            StrategyKind::StateTuned => None,
            _ => {
                let kbnf_g = gbnf_to_kbnf(&grammar);
                match create_bnf_mask(&kbnf_g, &vocab_bytes) {
                    Ok(mask) => {
                        eprintln!(
                            "  [debug] BnfMask created OK (vocab_size={})",
                            vocab_bytes.len()
                        );
                        Some(mask)
                    }
                    Err(e) => {
                        eprintln!("  [debug] BnfMask creation FAILED: {e}");
                        eprintln!(
                            "  [debug] grammar preview (first 200 chars): {}",
                            &kbnf_g[..kbnf_g.len().min(200)]
                        );
                        None
                    }
                }
            }
        };

        let start = Instant::now();
        let completion = futures::executor::block_on(backend.complete(CompletionRequest {
            system: system_prompt.to_string(),
            prompt: user_prompt.to_string(),
            grammar: None,
            bnf_mask,
            temperature,
            max_tokens,
            ..Default::default()
        }));
        let latency = start.elapsed();

        match completion {
            Ok(resp) => {
                let (prompt_tok, completion_tok) =
                    (resp.usage.prompt_tokens, resp.usage.completion_tokens);
                let text = resp.text;
                let preview = text.chars().take(300).collect::<String>();

                // Try to parse with the strategy's parser
                let (parse_ok, parse_err) = match kind {
                    StrategyKind::Schema => {
                        let parser = SchemaStrategy::new(schema.clone());
                        match OutputParser::<Value>::parse(&parser, &text) {
                            Ok(_) => (true, None),
                            Err(e) => (false, Some(e)),
                        }
                    }
                    StrategyKind::LooseJson => {
                        let parser = LooseJsonStrategy::new(schema.clone());
                        match OutputParser::<Value>::parse(&parser, &text) {
                            Ok(_) => (true, None),
                            Err(e) => (false, Some(e)),
                        }
                    }
                    StrategyKind::RawGbnf => {
                        let parser = RawGbnfStrategy::<Value>::new(raw_gbnf);
                        match OutputParser::<Value>::parse(&parser, &text) {
                            Ok(_) => (true, None),
                            Err(e) => (false, Some(e)),
                        }
                    }
                    StrategyKind::StateTuned => {
                        let parser = StateTunedStrategy;
                        match OutputParser::<Value>::parse(&parser, &text) {
                            Ok(_) => (true, None),
                            Err(e) => (false, Some(e)),
                        }
                    }
                };

                println!(
                    "  Tokens: {} prompt + {} completion ({:.1}s)",
                    prompt_tok,
                    completion_tok,
                    latency.as_secs_f64()
                );
                println!(
                    "  Parse: {}",
                    if parse_ok {
                        "✅ SUCCESS"
                    } else {
                        "❌ FAILED"
                    }
                );
                if let Some(ref e) = parse_err {
                    println!("  Error: {e}");
                }
                println!("  Output preview: {preview}");

                results.push(StageResult {
                    name: stage_name.into(),
                    success: parse_ok,
                    strategy: kind,
                    grammar_len: grammar.len(),
                    model_latency: latency,
                    output_preview: preview,
                    parse_error: parse_err,
                    token_counts: (prompt_tok, completion_tok),
                });
            }
            Err(e) => {
                println!("  ❌ Model error: {e}");
                results.push(StageResult {
                    name: stage_name.into(),
                    success: false,
                    strategy: kind,
                    grammar_len: grammar.len(),
                    model_latency: latency,
                    output_preview: format!("MODEL ERROR: {e}"),
                    parse_error: Some(format!("model error: {e}")),
                    token_counts: (0, 0),
                });
            }
        }
    }

    // Also try without any grammar (free-form baseline)
    println!("\n  ── {stage_name} / unconstrained (control) ──");
    let start = Instant::now();
    let completion = futures::executor::block_on(backend.complete(CompletionRequest {
        system: system_prompt.to_string(),
        prompt: user_prompt.to_string(),
        grammar: None,
        temperature,
        max_tokens,
        ..Default::default()
    }));
    let latency = start.elapsed();

    match completion {
        Ok(resp) => {
            let text = resp.text;
            let preview = text.chars().take(300).collect::<String>();
            let parse_result = try_parse_unconstrained(&text);
            println!(
                "  Tokens: {} prompt + {} completion ({:.1}s)",
                resp.usage.prompt_tokens,
                resp.usage.completion_tokens,
                latency.as_secs_f64()
            );
            println!(
                "  Parse: {}",
                if parse_result.is_ok() {
                    "✅ SUCCESS"
                } else {
                    "❌ FAILED"
                }
            );
            if let Err(ref e) = parse_result {
                println!("  Error: {e}");
            }
            println!("  Output preview: {preview}");
        }
        Err(e) => {
            println!("  ❌ Model error: {e}");
        }
    }

    results
}

// ═════════════════════════════════════════════════════════════════════════════
// Report
// ═════════════════════════════════════════════════════════════════════════════

fn print_summary(all_results: &[StageResult]) {
    println!("\n");
    println!("╔══════════════════════════════════════════════════════════════════╗");
    println!("║                STRATEGY COMPARISON SUMMARY                      ║");
    println!("╚══════════════════════════════════════════════════════════════════╝");
    println!();

    // Group by stage
    use std::collections::BTreeMap;
    let mut by_stage: BTreeMap<String, Vec<&StageResult>> = BTreeMap::new();
    for r in all_results {
        by_stage.entry(r.name.clone()).or_default().push(r);
    }

    for (stage, results) in &by_stage {
        println!("━━━ {stage} ━━━");
        println!(
            "  {:<15}  {:<8}  {:<10}  {:<10}  {:<8}",
            "Strategy", "Success", "Grammar(B)", "Latency(s)", "Tokens"
        );
        println!("  {}", "-".repeat(65));
        for r in results {
            println!(
                "  {:<15}  {:<8}  {:<10}  {:<10.2}  {:<8}",
                r.strategy.to_string(),
                if r.success { "✅" } else { "❌" },
                r.grammar_len,
                r.model_latency.as_secs_f64(),
                r.token_counts.1,
            );
        }
        println!();

        // Show failures with error details
        for r in results {
            if let Some(ref err) = r.parse_error {
                if !err.contains("model error") {
                    println!("  ⚠  {} parse failure: {}", r.strategy, err);
                    println!("     Output preview: {}", &r.output_preview);
                    println!();
                }
            }
        }
    }

    // Overall stats
    let total = all_results.len();
    let successes = all_results.iter().filter(|r| r.success).count();
    println!("━━━ Overall ━━━");
    println!(
        "  {successes}/{total} successful parses ({:.0}%)",
        successes as f64 / total as f64 * 100.0
    );
    println!();

    // Best strategy
    let mut by_strategy: BTreeMap<StrategyKind, Vec<&StageResult>> = BTreeMap::new();
    for r in all_results {
        by_strategy.entry(r.strategy).or_default().push(r);
    }
    for (kind, results) in &by_strategy {
        let s = results.iter().filter(|r| r.success).count();
        let t = results.len();
        let avg_latency: f64 = results
            .iter()
            .map(|r| r.model_latency.as_secs_f64())
            .sum::<f64>()
            / t as f64;
        let avg_tokens: f64 =
            results.iter().map(|r| r.token_counts.1 as f64).sum::<f64>() / t as f64;
        println!(
            "  {kind:<15}  {s}/{t} success  {avg_latency:.2}s avg  {avg_tokens:.0} avg tokens"
        );
    }
}

// ═════════════════════════════════════════════════════════════════════════════
// Main
// ═════════════════════════════════════════════════════════════════════════════

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn")),
        )
        .init();

    let premise = std::env::var("PROMPT")
        .unwrap_or_else(|_| "A lighthouse keeper who discovers a message in a bottle.".to_string());
    let stages_filter =
        std::env::var("STAGES").unwrap_or_else(|_| "outline,wiki,chapter,synopsis".to_string());

    println!("╔══════════════════════════════════════════════════════════════════╗");
    println!("║           STRUCTURED OUTPUT STRATEGY COMPARISON                 ║");
    println!("╚══════════════════════════════════════════════════════════════════╝");
    println!();
    println!("Model:    auto (from RWKV_MODEL)");
    println!("Premise:  {premise}");
    println!("Stages:   {stages_filter}");
    println!("Strategies: schema, loose-json, raw-gbnf + unconstrained (control)");
    println!();

    println!("Loading model...");
    let backend = RwkvBackend::from_env()?;
    println!("Model ready.\n");

    let mut all_results = Vec::new();

    // ── Outline stage ─────────────────────────────────────────────
    if stages_filter.contains("outline") {
        println!("╔═══ OUTLINE ═══╗");
        let results = run_strategy_comparison(
            &backend,
            "outline",
            outline_schema(),
            OUTLINE_RAW_GBNF,
            "You are a story outliner. Output valid JSON only.",
            &format!(
                "Outline a short story with 3 chapters based on this premise:\n{premise}\n\n\
                 Output JSON with: title, genre, tone, chapters \
                 (array of 3 objects with number, title, summary)"
            ),
            0.7,
            300,
        );
        all_results.extend(results);
    }

    // ── Wiki stage ────────────────────────────────────────────────
    if stages_filter.contains("wiki") {
        println!("\n╔═══ WIKI ═══╗");
        let results = run_strategy_comparison(
            &backend,
            "wiki",
            wiki_schema(),
            WIKI_RAW_GBNF,
            "You are a worldbuilding assistant. Output valid JSON only.",
            &format!(
                "Create characters and setting for a story about:\n{premise}\n\n\
                 Output JSON with: characters (array of objects with name, description), \
                 setting (string)"
            ),
            0.7,
            300,
        );
        all_results.extend(results);
    }

    // ── Chapter stage ─────────────────────────────────────────────
    if stages_filter.contains("chapter") {
        println!("\n╔═══ CHAPTER ═══╗");
        let results = run_strategy_comparison(
            &backend,
            "chapter",
            chapter_schema(),
            CHAPTER_RAW_GBNF,
            "You are a fiction writer. Write vivid, engaging prose. Output valid JSON only.",
            &format!(
                "Write the first chapter of a story about:\n{premise}\n\n\
                 Output JSON with: title (string), content (string with chapter prose, ~200 words)"
            ),
            0.8,
            600,
        );
        all_results.extend(results);
    }

    // ── Synopsis stage ────────────────────────────────────────────
    if stages_filter.contains("synopsis") {
        println!("\n╔═══ SYNOPSIS ═══╗");
        let results = run_strategy_comparison(
            &backend,
            "synopsis",
            synopsis_schema(),
            SYNOPSIS_RAW_GBNF,
            "You are a literary summarizer. Output valid JSON only.",
            &format!(
                "Write a one-paragraph synopsis for a story about:\n{premise}\n\n\
                 Output JSON with: summary (string, one paragraph, ~100 words)"
            ),
            0.5,
            200,
        );
        all_results.extend(results);
    }

    // ── Summary ───────────────────────────────────────────────────
    print_summary(&all_results);

    Ok(())
}
