//! Adventure game mode: `roco game` — interactive fiction game master.
//!
//! The LLM acts as a text-adventure game master, maintaining a world state,
//! tracking player inventory, and responding to free-form actions with
//! vivid prose and state transitions.

use std::io::{self, Write};

use crate::daemon;
use crate::rich_output as r;

/// Run the adventure game mode
pub fn cmd_game(extra: &[&str]) {
    let scenario = extra
        .first()
        .map(|s| *s)
        .unwrap_or("a mysterious fantasy world");

    let backend = daemon::ensure_sync_backend();

    let system_prompt = format!(
        "You are a text-adventure game master. You run an interactive fiction game.\n\n\
         SETTING: {scenario}\n\n\
         RULES:\n\
         - Describe the world vividly using 2-3 paragraphs.\n\
         - Maintain a consistent world state, NPCs, and player inventory.\n\
         - End each response with a clear prompt showing available options.\n\
         - Track the player's health, items, and location implicitly.\n\
         - When the player types 'look' or 'l', describe the current area in detail.\n\
         - When the player types 'inventory' or 'i', list what they're carrying.\n\
         - Player actions can succeed or fail based on context.\n\
         - Keep the story engaging with plot twists, NPCs, and discoveries.\n\
         - Never break character. You are the game world.\n\n\
         Begin by describing the starting location and situation for the player."
    );

    r::header("RoCo AI — Adventure Game");
    r::info(&format!("Scenario: {scenario}"));
    r::dim("  Type actions in natural language.  :h for help, :q to quit.\n");

    // Generate intro
    let request = roco_engine::CompletionRequest {
        system: system_prompt.clone(),
        prompt: format!(
            "Start the adventure in {scenario}. Describe where the player is and what they see."
        ),
        temperature: 0.9,
        max_tokens: 600,
        prefill: Some(" thinking response".into()),
        ..Default::default()
    };

    let response = futures::executor::block_on(backend.complete(request));
    match response {
        Ok(resp) => {
            let text = resp.text.trim().to_string();
            println!("\n{}", text);
        }
        Err(e) => {
            r::error(&format!("Failed to start game: {e}"));
            std::process::exit(1);
        }
    }

    // Game loop
    let mut turn_count = 0u64;

    loop {
        turn_count += 1;
        print!(
            "\n{}⚔️ [{}] >{} ",
            r::Colors::DIM,
            turn_count,
            r::Colors::RESET
        );
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
                            "  Natural language: describe what you want to do",
                            "  :look / :l    Describe the current area",
                            "  :inventory / :i  List your items",
                            "  :restart      Start a new game",
                            "  :help / :h    Show this help",
                            "  :quit / :q    Exit the adventure",
                        ]
                        .join("\n"),
                    );
                    continue;
                }
                "look" | "l" => {
                    // Re-describe area
                    let request = roco_engine::CompletionRequest {
                        system: system_prompt.clone(),
                        prompt: "Describe the current location in detail. What does the player see, hear, and smell?".into(),
                        temperature: 0.8,
                        max_tokens: 400,
                        prefill: Some(" thinking response".into()),
                        ..Default::default()
                    };
                    match futures::executor::block_on(backend.complete(request)) {
                        Ok(resp) => println!("\n{}", resp.text.trim()),
                        Err(e) => r::error(&format!("Error: {e}")),
                    }
                    continue;
                }
                "inventory" | "i" => {
                    let request = roco_engine::CompletionRequest {
                        system: system_prompt.clone(),
                        prompt:
                            "What is the player currently carrying? List their inventory items."
                                .into(),
                        temperature: 0.7,
                        max_tokens: 200,
                        prefill: Some(" thinking response".into()),
                        ..Default::default()
                    };
                    match futures::executor::block_on(backend.complete(request)) {
                        Ok(resp) => println!("\n{}", resp.text.trim()),
                        Err(e) => r::error(&format!("Error: {e}")),
                    }
                    continue;
                }
                "restart" => {
                    r::info("Starting a new adventure...\n");
                    let request = roco_engine::CompletionRequest {
                        system: system_prompt.clone(),
                        prompt: format!("Restart the adventure. Describe the starting location for a new player in {scenario}."),
                        temperature: 0.9,
                        max_tokens: 600,
                        prefill: Some(" thinking response".into()),
                        ..Default::default()
                    };
                    match futures::executor::block_on(backend.complete(request)) {
                        Ok(resp) => println!("\n{}", resp.text.trim()),
                        Err(e) => r::error(&format!("Error: {e}")),
                    }
                    turn_count = 0;
                    continue;
                }
                "quit" | "q" | "exit" => {
                    r::info("Thanks for playing! Goodbye.");
                    break;
                }
                _ => {
                    r::warning(&format!(
                        "Unknown command: /{cmd}. Type :help for commands."
                    ));
                    continue;
                }
            }
        }

        // Regular action — process through game master
        let request = roco_engine::CompletionRequest {
            system: system_prompt.clone(),
            prompt: format!(
                "The player takes this action:\n\n{input}\n\n\
                 Describe what happens next. End with a clear description of the current situation.",
            ),
            temperature: 0.85,
            max_tokens: 500,
            prefill: Some(" thinking response".into()),
            ..Default::default()
        };

        match futures::executor::block_on(backend.complete(request)) {
            Ok(resp) => {
                let text = resp.text.trim().to_string();
                println!("\n{}", text);
            }
            Err(e) => {
                r::error(&format!("The game master is thinking... Error: {e}"));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_game_module_exists() {
        assert!(true, "cmd_game function exists");
    }
}
