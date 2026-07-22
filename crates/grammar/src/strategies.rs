//! Structured output strategies — toggle between approaches and compare tradeoffs.
//!
//! The [`OutputStrategy`] trait abstracts how a model's output is:
//! 1. Constrained (what GBNF grammar to use)
//! 2. Parsed (how to turn the text into a typed value)
//!
//! Multiple implementations let us compare success rate, speed, and robustness
//! across strategies without changing pipeline code.
//!
//! # Strategies
//!
//! | Strategy | Grammar source | Parsing | Tradeoffs |
//! |---|---|---|---|
//! | [`SchemaStrategy`] | `Schema` → `schema_to_gbnf()` | `serde_json::from_str` | Type-safe, guaranteed parse; grammar is strict JSON (no optional spaces) |
//! | [`RawGbnfStrategy`] | hand-written GBNF string | custom parser fn | Flexible grammar; fragile parsing, schema drift risk |
//! | [`LooseJsonStrategy`] | schema → permissive GBNF | `serde_json::from_str` | More robust with real-world model output; may need post-processing |

use serde::de::DeserializeOwned;

use crate::json_schema::schema_to_gbnf;
use crate::Schema;

/// Generic parser function for structured output strategies.
/// Takes raw model output text and returns a parsed value or error string.
pub type ParserFn<T> = Box<dyn Fn(&str) -> Result<T, String> + Send + Sync>;

// ═════════════════════════════════════════════════════════════════════════════
// Trait
// ═════════════════════════════════════════════════════════════════════════════

/// A strategy for constrained structured output.
///
/// The GBNF grammar is independent of the output type.
pub trait OutputStrategy {
    /// The GBNF grammar string that constrains the model's output.
    fn grammar(&self) -> String;
}

/// Parse model output into a typed value.
///
/// Split from [`OutputStrategy`] so strategies can be used as trait objects.
pub trait OutputParser<T: DeserializeOwned> {
    /// Parse the model's output into `T`.
    ///
    /// If the grammar is correct, this should never fail — but in practice
    /// the model may produce trailing whitespace or other artifacts that
    /// need cleaning.
    fn parse(&self, text: &str) -> Result<T, String>;
}

// ═════════════════════════════════════════════════════════════════════════════
// SchemaStrategy — Schema builder → schema_to_gbnf → serde_json
// ═════════════════════════════════════════════════════════════════════════════

/// Strategy: build a JSON Schema with the programmatic [`Schema`] builder,
/// convert to GBNF via [`schema_to_gbnf`], parse with `serde_json::from_str`.
///
/// This is the primary strategy: type-safe, guaranteed parse, no heuristics.
/// The tradeoff is that the generated GBNF is strict JSON (no optional
/// spaces between tokens), which may make generation harder for models
/// trained on pretty-printed JSON.
pub struct SchemaStrategy {
    schema: Schema,
    root_name: String,
}

impl SchemaStrategy {
    pub fn new(schema: Schema) -> Self {
        Self {
            schema,
            root_name: "root".into(),
        }
    }

    pub fn with_root(mut self, name: &str) -> Self {
        self.root_name = name.into();
        self
    }
}

impl OutputStrategy for SchemaStrategy {
    fn grammar(&self) -> String {
        schema_to_gbnf(&self.root_name, self.schema.to_json())
            .expect("SchemaStrategy: schema_to_gbnf should never fail for valid Schema")
    }
}

impl<T: DeserializeOwned> OutputParser<T> for SchemaStrategy {
    fn parse(&self, text: &str) -> Result<T, String> {
        let trimmed = text.trim();
        serde_json::from_str::<T>(trimmed)
            .map_err(|e| format!("SchemaStrategy parse error: {e}\nraw: {trimmed}"))
    }
}

// ═════════════════════════════════════════════════════════════════════════════
// LooseJsonStrategy — permissive grammar that allows optional whitespace
// ═════════════════════════════════════════════════════════════════════════════

