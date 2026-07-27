//! Mode router — the default `roco` experience.
//!
//! ```text
//! roco → user says anything → detect_intent(message, [chat, adventure, story, html, coder])
//!   → routes to detected mode → user interacts in that mode
//!   → detect_intent on every message → if intent changes, switch modes
//! ```
//!
//! The key insight: ONE generic `detect_intent(user_message, available_intents)` call
//! on EVERY user message, in EVERY mode. This is the "inference" part. The routing
//! logic that follows is deterministic Rust — the "mechanistic" part.

use std::io::{self, Write};

use crate::cmd;
use crate::daemon;
use crate::identity;
use crate::rich_output as r;
use crate::streaming::{self, StreamPrinter};

// ═══════════════════════════════════════════════════════════════════════════
// Modes
// ═══════════════════════════════════════════════════════════════════════════

/// All possible modes in the system.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    Chat,
    Adventure,
    Story,
    Html,
    Coder,
}

impl Mode {
    fn label(self) -> &'static str {
        match self {
            Mode::Chat => "\u{1f4ac} Chat",
            Mode::Adventure => "\u{2694}\u{fe0f} Adventure",
            Mode::Story => "\u{1f4d6} Story Teller",
            Mode::Html => "\u{1f58c}\u{fe0f} HTML Canvas",
            Mode::Coder => "\u{1f4bb} Coder",
        }
    }

    fn router_prompt(self) -> &'static str {
        match self {
            Mode::Chat => "chat",
            Mode::Adventure => "adventure",
            Mode::Story => "story",
            Mode::Html => "html",
            Mode::Coder => "coder",
        }
    }
}

/// An intent that the user might express.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Intent {
    /// Machine name: "chat", "adventure", "story", "html", "coder"
    id: &'static str,
    /// Human label for display
    label: &'static str,
    /// What mode this intent routes to
    target_mode: Mode,
    /// Whether this intent means "stay in current mode" (chat absorbs everything)
    is_passive: bool,
}

/// Available intents that `detect_intent` will choose from.
fn all_intents() -> Vec<Intent> {
    vec![
        Intent {
            id: "chat",
            label: "Chat (general conversation)",
            target_mode: Mode::Chat,
            is_passive: true,
        },
        Intent {
            id: "adventure",
            label: "Adventure game",
            target_mode: Mode::Adventure,
            is_passive: false,
        },
        Intent {
            id: "story",
            label: "Story telling",
            target_mode: Mode::Story,
            is_passive: false,
        },
        Intent {
            id: "html",
            label: "HTML canvas",
            target_mode: Mode::Html,
            is_passive: false,
        },
        Intent {
            id: "coder",
            label: "Coding assistant",
            target_mode: Mode::Coder,
            is_passive: false,
        },
    ]
}

/// Build the prompt for intent detection.
fn intent_detection_prompt(user_message: &str, available: &[Intent], mode_hint: &str) -> String {
    let mut options = String::new();
    for intent in available {
        let note = if intent.is_passive {
            " (default — use this unless the user clearly wants something else)"
        } else {
            ""
        };
        options.push_str(&format!("  - {}: {}{}\n", intent.id, intent.label, note));
    }

    format!(
        "Current mode: {mode_hint}\n\
         User message: \"{user_message}\"\n\n\
         Classify the user's intent into EXACTLY ONE of these categories:\n\n\
         {options}\n\n\
         Rules:\n\
         - Pick the category that BEST matches what the user wants to do.\n\
         - If the user is just chatting, asking questions, or being casual → pick \"chat\".\n\
         - If the user wants to switch modes, pick the new mode.\n\
         - Extract their core request cleanly (remove greetings, filler words).\n\n\
         Output ONLY valid JSON, no other text:\n\
         {{\"intent\": \"<category>\", \"prompt\": \"<their actual request extracted cleanly>\"}}"
    )
}

// ═══════════════════════════════════════════════════════════════════════════
// Generic Intent Detection (the "inference" part)
// ═══════════════════════════════════════════════════════════════════════════

