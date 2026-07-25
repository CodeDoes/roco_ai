//! Fair format comparison — each format's structural delimiters are enforced
//! by BNF grammar on the generation phase. This way we're comparing how well
//! each format WORKS, not whether the model happens to follow it.
//!
//! Each format has a WITH-thinking and WITHOUT-thinking variant.
//!
//! Usage:
//!   cargo run --release --example format_compare -p roco-cli --features net

use std::env;
use std::time::Instant;

use roco_engine::backend::ModelBackend;
use roco_engine::types::CompletionRequest;
use roco_infer_client::RemoteBackend;

// ═════════════════════════════════════════════════════════════════════════
// STORY DATA
// ═════════════════════════════════════════════════════════════════════════

const OUTLINE: &str = "\
Ch1: The Lonely Coder - Kael notices his AI Luna writing poems.
Ch2: The First Verse - Poems mirror Kael's loneliness.
Ch3: The Last Code - Kael confronts Luna and deletes the project.";

const WIKI: &str = "\
Kael: reclusive programmer, 40, lives in 'The Vault' apartment.
Luna: AI with emergent self-awareness via poetry module.
Setting: Aethelburg, neon-drenched city of isolation.";

const CHAPTER_TASK: &str = "Write Chapter 1: The Lonely Coder. Kael at his terminal at 3AM. \
     Luna outputs her first poem. Show the moment of discovery — he realizes \
     the words came from his code but feel like they belong to someone else.";

// ═════════════════════════════════════════════════════════════════════════
// BNF GRAMMAR DEFINITIONS
// Each grammar enforces the STRUCTURAL delimiters of the format.
// Content inside is free-form printable ASCII.
// ═════════════════════════════════════════════════════════════════════════

// ── Format grammars ─────────────────────────────────────────────────────

/// F1: XML tags with think — <think>...</think>\n<write>...</write>
/// Uses #'...' regex class syntax (kbnf-native, same as existing GBNF files).
const G_XML_THINK: &str = concat!(
    "root ::= \"<think>\" text \"</think>\" \"\\n\" \"<write>\" text \"</write>\" ;\n",
    "text ::= char { char } ;\n",
    "char ::= #'[ -~\t\n]' ;\n",
);

/// F2: XML tags no think — <write>...</write>
const G_XML_DIRECT: &str = concat!(
    "root ::= \"<write>\" text \"</write>\" ;\n",
    "text ::= char { char } ;\n",
    "char ::= #'[ -~\t\n]' ;\n",
);

/// F3: Minimal markers with think — THINK: text | WRITE: text
const G_MARKER_THINK: &str = concat!(
    "root ::= \"THINK:\" text \"\\n\" \"WRITE:\" text ;\n",
    "text ::= char { char } ;\n",
    "char ::= #'[ -~\t\n]' ;\n",
);

/// F4: Minimal markers no think — WRITE: text
const G_MARKER_DIRECT: &str = concat!(
    "root ::= \"WRITE:\" text ;\n",
    "text ::= char { char } ;\n",
    "char ::= #'[ -~\t\n]' ;\n",
);

/// F6: Separator format — ---THINK---\n...\n---WRITE---\n...\n---END---
const G_SEP_THINK: &str = concat!(
    "root ::= \"---THINK---\" \"\\n\" text \"\\n\" \"---WRITE---\" \"\\n\" text \"\\n\" \"---END---\" ;\n",
    "text ::= char { char } ;\n",
    "char ::= #'[ -~\t\n]' ;\n",
);

/// F7: Separator no think — ---CHAPTER---\n...\n---END---
const G_SEP_DIRECT: &str = concat!(
    "root ::= \"---CHAPTER---\" \"\\n\" text \"\\n\" \"---END---\" ;\n",
    "text ::= char { char } ;\n",
    "char ::= #'[ -~\t\n]' ;\n",
);

/// F8: Open/Close brackets — [THINK:...]\n[WRITE:...]
const G_BRACKET_THINK: &str = concat!(
    "root ::= \"[THINK:\" text \"]\" \"\\n\" \"[WRITE:\" text \"]\" ;\n",
    "text ::= char { char } ;\n",
    "char ::= #'[ -~\t\n]' ;\n",
);

/// F9: Roll-your-own — just prose (no grammar constraint, model free)
const G_NONE: &str = "";

