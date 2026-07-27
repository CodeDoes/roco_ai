//! Shared conversation engine for every interactive `roco` surface.
//!
//! `interact`, `router`, `code` and `game` each grew their own copy of "build
//! a prompt from history, call the backend, print the answer". They drifted,
//! and all of them shared the same three bugs:
//!
//! 1. **Context was destroyed to fit.** `build_chat_context` kept the last 8
//!    messages but truncated *each one* to 300 characters, so any answer
//!    longer than a tweet was silently mutilated before being fed back. The
//!    model could not follow up on its own output — the single biggest cause
//!    of "it forgot what we were talking about".
//! 2. **No turn budget.** History grew until the prompt outgrew the context
//!    window, at which point quality fell off a cliff with no warning.
//! 3. **Nothing was streamed.** See [`crate::streaming`].
//!
//! [`ChatSession`] fixes all three: history is bounded by *characters* as well
//! as turns, whole turns are dropped from the front (never mid-message), the
//! oldest dropped turns are folded into a running summary line so long
//! conversations degrade gracefully instead of forgetting abruptly, and
//! generation is streamed through [`StreamPrinter`].
//!
//! It also owns the identity fast-path ([`crate::identity`]), so "who are
//! you?" is answered identically everywhere without a model round-trip.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use roco_engine::{CompletionRequest, ModelBackend};
use roco_protocol::ConversationState;

use crate::identity::{self, AssistantIdentity, UserProfile};
use crate::rich_output as r;
use crate::streaming::{self, StreamPrinter};

// ═════════════════════════════════════════════════════════════════════════════
// Budgets
// ═════════════════════════════════════════════════════════════════════════════

/// Maximum turns retained verbatim in the prompt.
pub const MAX_CONTEXT_TURNS: usize = 16;
/// Maximum characters of conversation history fed to the model (~2k tokens).
pub const MAX_CONTEXT_CHARS: usize = 8_000;
/// Maximum messages held in memory / written to the session file.
///
/// Without this a long-lived REPL grows `Vec<ConversationMessage>` forever and
/// rewrites an ever-larger JSON file after *every* turn — quadratic disk I/O
/// and unbounded RSS. Trimming here bounds both.
pub const MAX_HISTORY_MESSAGES: usize = 400;
/// Characters of a dropped turn folded into the rolling summary.
const SUMMARY_SNIPPET_CHARS: usize = 160;
/// Maximum characters of rolling summary carried in the prompt.
const MAX_SUMMARY_CHARS: usize = 1_200;

// ═════════════════════════════════════════════════════════════════════════════
// Turn outcome
// ═════════════════════════════════════════════════════════════════════════════

/// What happened during one turn.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TurnOutcome {
    /// The model produced a reply.
    Generated(String),
    /// Answered locally from program facts (no model call).
    Identity(String),
    /// Generation failed; the message is the error text.
    Failed(String),
}

impl TurnOutcome {
    /// The reply text, whatever its provenance.
    pub fn text(&self) -> &str {
        match self {
            TurnOutcome::Generated(t) | TurnOutcome::Identity(t) | TurnOutcome::Failed(t) => t,
        }
    }

    /// Whether the model was actually invoked.
    pub fn used_model(&self) -> bool {
        matches!(self, TurnOutcome::Generated(_))
    }
}

// ═════════════════════════════════════════════════════════════════════════════
// ChatSession
// ═════════════════════════════════════════════════════════════════════════════

/// A persistent, streaming, identity-aware conversation.
pub struct ChatSession {
    /// Serialized transcript (also what gets written to disk).
    pub state: ConversationState,
    /// Where the transcript is saved.
    pub path: PathBuf,
    /// Base persona for the surface using this session.
    persona: String,
    /// Who the assistant is.
    identity: AssistantIdentity,
    /// Who the user is.
    profile: UserProfile,
    /// Where the profile is saved.
    profile_path: PathBuf,
    /// Condensed record of turns evicted from the verbatim window.
    summary: String,
    /// Sampling temperature.
    pub temperature: f32,
    /// Generation cap.
    pub max_tokens: usize,
    /// Deterministic seed for reproducible sampling (None = non-deterministic).
    pub seed: Option<u64>,
    /// If true, record per-token trace metadata for debugging.
    pub record_trace: bool,
    /// Suppress terminal output (used by one-shot and scripted callers).
    quiet: bool,
    /// Turns that actually hit the model.
    pub model_turns: u64,
    /// Path to a persisted backend state file for instant resume.
    pub state_file: Option<PathBuf>,
    /// If true, the next turn skips context history because the loaded
    /// backend state already encodes the full conversation up to this point.
    pub instant_resume: bool,
    /// If true, display a generation progress bar with tok/s and ETA.
    pub show_progress: bool,
}

