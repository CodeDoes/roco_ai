//! Identity: who the assistant is, and who the user is.
//!
//! "Who am I / who are you / what can you do / what model are you" are the
//! questions every chat CLI gets asked first, and the ones a 2.9B local model
//! is *least* able to answer — it has no idea it is called RoCo, which
//! subcommands exist, or what the user's name is. Round-tripping them through
//! the model produces confident nonsense.
//!
//! So identity is handled in two layers:
//!
//! 1. **Deterministic fast path** — [`detect`] recognises identity questions
//!    with a keyword matcher and [`IdentityQuery::answer`] replies from real
//!    program facts (crate version, backend name, registered commands, the
//!    stored user profile). No tokens, no latency, no hallucination.
//! 2. **Model grounding** — [`identity_preamble`] injects the same facts into
//!    every system prompt, so when identity comes up *mid-sentence* ("write a
//!    poem about yourself") the model has the truth in context.
//!
//! The user half is a [`UserProfile`] persisted to `.roco/profile.json`. It is
//! written only from explicit statements ("my name is …", "remember that …"),
//! matching the consent requirement in RFC 0010, and is bounded so it cannot
//! grow without limit.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::rich_output as r;

/// Maximum free-form facts retained in a profile. Oldest are dropped first.
pub const MAX_PROFILE_FACTS: usize = 64;
/// Maximum characters kept for any single stored fact.
pub const MAX_FACT_CHARS: usize = 280;
/// Maximum characters for a stored name.
const MAX_NAME_CHARS: usize = 64;

// ═════════════════════════════════════════════════════════════════════════════
// User profile
// ═════════════════════════════════════════════════════════════════════════════

/// What RoCo knows about the human, persisted across sessions.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct UserProfile {
    /// What to call the user.
    pub name: Option<String>,
    /// Free-form remembered facts, oldest first.
    pub facts: Vec<String>,
    /// Explicit key/value preferences (`prefers`, `language`, …).
    pub preferences: BTreeMap<String, String>,
    /// RFC3339 timestamp of the last write.
    pub updated_at: Option<String>,
}

impl UserProfile {
    /// Default on-disk location: `<cwd>/.roco/profile.json`.
    pub fn default_path() -> PathBuf {
        std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join(".roco")
            .join("profile.json")
    }