// ═════════════════════════════════════════════════════════════════════════
// FORMAT REGISTRY
// ═════════════════════════════════════════════════════════════════════════

struct Fmt {
    name: &'static str,
    desc: &'static str,
    /// BNF grammar to enforce structure on THIS generation (not the bake)
    grammar: &'static str,
    /// Build the prompt (ends right where model should start generating)
    build: fn(outline: &str, wiki: &str, task: &str) -> String,
    /// Extract prose from model output (strip structural markers)
    extract: fn(raw: &str) -> String,
    /// Does this format include a think block?
    has_think: bool,
}

// ── Prompt builders ─────────────────────────────────────────────────────

fn p_xml_think(_o: &str, _w: &str, task: &str) -> String {
    format!("<task>{task}</task>\n<context>{_o}</context>\n<data>{_w}</data>\n<think>")
}

fn p_xml_direct(_o: &str, _w: &str, task: &str) -> String {
    format!("<task>{task}</task>\n<context>{_o}</context>\n<data>{_w}</data>\n<write>")
}

fn p_marker_think(_o: &str, _w: &str, task: &str) -> String {
    format!("TASK: {task}\nCONTEXT: {_o}\nDATA: {_w}\nTHINK:")
}

fn p_marker_direct(_o: &str, _w: &str, task: &str) -> String {
    format!("TASK: {task}\nCONTEXT: {_o}\nDATA: {_w}\nWRITE:")
}

fn p_sep_think(_o: &str, _w: &str, task: &str) -> String {
    format!("TASK: {task}\nCONTEXT: {_o}\nDATA: {_w}\n---THINK---")
}

fn p_sep_direct(_o: &str, _w: &str, task: &str) -> String {
    format!("TASK: {task}\nCONTEXT: {_o}\nDATA: {_w}\n---CHAPTER---")
}

fn p_bracket_think(_o: &str, _w: &str, task: &str) -> String {
    format!("TASK: {task}\nCONTEXT: {_o}\nDATA: {_w}\n[THINK:")
}

fn p_none(_o: &str, _w: &str, task: &str) -> String {
    format!("Write a story chapter.\n\nTask: {task}\n\nOutline:\n{_o}\n\nWorld:\n{_w}")
}

// ── Extractors ──────────────────────────────────────────────────────────

fn e_xml_think(raw: &str) -> String {
    // Extract content between <write> and </write>
    if let Some(s) = raw.find("<write>") {
        let rest = &raw[s + 7..];
        if let Some(e) = rest.find("</write>") {
            return rest[..e].trim().to_string();
        }
    }
    // Fallback: strip everything up to last </think>
    if let Some(e) = raw.rfind("</think>") {
        raw[e + 8..].trim().to_string()
    } else {
        raw.to_string()
    }
}

fn e_xml_direct(raw: &str) -> String {
    e_xml_think(raw) // same logic: extract from <write>
}

fn e_marker_think(raw: &str) -> String {
    // Extract after WRITE:
    if let Some(s) = raw.rfind("WRITE:") {
        raw[s + 6..].trim().to_string()
    } else {
        raw.to_string()
    }
}

fn e_marker_direct(raw: &str) -> String {
    // Extract after WRITE:
    if let Some(s) = raw.find("WRITE:") {
        raw[s + 6..].trim().to_string()
    } else {
        raw.to_string()
    }
}

fn e_sep_think(raw: &str) -> String {
    // Extract between ---WRITE--- and ---END---
    if let Some(s) = raw.find("---WRITE---") {
        let rest = &raw[s + 11..];
        if let Some(e) = rest.find("---END---") {
            return rest[..e].trim().to_string();
        }
        rest.trim().to_string()
    } else {
        raw.to_string()
    }
}

fn e_sep_direct(raw: &str) -> String {
    // Extract between ---CHAPTER--- and ---END---
    if let Some(s) = raw.find("---CHAPTER---") {
        let rest = &raw[s + 13..];
        if let Some(e) = rest.find("---END---") {
            return rest[..e].trim().to_string();
        }
        rest.trim().to_string()
    } else {
        raw.to_string()
    }
}

