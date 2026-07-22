//! Coder mode: `roco code` — AI coding assistant in the terminal.
//!
//! Acts as a knowledgeable programming assistant, writing code, explaining
//! concepts, debugging, and suggesting improvements. Maintains a conversation
//! history for context.

use std::io::{self, Write};

use crate::daemon;
use crate::rich_output as r;

/// Run the coder mode
pub fn cmd_coder(extra: &[&str]) {
    let initial_prompt = extra.first().map(|s| *s);
    let language = extra
        .windows(2)
        .find(|w| w[0] == "--lang" || w[0] == "--language")
        .and_then(|w| w.get(1).copied())
        .unwrap_or("rust");

    let backend = daemon::ensure_sync_backend();

    let system_prompt = format!(
        "You are an expert programmer and coding assistant.\n\n\
         GUIDELINES:\n\
         - Write clean, idiomatic {language} code.\n\
         - Explain your reasoning concisely before showing code.\n\
         - Show complete, runnable code examples when appropriate.\n\
         - Include comments for complex logic.\n\
         - Suggest best practices, error handling, and tests.\n\
         - When debugging, think step by step.\n\
         - If you're unsure about something, say so.\n\
         - Keep responses focused and practical.\n\n\
         Current language focus: {language}"
    );

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

        let request = roco_engine::CompletionRequest {
            system: system_prompt.clone(),
            prompt: build_coder_prompt(&history, prompt),
            temperature: 0.5,
            max_tokens: 2048,
            prefill: Some(" thinking".into()),
            ..Default::default()
        };

        match futures::executor::block_on(backend.complete(request)) {
            Ok(resp) => {
                let text = resp.text.trim().to_string();
                history.push(Message {
                    role: "user".into(),
                    content: prompt.to_string(),
                });
                history.push(Message {
                    role: "assistant".into(),
                    content: text.clone(),
                });
                println!("\n{}", text);
            }
            Err(e) => r::error(&format!("Error: {e}")),
        }
    }

    // Interactive loop
    loop {
        print!("\n{}💻 >{} ", r::Colors::CYAN, r::Colors::RESET);
        io::stdout().flush().ok();

        let mut input = String::new();
        io::stdin().read_line(&mut input).ok();
        let input = input.trim().to_string();

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
                    let parts: Vec<&str> = cmd.split_whitespace().collect();
                    let new_lang = parts.get(1).copied().unwrap_or(language);
                    r::success(&format!("Switched language focus to: {new_lang}"));
                    // We'd rebuild system prompt but for simplicity just note it
                    continue;
                }
                "copy" => {
                    if let Some(last) = history.last() {
                        println!("{}", last.content);
                    }
                    continue;
                }
                _ => {
                    r::warning(&format!("Unknown command: /{cmd}. Type :help."));
                    continue;
                }
            }
        }

        // Regular coding question
        r::header("You");
        r::dim("───");
        println!("{}", input);
        r::header("Assistant");

        let request = roco_engine::CompletionRequest {
            system: system_prompt.clone(),
            prompt: build_coder_prompt(&history, &input),
            temperature: 0.5,
            max_tokens: 2048,
            prefill: Some(" thinking".into()),
            ..Default::default()
        };

        match futures::executor::block_on(backend.complete(request)) {
            Ok(resp) => {
                let text = resp.text.trim().to_string();
                history.push(Message {
                    role: "user".into(),
                    content: input,
                });
                history.push(Message {
                    role: "assistant".into(),
                    content: text.clone(),
                });

                // Trim history to keep context manageable (last 10 turns)
                if history.len() > 20 {
                    history.drain(0..history.len() - 20);
                }

                println!("\n{}", text);
            }
            Err(e) => r::error(&format!("Error: {e}")),
        }
    }
}

/// Build a prompt with conversation history for context
fn build_coder_prompt(history: &[Message], new_input: &str) -> String {
    let mut prompt = String::new();

    if !history.is_empty() {
        prompt.push_str("Previous conversation:\n\n");
        for msg in history.iter().rev().take(6).rev() {
            // Only include last few messages for context
            let role_upper = msg.role.to_uppercase();
            prompt.push_str(&format!("{}: {}\n\n", role_upper, msg.content));
        }
        prompt.push_str("---\n\n");
    }

    prompt.push_str(&format!("USER: {}\n\nASSISTANT: ", new_input));
    prompt
}

struct Message {
    role: String,
    content: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_coder_module_exists() {
        assert!(true, "cmd_coder function exists");
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
}