    /// Load a profile, returning the default when absent or corrupt.
    ///
    /// A corrupt profile is never fatal — identity is a convenience, not a
    /// precondition for chatting.
    pub fn load(path: &Path) -> Self {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|s| serde_json::from_str::<Self>(&s).ok())
            .unwrap_or_default()
    }

    /// Load from [`UserProfile::default_path`].
    pub fn load_default() -> Self {
        Self::load(&Self::default_path())
    }

    /// Persist the profile, creating parent directories as needed.
    ///
    /// Writes to a temporary file and renames it into place so an interrupted
    /// write can never leave a truncated profile behind.
    pub fn save(&self, path: &Path) -> Result<(), String> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let json = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, &json).map_err(|e| e.to_string())?;
        std::fs::rename(&tmp, path).map_err(|e| {
            let _ = std::fs::remove_file(&tmp);
            e.to_string()
        })
    }

    /// True when nothing has ever been recorded.
    pub fn is_empty(&self) -> bool {
        self.name.is_none() && self.facts.is_empty() && self.preferences.is_empty()
    }

    /// Set the user's name (trimmed and length-capped).
    pub fn set_name(&mut self, name: &str) {
        let name: String = name.trim().chars().take(MAX_NAME_CHARS).collect();
        if !name.is_empty() {
            self.name = Some(name);
            self.touch();
        }
    }

    /// Remember a free-form fact. Duplicates are ignored; the list is bounded
    /// so a long-running session cannot grow the profile without limit.
    pub fn remember(&mut self, fact: &str) -> bool {
        let fact: String = fact.trim().chars().take(MAX_FACT_CHARS).collect();
        if fact.is_empty() {
            return false;
        }
        if self.facts.iter().any(|f| f.eq_ignore_ascii_case(&fact)) {
            return false;
        }
        self.facts.push(fact);
        if self.facts.len() > MAX_PROFILE_FACTS {
            let excess = self.facts.len() - MAX_PROFILE_FACTS;
            self.facts.drain(0..excess);
        }
        self.touch();
        true
    }

    /// Record a key/value preference.
    pub fn set_preference(&mut self, key: &str, value: &str) {
        let key = key.trim().to_lowercase();
        let value: String = value.trim().chars().take(MAX_FACT_CHARS).collect();
        if key.is_empty() || value.is_empty() {
            return;
        }
        self.preferences.insert(key, value);
        // Preferences are keyed, so the map is naturally bounded by the number
        // of distinct keys — but cap it anyway against adversarial input.
        while self.preferences.len() > MAX_PROFILE_FACTS {
            if let Some(first) = self.preferences.keys().next().cloned() {
                self.preferences.remove(&first);
            } else {
                break;
            }
        }
        self.touch();
    }

    /// Drop everything.
    pub fn clear(&mut self) {
        *self = Self {
            updated_at: Some(now_rfc3339()),
            ..Default::default()
        };
    }

    fn touch(&mut self) {
        self.updated_at = Some(now_rfc3339());
    }

    /// Compact description injected into the model's system prompt.
    /// Returns `None` when nothing is known, so we don't waste context.
    pub fn context_line(&self) -> Option<String> {
        if self.is_empty() {
            return None;
        }
        let mut parts = Vec::new();
        if let Some(name) = &self.name {
            parts.push(format!("The user's name is {name}."));
        }
        for (k, v) in &self.preferences {
            parts.push(format!("User {k}: {v}."));
        }
        // Only the most recent facts — the prompt budget matters more than
        // total recall, and older facts stay on disk for `roco whoami`.
        for fact in self.facts.iter().rev().take(8).rev() {
            parts.push(format!("Remembered: {fact}"));
        }
        Some(parts.join(" "))
    }

    /// Human-readable rendering for `roco whoami` and `:whoami`.
    pub fn render(&self) -> String {
        if self.is_empty() {
            return "I don't know anything about you yet. Tell me \"my name is …\" \
                    or \"remember that …\" and I'll keep it in ./.roco/profile.json."
                .to_string();
        }
        let mut out = String::new();
        match &self.name {
            Some(n) => out.push_str(&format!("You are {n}.\n")),
            None => out.push_str("I don't know your name yet.\n"),
        }
        if !self.preferences.is_empty() {
            out.push_str("\nPreferences:\n");
            for (k, v) in &self.preferences {
                out.push_str(&format!("  • {k}: {v}\n"));
            }
        }
        if !self.facts.is_empty() {
            out.push_str(&format!("\nRemembered ({}):\n", self.facts.len()));
            for fact in &self.facts {
                out.push_str(&format!("  • {fact}\n"));
            }
        }
        if let Some(ts) = &self.updated_at {
            out.push_str(&format!("\nLast updated: {ts}"));
        }
        out
    }
}

fn now_rfc3339() -> String {
    chrono::Utc::now().to_rfc3339()
}

// ═════════════════════════════════════════════════════════════════════════════
// Assistant identity
// ═════════════════════════════════════════════════════════════════════════════

/// Who the assistant is. Built from real program facts, never guessed.
#[derive(Debug, Clone)]
pub struct AssistantIdentity {
    pub name: &'static str,
    pub version: &'static str,
    /// Backend name as reported by the live `ModelBackend`.
    pub backend: String,
    /// Configured model path, if any.
    pub model: Option<String>,
}

impl Default for AssistantIdentity {
    fn default() -> Self {
        Self {
            name: "RoCo",
            version: env!("CARGO_PKG_VERSION"),
            backend: "unknown".to_string(),
            model: None,
        }
    }
}