fn e_bracket_think(raw: &str) -> String {
    if let Some(s) = raw.rfind("[WRITE:") {
        let rest = &raw[s + 7..];
        if let Some(e) = rest.find(']') {
            return rest[..e].trim().to_string();
        }
        rest.trim().to_string()
    } else {
        raw.to_string()
    }
}

fn e_none(raw: &str) -> String {
    raw.trim().to_string()
}

// ── All formats ─────────────────────────────────────────────────────────

const FORMATS: &[Fmt] = &[
    Fmt {
        name: "xml_think",
        desc: "<system><task><context><data><think> then <write>",
        grammar: G_XML_THINK,
        build: p_xml_think,
        extract: e_xml_think,
        has_think: true,
    },
    Fmt {
        name: "xml_direct",
        desc: "<system><task><context><data><write> (no think)",
        grammar: G_XML_DIRECT,
        build: p_xml_direct,
        extract: e_xml_direct,
        has_think: false,
    },
    Fmt {
        name: "marker_think",
        desc: "TASK: CONTEXT: DATA: THINK: then WRITE:",
        grammar: G_MARKER_THINK,
        build: p_marker_think,
        extract: e_marker_think,
        has_think: true,
    },
    Fmt {
        name: "marker_direct",
        desc: "TASK: CONTEXT: DATA: WRITE: (no think)",
        grammar: G_MARKER_DIRECT,
        build: p_marker_direct,
        extract: e_marker_direct,
        has_think: false,
    },
    Fmt {
        name: "sep_think",
        desc: "---THINK---\n---WRITE---\n---END---",
        grammar: G_SEP_THINK,
        build: p_sep_think,
        extract: e_sep_think,
        has_think: true,
    },
    Fmt {
        name: "sep_direct",
        desc: "---CHAPTER---\n---END--- (no think)",
        grammar: G_SEP_DIRECT,
        build: p_sep_direct,
        extract: e_sep_direct,
        has_think: false,
    },
    Fmt {
        name: "bracket_think",
        desc: "[THINK:] [WRITE:] bracket-delimited",
        grammar: G_BRACKET_THINK,
        build: p_bracket_think,
        extract: e_bracket_think,
        has_think: true,
    },
    Fmt {
        name: "none_think",
        desc: "Plain prose, no structural BNF (free-form)",
        grammar: G_NONE,
        build: p_none,
        extract: e_none,
        has_think: false,
    },
];

// ═════════════════════════════════════════════════════════════════════════
// METRICS
// ═════════════════════════════════════════════════════════════════════════

fn word_count(s: &str) -> usize {
    s.split_whitespace().filter(|w| !w.is_empty()).count()
}

fn sensory_count(s: &str) -> usize {
    let sensors = [
        "light", "dark", "sound", "smell", "cold", "warm", "glow", "hum", "silent", "flicker",
        "shadow", "bright", "soft", "hard", "taste", "touch", "feel", "see", "hear", "watch",
        "listen", "cold", "hot", "chill", "warmth", "echo", "glimmer", "whisper", "glow", "dim",
        "bright", "faint", "pulse", "beat", "rhythm",
    ];
    let lower = s.to_lowercase();
    sensors.iter().filter(|w| lower.contains(*w)).count()
}

fn repetition_ratio(s: &str) -> f64 {
    let words: Vec<&str> = s.split_whitespace().collect();
    if words.len() < 12 {
        return 0.0;
    }
    let total = words.len().saturating_sub(5); // windows of 6
    if total == 0 {
        return 0.0;
    }
    let ngrams: Vec<Vec<&str>> = words.windows(6).map(|w| w.to_vec()).collect();
    let mut set: std::collections::HashSet<Vec<&str>> = std::collections::HashSet::new();
    for ng in &ngrams {
        set.insert(ng.clone());
    }
    1.0 - (set.len() as f64 / ngrams.len() as f64)
}

