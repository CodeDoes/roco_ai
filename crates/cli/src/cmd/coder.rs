//! Coder mode: `roco code` — AI coding assistant in the terminal.
//!
//! Acts as a knowledgeable programming assistant, writing code, explaining
//! concepts, debugging, and suggesting improvements. Maintains a conversation
//! history for context.

use std::io::{self, Write};

use crate::daemon;
use crate::identity;
use crate::rich_output as r;
use crate::streaming::{self, StreamPrinter};

/// Messages retained in the coder conversation (10 exchanges).
const MAX_CODER_HISTORY: usize = 20;

/// Run the coder mode
pub fn cmd_coder(extra: &[&str]) {
    let language = extra
        .windows(2)
        .find(|w| w[0] == "--lang" || w[0] == "--language")
        .and_then(|w| w.get(1).copied())
        .unwrap_or("rust");
    // The opening question is the first *positional* argument. Using
    // `extra.first()` meant `roco code --lang rust` asked the model the
    // literal question "--lang".
    let initial_prompt = first_positional(extra);

    let backend = daemon::ensure_sync_backend();

    // Ground the model in its own identity so "who are you" mid-session
    // doesn't produce an invented answer; the profile also lets it address
    // the user by name.
    let assistant = identity::AssistantIdentity::detect(&*backend);
    let profile_path = identity::UserProfile::default_path();
    let mut profile = identity::UserProfile::load(&profile_path);

    // `:lang` used to print a confirmation and change nothing — the system
    // prompt was built once, before the loop. It is now rebuilt on change.
    let mut language = language.to_string();
    let mut system_prompt = coder_system_prompt(&language, &assistant, &profile);

    r::header("RoCo AI — Coder Mode");
    r::info(&format!("Language focus: {language}"));
    r::dim("  Ask coding questions.  :h for help, :q to quit.\n");

    // Build conversation history as a simple Vec
    let mut history: Vec<Message> = Vec::new();

    // If initial prompt provided, process it immediately
    if let Some(prompt) = initial_prompt {
        r::header("You");
        r::dim("───");
        println!("{}", prompt);
        r::header("Assistant");
        ask(&*backend, &system_prompt, &mut history, prompt);
    }

    // Interactive loop. One reusable buffer instead of a fresh String per turn.
    let mut buf = String::new();
    loop {
        print!("\n{}💻 >{} ", r::Colors::CYAN, r::Colors::RESET);
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

        // Handle commands
        if input.starts_with('/') || input.starts_with(':') {
            let cmd = input
                .trim_start_matches('/')
                .trim_start_matches(':')
                .trim()
                .to_lowercase();

            match cmd.as_str() {
                "help" | "h" | "?" => {
                    r::panel(
                        "Commands",
                        &[
                            "  :help / :h      Show this help",
                            "  :clear          Clear conversation history",
                            "  :history        Show conversation summary",
                            "  :lang <lang>    Switch language focus",
                            "  :copy           Copy last response (placeholder)",
                            "  :quit / :q      Exit coder mode",
                        ]
                        .join("\n"),
                    );
                    continue;
                }
                "clear" => {
                    history.clear();
                    r::success("Conversation history cleared.");
                    continue;
                }
                "history" => {
                    r::header(&format!("Conversation ({} messages)", history.len()));
                    for (i, msg) in history.iter().enumerate() {
                        let preview: String = msg.content.chars().take(80).collect();
                        let role_label = match msg.role.as_str() {
                            "user" => format!("{}U{}", r::Colors::BLUE, r::Colors::RESET),
                            "assistant" => format!("{}A{}", r::Colors::GREEN, r::Colors::RESET),
                            _ => format!("{}?{}", r::Colors::MAGENTA, r::Colors::RESET),
                        };
                        println!("  {} {:2}. {}", role_label, i + 1, preview);
                    }
                    continue;
                }
                "quit" | "q" | "exit" => {
                    r::info("Happy coding! Goodbye.");
                    break;
                }
                _ if cmd.starts_with("lang") => {
                    match cmd.split_whitespace().nth(1) {
                        Some(new_lang) => {
                            language = new_lang.to_string();
                            // Actually rebuild the system prompt — the old code
                            // printed a confirmation and changed nothing.
                            system_prompt = coder_system_prompt(&language, &assistant, &profile);
                            r::success(&format!("Switched language focus to: {language}"));
                        }
                        None => r::info(&format!("Current language focus: {language}")),
                    }
                    continue;
                }
                "copy" => {
                    if let Some(last) = history.last() {
                        println!("{}", last.content);
                    }
                    continue;
                }
                _ => {
                    // Identity commands shared with every other surface.
                    if let Some(reply) =
                        identity_command(&cmd, &assistant, &mut profile, &profile_path)
                    {
                        println!("{reply}");
                        // A changed profile changes the system prompt.
                        system_prompt = coder_system_prompt(&language, &assistant, &profile);
                        continue;
                    }
                    r::warning(&format!("Unknown command: /{cmd}. Type :help."));
                    continue;
                }
            }
        }

        // Identity questions are answered from real facts, not generated.
        if let Some(query) = identity::detect(&input) {
            let (reply, changed) = query.answer(&assistant, &mut profile);
            if changed {
                if let Err(e) = profile.save(&profile_path) {
                    r::warning(&format!("Could not save profile: {e}"));
                }
                system_prompt = coder_system_prompt(&language, &assistant, &profile);
            }
            println!("\n{reply}");
            continue;
        }

        // Regular coding question
        r::header("You");
        r::dim("───");
        println!("{}", input);
        r::header("Assistant");
        ask(&*backend, &system_prompt, &mut history, &input);
    }
}