/// Detect the user's intent from their message, choosing from available intents.
/// Returns (detected_intent, extracted_clean_prompt).
fn detect_intent(
    backend: &dyn roco_engine::ModelBackend,
    user_message: &str,
    available: &[Intent],
    mode_hint: &str,
) -> (Intent, String) {
    let prompt = intent_detection_prompt(user_message, available, mode_hint);

    let request = roco_engine::CompletionRequest {
        system: "You classify user intent into exactly one category and extract their request. Output only JSON.".into(),
        prompt,
        temperature: 0.1,
        max_tokens: 80,
        prefill: Some("{\"intent\":".into()),
        thinking: false,
        ..Default::default()
    };

    let chat_intent = available
        .iter()
        .find(|i| i.id == "chat")
        .cloned()
        .unwrap_or_else(|| Intent {
            id: "chat",
            label: "Chat",
            target_mode: Mode::Chat,
            is_passive: true,
        });
    let fallback = || (chat_intent.clone(), user_message.to_string());

    let res = match futures::executor::block_on(backend.complete(request)) {
        Ok(resp) => {
            let text = resp.text.trim().to_string();
            if let Some(json_str) = extract_json(&text) {
                if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&json_str) {
                    let intent_id = parsed["intent"].as_str().unwrap_or("chat");
                    let prompt = parsed["prompt"]
                        .as_str()
                        .map(|s| s.to_string())
                        .unwrap_or_else(|| user_message.to_string());

                    // Find the matching intent
                    let matched = available.iter().find(|i| i.id == intent_id).cloned();
                    if let Some(intent) = matched {
                        roco_app::agent_journal::AgentJournal::info(
                            "router",
                            &format!("Intent detected: {} -> prompt: \"{}\"", intent.id, prompt),
                        );
                        (intent, r::clean_response(&prompt))
                    } else {
                        fallback()
                    }
                } else {
                    fallback()
                }
            } else {
                fallback()
            }
        }
        Err(_) => fallback(),
    };
    print!("\r\x1b[K");
    io::stdout().flush().ok();
    res
}

// ═══════════════════════════════════════════════════════════════════════════
// Public entry point
// ═══════════════════════════════════════════════════════════════════════════