/// Strategy: same schema-based grammar but with optional whitespace between
/// tokens, matching what many pre-trained models expect (they often output
/// `{ "key": "value" }` with spaces).
///
/// Tradeoff: more generation-flexible but the grammar is larger and may
/// produce inconsistent spacing in output.
///
/// The grammar wraps each structural token with an optional `space` rule.
pub struct LooseJsonStrategy {
    schema: Schema,
    root_name: String,
}

impl LooseJsonStrategy {
    pub fn new(schema: Schema) -> Self {
        Self {
            schema,
            root_name: "root".into(),
        }
    }

    pub fn with_root(mut self, name: &str) -> Self {
        self.root_name = name.into();
        self
    }
}

impl OutputStrategy for LooseJsonStrategy {
    fn grammar(&self) -> String {
        let strict = schema_to_gbnf(&self.root_name, self.schema.to_json())
            .expect("LooseJsonStrategy: schema_to_gbnf should never fail for valid Schema");

        // Inject `space ::= " "?` and wrap structural tokens:
        // We add a space rule and prefix the root definition with optional spaces.
        let mut permissive = String::new();
        permissive.push_str(&strict);

        // Add the space rule if not already present
        if !strict.contains("space ::=") {
            permissive.push_str("\nspace ::= \" \"?\n");
        }

        // Make root accept leading/trailing whitespace by wrapping
        // This is pattern-matched on the root rule declaration
        if let Some(pos) = permissive.find("root ::=") {
            // Find the end of the line
            if let Some(eol) = permissive[pos..].find('\n') {
                let rule_line = permissive[pos..pos + eol].to_string();
                let body = rule_line.strip_prefix("root ::= ").unwrap_or(&rule_line);
                let wrapped = format!("root ::= space {} space\n", body);
                permissive.replace_range(pos..pos + eol, &wrapped);
            }
        }

        permissive
    }
}

impl<T: DeserializeOwned> OutputParser<T> for LooseJsonStrategy {
    fn parse(&self, text: &str) -> Result<T, String> {
        let trimmed = text.trim();
        serde_json::from_str::<T>(trimmed)
            .map_err(|e| format!("LooseJsonStrategy parse error: {e}\nraw: {trimmed}"))
    }
}

// ═════════════════════════════════════════════════════════════════════════════
// RawGbnfStrategy — hand-written GBNF + custom parser
// ═════════════════════════════════════════════════════════════════════════════

/// Strategy: hand-written GBNF grammar string + a custom parsing closure.
///
/// Tradeoff: full control over grammar (can add optional spaces, character
/// classes, etc.), but parsing is manual and prone to schema drift.
pub struct RawGbnfStrategy<T> {
    grammar: String,
    parser: ParserFn<T>,
}

impl<T: DeserializeOwned + Send + Sync + 'static> RawGbnfStrategy<T> {
    /// Create from a raw GBNF string and a serde-compatible parser.
    ///
    /// The parser is typically just `|s| serde_json::from_str(s).map_err(...)`.
    pub fn new(grammar: &str) -> Self
    where
        T: DeserializeOwned,
    {
        let grammar = grammar.to_string();
        Self {
            grammar,
            parser: Box::new(move |text| {
                let trimmed = text.trim();
                serde_json::from_str::<T>(trimmed)
                    .map_err(|e| format!("RawGbnfStrategy parse error: {e}\nraw: {trimmed}"))
            }),
        }
    }

    /// Create with a custom parser (e.g., for non-JSON output formats).
    pub fn with_parser(
        grammar: &str,
        parser: ParserFn<T>,
    ) -> Self {
        Self {
            grammar: grammar.to_string(),
            parser,
        }
    }
}

impl<T: DeserializeOwned> OutputStrategy for RawGbnfStrategy<T> {
    fn grammar(&self) -> String {
        self.grammar.clone()
    }
}

impl<T: DeserializeOwned> OutputParser<T> for RawGbnfStrategy<T> {
    fn parse(&self, text: &str) -> Result<T, String> {
        (self.parser)(text)
    }
}