impl ChatSession {
    /// Build a session. `persona` is the surface-specific system prompt; the
    /// identity preamble is prepended automatically.
    pub fn new(
        state: ConversationState,
        path: PathBuf,
        persona: impl Into<String>,
        backend: &dyn ModelBackend,
    ) -> Self {
        let profile_path = UserProfile::default_path();
        Self {
            state,
            path,
            persona: persona.into(),
            identity: AssistantIdentity::detect(backend),
            profile: UserProfile::load(&profile_path),
            profile_path,
            summary: String::new(),
            temperature: 0.8,
            max_tokens: 1024,
            seed: None,
            record_trace: false,
            quiet: false,
            model_turns: 0,
            state_file: None,
            instant_resume: false,
            show_progress: std::env::var("ROCO_PROGRESS").is_ok(),
        }
    }

    /// Enable generation progress bar display with tok/s and ETA.
    pub fn with_progress(mut self, enabled: bool) -> Self {
        self.show_progress = enabled;
        self
    }

    /// Suppress terminal writes (the caller prints the result itself).
    pub fn quiet(mut self, quiet: bool) -> Self {
        self.quiet = quiet;
        self
    }

    /// Attach a state file for instant-resume support.
    pub fn with_state_file(mut self, path: PathBuf) -> Self {
        self.state_file = Some(path);
        self
    }

    /// Mark that the backend state has been pre-loaded and the next turn
    /// should skip context history.
    pub fn with_instant_resume(mut self) -> Self {
        self.instant_resume = true;
        self
    }

    /// Override sampling parameters.
    pub fn with_sampling(mut self, temperature: f32, max_tokens: usize) -> Self {
        self.temperature = temperature;
        self.max_tokens = max_tokens;
        self
    }

    /// Set a deterministic seed for reproducible generation.
    pub fn with_seed(mut self, seed: u64) -> Self {
        self.seed = Some(seed);
        self
    }

    /// Enable per-token trace recording for debugging generations.
    pub fn with_trace(mut self, enabled: bool) -> Self {
        self.record_trace = enabled;
        self
    }

    /// Replace the persona (e.g. the router switching modes).
    pub fn set_persona(&mut self, persona: impl Into<String>) {
        self.persona = persona.into();
    }

    /// Read-only access to the user profile.
    pub fn profile(&self) -> &UserProfile {
        &self.profile
    }

    /// The assistant's identity facts.
    pub fn identity(&self) -> &AssistantIdentity {
        &self.identity
    }

    /// The greeting shown when a session starts, personalised if we know the user.
    pub fn greeting(&self) -> String {
        match &self.profile.name {
            Some(name) => format!("Welcome back, {name}. What are we working on?"),
            None => "Hi — I'm RoCo. Ask me anything, or say \"what can you do?\"".to_string(),
        }
    }

    // ── Turns ────────────────────────────────────────────────────────────