impl AssistantIdentity {
    /// Build from the live backend plus environment/config.
    pub fn detect(backend: &dyn roco_engine::ModelBackend) -> Self {
        Self {
            backend: backend.name().to_string(),
            model: std::env::var("RWKV_MODEL").ok().filter(|s| !s.is_empty()),
            ..Default::default()
        }
    }

    /// The capabilities line-up shown for "what can you do?".
    pub fn capabilities() -> &'static [(&'static str, &'static str)] {
        &[
            ("chat", "open-ended conversation, questions, brainstorming"),
            ("story", "structured short stories from a premise"),
            ("story-mode", "interactive writing assistant over a workspace"),
            ("game", "text-adventure game master"),
            ("html", "live HTML canvas you can iterate on"),
            ("code", "programming help, explanations and debugging"),
            ("export", "export a finished story to md/html/txt"),
        ]
    }

    fn model_line(&self) -> String {
        match &self.model {
            Some(m) => {
                let short = Path::new(m)
                    .file_name()
                    .map(|f| f.to_string_lossy().to_string())
                    .unwrap_or_else(|| m.clone());
                format!("{short} via the {} backend", self.backend)
            }
            None => format!("the {} backend", self.backend),
        }
    }

    /// Answer for "who are you?".
    pub fn who_are_you(&self) -> String {
        format!(
            "I'm {n} — a local, offline writing and coding assistant (v{v}).\n\
             I run entirely on this machine using {m}; nothing you type leaves it.\n\
             Ask me what I can do, or just start talking.",
            n = self.name,
            v = self.version,
            m = self.model_line(),
        )
    }

    /// Answer for "what model are you?".
    pub fn what_model(&self) -> String {
        let mut out = format!(
            "I'm {} v{}, running {}.",
            self.name,
            self.version,
            self.model_line()
        );
        out.push_str("\nRun `roco gpu-check` for device and weight details.");
        out
    }

    /// Answer for "what can you do?".
    pub fn what_can_you_do(&self) -> String {
        let mut out = String::from("Here's what I can do:\n");
        for (cmd, desc) in Self::capabilities() {
            out.push_str(&format!("  • {cmd:<11} {desc}\n"));
        }
        out.push_str(
            "\nYou don't have to pick — describe what you want and I'll switch modes.\n\
             In chat: :help for commands, :whoami for what I know about you.",
        );
        out
    }

    /// Facts injected into every system prompt so the model stays grounded.
    pub fn preamble(&self) -> String {
        format!(
            "You are {n}, a local offline assistant running on the user's own machine \
             (version {v}, {m}). You never send data to the cloud. \
             You can chat, write stories, run text adventures, generate HTML and help with code. \
             If you are asked who you are, answer as {n} — never claim to be another assistant \
             and never invent a company, model or training details you were not told.",
            n = self.name,
            v = self.version,
            m = self.model_line(),
        )
    }
}

/// Build the full identity preamble: who the assistant is plus who the user is.
pub fn identity_preamble(identity: &AssistantIdentity, profile: &UserProfile) -> String {
    match profile.context_line() {
        Some(user) => format!("{}\n{}", identity.preamble(), user),
        None => identity.preamble(),
    }
}

// ═════════════════════════════════════════════════════════════════════════════
// Question detection
// ═════════════════════════════════════════════════════════════════════════════

/// A recognised identity question or memory instruction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IdentityQuery {
    /// "who are you", "what are you", "what's your name"
    WhoAreYou,
    /// "what can you do", "what are your capabilities", "help me understand you"
    WhatCanYouDo,
    /// "what model are you", "which llm", "are you gpt/claude"
    WhatModel,
    /// "who am i", "what's my name", "what do you know about me"
    WhoAmI,
    /// "my name is X" / "call me X" / "i'm X"
    SetName(String),
    /// "remember that X" / "remember: X"
    Remember(String),
    /// "forget me", "forget everything about me"
    ForgetMe,
}