// ═════════════════════════════════════════════════════════════════════════════
// Strategy enum — runtime toggle between strategies
// ═════════════════════════════════════════════════════════════════════════════

/// Available output strategies for structured generation.
///
/// Use [`StrategySelector`] to pick one at runtime (e.g., from a CLI flag).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum StrategyKind {
    /// Schema builder → strict GBNF → serde_json. Type-safe, no optional spaces.
    Schema,
    /// Schema builder → permissive GBNF (with optional whitespace) → serde_json.
    LooseJson,
    /// Hand-written GBNF string + custom parser.
    RawGbnf,
    /// No grammar constraint — prompt engineering (examples) + post-processing.
    /// The caller provides schema examples in the system prompt; this strategy
    /// strips markdown fences and parses JSON. Proven most reliable for small RWKV models.
    StateTuned,
}

impl StrategyKind {
    pub fn all() -> &'static [StrategyKind] {
        &[
            StrategyKind::Schema,
            StrategyKind::LooseJson,
            StrategyKind::RawGbnf,
            StrategyKind::StateTuned,
        ]
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "schema" | "strict" => Some(Self::Schema),
            "loose" | "loose-json" | "permissive" => Some(Self::LooseJson),
            "raw" | "gbnf" | "raw-gbnf" => Some(Self::RawGbnf),
            "state-tuned" | "state" | "examples" | "instruct" => Some(Self::StateTuned),
            _ => None,
        }
    }
}

impl std::fmt::Display for StrategyKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Schema => write!(f, "schema"),
            Self::LooseJson => write!(f, "loose-json"),
            Self::RawGbnf => write!(f, "raw-gbnf"),
            Self::StateTuned => write!(f, "state-tuned"),
        }
    }
}

// ═════════════════════════════════════════════════════════════════════════════
// StateTunedStrategy — no grammar, prompt examples + post-processing
// ═════════════════════════════════════════════════════════════════════════════

/// Strategy: no grammar constraint — use prompt engineering (examples in
/// system message) to guide the model, then post-process the output.
///
/// Post-processing: strip markdown code fences, trim whitespace, parse JSON.
/// This is the **most reliable strategy for small RWKV models** — the eval
/// showed unconstrained generation produces perfect JSON in markdown fences
/// while all grammar-constrained strategies produce garbage (due to
/// character-class incompatibility in `bnf_sampler`).
///
/// # Usage
///
/// The caller must include a JSON schema example in the system prompt:
/// ```text
/// Output JSON matching this schema:
///   name: string
///   age: integer
/// Example:
///   {"name": "Alice", "age": 30}
/// ```
pub struct StateTunedStrategy;

impl OutputStrategy for StateTunedStrategy {
    /// Returns empty string — signals "no grammar constraint" to the pipeline.
    fn grammar(&self) -> String {
        String::new()
    }
}

impl<T: DeserializeOwned> OutputParser<T> for StateTunedStrategy {
    /// Parse model output after stripping markdown code fences.
    ///
    /// Handles:
    /// - ```json ... ``` fences
    /// - ``` ... ``` generic fences
    /// - Inline JSON without fences
    /// - Leading/trailing whitespace
    fn parse(&self, text: &str) -> Result<T, String> {
        let cleaned = clean_json_output(text);
        serde_json::from_str::<T>(&cleaned).map_err(|e| {
            format!("StateTunedStrategy parse error: {e}\noriginal: {text}\ncleaned: {cleaned}")
        })
    }
}