    /// Run one full turn: identity fast-path, else stream a model reply.
    /// The user message and reply are both recorded and persisted.
    pub fn turn(&mut self, backend: &dyn ModelBackend, input: &str) -> TurnOutcome {
        // Build the prompt from history *before* recording the new message, so
        // the user's line isn't duplicated in the context.
        let was_instant = self.instant_resume;
        self.instant_resume = false;
        let context = if was_instant {
            // Backend state already encodes the full history; only the new
            // user message needs to be sent.
            format!("User: {}\nAssistant:", input.trim())
        } else {
            self.build_context(input)
        };
        self.push("user", input);

        // ── Identity fast-path: correct, instant, no tokens ──────────────
        if let Some(query) = identity::detect(input) {
            let (reply, changed) = query.answer(&self.identity, &mut self.profile);
            if changed {
                self.save_profile();
            }
            if !self.quiet {
                println!("{}{}", self.prefix(), reply);
            }
            self.push("assistant", &reply);
            self.save();
            return TurnOutcome::Identity(reply);
        }

        // ── Model path ───────────────────────────────────────────────────
        if !self.quiet {
            streaming::thinking_hint();
        }

        let printer: Arc<Mutex<StreamPrinter>> = if self.quiet {
            StreamPrinter::quiet().shared()
        } else {
            StreamPrinter::new(self.prefix()).shared()
        };

        let tracker = Arc::new(Mutex::new(streaming::ProgressTracker::new(self.max_tokens)));
        let on_token = if self.show_progress {
            streaming::on_token_with_progress(&printer, &tracker)
        } else {
            streaming::on_token_for(&printer)
        };

        let mut request = CompletionRequest {
            system: self.system_prompt(),
            prompt: context,
            temperature: self.temperature,
            max_tokens: self.max_tokens,
            prefill: Some(roco_engine::NO_THINK_PREFILL.to_string()),
            on_token: Some(on_token),
            seed: self.seed,
            ..Default::default()
        };
        request.record_trace = self.record_trace;
        request.record_trace = self.record_trace;

        // `request` (and the `on_token` closure holding a clone of `printer`)
        // is consumed by `complete`, and the returned future is dropped at the
        // end of this statement — so by the time we lock the printer below,
        // nothing else can be writing to it.
        let result = futures::executor::block_on(backend.complete(request));

        if self.show_progress {
            eprint!("\r\x1b[K");
        }

        let outcome = match result {
            Ok(response) => {
                let text = Self::finish_stream(&printer, &response.text);
                self.model_turns += 1;
                if text.trim().is_empty() {
                    let msg = "(no response — try rephrasing)".to_string();
                    if !self.quiet {
                        streaming::clear_line();
                        println!("{}{}", self.prefix(), msg);
                    }
                    TurnOutcome::Failed(msg)
                } else {
                    TurnOutcome::Generated(text)
                }
            }
            Err(e) => {
                if !self.quiet {
                    streaming::clear_line();
                    r::error(&format!("Generation failed: {e}"));
                }
                TurnOutcome::Failed(format!("[Error: {e}]"))
            }
        };

        self.push("assistant", outcome.text());
        self.save();

        // Persist backend recurrent state for instant resume on next session.
        if outcome.used_model() {
            if let Some(ref state_path) = self.state_file {
                if let Ok(bytes) = futures::executor::block_on(backend.save_state()) {
                    let _ = std::fs::write(state_path, bytes);
                }
            }
        }

        outcome
    }

    /// Finish the stream and return the cleaned text, never holding the lock
    /// across a panic-prone operation.
    fn finish_stream(printer: &Arc<Mutex<StreamPrinter>>, full: &str) -> String {
        match printer.lock() {
            Ok(mut p) => p.finish(full),
            // A poisoned lock means a token callback panicked; fall back to
            // the raw text rather than taking the whole REPL down.
            Err(poisoned) => poisoned.into_inner().finish(full),
        }
    }

    /// Answer an identity question without recording a turn. Used by surfaces
    /// that want the fast path but manage their own transcript.
    pub fn identity_reply(&mut self, input: &str) -> Option<String> {
        let query = identity::detect(input)?;
        let (reply, changed) = query.answer(&self.identity, &mut self.profile);
        if changed {
            self.save_profile();
        }
        Some(reply)
    }

    /// Handle the identity-related slash commands shared by all surfaces.
    /// Returns `Some(reply)` when handled.
    pub fn identity_command(&mut self, cmd: &str) -> Option<String> {
        let (verb, rest) = match cmd.split_once(char::is_whitespace) {
            Some((v, r)) => (v, r.trim()),
            None => (cmd, ""),
        };
        match verb {
            "whoami" | "me" => Some(self.profile.render()),
            "whois" | "about" => Some(self.identity.who_are_you()),
            "remember" if !rest.is_empty() => {
                let reply = if self.profile.remember(rest) {
                    format!("Noted: {rest}")
                } else {
                    "I already had that one.".to_string()
                };
                self.save_profile();
                Some(reply)
            }
            "forget" => {
                self.profile.clear();
                self.save_profile();
                Some("Forgotten.".to_string())
            }
            "name" if !rest.is_empty() => {
                self.profile.set_name(rest);
                self.save_profile();
                Some(format!("I'll call you {rest}."))
            }
            _ => None,
        }
    }

    // ── History management ───────────────────────────────────────────────

    /// Record a message and enforce the in-memory bound.
    pub fn push(&mut self, role: &str, content: &str) {
        self.state.add_message(role, content);
        self.trim_history();
    }

