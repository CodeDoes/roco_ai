//! Message envelope format catalogue.
//!
//! A [`FormatSpec`] is a self-contained description of one message-envelope
//! style: the BNF grammar that enforces its structural delimiters, the prompt
//! builder that frames a generation request in that style, and the extractor
//! that pulls the prose back out of the model's raw output.
//!
//! # Why this exists
//!
//! `gbnf.rs` already covers the `System:/User:/Assistant:` envelope used by
//! `CommonAgent` (the ReAct chat agent). `format.rs` covers `PromptStyle`
//! (how multi-turn history is laid out). Neither captures the *chapter-
//! generation* formats that the multi-format eval compares — XML tags,
//! `TASK:/WRITE:` markers, `---THINK---/---WRITE---/---END---` separators,
//! bracketed `[THINK:][WRITE:]`, and plain prose.
//!
//! `FormatSpec` collects those chapter-generation formats in one place so
//! production code and the eval harness share the same grammar strings,
//! prompt builders, and extractors. Each variant has a `think: bool` flag,
//! collapsing the old 8-entry table to 4 variants × {think, no-think}.
//!
//! # Grammar dialect
//!
//! All grammars here use kbnf-native syntax (`#'...'` regex strings, `{}`
//! repetition, `[]` optionality, `;` terminators) so they can be fed
//! directly to `roco_bnf_engine::create_bnf_mask` without conversion.

// ── Grammar source ──────────────────────────────────────────────────────

/// Shared character rule: any printable ASCII byte plus tab/newline.
/// Used by every prose-bearing format so the text body is unconstrained
/// while the structural delimiters are pinned by the grammar.
const CHAR_RULE: &str = "char ::= #'[ -~\\t\\n]';\n";

/// `text ::= char { char }` — one or more characters.
const TEXT_RULE: &str = "text ::= char { char };\n";

/// XML envelope: `<task>...<context>...<data>...` then either `mid...end<write>prose</write>`
/// (think variant) or `<write>prose</write>` (direct variant). The grammar
/// only enforces the *generated* suffix — the prompt ends at `<data>...`
/// (direct) or right after a `mid` token (think), and the model must close
/// with `</write>`.
const G_XML_THINK: &str =
    "root ::= \"mid\" text \"end\" \"\\n\" \"<write>\" text \"</write>\";\ntext ::= char { char };\nchar ::= #'[ -~\\t\\n]';\n";

const G_XML_DIRECT: &str =
    "root ::= \"<write>\" text \"</write>\";\ntext ::= char { char };\nchar ::= #'[ -~\\t\\n]';\n";

/// `TASK:...CONTEXT:...DATA:...` then `THINK:...WRITE:prose` or `WRITE:prose`.
const G_MARKER_THINK: &str =
    "root ::= \"THINK:\" text \"\\n\" \"WRITE:\" text;\ntext ::= char { char };\nchar ::= #'[ -~\\t\\n]';\n";

const G_MARKER_DIRECT: &str =
    "root ::= \"WRITE:\" text;\ntext ::= char { char };\nchar ::= #'[ -~\\t\\n]';\n";

/// Separator format: `---THINK---\n...\n---WRITE---\n...\n---END---` (think)
/// or `---CHAPTER---\n...\n---END---` (direct).
const G_SEP_THINK: &str = "root ::= \"---THINK---\" \"\\n\" text \"\\n\" \"---WRITE---\" \"\\n\" text \"\\n\" \"---END---\";\ntext ::= char { char };\nchar ::= #'[ -~\\t\\n]';\n";

const G_SEP_DIRECT: &str =
    "root ::= \"---CHAPTER---\" \"\\n\" text \"\\n\" \"---END---\";\ntext ::= char { char };\nchar ::= #'[ -~\\t\\n]';\n";

/// Bracketed: `[THINK:...]\n[WRITE:...]` (think) — no direct variant.
const G_BRACKET_THINK: &str =
    "root ::= \"[THINK:\" text \"]\" \"\\n\" \"[WRITE:\" text \"]\";\ntext ::= char { char };\nchar ::= #'[ -~\\t\\n]';\n";