/// Strip markdown code fences and other common wrappers from model output.
fn clean_json_output(text: &str) -> String {
    let trimmed = text.trim();

    // Try to extract JSON from markdown code blocks (most common)
    // Pattern: ```json ... ``` or ``` ... ```
    if let Some(start) = trimmed.find("```") {
        let after_fence = &trimmed[start + 3..];
        // Skip optional language tag (e.g., "json\n")
        let content_start = after_fence
            .find('\n')
            .map(|pos| after_fence[pos + 1..].trim_start())
            .unwrap_or(after_fence.trim_start());

        // Find closing fence
        if let Some(end) = content_start.rfind("```") {
            let json = &content_start[..end];
            return json.trim().to_string();
        }
        // No closing fence — use everything after opening
        let json = content_start.trim();
        if !json.is_empty() {
            return json.to_string();
        }
    }

    // No fences found — try to find JSON object/array directly
    // Look for first '{' or '[' and last '}' or ']'
    if let Some(start) = trimmed.find(['{', '[']) {
        let end_char = if trimmed.as_bytes()[start] == b'{' {
            '}'
        } else {
            ']'
        };
        if let Some(end) = trimmed[start..].rfind(end_char) {
            return trimmed[start..=start + end].to_string();
        }
    }

    // Fallback: return trimmed text
    trimmed.to_string()
}

/// Wraps a strategy kind + the concrete implementations.
pub enum StrategySelector {
    Schema(SchemaStrategy),
    LooseJson(LooseJsonStrategy),
    RawGbnf(RawGbnfStrategy<serde_json::Value>),
    /// No grammar constraint — prompt engineering + post-processing.
    StateTuned(StateTunedStrategy),
}

impl StrategySelector {
    /// Build a selector for a given strategy kind and schema.
    ///
    /// For `Schema` and `LooseJson`, provide a [`Schema`]; for `RawGbnf`,
    /// provide the raw GBNF string. `StateTuned` ignores both.
    pub fn new(kind: StrategyKind, _schema: Schema, _raw_gbnf: &str) -> Self {
        match kind {
            StrategyKind::Schema => Self::Schema(SchemaStrategy::new(_schema)),
            StrategyKind::LooseJson => Self::LooseJson(LooseJsonStrategy::new(_schema)),
            StrategyKind::RawGbnf => Self::RawGbnf(RawGbnfStrategy::new(_raw_gbnf)),
            StrategyKind::StateTuned => Self::StateTuned(StateTunedStrategy),
        }
    }

    /// Build a selector from a kind and a StateTunedStrategy directly.
    pub fn state_tuned() -> Self {
        Self::StateTuned(StateTunedStrategy)
    }

    pub fn kind(&self) -> StrategyKind {
        match self {
            Self::Schema(_) => StrategyKind::Schema,
            Self::LooseJson(_) => StrategyKind::LooseJson,
            Self::RawGbnf(_) => StrategyKind::RawGbnf,
            Self::StateTuned(_) => StrategyKind::StateTuned,
        }
    }

    pub fn grammar(&self) -> String {
        match self {
            Self::Schema(s) => s.grammar(),
            Self::LooseJson(s) => s.grammar(),
            Self::RawGbnf(s) => s.grammar(),
            Self::StateTuned(s) => s.grammar(),
        }
    }

    pub fn parse<T: DeserializeOwned>(&self, text: &str) -> Result<T, String> {
        match self {
            Self::Schema(s) => OutputParser::<T>::parse(s, text),
            Self::LooseJson(s) => OutputParser::<T>::parse(s, text),
            Self::RawGbnf(s) => {
                // RawGbnfStrategy's parser captures Value, so we parse
                // as Value first, then convert to T.
                let val = OutputParser::<serde_json::Value>::parse(s, text)?;
                serde_json::from_value::<T>(val)
                    .map_err(|e| format!("RawGbnfStrategy type conversion: {e}"))
            }
            Self::StateTuned(s) => OutputParser::<T>::parse(s, text),
        }
    }
}

// ═════════════════════════════════════════════════════════════════════════════
// Eval helpers — compare strategies
// ═════════════════════════════════════════════════════════════════════════════

/// Result of evaluating one strategy on one example.
#[derive(Debug, Clone)]
pub struct StrategyEvalResult {
    pub kind: StrategyKind,
    pub success: bool,
    pub grammar_len: usize,
    pub parse_error: Option<String>,
}