/// Send one coding question, streaming the answer, and record the exchange.
///
/// The old inline version printed the answer **twice**: once dribbled out by
/// the `on_token` callback, then again in full via `println!("\n{text}")` after
/// the call returned. It also emitted raw tokens with no `<think>` filtering.
fn ask(
    backend: &dyn roco_engine::ModelBackend,
    system_prompt: &str,
    history: &mut Vec<Message>,
    input: &str,
) {
    streaming::thinking_hint();

    let printer = StreamPrinter::new("").shared();
    let request = roco_engine::CompletionRequest::builder()

        .prompt(build_coder_prompt(history, input))
        .temperature(0.5)
        .max_tokens(2048)
        .prefill(roco_engine::NO_THINK_PREFILL)
        .on_token(streaming::on_token_for(&printer))
        .build();

    let result = futures::executor::block_on(backend.complete(request));

    let text = match result {
        Ok(resp) => match printer.lock() {
            Ok(mut p) => p.finish(&resp.text),
            Err(poisoned) => poisoned.into_inner().finish(&resp.text),
        },
        Err(e) => {
            streaming::clear_line();
            r::error(&format!("Error: {e}"));
            return;
        }
    };

    if text.trim().is_empty() {
        streaming::clear_line();
        r::warning("(no response — try rephrasing)");
        return;
    }

    history.push(Message {
        role: "user".into(),
        content: input.to_string(),
    });
    history.push(Message {
        role: "assistant".into(),
        content: text,
    });
    trim_history(history);
}

/// First argument that is neither a flag nor a flag's value.
fn first_positional<'a>(args: &[&'a str]) -> Option<&'a str> {
    const VALUE_FLAGS: &[&str] = &["--lang", "--language"];

    let mut skip_next = false;
    for arg in args {
        if skip_next {
            skip_next = false;
            continue;
        }
        if arg.starts_with('-') {
            if VALUE_FLAGS.contains(arg) {
                skip_next = true;
            }
            continue;
        }
        return Some(*arg);
    }
    None
}

/// Keep the conversation bounded so a long session can't grow without limit.
fn trim_history(history: &mut Vec<Message>) {
    if history.len() > MAX_CODER_HISTORY {
        history.drain(0..history.len() - MAX_CODER_HISTORY);
    }
}

/// Build the coder system prompt for a language, grounded in identity.
fn coder_system_prompt(
    language: &str,
    assistant: &identity::AssistantIdentity,
    profile: &identity::UserProfile,
) -> String {
    format!(
        "{}\n\n\
         You are an expert programmer and coding assistant.\n\n\
         GUIDELINES:\n\
         - Write clean, idiomatic {language} code.\n\
         - Explain your reasoning concisely before showing code.\n\
         - Show complete, runnable code examples when appropriate.\n\
         - Include comments for complex logic.\n\
         - Suggest best practices, error handling, and tests.\n\
         - When debugging, think step by step.\n\
         - If you're unsure about something, say so.\n\
         - Keep responses focused and practical.\n\n\
         Current language focus: {language}",
        identity::identity_preamble(assistant, profile)
    )
}

/// Handle identity slash commands, persisting profile changes.
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