    /// Drop the oldest messages once over budget, folding them into the
    /// rolling summary so context degrades smoothly rather than vanishing.
    fn trim_history(&mut self) {
        if self.state.messages.len() <= MAX_HISTORY_MESSAGES {
            return;
        }
        let excess = self.state.messages.len() - MAX_HISTORY_MESSAGES;
        let dropped: Vec<_> = self.state.messages.drain(0..excess).collect();
        for msg in dropped {
            self.fold_into_summary(&msg.role, &msg.content);
        }
    }

    fn fold_into_summary(&mut self, role: &str, content: &str) {
        let label = match role {
            "user" => "User",
            "assistant" | "ai" => "RoCo",
            _ => return,
        };
        let snippet: String = content
            .chars()
            .take(SUMMARY_SNIPPET_CHARS)
            .collect::<String>()
            .replace('\n', " ");
        if snippet.trim().is_empty() {
            return;
        }
        self.summary.push_str(&format!("{label}: {snippet}\n"));
        // Bound the summary itself, dropping whole lines from the front.
        while self.summary.len() > MAX_SUMMARY_CHARS {
            match self.summary.find('\n') {
                Some(i) => {
                    self.summary.drain(0..=i);
                }
                None => {
                    self.summary.clear();
                    break;
                }
            }
        }
    }

    /// Clear the transcript (the `/clear` command).
    pub fn clear(&mut self) {
        self.state.messages.clear();
        self.summary.clear();
    }

    /// Remove the last exchange (the `/undo` command). Returns whether
    /// anything was removed.
    pub fn undo(&mut self) -> bool {
        if self.state.messages.len() < 2 {
            return false;
        }
        self.state.messages.pop();
        self.state.messages.pop();
        true
    }

    // ── Prompt construction ──────────────────────────────────────────────

    /// The full system prompt: identity facts + surface persona.
    pub fn system_prompt(&self) -> String {
        let preamble = identity::identity_preamble(&self.identity, &self.profile);
        if self.persona.trim().is_empty() {
            preamble
        } else {
            format!("{preamble}\n\n{}", self.persona)
        }
    }

    /// Build the conversation context for `new_input`.
    ///
    /// Unlike the old `build_chat_context`, messages are included **whole or
    /// not at all**: turns are walked newest-first and admitted while they fit
    /// the character budget. Truncating individual replies to 300 chars — the
    /// previous behaviour — produced prompts full of half-sentences and was
    /// why the model kept losing the thread.
    pub fn build_context(&self, new_input: &str) -> String {
        /// Approximate overhead of a `"Assistant: …\n"` label per turn.
        const LABEL_OVERHEAD: usize = 12;

        let mut selected: Vec<&roco_protocol::ConversationMessage> = Vec::new();
        let mut used = new_input.len();

        for msg in self.state.messages.iter().rev() {
            let cost = msg.content.len() + LABEL_OVERHEAD;
            if selected.len() >= MAX_CONTEXT_TURNS || used + cost > MAX_CONTEXT_CHARS {
                break;
            }
            match msg.role.as_str() {
                "user" | "assistant" | "ai" => {
                    used += cost;
                    selected.push(msg);
                }
                _ => {}
            }
        }
        selected.reverse();

        let mut ctx = String::with_capacity(used + self.summary.len() + 64);

        // Turns evicted from the verbatim window, condensed. Present only once
        // something has actually been dropped (which is when `summary` becomes
        // non-empty), so short conversations carry no extra preamble.
        if !self.summary.is_empty() {
            ctx.push_str("[Earlier in this conversation]\n");
            ctx.push_str(&self.summary);
            ctx.push('\n');
        }

        for msg in selected {
            let label = match msg.role.as_str() {
                "user" => "User",
                _ => "Assistant",
            };
            ctx.push_str(label);
            ctx.push_str(": ");
            ctx.push_str(msg.content.trim());
            ctx.push('\n');
        }

        ctx.push_str("User: ");
        ctx.push_str(new_input.trim());
        ctx.push_str("\nAssistant:");
        ctx
    }

    // ── Persistence ──────────────────────────────────────────────────────

    fn prefix(&self) -> String {
        format!("{}RoCo:{} ", r::Colors::CYAN, r::Colors::RESET)
    }

    /// Persist the transcript, warning (but not failing) on error.
    pub fn save(&self) {
        if let Err(e) = self.state.save(&self.path) {
            if !self.quiet {
                r::warning(&format!("Auto-save failed: {e}"));
            }
        }
    }

    fn save_profile(&self) {
        if let Err(e) = self.profile.save(&self.profile_path) {
            if !self.quiet {
                r::warning(&format!("Could not save profile: {e}"));
            }
        }
    }