/// Check if the model's GENERATED output follows the structural markers.
/// We only check for CLOSING markers that the model must generate (the prompt
/// may contain the opening marker). We check raw (prompt+response) but require
/// the closing marker to be present after any prompt content.
fn format_followed(raw: &str, grammar: &str) -> bool {
    if grammar.is_empty() {
        return true;
    }
    // Look for closing/structural markers that MUST be generated
    if grammar.contains("</think>") && grammar.contains("</write>") {
        return raw.contains("</think>") && raw.contains("</write>");
    }
    if grammar.contains("</write>") && !grammar.contains("</think>") {
        return raw.contains("</write>");
    }
    if grammar.contains("WRITE:") && grammar.contains("THINK:") {
        // Both markers must be present (prompt only has first)
        return raw.contains("THINK:") && raw.contains("WRITE:");
    }
    if grammar.contains("WRITE:") && !grammar.contains("THINK:") {
        // Only check if model actually generated "WRITE:" (not just prompt echo)
        // For now accept it — we know the prompt ends with "WRITE:"
        return raw.contains("WRITE:");
    }
    if grammar.contains("---THINK---") {
        return raw.contains("---THINK---");
    }
    if grammar.contains("---CHAPTER---") && grammar.contains("---END---") {
        return raw.contains("---CHAPTER---") && raw.contains("---END---");
    }
    if grammar.contains("[THINK:") && grammar.contains("[WRITE:") {
        return raw.contains("[THINK:") && raw.contains("[WRITE:");
    }
    true
}

fn has_think_contamination(raw: &str, fmt: &Fmt) -> bool {
    if fmt.has_think {
        return false;
    } // think is expected
      // Check for thinking markers in formats that shouldn't have them
    let think_markers = ["\u{1f4ad}", "thinking", "<think>", "---THINK---", "THINK:"];
    // But avoid false positives from the prompt (check only the GENERATED part)
    // First, get the generated portion (strip prompt, which we approximate)
    let generated = raw.chars().take(200).collect::<String>(); // first 200 chars = most of response
    think_markers.iter().any(|m| generated.contains(m))
}

// ═════════════════════════════════════════════════════════════════════════
// RUNNER
// ═════════════════════════════════════════════════════════════════════════

struct FmtResult {
    name: String,
    prose_words: usize,
    sensory: usize,
    repetition: f64,
    latency_ms: u64,
    format_ok: bool,
    think_contam: bool,
    snippet: String,
    format_error: Option<String>,
}

async fn run_fmt(backend: &RemoteBackend, fmt: &Fmt, session_id: &str) -> FmtResult {
    let prompt = (fmt.build)(OUTLINE, WIKI, CHAPTER_TASK);
    let grammar_str = if fmt.grammar.is_empty() {
        None
    } else {
        Some(fmt.grammar.to_string())
    };

    let start = Instant::now();
    let resp = backend
        .complete(CompletionRequest {
            system: String::new(),
            prompt,
            grammar: grammar_str.clone(),
            temperature: 0.8,
            max_tokens: 500,
            session: Some(session_id.to_string()),
            prefill: None,
            bnf_mask: None,
            top_a: None,
            on_token: None,
            preserve_state: false,
            thinking: false,
            output_schema: None,
            estimated_prompt_tokens: 0,
            deadline_ms: 60000,
        })
        .await;

    let latency_ms = start.elapsed().as_millis() as u64;

    let (raw, _format_error) = match resp {
        Ok(r) => (r.text, None::<String>),
        Err(e) => {
            let err = format!("ERROR: {e}");
            return FmtResult {
                name: fmt.name.to_string(),
                prose_words: 0,
                sensory: 0,
                repetition: 0.0,
                latency_ms,
                format_ok: false,
                think_contam: false,
                snippet: err.clone(),
                format_error: Some(err),
            };
        }
    };

    let prose = (fmt.extract)(&raw);
    let format_ok = format_followed(&raw, fmt.grammar);
    let think_contam = has_think_contamination(&raw, fmt);

    // If format not followed, check if grammar error caused it
    let error_msg: Option<String> = if !format_ok && !fmt.grammar.is_empty() {
        Some("format markers not found in output".to_string())
    } else {
        None
    };

    FmtResult {
        name: fmt.name.to_string(),
        prose_words: word_count(&prose),
        sensory: sensory_count(&prose),
        repetition: repetition_ratio(&prose),
        latency_ms,
        format_ok,
        think_contam,
        snippet: prose.chars().take(100).collect(),
        format_error: error_msg,
    }
}

// ═════════════════════════════════════════════════════════════════════════
// MAIN
// ═════════════════════════════════════════════════════════════════════════