/// Normalise input for matching: lowercase, strip punctuation noise.
fn normalize(input: &str) -> String {
    input
        .trim()
        .trim_end_matches(['?', '!', '.', ' '])
        .to_lowercase()
        .replace('’', "'")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// Names that are obviously not names — guards against
/// "I'm confused" being stored as the user's name.
const NAME_STOPWORDS: &[&str] = &[
    "not", "sure", "confused", "sorry", "here", "back", "done", "good", "fine", "ok", "okay",
    "trying", "looking", "working", "just", "going", "wondering", "curious", "new", "afraid",
    "thinking", "hoping", "still", "a", "an", "the",
];

/// Extract a plausible personal name from the tail of a phrase.
fn extract_name(rest: &str) -> Option<String> {
    let rest = rest.trim().trim_matches(|c: char| {
        c == '.' || c == ',' || c == '!' || c == '?' || c == '"' || c == '\''
    });
    if rest.is_empty() {
        return None;
    }
    // Take at most two words — "Sam" or "Sam Vimes", not a whole sentence.
    let words: Vec<&str> = rest.split_whitespace().take(2).collect();
    let first = words.first()?;
    if NAME_STOPWORDS.contains(&first.to_lowercase().as_str()) {
        return None;
    }
    if !first.chars().next()?.is_alphabetic() {
        return None;
    }
    let candidate = if words.len() == 2
        && words[1]
            .chars()
            .next()
            .is_some_and(|c| c.is_uppercase() || rest.chars().all(|c| !c.is_uppercase()))
        && words[1].chars().all(|c| c.is_alphabetic() || c == '-')
    {
        words.join(" ")
    } else {
        (*first).to_string()
    };
    let candidate: String = candidate.chars().take(MAX_NAME_CHARS).collect();
    if candidate.chars().all(|c| c.is_alphabetic() || c == '-' || c == ' ') {
        Some(candidate)
    } else {
        None
    }
}

/// Strip a case-insensitive ASCII prefix from `raw`, returning the remainder
/// with its **original casing** intact.
///
/// This must operate on the raw string, not the normalized one: `normalize`
/// lowercases, collapses whitespace and strips trailing punctuation, so
/// `raw.len() - normalized_rest.len()` is not a valid offset into `raw`.
/// (That arithmetic silently turned "You can call me Sam." into the name
/// "am".)
fn strip_prefix_ci<'a>(raw: &'a str, prefix: &str) -> Option<&'a str> {
    let raw = raw.trim_start();
    // `get` (not `split_at`) so a multi-byte char straddling `prefix.len()`
    // yields `None` instead of panicking. `to_lowercase` can change a string's
    // byte length, so the normalized prefix length is not guaranteed to land
    // on a char boundary in the raw input.
    let head = raw.get(..prefix.len())?;
    let rest = raw.get(prefix.len()..)?;
    head.eq_ignore_ascii_case(prefix).then_some(rest)
}