// ── FormatSpec ───────────────────────────────────────────────────────────

/// A self-contained message-envelope format for chapter-style generation.
///
/// Each variant pairs a structural grammar (kbnf-native) with a prompt
/// builder and an extractor. The `think` flag selects between the
/// reasoning-block and direct-prose variants of the same envelope.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FormatSpec {
    /// `<task><context><data>` then `mid...end<write>prose</write>` (think)
    /// or `<write>prose</write>` (direct).
    Xml { think: bool },
    /// `TASK:/CONTEXT:/DATA:` then `THINK:...WRITE:prose` or `WRITE:prose`.
    Marker { think: bool },
    /// `---THINK---/---WRITE---/---END---` or `---CHAPTER---/---END---`.
    Separator { think: bool },
    /// `[THINK:...]\n[WRITE:...]` bracketed envelope (think only).
    Bracket,
    /// Plain prose, no structural BNF (the "StateTuned" baseline).
    Prose,
}

impl FormatSpec {
    /// All variants in a stable evaluation order.
    pub fn all() -> &'static [FormatSpec] {
        &[
            FormatSpec::Xml { think: true },
            FormatSpec::Xml { think: false },
            FormatSpec::Marker { think: true },
            FormatSpec::Marker { think: false },
            FormatSpec::Separator { think: true },
            FormatSpec::Separator { think: false },
            FormatSpec::Bracket,
            FormatSpec::Prose,
        ]
    }

    /// Stable lowercase name (kebab-case) for logging and table rows.
    pub fn name(&self) -> &'static str {
        match self {
            FormatSpec::Xml { think: true } => "xml-think",
            FormatSpec::Xml { think: false } => "xml-direct",
            FormatSpec::Marker { think: true } => "marker-think",
            FormatSpec::Marker { think: false } => "marker-direct",
            FormatSpec::Separator { think: true } => "sep-think",
            FormatSpec::Separator { think: false } => "sep-direct",
            FormatSpec::Bracket => "bracket-think",
            FormatSpec::Prose => "prose",
        }
    }

    /// One-line human description of the envelope.
    pub fn desc(&self) -> &'static str {
        match self {
            FormatSpec::Xml { think: true } => "<task><context><data> mid then <write>",
            FormatSpec::Xml { think: false } => "<task><context><data> <write> (no think)",
            FormatSpec::Marker { think: true } => "TASK: CONTEXT: DATA: THINK: then WRITE:",
            FormatSpec::Marker { think: false } => "TASK: CONTEXT: DATA: WRITE: (no think)",
            FormatSpec::Separator { think: true } => "---THINK--- ---WRITE--- ---END---",
            FormatSpec::Separator { think: false } => "---CHAPTER--- ---END--- (no think)",
            FormatSpec::Bracket => "[THINK:] [WRITE:] bracket-delimited",
            FormatSpec::Prose => "plain prose, no structural BNF (free-form)",
        }
    }

    /// Does this variant include a think block?
    pub fn has_think(&self) -> bool {
        match self {
            FormatSpec::Xml { think }
            | FormatSpec::Marker { think }
            | FormatSpec::Separator { think } => *think,
            FormatSpec::Bracket => true,
            FormatSpec::Prose => false,
        }
    }

    /// kbnf-native grammar enforcing the *generated* suffix. Empty for
    /// [`FormatSpec::Prose`] (no structural constraint).
    pub fn grammar(&self) -> &'static str {
        match self {
            FormatSpec::Xml { think: true } => G_XML_THINK,
            FormatSpec::Xml { think: false } => G_XML_DIRECT,
            FormatSpec::Marker { think: true } => G_MARKER_THINK,
            FormatSpec::Marker { think: false } => G_MARKER_DIRECT,
            FormatSpec::Separator { think: true } => G_SEP_THINK,
            FormatSpec::Separator { think: false } => G_SEP_DIRECT,
            FormatSpec::Bracket => G_BRACKET_THINK,
            FormatSpec::Prose => "",
        }
    }

    /// Build the prompt that frames `task` with `outline` and `wiki` context.
    /// The prompt ends exactly where the model should begin generating —
    /// the grammar then enforces the closing delimiters.
    pub fn build_prompt(&self, outline: &str, wiki: &str, task: &str) -> String {
        match self {
            FormatSpec::Xml { think: true } => {
                format!("<task>{task}</task>\n<context>{outline}</context>\n<data>{wiki}</data>\nmid")
            }
            FormatSpec::Xml { think: false } => {
                format!("<task>{task}</task>\n<context>{outline}</context>\n<data>{wiki}</data>\n<write>")
            }
            FormatSpec::Marker { think: true } => {
                format!("TASK: {task}\nCONTEXT: {outline}\nDATA: {wiki}\nTHINK:")
            }
            FormatSpec::Marker { think: false } => {
                format!("TASK: {task}\nCONTEXT: {outline}\nDATA: {wiki}\nWRITE:")
            }
            FormatSpec::Separator { think: true } => {
                format!("TASK: {task}\nCONTEXT: {outline}\nDATA: {wiki}\n---THINK---")
            }
            FormatSpec::Separator { think: false } => {
                format!("TASK: {task}\nCONTEXT: {outline}\nDATA: {wiki}\n---CHAPTER---")
            }
            FormatSpec::Bracket => {
                format!("TASK: {task}\nCONTEXT: {outline}\nDATA: {wiki}\n[THINK:")
            }
            FormatSpec::Prose => {
                format!(
                    "Write a story chapter.\n\nTask: {task}\n\nOutline:\n{outline}\n\nWorld:\n{wiki}"
                )
            }
        }
    }

    /// Pull the prose out of the model's raw output by stripping the
    /// structural delimiters the grammar enforced.
    pub fn extract(&self, raw: &str) -> String {
        match self {
            FormatSpec::Xml { .. } => extract_between(raw, "<write>", "</write>")
                .or_else(|| after_last(raw, "end"))
                .unwrap_or_else(|| raw.trim().to_string()),
            FormatSpec::Marker { think: true } => after_last(raw, "WRITE:").unwrap_or_else(|| raw.trim().to_string()),
            FormatSpec::Marker { think: false } => after_first(raw, "WRITE:").unwrap_or_else(|| raw.trim().to_string()),
            FormatSpec::Separator { think: true } => {
                extract_between(raw, "---WRITE---", "---END---").unwrap_or_else(|| raw.trim().to_string())
            }
            FormatSpec::Separator { think: false } => {
                extract_between(raw, "---CHAPTER---", "---END---").unwrap_or_else(|| raw.trim().to_string())
            }
            FormatSpec::Bracket => {
                extract_between(raw, "[WRITE:", "]").unwrap_or_else(|| raw.trim().to_string())
            }
            FormatSpec::Prose => raw.trim().to_string(),
        }
    }

    /// Look up a variant by its [`name`].
    pub fn from_name(name: &str) -> Option<Self> {
        Self::all().iter().copied().find(|f| f.name() == name)
    }
}