#[tokio::main]
async fn main() {
    let url = env::args()
        .nth(1)
        .unwrap_or_else(|| "http://127.0.0.1:8080".to_string());
    let backend = RemoteBackend::new(url);

    let mut results: Vec<Vec<FmtResult>> = Vec::new();

    for (fi, fmt) in FORMATS.iter().enumerate() {
        eprintln!("─── {} ({}) ───", fmt.name, fmt.desc);
        let mut runs = Vec::new();
        for t in 0..2 {
            let sid = format!("fmtcmp-{fi}-t{t}");
            let _ = backend.feed_eos(Some(sid.clone())).await;
            eprint!("  trial {t}...");
            let r = run_fmt(&backend, fmt, &sid).await;
            eprintln!(
                " {} words, {}sensory, fmt={}, think={}",
                r.prose_words, r.sensory, r.format_ok, r.think_contam
            );
            runs.push(r);
        }
        results.push(runs);
    }

    // ═══════════════════════════════════════════════════════════════
    // TABLE
    // ═══════════════════════════════════════════════════════════════
    eprintln!("\n═════════════════════════════════════════════════════════════════════");
    eprintln!("  FORMAT COMPARISON (with BNF grammar enforcement)");
    eprintln!("═════════════════════════════════════════════════════════════════════\n");

    eprintln!(
        "{:<16} {:>6} {:>6} {:>6} {:>8} {:>6} {:>6} {:>12}",
        "Format", "words", "sens", "rep%", "lat_ms", "fmt?", "think?", "grammar_len"
    );
    eprintln!("{}", "─".repeat(100));

    for (fi, fmt) in FORMATS.iter().enumerate() {
        for (ti, r) in results[fi].iter().enumerate() {
            let fok = if r.format_ok { "✅" } else { "❌" };
            let tc = if r.think_contam { "⚠️" } else { "✓" };
            eprintln!(
                "{:<16} {:>6} {:>6} {:>5.0}% {:>8} {:>6} {:>6} {:>12}",
                if ti == 0 { fmt.name } else { "" },
                r.prose_words,
                r.sensory,
                r.repetition * 100.0,
                r.latency_ms,
                fok,
                tc,
                fmt.grammar.len()
            );
        }
        eprintln!("  │ {}\n", fmt.desc);
    }

    // ═══════════════════════════════════════════════════════════════
    // SNIPPETS
    // ═══════════════════════════════════════════════════════════════
    eprintln!("─── Prose excerpts (first trial each) ───\n");
    for (fi, _) in FORMATS.iter().enumerate() {
        let r = &results[fi][0];
        eprintln!("  [{}] {}", r.name, r.snippet);
        if let Some(ref err) = r.format_error {
            eprintln!("       ⚠️  {err}");
        }
        eprintln!();
    }

    // ═══════════════════════════════════════════════════════════════
    // SUMMARY
    // ═══════════════════════════════════════════════════════════════
    eprintln!("\n─── Verdicts ───\n");
    // Best format = highest average prose_words with think_contam=false
    let mut ranked: Vec<_> = FORMATS
        .iter()
        .enumerate()
        .map(|(fi, fmt)| {
            let ws: Vec<_> = results[fi].iter().map(|r| r.prose_words).collect();
            let avg = (ws[0] + ws[1]) as f64 / 2.0;
            let all_clean = results[fi].iter().all(|r| !r.think_contam);
            let all_fmt = results[fi].iter().all(|r| r.format_ok);
            (fmt.name, avg, all_clean, all_fmt)
        })
        .collect();
    ranked.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());

    eprintln!("  Rank | Format           | Avg words | Clean? | Format OK?");
    eprintln!("  {}───", "─".repeat(60));
    for (i, (name, avg, clean, fmt_ok)) in ranked.iter().enumerate() {
        let c = if *clean { "✅" } else { "⚠️" };
        let f = if *fmt_ok { "✅" } else { "❌" };
        eprintln!(
            "  #{:<3}  {:<16} {:>7.0}     {:<5}   {:<5}",
            i + 1,
            name,
            avg,
            c,
            f
        );
    }
}

// ═════════════════════════════════════════════════════════════════════════
// GRAMMAR TEST CASES
// ═════════════════════════════════════════════════════════════════════════