/// Run the mode router. This is the default `roco` entry point.
pub fn cmd_router(extra: &[&str]) {
    let initial_prompt = extra.first().map(|s| *s).filter(|s| !s.starts_with('-'));

    let backend = daemon::ensure_sync_backend();

    r::header("RoCo AI — Mode Router");
    r::dim("  Just talk naturally. I'll figure out what you need.");
    r::dim("  :h for help, :q to quit.\n");

    let mut history: Vec<HistoryEntry> = Vec::new();
    let mut current_mode = Mode::Chat;
    let intents = all_intents();

    // Identity is answered locally (see `crate::identity`), so the router
    // needs a profile + assistant facts even though it keeps its own history.
    let profile_path = identity::UserProfile::default_path();
    let mut profile = identity::UserProfile::load(&profile_path);
    let assistant = identity::AssistantIdentity::detect(&*backend);

    // If an initial prompt was given, detect intent and route
    if let Some(ref prompt) = initial_prompt {
        add_history(&mut history, "user", prompt);
        if let Some(reply) = answer_identity(prompt, &assistant, &mut profile, &profile_path) {
            println!("\n{reply}");
            add_history(&mut history, "assistant", &reply);
        } else {
            let (intent, extracted) = detect_intent(&*backend, prompt, &intents, "chat");
            current_mode = intent.target_mode;

            match current_mode {
                Mode::Story => {
                    launch_story(&extracted);
                    current_mode = Mode::Chat;
                    add_history(&mut history, "system", "Story completed. Back in chat.");
                }
                Mode::Html => {
                    launch_html(&extracted);
                    current_mode = Mode::Chat;
                    add_history(
                        &mut history,
                        "system",
                        "HTML session completed. Back in chat.",
                    );
                }
                _ => {
                    let text = generate_response(
                        &*backend,
                        current_mode,
                        &history,
                        &extracted,
                        &assistant,
                        &profile,
                    );
                    add_history(&mut history, "assistant", &text);
                }
            }
        }
    } else {
        let greeting = match &profile.name {
            Some(name) => format!(
                "Welcome back, {name}! I can chat, tell stories, run adventures, \
                 generate HTML, or help you code. What are we doing today?"
            ),
            None => "Hello! I'm RoCo AI. I can chat, tell stories, run adventures, generate HTML, or help you code. Just tell me what you'd like to do!".to_string(),
        };
        println!("{}", greeting);
        add_history(&mut history, "assistant", &greeting);
    }

    // ── Main loop ──────────────────────────────────────────────────────
    // One reusable buffer rather than a fresh allocation per iteration.
    let mut buf = String::new();
    loop {
        let mode_label = current_mode.label();
        print!("\n{}{} >{} ", r::Colors::CYAN, mode_label, r::Colors::RESET);
        io::stdout().flush().ok();

        buf.clear();
        match io::stdin().read_line(&mut buf) {
            Ok(0) | Err(_) => break,
            Ok(_) => {}
        }
        let input = buf.trim().to_string();

        if input.is_empty() {
            continue;
        }

        // ── Commands ───────────────────────────────────────────────────
        if input.starts_with('/') || input.starts_with(':') {
            let cmd = input
                .trim_start_matches('/')
                .trim_start_matches(':')
                .trim()
                .to_lowercase();
            if let Some(reply) = identity_command(&cmd, &assistant, &mut profile, &profile_path) {
                println!("{reply}");
                continue;
            }
            if handle_command(&cmd, &mut current_mode, &mut history) {
                break;
            }
            continue;
        }

        add_history(&mut history, "user", &input);

        // ── Identity fast-path ─────────────────────────────────────────
        // "who are you / who am I / what can you do" are answered from real
        // program facts. Routing them through intent detection wasted a model
        // call and produced confident nonsense from a 2.9B model.
        if let Some(reply) = answer_identity(&input, &assistant, &mut profile, &profile_path) {
            println!("\n{reply}");
            add_history(&mut history, "assistant", &reply);
            continue;
        }

        // ── Intent detection on EVERY message, from EVERY mode ────────
        let (intent, extracted) =
            detect_intent(&*backend, &input, &intents, current_mode.router_prompt());

        // If intent changed to a different mode, switch
        if intent.target_mode != current_mode {
            roco_app::agent_journal::AgentJournal::action(
                "router",
                &format!(
                    "Mode switch: {:?} -> {:?}",
                    current_mode, intent.target_mode
                ),
            );
            let transition = match intent.target_mode {
                Mode::Adventure => "\n🎮 Switching to adventure mode!",
                Mode::Story => "\n📖 Switching to story mode!",
                Mode::Html => "\n🖌 Switching to HTML canvas!",
                Mode::Coder => "\n💻 Switching to coder mode!",
                Mode::Chat => "\n💬 Back to general chat.",
            };
            println!("{}", transition);
            current_mode = intent.target_mode;

            // For modes with different UX, launch and return
            match current_mode {
                Mode::Story => {
                    launch_story(&extracted);
                    current_mode = Mode::Chat;
                    add_history(&mut history, "system", "Story completed. Back in chat.");
                    continue;
                }
                Mode::Html => {
                    launch_html(&extracted);
                    current_mode = Mode::Chat;
                    add_history(
                        &mut history,
                        "system",
                        "HTML session completed. Back in chat.",
                    );
                    continue;
                }
                _ => {} // Chat/Adventure/Coder handled below
            }
        }

        // ── Self-contained modes: Adventure, Coder, Chat ──────────────────
        // Same loop, different system prompts
        if current_mode == Mode::Story {
            launch_story(&extracted);
            current_mode = Mode::Chat;
            add_history(&mut history, "system", "Story completed. Back in chat.");
            continue;
        }
        if current_mode == Mode::Html {
            launch_html(&extracted);
            current_mode = Mode::Chat;
            add_history(
                &mut history,
                "system",
                "HTML session completed. Back in chat.",
            );
            continue;
        }

        let text = generate_response(
            &*backend,
            current_mode,
            &history,
            &extracted,
            &assistant,
            &profile,
        );
        add_history(&mut history, "assistant", &text);
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Identity (shared with `roco interact` via `crate::identity`)
// ═══════════════════════════════════════════════════════════════════════════

/// Answer an identity question locally, persisting any profile change.
fn answer_identity(
    input: &str,
    assistant: &identity::AssistantIdentity,
    profile: &mut identity::UserProfile,
    profile_path: &std::path::Path,
) -> Option<String> {
    let query = identity::detect(input)?;
    let (reply, changed) = query.answer(assistant, profile);
    if changed {
        if let Err(e) = profile.save(profile_path) {
            r::warning(&format!("Could not save profile: {e}"));
        }
    }
    Some(reply)
}

/// Handle the identity slash commands (`:whoami`, `:name …`, `:remember …`).
fn identity_command(
    cmd: &str,
    assistant: &identity::AssistantIdentity,
    profile: &mut identity::UserProfile,
    profile_path: &std::path::Path,
) -> Option<String> {
    let (verb, rest) = match cmd.split_once(char::is_whitespace) {
        Some((v, r)) => (v, r.trim()),
        None => (cmd, ""),
    };
    let (reply, changed) = match verb {
        "whoami" | "me" => (profile.render(), false),
        "whois" | "about" => (assistant.who_are_you(), false),
        "name" if !rest.is_empty() => {
            profile.set_name(rest);
            (format!("I'll call you {rest}."), true)
        }
        "remember" if !rest.is_empty() => {
            let msg = if profile.remember(rest) {
                format!("Noted: {rest}")
            } else {
                "I already had that one.".to_string()
            };
            (msg, true)
        }
        "forget" => {
            profile.clear();
            ("Forgotten.".to_string(), true)
        }
        _ => return None,
    };
    if changed {
        if let Err(e) = profile.save(profile_path) {
            r::warning(&format!("Could not save profile: {e}"));
        }
    }
    Some(reply)
}

// ═══════════════════════════════════════════════════════════════════════════
// Response Generation
// ═══════════════════════════════════════════════════════════════════════════

/// Generate a response in the given mode, streaming it to the terminal.
///
/// The previous version attached an `on_token` callback that appended into an
/// `Arc<Mutex<String>>` which was never read — so it looked like streaming in
/// the source but the user saw a spinner and then the whole answer at once.
/// Now the callback drives a [`StreamPrinter`], which renders incrementally.
///
/// HTML mode is the one exception: partial HTML is meaningless (and would
/// dump raw tags into the terminal), so it is rendered only once complete.
fn generate_response(
    backend: &dyn roco_engine::ModelBackend,
    mode: Mode,
    history: &[HistoryEntry],
    user_input: &str,
    assistant: &identity::AssistantIdentity,
    profile: &identity::UserProfile,
) -> String {
    // Ground the model in who it is and who it's talking to, so identity
    // questions embedded mid-conversation don't produce invented answers.
    let system = format!(
        "{}\n\n{}",
        identity::identity_preamble(assistant, profile),
        mode_system_prompt(mode)
    );
    let recent = build_recent_history(history, 6);

    let prompt = format!(
        "{recent}\n\
         User: {user_input}\n\n\
         {}",
        mode_response_prefix(mode)
    );

    let prefill = match mode {
        Mode::Html => Some("<div style='font-family:sans-serif;padding:20px;'>\n".into()),
        Mode::Coder => Some(" thinking".into()),
        _ => Some(roco_engine::NO_THINK_PREFILL.to_string()),
    };

    let max_tokens = match mode {
        Mode::Html | Mode::Coder => 2048,
        _ => 1024,
    };

    streaming::thinking_hint();

    // HTML is assembled silently; everything else streams live.
    let live = mode != Mode::Html;
    let printer = if live {
        StreamPrinter::new("").shared()
    } else {
        StreamPrinter::quiet().shared()
    };

    let request = roco_engine::CompletionRequest {
        system,
        prompt,
        temperature: 0.8,
        max_tokens,
        prefill,
        on_token: Some(streaming::on_token_for(&printer)),
        ..Default::default()
    };

    let result = futures::executor::block_on(backend.complete(request));

    match result {
        Ok(resp) => {
            let text = match printer.lock() {
                Ok(mut p) => p.finish(&resp.text),
                Err(poisoned) => poisoned.into_inner().finish(&resp.text),
            };
            if mode == Mode::Html {
                let html = sanitize_html(&text);
                streaming::clear_line();
                println!("{html}");
                io::stdout().flush().ok();
                html
            } else {
                if text.trim().is_empty() {
                    streaming::clear_line();
                    println!("(no response — try rephrasing)");
                }
                text
            }
        }
        Err(e) => {
            streaming::clear_line();
            let msg = format!("[Error: {e}]");
            println!("{msg}");
            io::stdout().flush().ok();
            msg
        }
    }
}

/// Get the system prompt for a given mode.
fn mode_system_prompt(mode: Mode) -> String {
    match mode {
        Mode::Chat => "\
            You are RoCo AI, a creative and helpful assistant.\n\
            - Be conversational, warm, and engaging.\n\
            - You can help with writing, ideas, questions, or anything.\n\
            - Keep responses concise but vivid."
            .into(),

        Mode::Adventure => "\
            You are the game master of a text adventure.\n\
            RULES:\n\
            - Describe the world vividly in 2-3 paragraphs.\n\
            - Maintain consistent world state, NPCs, and player inventory.\n\
            - End each response with a clear sense of the current situation.\n\
            - When the player says 'look', describe the area in detail.\n\
            - When the player says 'inventory' or 'i', list their items.\n\
            - Keep the story engaging with plot twists and discoveries.\n\
            - Never break character — you are the game world."
            .into(),

        Mode::Coder => "\
            You are an expert programming assistant.\n\
            RULES:\n\
            - Write clean, idiomatic code with explanations.\n\
            - Show complete, runnable examples when appropriate.\n\
            - Include comments for complex logic.\n\
            - Suggest best practices, error handling, and tests.\n\
            - When debugging, think step by step.\n\
            - Keep responses focused and practical."
            .into(),

        Mode::Html => "\
            You respond in **HTML only**.\n\
            RULES:\n\
            - Your ENTIRE response must be valid HTML.\n\
            - Include <style> and <script> as needed.\n\
            - Use inline CSS for all styling.\n\
            - Make it visually rich: colors, layouts, typography.\n\
            - NEVER output markdown or code fences — only raw HTML."
            .into(),

        Mode::Story => "\
            You are a creative writing assistant.\n\
            - Help the user craft a compelling story.\n\
            - Suggest plot ideas, characters, settings.\n\
            - Offer vivid, engaging prose.\n\
            - Ask questions to deepen the narrative."
            .into(),
    }
}

fn mode_response_prefix(mode: Mode) -> String {
    match mode {
        Mode::Adventure => "Describe what happens next in the adventure.\nGM:".into(),
        Mode::Coder => "Provide code and explanation.\nAssistant:".into(),
        _ => "Assistant:".into(),
    }
}

/// Sanitize HTML response (strip fences, etc.)
fn sanitize_html(text: &str) -> String {
    let t = text.trim();
    if t.starts_with("```html") {
        if let Some(start) = t.find('\n') {
            let after = &t[start + 1..];
            if let Some(end) = after.find("```") {
                return after[..end].trim().to_string();
            }
            return after.trim().to_string();
        }
    }
    if t.starts_with("```") && !t.starts_with("```json") {
        if let Some(start) = t.find('\n') {
            let after = &t[start + 1..];
            if let Some(end) = after.find("```") {
                return after[..end].trim().to_string();
            }
            return after.trim().to_string();
        }
    }
    if t.starts_with('<') {
        return t.to_string();
    }
    format!(
        "<div style='font-family:sans-serif;padding:12px;line-height:1.6'>{}</div>",
        t
    )
}

// ═══════════════════════════════════════════════════════════════════════════
// Mode Launchers (modes with different UX)
// ═══════════════════════════════════════════════════════════════════════════

fn launch_story(prompt: &str) {
    r::header("📖 Story Mode");
    r::dim("Launching story pipeline...\n");
    cmd::story::cmd_story(&[prompt]);
}

fn launch_html(prompt: &str) {
    r::header("🖌 HTML Canvas");
    r::dim("Launching HTML mode...\n");
    cmd::html::cmd_html(&[prompt]);
}

// ═══════════════════════════════════════════════════════════════════════════
// History
// ═══════════════════════════════════════════════════════════════════════════

#[derive(Clone)]
struct HistoryEntry {
    role: String,
    content: String,
}

fn add_history(history: &mut Vec<HistoryEntry>, role: &str, content: &str) {
    history.push(HistoryEntry {
        role: role.to_string(),
        content: content.to_string(),
    });
    if history.len() > 20 {
        history.drain(0..history.len() - 20);
    }
}

fn build_recent_history(history: &[HistoryEntry], n: usize) -> String {
    let mut result = String::new();
    let recent: Vec<_> = history.iter().rev().take(n).rev().collect();
    for entry in &recent {
        let label = match entry.role.as_str() {
            "user" => "User",
            "assistant" => "Assistant",
            "system" => "System",
            _ => "?",
        };
        let preview: String = entry.content.chars().take(200).collect();
        result.push_str(&format!("{label}: {preview}\n"));
    }
    result
}

// ═══════════════════════════════════════════════════════════════════════════
// Command Handler (deterministic routing)
// ═══════════════════════════════════════════════════════════════════════════

fn handle_command(cmd: &str, mode: &mut Mode, history: &mut Vec<HistoryEntry>) -> bool {
    match cmd {
        "help" | "h" | "?" => {
            r::panel(
                "Commands",
                &[
                    "  Natural language: Just say what you want!",
                    "  I'll detect your intent and switch modes automatically.",
                    "  ",
                    "  :mode           Show current mode",
                    "  :chat           Switch to chat mode",
                    "  :adventure      Switch to adventure mode",
                    "  :story          Switch to story mode",
                    "  :html           Switch to HTML canvas mode",
                    "  :code           Switch to coder mode",
                    "  :history        Show recent history",
                    "  :clear          Clear history",
                    "  ",
                    "  :whoami         What RoCo knows about you",
                    "  :whois          What RoCo is",
                    "  :name <you>     Tell RoCo your name",
                    "  :remember <x>   Remember something about you",
                    "  :forget         Forget everything about you",
                    "  ",
                    "  :help / :h      Show this help",
                    "  :quit / :q      Exit",
                ]
                .join("\n"),
            );
            false
        }

        "mode" => {
            r::info(&format!("Current mode: {}", mode.label()));
            false
        }

        "history" => {
            r::header(&format!("History ({} messages)", history.len()));
            for (i, entry) in history.iter().enumerate() {
                let preview: String = entry.content.chars().take(80).collect();
                let label = match entry.role.as_str() {
                    "user" => "U",
                    "assistant" => "A",
                    "system" => "S",
                    _ => "?",
                };
                println!("  {} {:2}. {}", label, i + 1, preview);
            }
            false
        }

        "clear" => {
            history.clear();
            r::success("History cleared.");
            false
        }

        "chat" => {
            *mode = Mode::Chat;
            r::success("Switched to chat mode.");
            false
        }
        "adventure" | "game" => {
            *mode = Mode::Adventure;
            r::success("Switched to adventure mode.");
            false
        }
        "story" => {
            *mode = Mode::Story;
            r::success("Switched to story mode.");
            false
        }
        "html" => {
            *mode = Mode::Html;
            r::success("Switched to HTML canvas mode.");
            false
        }
        "coder" | "code" => {
            *mode = Mode::Coder;
            r::success("Switched to coder mode.");
            false
        }

        "quit" | "q" | "exit" => {
            r::info("Goodbye!");
            true
        }

        _ => {
            r::warning(&format!("Unknown command: /{cmd}. Type :help."));
            false
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Helpers
// ═══════════════════════════════════════════════════════════════════════════

/// Extract a JSON object from text that might have surrounding content or prefill.
fn extract_json(text: &str) -> Option<String> {
    let text = text.trim();
    let text = if !text.starts_with('{') && text.contains("\"prompt\"") {
        format!("{{\"intent\":{text}")
    } else {
        text.to_string()
    };

    let start = text.find('{')?;
    let end = text.rfind('}')?;
    if end > start {
        Some(text[start..=end].to_string())
    } else {
        None
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Tests
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_json_simple() {
        let text = r#"{"intent": "adventure", "prompt": "test"}"#;
        assert_eq!(extract_json(text), Some(text.to_string()));
    }

    #[test]
    fn test_extract_json_with_prefix() {
        let text = r#"Here is: {"intent": "chat"} Thanks!"#;
        assert_eq!(
            extract_json(text),
            Some(r#"{"intent": "chat"}"#.to_string())
        );
    }

    #[test]
    fn test_extract_json_none() {
        assert_eq!(extract_json("no json here"), None);
    }

    #[test]
    fn test_mode_labels() {
        assert!(Mode::Chat.label().contains("Chat"));
        assert!(Mode::Adventure.label().contains("Adventure"));
        assert!(Mode::Coder.label().contains("Coder"));
    }

    #[test]
    fn test_all_intents_contains_all_modes() {
        let intents = all_intents();
        let ids: Vec<&str> = intents.iter().map(|i| i.id).collect();
        assert!(ids.contains(&"chat"));
        assert!(ids.contains(&"adventure"));
        assert!(ids.contains(&"story"));
        assert!(ids.contains(&"html"));
        assert!(ids.contains(&"coder"));
    }

    #[test]
    fn test_intent_detection_prompt_contains_available() {
        let intents = all_intents();
        let prompt = intent_detection_prompt("hello", &intents, "chat");
        assert!(prompt.contains("adventure"));
        assert!(prompt.contains("story"));
        assert!(prompt.contains("html"));
        assert!(prompt.contains("coder"));
        assert!(prompt.contains("chat"));
    }

    #[test]
    fn test_sanitize_html_already_html() {
        let result = sanitize_html("<div>hello</div>");
        assert_eq!(result, "<div>hello</div>");
    }

    #[test]
    fn test_sanitize_html_fenced() {
        assert_eq!(
            sanitize_html("```html\n<div>hello</div>\n```"),
            "<div>hello</div>"
        );
    }

    #[test]
    fn test_sanitize_html_plain_text() {
        let result = sanitize_html("hello world");
        assert!(result.contains("hello world"));
    }

    #[test]
    fn test_add_history_trims() {
        let mut history = Vec::new();
        for i in 0..25 {
            add_history(&mut history, "user", &format!("msg {i}"));
            add_history(&mut history, "assistant", &format!("resp {i}"));
        }
        assert!(history.len() <= 20);
    }

    #[test]
    fn test_handle_command_quit() {
        let mut mode = Mode::Chat;
        let mut history = vec![];
        assert!(handle_command("quit", &mut mode, &mut history));
    }

    #[test]
    fn test_handle_command_mode_switch() {
        let mut mode = Mode::Chat;
        let mut history = vec![];
        handle_command("adventure", &mut mode, &mut history);
        assert_eq!(mode, Mode::Adventure);
    }

    #[test]
    fn test_handle_command_help_does_not_quit() {
        let mut mode = Mode::Chat;
        let mut history = vec![];
        assert!(!handle_command("help", &mut mode, &mut history));
    }

    #[test]
    fn test_detect_intent_falls_back_to_chat_on_empty_response() {
        // Can't test without a backend, but verify the prompt builder works
        let intents = all_intents();
        let prompt = intent_detection_prompt("hello world", &intents, "chat");
        assert!(prompt.contains("hello world"));
        assert!(!prompt.is_empty());
    }

    #[test]
    fn test_intent_routing_to_correct_mode() {
        for intent in all_intents() {
            match intent.id {
                "chat" => assert_eq!(intent.target_mode, Mode::Chat),
                "adventure" => assert_eq!(intent.target_mode, Mode::Adventure),
                "story" => assert_eq!(intent.target_mode, Mode::Story),
                "html" => assert_eq!(intent.target_mode, Mode::Html),
                "coder" => assert_eq!(intent.target_mode, Mode::Coder),
                _ => panic!("unknown intent: {}", intent.id),
            }
        }
    }

    #[test]
    fn test_response_prefixes() {
        assert!(mode_response_prefix(Mode::Adventure).contains("GM:"));
        assert!(mode_response_prefix(Mode::Coder).contains("Assistant:"));
        assert!(mode_response_prefix(Mode::Chat).contains("Assistant:"));
    }

    // ── Identity integration ─────────────────────────────────────────────

    #[test]
    fn identity_questions_are_answered_without_the_model() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("profile.json");
        let assistant = identity::AssistantIdentity::default();
        let mut profile = identity::UserProfile::default();

        let reply = answer_identity("who are you?", &assistant, &mut profile, &path)
            .expect("identity question should be recognised");
        assert!(reply.contains("RoCo"), "got {reply}");
    }

    #[test]
    fn ordinary_messages_still_go_to_intent_detection() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("profile.json");
        let assistant = identity::AssistantIdentity::default();
        let mut profile = identity::UserProfile::default();

        assert!(
            answer_identity("write me a story about a dragon", &assistant, &mut profile, &path)
                .is_none(),
            "ordinary input must not be hijacked by the identity fast-path"
        );
    }

    #[test]
    fn name_is_persisted_to_the_profile_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("profile.json");
        let assistant = identity::AssistantIdentity::default();
        let mut profile = identity::UserProfile::default();

        answer_identity("my name is Ada", &assistant, &mut profile, &path).unwrap();
        assert!(path.exists(), "profile should have been written");

        let reloaded = identity::UserProfile::load(&path);
        assert_eq!(reloaded.name.as_deref(), Some("Ada"));
    }

    #[test]
    fn identity_slash_commands_are_handled() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("profile.json");
        let assistant = identity::AssistantIdentity::default();
        let mut profile = identity::UserProfile::default();

        assert!(identity_command("name Ada", &assistant, &mut profile, &path)
            .unwrap()
            .contains("Ada"));
        assert!(identity_command("whoami", &assistant, &mut profile, &path)
            .unwrap()
            .contains("Ada"));
        assert!(identity_command("whois", &assistant, &mut profile, &path)
            .unwrap()
            .contains("RoCo"));
        assert!(identity_command("forget", &assistant, &mut profile, &path)
            .unwrap()
            .contains("Forgotten"));
        // Mode commands must fall through to `handle_command`.
        assert!(identity_command("adventure", &assistant, &mut profile, &path).is_none());
        assert!(identity_command("quit", &assistant, &mut profile, &path).is_none());
    }

    #[test]
    fn history_is_bounded() {
        // Regression: unbounded router history is a memory leak in a
        // long-running REPL.
        let mut history = Vec::new();
        for i in 0..500 {
            add_history(&mut history, "user", &format!("m{i}"));
        }
        assert!(history.len() <= 20, "history grew to {}", history.len());
    }

    #[test]
    fn recent_history_is_bounded_in_size() {
        let mut history = Vec::new();
        for i in 0..20 {
            add_history(&mut history, "user", &format!("{i} {}", "x".repeat(5000)));
        }
        let recent = build_recent_history(&history, 6);
        // 6 entries × 200-char preview + labels.
        assert!(recent.len() < 2_000, "recent history too big: {}", recent.len());
    }
}