impl std::fmt::Display for FormatSpec {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.name())
    }
}

// ── Extractor helpers ────────────────────────────────────────────────────

/// Return the substring between `open` and `close` (first occurrence),
/// trimmed. `None` if either marker is absent.
fn extract_between(haystack: &str, open: &str, close: &str) -> Option<String> {
    let s = haystack.find(open)? + open.len();
    let rest = &haystack[s..];
    let e = rest.find(close)?;
    Some(rest[..e].trim().to_string())
}

/// Return everything after the first occurrence of `marker`, trimmed.
fn after_first(haystack: &str, marker: &str) -> Option<String> {
    let s = haystack.find(marker)? + marker.len();
    Some(haystack[s..].trim().to_string())
}

/// Return everything after the last occurrence of `marker`, trimmed.
fn after_last(haystack: &str, marker: &str) -> Option<String> {
    let s = haystack.rfind(marker)? + marker.len();
    Some(haystack[s..].trim().to_string())
}

// ── Structural-follower check ────────────────────────────────────────────

/// Check whether `raw` (the model's full output) contains the closing
/// structural markers that `spec`'s grammar enforces. The prompt supplies
/// the opening marker; the model must generate the closing one.
pub fn format_followed(raw: &str, spec: &FormatSpec) -> bool {
    match spec {
        FormatSpec::Xml { think: true } => raw.contains("end") && raw.contains("</write>"),
        FormatSpec::Xml { think: false } => raw.contains("</write>"),
        FormatSpec::Marker { think: true } => raw.contains("THINK:") && raw.contains("WRITE:"),
        FormatSpec::Marker { think: false } => raw.contains("WRITE:"),
        FormatSpec::Separator { think: true } => raw.contains("---WRITE---") && raw.contains("---END---"),
        FormatSpec::Separator { think: false } => raw.contains("---END---"),
        FormatSpec::Bracket => raw.contains("[WRITE:"),
        FormatSpec::Prose => true,
    }
}