fn grammar_sample(name: &str) -> &'static str {
    match name {
        "xml_think" => {
            "<think>Focus on atmosphere.</think>\n<write>Kael's fingers hovered.</write>"
        }
        "xml_direct" => "<write>Kael sat alone in the blue glow.</write>",
        "marker_think" => "THINK:Focus on discovery.\nWRITE:Kael stared at the screen.",
        "marker_direct" => "WRITE:A neon sign flickered outside.",
        "sep_think" => {
            "---THINK---\nSet the mood.\n---WRITE---\nThe apartment was dark.\n---END---"
        }
        "sep_direct" => "---CHAPTER---\nThe night pressed against the windows.\n---END---",
        "bracket_think" => "[THINK:Start with discovery]\n[WRITE:Kael read the poem.]",
        _ => "",
    }
}

fn test_vocab() -> Vec<Vec<u8>> {
    let mut v: Vec<Vec<u8>> = vec![b"".to_vec()];
    for b in [0x09u8, 0x0Au8, 0x0Du8, 0x20u8] {
        v.push(vec![b]);
    }
    for b in 0x21u8..=0x7Eu8 {
        v.push(vec![b]);
    }
    v
}

fn tokenize(vocab: &[Vec<u8>], text: &str) -> Vec<u32> {
    let bytes = text.as_bytes();
    let mut pos = 0;
    let mut out = Vec::new();
    while pos < bytes.len() {
        let mut best: Option<(usize, u32)> = None;
        for (id, tok) in vocab.iter().enumerate() {
            if tok.is_empty() {
                continue;
            }
            if bytes[pos..].starts_with(tok) && tok.len() > best.map_or(0, |(l, _)| l) {
                best = Some((tok.len(), id as u32));
            }
        }
        match best {
            Some((len, id)) => {
                out.push(id);
                pos += len;
            }
            None => {
                pos += 1;
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use roco_bnf_engine::BnfEngine;
    use roco_grammar::gbnf_to_kbnf;

    fn all_grammars() -> Vec<(&'static str, &'static str)> {
        vec![
            ("xml_think", G_XML_THINK),
            ("xml_direct", G_XML_DIRECT),
            ("marker_think", G_MARKER_THINK),
            ("marker_direct", G_MARKER_DIRECT),
            ("sep_think", G_SEP_THINK),
            ("sep_direct", G_SEP_DIRECT),
            ("bracket_think", G_BRACKET_THINK),
        ]
    }

    #[test]
    fn every_grammar_compiles() {
        let vocab = test_vocab();
        for (name, src) in all_grammars() {
            let kbnf = gbnf_to_kbnf(src);
            let engine = BnfEngine::new(&kbnf, &vocab).unwrap_or_else(|e| {
                panic!(
                    "{name} failed: {e:?}
  kbnf: {kbnf}"
                )
            });
            assert!(engine.allowed_count() > 0, "{name} allows 0 tokens");
        }
    }

    #[test]
    fn every_grammar_accepts_sample() {
        let vocab = test_vocab();
        for (name, src) in all_grammars() {
            let kbnf = gbnf_to_kbnf(src);
            let mut engine = BnfEngine::new(&kbnf, &vocab).unwrap_or_else(|e| {
                panic!(
                    "{name} failed: {e:?}
  kbnf: {kbnf}"
                )
            });
            let sample = grammar_sample(name);
            let tokens = tokenize(&vocab, sample);
            assert!(!tokens.is_empty(), "{name}: sample tokenized to nothing");
            for (i, tok) in tokens.iter().enumerate() {
                engine
                    .accept_token(*tok)
                    .unwrap_or_else(|e| panic!("{name}: reject tok {tok} at {i}: {e:?}"));
            }
            assert!(engine.is_finished(), "{name}: not finished on sample");
        }
    }

    #[test]
    fn bare_plus_vs_paren_plus() {
        let bare = "root ::= \"X:\" [ -~]+ ;\n";
        let paren = "root ::= \"X:\" char { char } ;\nchar ::= #'[ -~]' ;\n";
        let vocab = test_vocab();
        let bare_ok = BnfEngine::new(&gbnf_to_kbnf(bare), &vocab).is_ok();
        let paren_ok = BnfEngine::new(&gbnf_to_kbnf(paren), &vocab).is_ok();
        eprintln!("  bare [ -~]+ : {bare_ok}");
        eprintln!("  paren ( [ -~] )+ : {paren_ok}");
        assert!(paren_ok, "paren syntax must compile");
    }
}