    /// Point the profile at a different file (used by tests).
    pub fn with_profile_path(mut self, path: impl AsRef<Path>) -> Self {
        self.profile_path = path.as_ref().to_path_buf();
        self.profile = UserProfile::load(&self.profile_path);
        self
    }
}

// ═════════════════════════════════════════════════════════════════════════════
// Tests
// ═════════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use roco_engine::MockBackend;

    fn session(dir: &Path) -> ChatSession {
        let backend = MockBackend::default();
        ChatSession::new(
            ConversationState::new("test".into(), "careful"),
            dir.join("session.json"),
            "You are a test persona.",
            &backend,
        )
        .with_profile_path(dir.join("profile.json"))
        .quiet(true)
    }

    // ── Context construction ─────────────────────────────────────────────

    #[test]
    fn context_keeps_whole_messages_not_truncated_ones() {
        let dir = tempfile::tempdir().unwrap();
        let mut s = session(dir.path());

        // The old implementation cut every message at 300 chars.
        let long = "A".repeat(1200);
        s.push("user", "tell me a long story");
        s.push("assistant", &long);

        let ctx = s.build_context("continue");
        assert!(
            ctx.contains(&long),
            "long assistant turn must survive verbatim (len {})",
            long.len()
        );
    }

    #[test]
    fn context_respects_the_character_budget() {
        let dir = tempfile::tempdir().unwrap();
        let mut s = session(dir.path());
        for i in 0..200 {
            s.push("user", &format!("u{i} {}", "x".repeat(200)));
            s.push("assistant", &format!("a{i} {}", "y".repeat(200)));
        }
        let ctx = s.build_context("next");
        assert!(
            ctx.len() <= MAX_CONTEXT_CHARS + MAX_SUMMARY_CHARS + 512,
            "context too big: {}",
            ctx.len()
        );
        // The most recent turn must always be present.
        assert!(ctx.contains("a199"), "newest turn missing");
    }

    #[test]
    fn context_ends_with_the_new_input_and_assistant_cue() {
        let dir = tempfile::tempdir().unwrap();
        let s = session(dir.path());
        let ctx = s.build_context("hello there");
        assert!(
            ctx.ends_with("User: hello there\nAssistant:"),
            "got {ctx:?}"
        );
    }

    #[test]
    fn context_excludes_system_messages() {
        let dir = tempfile::tempdir().unwrap();
        let mut s = session(dir.path());
        s.push("system", "internal bookkeeping note");
        s.push("user", "hi");
        let ctx = s.build_context("again");
        assert!(!ctx.contains("internal bookkeeping"));
    }

    #[test]
    fn context_never_duplicates_the_new_input() {
        let dir = tempfile::tempdir().unwrap();
        let dirp = dir.path();
        let backend = MockBackend::default();
        let mut s = session(dirp);
        s.turn(&backend, "unique-probe-string");
        // After the turn the message is in history; the *next* context should
        // contain it exactly once as history, and the new one once.
        let ctx = s.build_context("second");
        assert_eq!(ctx.matches("unique-probe-string").count(), 1);
    }

    // ── History bounds (memory leak regression) ──────────────────────────

    #[test]
    fn history_is_bounded_in_memory() {
        let dir = tempfile::tempdir().unwrap();
        let mut s = session(dir.path());
        for i in 0..(MAX_HISTORY_MESSAGES * 3) {
            s.push("user", &format!("msg {i}"));
        }
        assert!(
            s.state.messages.len() <= MAX_HISTORY_MESSAGES,
            "history grew unbounded: {}",
            s.state.messages.len()
        );
    }

    #[test]
    fn evicted_turns_are_folded_into_a_bounded_summary() {
        let dir = tempfile::tempdir().unwrap();
        let mut s = session(dir.path());
        for i in 0..(MAX_HISTORY_MESSAGES * 2) {
            s.push("user", &format!("question {i} {}", "z".repeat(400)));
            s.push("assistant", &format!("answer {i}"));
        }
        assert!(
            s.summary.len() <= MAX_SUMMARY_CHARS + SUMMARY_SNIPPET_CHARS + 16,
            "summary grew unbounded: {}",
            s.summary.len()
        );
        assert!(!s.summary.is_empty(), "summary should retain something");
    }

    #[test]
    fn undo_and_clear_work() {
        let dir = tempfile::tempdir().unwrap();
        let mut s = session(dir.path());
        s.push("user", "a");
        s.push("assistant", "b");
        assert!(s.undo());
        assert!(s.state.messages.is_empty());
        assert!(!s.undo(), "undo on empty history must be a no-op");

        s.push("user", "c");
        s.clear();
        assert!(s.state.messages.is_empty());
    }

    // ── Identity integration ─────────────────────────────────────────────

    #[test]
    fn identity_questions_skip_the_model() {
        let dir = tempfile::tempdir().unwrap();
        let backend = MockBackend::default();
        let mut s = session(dir.path());

        let out = s.turn(&backend, "who are you?");
        assert!(matches!(out, TurnOutcome::Identity(_)));
        assert!(out.text().contains("RoCo"));
        assert_eq!(s.model_turns, 0, "identity must not call the backend");
    }

    #[test]
    fn name_is_remembered_across_turns_and_sessions() {
        let dir = tempfile::tempdir().unwrap();
        let backend = MockBackend::default();

        {
            let mut s = session(dir.path());
            s.turn(&backend, "my name is Ada");
        }
        // Fresh session, same profile path → the name persists.
        let mut s2 = session(dir.path());
        let out = s2.turn(&backend, "who am I?");
        assert!(out.text().contains("Ada"), "got {}", out.text());
    }

    #[test]
    fn known_user_appears_in_the_system_prompt() {
        let dir = tempfile::tempdir().unwrap();
        let backend = MockBackend::default();
        let mut s = session(dir.path());
        s.turn(&backend, "my name is Grace");
        let sys = s.system_prompt();
        assert!(sys.contains("Grace"), "system prompt missing user name");
        assert!(sys.contains("test persona"), "persona must be preserved");
    }

    #[test]
    fn greeting_is_personalised_once_the_name_is_known() {
        let dir = tempfile::tempdir().unwrap();
        let backend = MockBackend::default();
        let mut s = session(dir.path());
        assert!(!s.greeting().contains("Welcome back"));
        s.turn(&backend, "call me Ada");
        assert!(s.greeting().contains("Ada"));
    }

    #[test]
    fn identity_commands_are_handled() {
        let dir = tempfile::tempdir().unwrap();
        let mut s = session(dir.path());
        assert!(s.identity_command("name Ada").unwrap().contains("Ada"));
        assert!(s.identity_command("whoami").unwrap().contains("Ada"));
        assert!(s
            .identity_command("remember I like haiku")
            .unwrap()
            .contains("Noted"));
        assert!(s.identity_command("forget").unwrap().contains("Forgotten"));
        assert!(s.identity_command("not-a-command").is_none());
    }

    // ── Model turns ──────────────────────────────────────────────────────

    #[test]
    fn ordinary_turn_calls_the_model_and_records_both_sides() {
        let dir = tempfile::tempdir().unwrap();
        let backend = MockBackend::default();
        let mut s = session(dir.path());

        let out = s.turn(&backend, "write me a haiku");
        assert!(out.used_model(), "expected a model turn, got {out:?}");
        assert_eq!(s.model_turns, 1);
        assert_eq!(s.state.messages.len(), 2);
        assert_eq!(s.state.messages[0].role, "user");
        assert_eq!(s.state.messages[1].role, "assistant");
        assert!(!s.state.messages[1].content.is_empty());
    }

    #[test]
    fn backend_failure_is_recorded_not_panicked() {
        let dir = tempfile::tempdir().unwrap();
        let backend = MockBackend::new("failing", 1);
        let mut s = session(dir.path());

        let out = s.turn(&backend, "hello");
        assert!(matches!(out, TurnOutcome::Failed(_)), "got {out:?}");
        assert_eq!(s.state.messages.len(), 2, "the failure is still a turn");
    }

    #[test]
    fn transcript_is_persisted_after_each_turn() {
        let dir = tempfile::tempdir().unwrap();
        let backend = MockBackend::default();
        let mut s = session(dir.path());
        s.turn(&backend, "hello");

        let loaded = ConversationState::load(&dir.path().join("session.json")).unwrap();
        assert_eq!(loaded.messages.len(), 2);
    }

    #[test]
    fn mock_json_envelope_does_not_leak_into_the_transcript() {
        let dir = tempfile::tempdir().unwrap();
        let backend = MockBackend::default();
        let mut s = session(dir.path());
        let out = s.turn(&backend, "hello");
        assert!(
            !out.text().starts_with('{'),
            "raw envelope leaked: {}",
            out.text()
        );
    }
}