/// Detect `mid`/think contamination in a format that shouldn't emit one.
/// Only flags formats whose `has_think()` is false.
pub fn has_think_contamination(raw: &str, spec: &FormatSpec) -> bool {
    if spec.has_think() {
        return false;
    }
    // Check only the generated portion (approximated as the first 200 chars)
    // to avoid false positives from prompt echoes.
    let generated: String = raw.chars().take(200).collect();
    const MARKERS: &[&str] = &["mid", "thinking", "---THINK---", "THINK:"];
    MARKERS.iter().any(|m| generated.contains(m))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vocab() -> Vec<Vec<u8>> {
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
                None => pos += 1,
            }
        }
        out
    }

    /// One known-good sample per spec, used to verify the grammar accepts it.
    fn sample(spec: &FormatSpec) -> &'static str {
        match spec {
            FormatSpec::Xml { think: true } => {
                "midFocus on atmosphere.end\n<write>Kael's fingers hovered.</write>"
            }
            FormatSpec::Xml { think: false } => "<write>Kael sat alone in the blue glow.</write>",
            FormatSpec::Marker { think: true } => "THINK:Focus on discovery.\nWRITE:Kael stared at the screen.",
            FormatSpec::Marker { think: false } => "WRITE:A neon sign flickered outside.",
            FormatSpec::Separator { think: true } => {
                "---THINK---\nSet the mood.\n---WRITE---\nThe apartment was dark.\n---END---"
            }
            FormatSpec::Separator { think: false } => {
                "---CHAPTER---\nThe night pressed against the windows.\n---END---"
            }
            FormatSpec::Bracket => "[THINK:Start with discovery]\n[WRITE:Kael read the poem.]",
            FormatSpec::Prose => "The wind howled across the empty street.",
        }
    }

    #[test]
    fn every_grammar_compiles() {
        let vocab = vocab();
        for spec in FormatSpec::all() {
            let grammar = spec.grammar();
            if grammar.is_empty() {
                continue;
            }
            let engine = roco_bnf_engine::BnfEngine::new(grammar, &vocab).unwrap_or_else(|e| {
                panic!("{} failed: {e:?}\ngrammar: {grammar}", spec.name())
            });
            assert!(engine.allowed_count() > 0, "{} allows 0 tokens", spec.name());
        }
    }

    #[test]
    fn every_grammar_accepts_its_sample() {
        let vocab = vocab();
        for spec in FormatSpec::all() {
            let grammar = spec.grammar();
            if grammar.is_empty() {
                continue;
            }
            let mut engine = roco_bnf_engine::BnfEngine::new(grammar, &vocab).unwrap_or_else(|e| {
                panic!("{} failed: {e:?}\ngrammar: {grammar}", spec.name())
            });
            let tokens = tokenize(&vocab, sample(spec));
            assert!(!tokens.is_empty(), "{}: sample tokenized to nothing", spec.name());
            for (i, tok) in tokens.iter().enumerate() {
                engine.accept_token(*tok).unwrap_or_else(|e| {
                    panic!("{}: rejected token {tok} at {i}: {e:?}", spec.name())
                });
            }
            assert!(engine.is_finished(), "{}: not finished on sample", spec.name());
        }
    }

    #[test]
    fn extractors_pull_prose() {
        let cases: &[(&FormatSpec, &str, &str)] = &[
            (&FormatSpec::Xml { think: true },
             "midNote.end\n<write>Prose here.</write>",
             "Prose here."),
            (&FormatSpec::Xml { think: false },
             "<write>Direct prose.</write>",
             "Direct prose."),
            (&FormatSpec::Marker { think: true },
             "THINK:reasoning\nWRITE:visible prose",
             "visible prose"),
            (&FormatSpec::Marker { think: false },
             "WRITE:just prose",
             "just prose"),
            (&FormatSpec::Separator { think: true },
             "---THINK---\nx\n---WRITE---\nprose\n---END---",
             "prose"),
            (&FormatSpec::Separator { think: false },
             "---CHAPTER---\nprose\n---END---",
             "prose"),
            (&FormatSpec::Bracket,
             "[THINK:x]\n[WRITE:bracketed prose]",
             "bracketed prose"),
            (&FormatSpec::Prose, "plain prose", "plain prose"),
        ];
        for (spec, raw, expected) in cases {
            let got = spec.extract(raw);
            assert_eq!(&got, expected, "{}: extract({raw:?}) = {got:?}", spec.name());
        }
    }

    #[test]
    fn format_followed_detects_missing_markers() {
        assert!(format_followed("<write>x</write>", &FormatSpec::Xml { think: false }));
        assert!(!format_followed("<write>x", &FormatSpec::Xml { think: false }));
        assert!(format_followed("WRITE:x", &FormatSpec::Marker { think: false }));
        assert!(!format_followed("no marker", &FormatSpec::Marker { think: false }));
        assert!(format_followed("---CHAPTER---\nx\n---END---", &FormatSpec::Separator { think: false }));
        assert!(format_followed("anything", &FormatSpec::Prose));
    }

    #[test]
    fn has_think_contamination_flags_unexpected_think() {
        // Prose format shouldn't emit mid/THINK: markers.
        assert!(has_think_contamination("midI should think.end\nprose", &FormatSpec::Prose));
        assert!(has_think_contamination("THINK:reasoning\nprose", &FormatSpec::Prose));
        assert!(!has_think_contamination("clean prose only", &FormatSpec::Prose));
        // Think formats never flag.
        assert!(!has_think_contamination("midx.end\n<write>y</write>", &FormatSpec::Xml { think: true }));
    }

    #[test]
    fn lookup_round_trips_by_name() {
        for spec in FormatSpec::all() {
            assert_eq!(FormatSpec::from_name(spec.name()), Some(*spec), "{}", spec.name());
        }
        assert_eq!(FormatSpec::from_name("nonexistent"), None);
    }

    #[test]
    fn build_prompt_ends_at_generation_point() {
        // The prompt must end exactly where the grammar's root rule begins.
        let outline = "Ch1: x";
        let wiki = "World: y";
        let task = "Write Ch1";
        assert!(FormatSpec::Xml { think: true }.build_prompt(outline, wiki, task).ends_with("mid"));
        assert!(FormatSpec::Xml { think: false }.build_prompt(outline, wiki, task).ends_with("<write>"));
        assert!(FormatSpec::Marker { think: true }.build_prompt(outline, wiki, task).ends_with("THINK:"));
        assert!(FormatSpec::Marker { think: false }.build_prompt(outline, wiki, task).ends_with("WRITE:"));
        assert!(FormatSpec::Separator { think: true }.build_prompt(outline, wiki, task).ends_with("---THINK---"));
        assert!(FormatSpec::Separator { think: false }.build_prompt(outline, wiki, task).ends_with("---CHAPTER---"));
        assert!(FormatSpec::Bracket.build_prompt(outline, wiki, task).ends_with("[THINK:"));
    }

    /// The unused rule constants would otherwise warn; reference them so
    /// the module compiles cleanly without dead-code warnings.
    #[test]
    fn rule_constants_are_referenced() {
        assert!(CHAR_RULE.contains("char"));
        assert!(TEXT_RULE.contains("char { char }"));
    }
}