/// Build a prompt with conversation history for context.
///
/// History entries are included **whole**: the previous code passed complete
/// messages too, but only the last 6 — combined with the unbounded 2048-token
/// answers this mode produces, that could blow the context window. The cap is
/// now on characters as well as turns.
fn build_coder_prompt(history: &[Message], new_input: &str) -> String {
    /// Character budget for replayed history (~1.5k tokens).
    const MAX_HISTORY_CHARS: usize = 6_000;

    let mut selected: Vec<&Message> = Vec::new();
    let mut used = 0usize;
    for msg in history.iter().rev().take(6) {
        let cost = msg.content.len() + msg.role.len() + 4;
        if used + cost > MAX_HISTORY_CHARS {
            break;
        }
        used += cost;
        selected.push(msg);
    }
    selected.reverse();

    let mut prompt = String::with_capacity(used + new_input.len() + 64);
    if !selected.is_empty() {
        prompt.push_str("Previous conversation:\n\n");
        for msg in selected {
            prompt.push_str(&msg.role.to_uppercase());
            prompt.push_str(": ");
            prompt.push_str(&msg.content);
            prompt.push_str("\n\n");
        }
        prompt.push_str("---\n\n");
    }

    prompt.push_str("USER: ");
    prompt.push_str(new_input);
    prompt.push_str("\n\nASSISTANT: ");
    prompt
}

struct Message {
    role: String,
    content: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn msg(role: &str, content: &str) -> Message {
        Message {
            role: role.into(),
            content: content.into(),
        }
    }

    #[test]
    fn test_build_coder_prompt_empty() {
        let history: Vec<super::Message> = vec![];
        let prompt = build_coder_prompt(&history, "Hello");
        assert!(prompt.contains("Hello"));
        assert!(prompt.contains("ASSISTANT:"));
    }

    #[test]
    fn test_build_coder_prompt_with_history() {
        let history = vec![
            super::Message {
                role: "user".into(),
                content: "What is Rust?".into(),
            },
            super::Message {
                role: "assistant".into(),
                content: "Rust is a systems language.".into(),
            },
        ];
        let prompt = build_coder_prompt(&history, "What is ownership?");
        assert!(prompt.contains("What is Rust?"));
        assert!(prompt.contains("Rust is a systems language."));
        assert!(prompt.contains("What is ownership?"));
    }

    #[test]
    fn coder_prompt_respects_a_character_budget() {
        // 2048-token answers × 6 turns would otherwise overflow the context.
        let history: Vec<Message> = (0..6)
            .map(|i| msg("assistant", &format!("{i} {}", "x".repeat(4000))))
            .collect();
        let prompt = build_coder_prompt(&history, "next");
        assert!(prompt.len() < 8_000, "prompt too big: {}", prompt.len());
        // The most recent turn must survive.
        assert!(prompt.contains('5'), "newest turn dropped");
    }

    #[test]
    fn coder_history_is_bounded() {
        let mut history: Vec<Message> = Vec::new();
        for i in 0..200 {
            history.push(msg("user", &format!("q{i}")));
            history.push(msg("assistant", &format!("a{i}")));
            trim_history(&mut history);
        }
        assert!(
            history.len() <= MAX_CODER_HISTORY,
            "history grew to {}",
            history.len()
        );
        assert!(history.last().unwrap().content.contains("a199"));
    }

    #[test]
    fn system_prompt_reflects_the_selected_language() {
        let assistant = identity::AssistantIdentity::default();
        let profile = identity::UserProfile::default();
        let py = coder_system_prompt("python", &assistant, &profile);
        assert!(py.contains("python"));
        assert!(py.contains("RoCo"), "identity preamble must be included");
        assert!(!py.contains("idiomatic rust code"));
    }

    #[test]
    fn flags_are_not_mistaken_for_the_opening_question() {
        // Regression: `roco code --lang rust` used to ask the model "--lang".
        assert_eq!(first_positional(&["--lang", "rust"]), None);
        assert_eq!(
            first_positional(&["--lang", "rust", "how do I sort?"]),
            Some("how do I sort?")
        );
        assert_eq!(
            first_positional(&["how do I sort?"]),
            Some("how do I sort?")
        );
        assert_eq!(first_positional(&[]), None);
    }

    #[test]
    fn identity_commands_are_handled_and_persisted() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("profile.json");
        let assistant = identity::AssistantIdentity::default();
        let mut profile = identity::UserProfile::default();

        assert!(
            identity_command("name Ada", &assistant, &mut profile, &path)
                .unwrap()
                .contains("Ada")
        );
        assert_eq!(
            identity::UserProfile::load(&path).name.as_deref(),
            Some("Ada")
        );
        // Coder-mode commands must fall through.
        assert!(identity_command("clear", &assistant, &mut profile, &path).is_none());
        assert!(identity_command("lang python", &assistant, &mut profile, &path).is_none());
    }
}