/// Classify an input as an identity question / memory instruction.
///
/// Returns `None` for ordinary conversation, which then goes to the model as
/// usual. Matching is intentionally conservative: a false negative just costs
/// a model round-trip, a false positive hijacks the user's message.
pub fn detect(input: &str) -> Option<IdentityQuery> {
    let raw = input.trim();
    if raw.is_empty() || raw.len() > 400 {
        return None;
    }
    let n = normalize(raw);

    // ── Memory instructions (checked first: they contain "i am"/"my name") ──
    for prefix in ["remember that ", "remember this: ", "remember: ", "remember "] {
        if !n.starts_with(prefix) {
            continue;
        }
        // Take the remainder from the RAW string so casing survives.
        let Some(rest) = strip_prefix_ci(raw, prefix) else {
            continue;
        };
        let fact = rest.trim().trim_end_matches(['.', '!', '?']).trim();
        // "remember me" isn't a fact.
        if fact.is_empty() || fact.eq_ignore_ascii_case("me") {
            continue;
        }
        return Some(IdentityQuery::Remember(fact.to_string()));
    }

    if matches!(
        n.as_str(),
        "forget me"
            | "forget everything about me"
            | "forget everything"
            | "forget what you know about me"
            | "forget my name"
            | "clear my profile"
    ) {
        return Some(IdentityQuery::ForgetMe);
    }

    for prefix in [
        "my name is ",
        "my name's ",
        "you can call me ",
        "call me ",
        "i am called ",
        "i'm called ",
        "the name's ",
    ] {
        if !n.starts_with(prefix) {
            continue;
        }
        let Some(rest) = strip_prefix_ci(raw, prefix) else {
            continue;
        };
        if let Some(name) = extract_name(rest) {
            return Some(IdentityQuery::SetName(name));
        }
    }

    // ── Who am I? ────────────────────────────────────────────────────────
    if matches!(
        n.as_str(),
        "who am i"
            | "who am i again"
            | "do you know who i am"
            | "do you know my name"
            | "what's my name"
            | "what is my name"
            | "what do you call me"
            | "what do you know about me"
            | "what do you remember about me"
            | "do you remember me"
            | "tell me about myself"
            | "what have you remembered"
    ) {
        return Some(IdentityQuery::WhoAmI);
    }

    // ── What model are you? (before "who are you" — more specific) ───────
    if n.contains("what model")
        || n.contains("which model")
        || n.contains("what llm")
        || n.contains("which llm")
        || n.contains("what language model")
        || n.contains("are you gpt")
        || n.contains("are you chatgpt")
        || n.contains("are you claude")
        || n.contains("are you gemini")
        || n.contains("are you llama")
        || n.contains("what are you running on")
        || n.contains("what are you built on")
        || n.contains("what's under the hood")
    {
        return Some(IdentityQuery::WhatModel);
    }

    // ── What can you do? ─────────────────────────────────────────────────
    if n.contains("what can you do")
        || n.contains("what can you help")
        || n.contains("what do you do")
        || n.contains("what are you capable")
        || n.contains("what are your capabilities")
        || n.contains("what are your features")
        || n.contains("how can you help")
        || n.contains("what else can you do")
    {
        return Some(IdentityQuery::WhatCanYouDo);
    }

    // ── Who are you? ─────────────────────────────────────────────────────
    if matches!(
        n.as_str(),
        "who are you"
            | "who are you again"
            | "what are you"
            | "who is this"
            | "what is this"
            | "introduce yourself"
            | "tell me about yourself"
            | "what's your name"
            | "what is your name"
            | "who am i talking to"
            | "who am i speaking to"
            | "whoami"
    ) {
        return Some(IdentityQuery::WhoAreYou);
    }

    None
}

impl IdentityQuery {
    /// Produce the answer, applying any profile mutation the query implies.
    ///
    /// Returns `(reply, profile_changed)`.
    pub fn answer(
        &self,
        identity: &AssistantIdentity,
        profile: &mut UserProfile,
    ) -> (String, bool) {
        match self {
            IdentityQuery::WhoAreYou => (identity.who_are_you(), false),
            IdentityQuery::WhatCanYouDo => (identity.what_can_you_do(), false),
            IdentityQuery::WhatModel => (identity.what_model(), false),
            IdentityQuery::WhoAmI => (profile.render(), false),
            IdentityQuery::SetName(name) => {
                profile.set_name(name);
                (
                    format!("Got it — I'll call you {name} from now on."),
                    true,
                )
            }
            IdentityQuery::Remember(fact) => {
                if profile.remember(fact) {
                    (format!("Noted: {fact}"), true)
                } else {
                    ("I already had that one.".to_string(), false)
                }
            }
            IdentityQuery::ForgetMe => {
                profile.clear();
                (
                    "Done — I've forgotten everything I knew about you.".to_string(),
                    true,
                )
            }
        }
    }
}

// ═════════════════════════════════════════════════════════════════════════════
// `roco whoami`
// ═════════════════════════════════════════════════════════════════════════════