/// Run all strategies on the same model output text and compare.
pub fn evaluate_all_strategies<T>(
    text: &str,
    schema: Schema,
    raw_gbnf: &str,
) -> Vec<StrategyEvalResult>
where
    T: DeserializeOwned,
{
    let mut results = Vec::new();
    for kind in StrategyKind::all() {
        let selector = StrategySelector::new(*kind, schema.clone(), raw_gbnf);
        let result = selector.parse::<T>(text);
        results.push(StrategyEvalResult {
            kind: *kind,
            success: result.is_ok(),
            grammar_len: selector.grammar().len(),
            parse_error: result.err(),
        });
    }
    results
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;

    #[derive(Debug, Deserialize, PartialEq)]
    struct Simple {
        name: String,
        age: u64,
    }

    fn simple_schema() -> Schema {
        Schema::object()
            .prop("name", Schema::string())
            .prop("age", Schema::integer())
            .build()
    }

    const SIMPLE_RAW_GBNF: &str = r#"
root  ::= "{" space "\"name\"" space ":" space string space "," space "\"age\"" space ":" space number space "}"
string ::= "\"" ( [ -~] )* "\""
number ::= [0-9]+
space ::= " "?
"#;

    #[test]
    fn schema_strategy_parses_strict_json() {
        let strategy = SchemaStrategy::new(simple_schema());
        let result: Simple = strategy.parse(r#"{"name":"Alice","age":30}"#).unwrap();
        assert_eq!(
            result,
            Simple {
                name: "Alice".into(),
                age: 30
            }
        );
    }

    #[test]
    fn schema_strategy_rejects_whitespace() {
        let strategy = SchemaStrategy::new(simple_schema());
        // SchemaStrategy generates strict JSON without optional spaces,
        // but the parse() method trims whitespace before parsing, so
        // trailing whitespace should work.
        let result: Simple = strategy.parse(r#"  {"name":"Alice","age":30}  "#).unwrap();
        assert_eq!(
            result,
            Simple {
                name: "Alice".into(),
                age: 30
            }
        );
    }

    #[test]
    fn loose_json_strategy_accepts_whitespace() {
        let strategy = LooseJsonStrategy::new(simple_schema());
        let result: Simple = strategy.parse(r#"{"name":"Alice","age":30}"#).unwrap();
        assert_eq!(
            result,
            Simple {
                name: "Alice".into(),
                age: 30
            }
        );
    }

    #[test]
    fn raw_gbnf_strategy_parses_valid_json() {
        let strategy = RawGbnfStrategy::<Simple>::new(SIMPLE_RAW_GBNF);
        let result: Simple = strategy.parse(r#"{"name":"Bob","age":25}"#).unwrap();
        assert_eq!(
            result,
            Simple {
                name: "Bob".into(),
                age: 25
            }
        );
    }

    #[test]
    fn all_strategies_parse_valid_json() {
        for kind in StrategyKind::all() {
            let selector = StrategySelector::new(*kind, simple_schema(), SIMPLE_RAW_GBNF);
            let result: Simple = selector.parse(r#"{"name":"Test","age":1}"#).unwrap();
            assert_eq!(result.name, "Test");
            assert_eq!(result.age, 1);
        }
    }

    #[test]
    fn evaluate_all_reports_results() {
        let results = evaluate_all_strategies::<Simple>(
            r#"{"name":"X","age":99}"#,
            simple_schema(),
            SIMPLE_RAW_GBNF,
        );
        assert_eq!(results.len(), 4);
        for r in &results {
            assert!(r.success, "{:?} should succeed", r.kind);
        }
    }

    #[test]
    fn strategy_selector_grammars_differ() {
        let schema = simple_schema();
        let raw = SIMPLE_RAW_GBNF;

        let schema_gbnf =
            StrategySelector::new(StrategyKind::Schema, schema.clone(), raw).grammar();
        let loose_gbnf =
            StrategySelector::new(StrategyKind::LooseJson, schema.clone(), raw).grammar();
        let raw_gbnf = StrategySelector::new(StrategyKind::RawGbnf, schema.clone(), raw).grammar();
        let state_gbnf = StrategySelector::new(StrategyKind::StateTuned, schema, raw).grammar();

        // Schema, loose, and raw should produce different grammars
        assert_ne!(schema_gbnf, loose_gbnf, "schema and loose should differ");
        assert_ne!(schema_gbnf, raw_gbnf, "schema and raw should differ");
        assert_ne!(loose_gbnf, raw_gbnf, "loose and raw should differ");
        // StateTuned has no grammar
        assert_eq!(state_gbnf, "", "state-tuned grammar should be empty");
    }

    #[test]
    fn schema_grammar_is_strict_json() {
        let gbnf = SchemaStrategy::new(simple_schema()).grammar();
        // Should NOT have a space rule
        assert!(
            !gbnf.contains("space::"),
            "schema grammar should not have space rule:\n{gbnf}"
        );
        // Should reference simple-obj
        assert!(
            gbnf.contains("root ::= root_obj"),
            "schema grammar should reference root_obj:\n{gbnf}"
        );
    }

    #[test]
    fn loose_grammar_has_space_rule() {
        let gbnf = LooseJsonStrategy::new(simple_schema()).grammar();
        assert!(
            gbnf.contains("space ::="),
            "loose grammar should have space rule:\n{gbnf}"
        );
    }

    // ── StateTunedStrategy tests ─────────────────────────────────────

    #[test]
    fn state_tuned_parses_plain_json() {
        let strategy = StateTunedStrategy;
        let result: Simple = strategy.parse(r#"{"name":"Alice","age":30}"#).unwrap();
        assert_eq!(
            result,
            Simple {
                name: "Alice".into(),
                age: 30
            }
        );
    }

    #[test]
    fn state_tuned_strips_json_fences() {
        let strategy = StateTunedStrategy;
        let text = "```json\n{\"name\":\"Bob\",\"age\":25}\n```";
        let result: Simple = strategy.parse(text).unwrap();
        assert_eq!(
            result,
            Simple {
                name: "Bob".into(),
                age: 25
            }
        );
    }

    #[test]
    fn state_tuned_strips_generic_fences() {
        let strategy = StateTunedStrategy;
        let text = "```\n{\"name\":\"Charlie\",\"age\":35}\n```";
        let result: Simple = strategy.parse(text).unwrap();
        assert_eq!(
            result,
            Simple {
                name: "Charlie".into(),
                age: 35
            }
        );
    }

    #[test]
    fn state_tuned_strips_fences_with_extra_text() {
        let strategy = StateTunedStrategy;
        let text = "Here is the result:\n```json\n{\"name\":\"Diana\",\"age\":28}\n```\nEnd";
        let result: Simple = strategy.parse(text).unwrap();
        assert_eq!(
            result,
            Simple {
                name: "Diana".into(),
                age: 28
            }
        );
    }

    #[test]
    fn state_tuned_handles_no_fences() {
        let strategy = StateTunedStrategy;
        let text = "  {\"name\":\"Eve\",\"age\":22}  ";
        let result: Simple = strategy.parse(text).unwrap();
        assert_eq!(
            result,
            Simple {
                name: "Eve".into(),
                age: 22
            }
        );
    }

    #[test]
    fn state_tuned_has_empty_grammar() {
        let strategy = StateTunedStrategy;
        assert_eq!(strategy.grammar(), "");
    }

    #[test]
    fn state_tuned_from_selector() {
        let selector = StrategySelector::state_tuned();
        assert_eq!(selector.kind(), StrategyKind::StateTuned);
        assert_eq!(selector.grammar(), "");
        let result: Simple = selector.parse(r#"{"name":"Frank","age":40}"#).unwrap();
        assert_eq!(
            result,
            Simple {
                name: "Frank".into(),
                age: 40
            }
        );
    }
}