/// Implementation of the `roco whoami` subcommand.
///
/// Deliberately does **not** start the daemon chain: asking who you are should
/// never spend 25 seconds loading a 2.9B model.
pub fn cmd_whoami(extra: &[&str]) {
    let path = UserProfile::default_path();
    let mut profile = UserProfile::load(&path);

    if extra.iter().any(|&a| a == "--forget" || a == "--clear") {
        profile.clear();
        match profile.save(&path) {
            Ok(()) => r::success("Profile cleared."),
            Err(e) => r::error(&format!("Could not clear profile: {e}")),
        }
        return;
    }

    if let Some(pos) = extra.iter().position(|&a| a == "--set-name") {
        match extra.get(pos + 1) {
            Some(name) => {
                profile.set_name(name);
                match profile.save(&path) {
                    Ok(()) => r::success(&format!("Name set to {name}.")),
                    Err(e) => r::error(&format!("Could not save profile: {e}")),
                }
            }
            None => r::error("--set-name requires a value"),
        }
        return;
    }

    if extra.iter().any(|&a| a == "--json") {
        match serde_json::to_string_pretty(&profile) {
            Ok(j) => println!("{j}"),
            Err(e) => r::error(&format!("serialize failed: {e}")),
        }
        return;
    }

    // Identity of the assistant does not need a live backend here — the
    // backend name is only used for a display string.
    let identity = AssistantIdentity::default();

    r::header("Who is RoCo?");
    println!("{}", identity.who_are_you());
    r::header("Who are you?");
    println!("{}", profile.render());
    println!();
    r::dim(&format!("Profile: {}", path.display()));
}

// ═════════════════════════════════════════════════════════════════════════════
// Tests
// ═════════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    // ── Detection: assistant identity ────────────────────────────────────

    #[test]
    fn detects_who_are_you_variants() {
        for q in [
            "Who are you?",
            "who are you",
            "What are you?",
            "Introduce yourself.",
            "what's your name?",
            "Who am I talking to?",
        ] {
            assert_eq!(detect(q), Some(IdentityQuery::WhoAreYou), "failed on {q:?}");
        }
    }

    #[test]
    fn detects_capability_questions() {
        for q in [
            "What can you do?",
            "what can you help me with",
            "What are your capabilities?",
            "how can you help?",
        ] {
            assert_eq!(
                detect(q),
                Some(IdentityQuery::WhatCanYouDo),
                "failed on {q:?}"
            );
        }
    }

    #[test]
    fn detects_model_questions_before_generic_identity() {
        for q in [
            "What model are you?",
            "which LLM is this",
            "Are you GPT-4?",
            "are you claude",
            "what are you running on?",
        ] {
            assert_eq!(detect(q), Some(IdentityQuery::WhatModel), "failed on {q:?}");
        }
    }

    // ── Detection: user identity ─────────────────────────────────────────

    #[test]
    fn detects_who_am_i_variants() {
        for q in [
            "Who am I?",
            "what's my name",
            "Do you know who I am?",
            "what do you know about me",
            "do you remember me?",
        ] {
            assert_eq!(detect(q), Some(IdentityQuery::WhoAmI), "failed on {q:?}");
        }
    }

    #[test]
    fn detects_name_statements_and_preserves_casing() {
        assert_eq!(
            detect("My name is Ada"),
            Some(IdentityQuery::SetName("Ada".into()))
        );
        assert_eq!(
            detect("call me Grace"),
            Some(IdentityQuery::SetName("Grace".into()))
        );
        // Regression: the longer "you can call me " prefix must win over
        // "call me ", and the offset arithmetic must not eat leading
        // characters. This used to yield the name "am".
        assert_eq!(
            detect("You can call me Sam."),
            Some(IdentityQuery::SetName("Sam".into()))
        );
    }

    #[test]
    fn name_extraction_is_offset_correct_for_every_prefix() {
        // Each phrasing must recover exactly "Ada", never a truncated suffix.
        for phrase in [
            "my name is Ada",
            "My name is Ada.",
            "MY NAME IS Ada",
            "my name's Ada",
            "call me Ada",
            "Call me Ada!",
            "you can call me Ada",
            "You can call me Ada.",
            "i am called Ada",
            "I'm called Ada",
            "the name's Ada",
        ] {
            assert_eq!(
                detect(phrase),
                Some(IdentityQuery::SetName("Ada".into())),
                "wrong name extracted from {phrase:?}"
            );
        }
    }

    #[test]
    fn remember_preserves_original_casing_and_strips_terminators() {
        assert_eq!(
            detect("Remember that I prefer Rust over Go."),
            Some(IdentityQuery::Remember("I prefer Rust over Go".into()))
        );
        // "remember me" is not a fact.
        assert_eq!(detect("remember me"), None);
    }

    #[test]
    fn strip_prefix_ci_is_utf8_safe() {
        // A multi-byte char straddling the prefix boundary must not panic.
        assert_eq!(strip_prefix_ci("café au lait", "café "), Some("au lait"));
        assert_eq!(strip_prefix_ci("naïve", "nai"), None);
        assert_eq!(strip_prefix_ci("hi", "much longer prefix"), None);
        assert_eq!(strip_prefix_ci("CALL ME Ada", "call me "), Some("Ada"));
    }

    #[test]
    fn multibyte_input_never_panics() {
        // Fuzz-lite: these all exercise prefix matching against non-ASCII.
        for s in [
            "my name is Ådne",
            "café",
            "私の名前はアダです",
            "remember that ☕ matters",
            "call me 🎉",
            "my name is é",
        ] {
            let _ = detect(s);
        }
    }

    #[test]
    fn does_not_mistake_feelings_for_names() {
        // "I'm confused" must not set the user's name to "confused".
        assert_eq!(detect("I'm confused"), None);
        assert_eq!(detect("i am not sure"), None);
        assert_eq!(detect("I'm just looking around"), None);
    }

    #[test]
    fn detects_remember_and_forget() {
        assert_eq!(
            detect("remember that I write sci-fi"),
            Some(IdentityQuery::Remember("I write sci-fi".into()))
        );
        assert_eq!(
            detect("Remember: deadlines are Fridays"),
            Some(IdentityQuery::Remember("deadlines are Fridays".into()))
        );
        assert_eq!(detect("forget me"), Some(IdentityQuery::ForgetMe));
    }

    #[test]
    fn ordinary_conversation_is_not_hijacked() {
        for q in [
            "Write me a poem about the sea",
            "who is the main character in my story?",
            "what model of car should I buy for a road trip",
            "",
            "hi",
        ] {
            // The car question mentions "what model" — make sure it is at
            // least never treated as a name/memory write.
            match detect(q) {
                None | Some(IdentityQuery::WhatModel) => {}
                other => panic!("{q:?} unexpectedly matched {other:?}"),
            }
        }
        assert_eq!(detect("Write me a poem about the sea"), None);
        assert_eq!(detect("hi"), None);
    }

    #[test]
    fn overlong_input_is_ignored() {
        let long = "who are you ".repeat(80);
        assert_eq!(detect(&long), None);
    }

    // ── Profile behaviour ────────────────────────────────────────────────

    #[test]
    fn profile_roundtrips_through_disk() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("profile.json");

        let mut p = UserProfile::default();
        p.set_name("Ada");
        p.remember("prefers terse answers");
        p.set_preference("language", "rust");
        p.save(&path).unwrap();

        let loaded = UserProfile::load(&path);
        assert_eq!(loaded.name.as_deref(), Some("Ada"));
        assert_eq!(loaded.facts, vec!["prefers terse answers".to_string()]);
        assert_eq!(loaded.preferences.get("language").unwrap(), "rust");
    }

    #[test]
    fn corrupt_profile_falls_back_to_default() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("profile.json");
        std::fs::write(&path, "{ not json").unwrap();
        assert!(UserProfile::load(&path).is_empty());
    }

    #[test]
    fn facts_are_bounded_and_deduplicated() {
        let mut p = UserProfile::default();
        for i in 0..(MAX_PROFILE_FACTS * 3) {
            p.remember(&format!("fact {i}"));
        }
        assert_eq!(p.facts.len(), MAX_PROFILE_FACTS);
        // Oldest dropped, newest retained.
        assert!(p.facts.last().unwrap().contains(&format!(
            "fact {}",
            MAX_PROFILE_FACTS * 3 - 1
        )));

        assert!(!p.remember("fact 100"), "duplicates must be rejected");
    }

    #[test]
    fn long_facts_are_truncated() {
        let mut p = UserProfile::default();
        p.remember(&"x".repeat(MAX_FACT_CHARS * 4));
        assert_eq!(p.facts[0].chars().count(), MAX_FACT_CHARS);
    }

    #[test]
    fn preferences_are_bounded() {
        let mut p = UserProfile::default();
        for i in 0..(MAX_PROFILE_FACTS * 2) {
            p.set_preference(&format!("key{i}"), "v");
        }
        assert!(p.preferences.len() <= MAX_PROFILE_FACTS);
    }

    #[test]
    fn clear_empties_the_profile() {
        let mut p = UserProfile::default();
        p.set_name("Ada");
        p.remember("x");
        p.clear();
        assert!(p.is_empty());
    }

    // ── Answers ──────────────────────────────────────────────────────────

    #[test]
    fn answers_are_grounded_in_real_facts() {
        let id = AssistantIdentity::default();
        let mut p = UserProfile::default();

        let (who, changed) = IdentityQuery::WhoAreYou.answer(&id, &mut p);
        assert!(who.contains("RoCo"));
        assert!(who.contains(env!("CARGO_PKG_VERSION")));
        assert!(!changed);

        let (caps, _) = IdentityQuery::WhatCanYouDo.answer(&id, &mut p);
        for (cmd, _) in AssistantIdentity::capabilities() {
            assert!(caps.contains(cmd), "capabilities missing {cmd}");
        }
    }

    #[test]
    fn setting_a_name_then_asking_who_am_i_recalls_it() {
        let id = AssistantIdentity::default();
        let mut p = UserProfile::default();

        let (ack, changed) = IdentityQuery::SetName("Ada".into()).answer(&id, &mut p);
        assert!(ack.contains("Ada"));
        assert!(changed);

        let (who, _) = IdentityQuery::WhoAmI.answer(&id, &mut p);
        assert!(who.contains("Ada"), "got {who}");
    }

    #[test]
    fn empty_profile_gives_a_helpful_answer_not_a_lie() {
        let id = AssistantIdentity::default();
        let mut p = UserProfile::default();
        let (who, _) = IdentityQuery::WhoAmI.answer(&id, &mut p);
        assert!(who.contains("don't know"), "got {who}");
    }

    #[test]
    fn forget_me_wipes_the_profile() {
        let id = AssistantIdentity::default();
        let mut p = UserProfile::default();
        p.set_name("Ada");
        let (msg, changed) = IdentityQuery::ForgetMe.answer(&id, &mut p);
        assert!(changed);
        assert!(msg.contains("forgotten"));
        assert!(p.is_empty());
    }

    // ── Preamble ─────────────────────────────────────────────────────────

    #[test]
    fn preamble_includes_user_context_when_known() {
        let id = AssistantIdentity::default();
        let mut p = UserProfile::default();
        assert!(!identity_preamble(&id, &p).contains("user's name"));

        p.set_name("Ada");
        let pre = identity_preamble(&id, &p);
        assert!(pre.contains("RoCo"));
        assert!(pre.contains("Ada"));
    }

    #[test]
    fn preamble_forbids_claiming_to_be_another_assistant() {
        let pre = AssistantIdentity::default().preamble();
        assert!(pre.contains("never claim to be another assistant"));
    }

    #[test]
    fn context_line_is_bounded_by_recent_facts() {
        let mut p = UserProfile::default();
        for i in 0..40 {
            p.remember(&format!("fact {i}"));
        }
        let line = p.context_line().unwrap();
        assert!(line.contains("fact 39"));
        assert!(!line.contains("fact 0 "), "old facts should be dropped");
    }
}
