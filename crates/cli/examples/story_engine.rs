//! Interactive Story Engine — dynamic, unlimited story generation.
//!
//! This example demonstrates the new story engine features:
//! - Dynamic outline expansion (no fixed chapter limit)
//! - Plot state tracking (structured, not raw text)
//! - Interactive mode (human-in-the-loop)
//! - Chapter continuation (resume from where left off)
//!
//! Usage:
//!   # Generate with default settings (3 chapters, non-interactive)
//!   RWKV_MODEL=... cargo run --release --example story_engine -p roco-cli \
//!     "Write a xianxia story about a lone cultivator"
//!
//!   # Interactive mode
//!   RWKV_MODEL=... cargo run --release --example story_engine -p roco-cli \
//!     --interactive "Write a dark fantasy"
//!
//!   # Unlimited chapters (continues until arc completes)
//!   RWKV_MODEL=... cargo run --release --example story_engine -p roco-cli \
//!     --unlimited "Write an epic saga"
//!
//!   # Specify chapter count
//!   RWKV_MODEL=... cargo run --release --example story_engine -p roco-cli \
//!     --chapters 10 "Write a mystery novel"

use std::io::{self, Write};

use roco_agent::interaction::{HumanAction, InteractionMode};
use roco_agent::story_engine::{StoryConfig, StoryEngine};
use roco_inference::RwkvBackend;

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    // Parse command line arguments
    let args: Vec<String> = std::env::args().collect();
    let mut config = StoryConfig::default();
    let mut premise = String::new();
    let mut i = 1;

    while i < args.len() {
        match args[i].as_str() {
            "--interactive" => {
                config.interactive = true;
                config.interaction_mode = InteractionMode::FullControl;
                i += 1;
            }
            "--batch" => {
                i += 1;
                if i < args.len() {
                    let batch_size = args[i].parse().unwrap_or(3);
                    config.interactive = true;
                    config.interaction_mode = InteractionMode::ModerateControl { batch_size };
                    i += 1;
                }
            }
            "--go-ham" => {
                config.interactive = false;
                config.interaction_mode = InteractionMode::GoHam;
                i += 1;
            }
            "--unlimited" => {
                config.max_chapters = 0;
                i += 1;
            }
            "--chapters" => {
                i += 1;
                if i < args.len() {
                    config.max_chapters = args[i].parse().unwrap_or(10);
                    config.min_chapters = config.max_chapters;
                    i += 1;
                }
            }
            "--words" => {
                i += 1;
                if i < args.len() {
                    config.words_per_chapter = args[i].parse().unwrap_or(400);
                    i += 1;
                }
            }
            _ => {
                if !premise.is_empty() {
                    premise.push(' ');
                }
                premise.push_str(&args[i]);
                i += 1;
            }
        }
    }

    if premise.is_empty() {
        premise =
            "Write a short story about a lighthouse keeper who discovers a message in a bottle."
                .to_string();
    }

    println!("Loading model...");
    let backend = RwkvBackend::from_env()?;
    println!("Model ready.\n");

    // Create story engine
    let mut engine = StoryEngine::new(config.clone())?;
    println!("Workspace: {}\n", engine.workspace_path().display());

    // Phase 1: Generate outline
    println!("📝 Generating outline...");
    engine.generate_outline(&backend, &premise)?;
    println!(
        "✅ Outline generated ({} chapters)\n",
        engine.outline().len()
    );

    // Print outline
    for ch in engine.outline() {
        println!("  Chapter {}: {} - {}", ch.number, ch.title, ch.summary);
    }
    println!();

    // Phase 2: Generate chapters
    let mut chapter_num = 0;
    loop {
        // Check if we need more chapters in the outline
        if chapter_num >= engine.outline().len() {
            if config.max_chapters > 0 && chapter_num >= config.max_chapters {
                println!(
                    "\n🎯 Reached target chapter count ({})",
                    config.max_chapters
                );
                break;
            }

            println!("\n📝 Expanding outline...");
            let expanded = engine.expand_outline(&backend)?;
            if !expanded {
                println!("\n🎯 Story arc complete — no more chapters needed");
                break;
            }
            println!(
                "✅ Outline expanded ({} chapters total)",
                engine.outline().len()
            );
        }

        // Generate next chapter
        chapter_num += 1;
        println!("\n✍️  Generating Chapter {}...", chapter_num);
        let chapter = engine.generate_chapter(&backend)?;
        println!(
            "✅ Chapter {} generated ({} chars)\n",
            chapter_num,
            chapter.len()
        );

        // Show preview
        let preview: String = chapter.chars().take(200).collect();
        println!("Preview:\n{}...\n", preview);

        // Show plot state
        let plot = engine.plot_state();
        println!("📊 Plot State:");
        println!("  Arc stage: {}", plot.arc_stage);
        println!("  Location: {}", plot.current_location);
        println!("  Characters: {}", plot.characters.len());
        println!("  Active conflicts: {}", plot.active_conflicts.len());
        println!();

        // Quality evaluation
        if config.validate_quality {
            println!("🔍 Evaluating quality...");
            match engine.evaluate_chapter_quality(&backend, chapter_num) {
                Ok(critique) => {
                    println!("📊 Quality Score: {:.1}/10", critique.scores.overall);
                    println!("  Pacing: {:.1}/10", critique.scores.pacing);
                    println!("  Engagement: {:.1}/10", critique.scores.engagement);
                    println!("  Plot coherence: {:.1}/10", critique.scores.plot_coherence);

                    if critique.should_revise && config.max_revisions > 0 {
                        println!("\n⚠️  Quality below threshold. Revisions needed:");
                        for (i, rev) in critique.priority_revisions.iter().enumerate() {
                            println!("  {}. {}", i + 1, rev);
                        }

                        // Auto-revise if not interactive
                        if !config.interactive {
                            println!("\n🔄 Auto-revising...");
                            let revised =
                                engine.revise_chapter(&backend, chapter_num, &critique)?;
                            println!("✅ Chapter revised ({} chars)\n", revised.len());
                        }
                    }
                }
                Err(e) => {
                    println!("⚠️  Quality evaluation failed: {e}");
                }
            }
        }

        // Check if we should pause for human input
        if engine.should_pause() {
            println!("\n{}", engine.human_prompt());
            print!("Choice: ");
            io::stdout().flush()?;

            let mut input = String::new();
            io::stdin().read_line(&mut input)?;

            match input.trim().to_lowercase().as_str() {
                "a" | "" => {
                    println!("Continuing...\n");
                    engine.process_human_action(HumanAction::Accept);
                }
                "r" => {
                    println!("🔍 Evaluating for revision...");
                    match engine.evaluate_chapter_quality(&backend, chapter_num) {
                        Ok(critique) => {
                            println!("📊 Current quality: {:.1}/10", critique.scores.overall);
                            if critique.should_revise {
                                println!("\n🔄 Revising based on critique...");
                                let revised =
                                    engine.revise_chapter(&backend, chapter_num, &critique)?;
                                println!("✅ Chapter revised ({} chars)\n", revised.len());
                            } else {
                                println!("✅ Chapter quality is acceptable\n");
                            }
                        }
                        Err(e) => {
                            println!("⚠️  Evaluation failed: {e}\n");
                        }
                    }
                    engine.process_human_action(HumanAction::Revise("revised".to_string()));
                }
                "s" => {
                    println!("Skipping to next chapter...\n");
                    engine.process_human_action(HumanAction::Skip);
                }
                "j" => {
                    print!("Jump to chapter: ");
                    io::stdout().flush()?;
                    let mut jump_input = String::new();
                    io::stdin().read_line(&mut jump_input)?;
                    if let Ok(n) = jump_input.trim().parse::<usize>() {
                        println!("Jumping to chapter {}...\n", n);
                        engine.process_human_action(HumanAction::JumpTo(n));
                    }
                }
                "x" => {
                    println!("Accepting all remaining chapters...\n");
                    engine.process_human_action(HumanAction::AcceptAll);
                }
                "g" => {
                    println!("🚀 Going ham! Running without stopping...\n");
                    engine.process_human_action(HumanAction::GoHam);
                }
                "q" => {
                    println!("\n🛑 Stopping story generation");
                    engine.process_human_action(HumanAction::Stop);
                    break;
                }
                _ => {
                    println!("Unknown command, continuing...\n");
                }
            }
        }

        // Small delay between chapters (non-interactive mode)
        if !config.interactive {
            // Check if we should continue
            if config.max_chapters > 0 && chapter_num >= config.max_chapters {
                println!(
                    "\n🎯 Reached target chapter count ({})",
                    config.max_chapters
                );
                break;
            }
        }
    }

    // Phase 3: Publish
    println!("\n📦 Publishing story...");
    let story = engine.publish()?;
    println!("✅ Story published ({} chars total)\n", story.len());

    // Show quality summary
    let avg_quality = engine.average_quality();
    if avg_quality > 0.0 {
        println!("📊 Quality Summary:");
        println!("  Average score: {:.1}/10", avg_quality);
        println!("  Chapters evaluated: {}", engine.chapter_scores().len());
        println!("  Revisions made: {}", engine.revisions().len());
        println!();
    }

    // Print workspace contents
    println!("📁 Workspace contents:");
    let ws_path = engine.workspace_path();
    if let Ok(entries) = std::fs::read_dir(ws_path) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
            println!("  {} ({} bytes)", name, size);
        }
    }

    println!("\n✨ Story generation complete!");
    println!("   Workspace: {}", ws_path.display());
    println!("   Full story: {}/06-STORY.md", ws_path.display());

    Ok(())
}

/// User actions in interactive mode
enum UserAction {
    Continue,
    Revise,
    Direct(String),
    ContinueChapter,
    Quit,
}

/// Prompt user for action in interactive mode
fn prompt_user_action() -> Result<UserAction, io::Error> {
    println!("What would you like to do?");
    println!("  [c] Continue to next chapter");
    println!("  [r] Revise last chapter");
    println!("  [d] Give direction for next chapter");
    println!("  [x] Continue/extend current chapter");
    println!("  [q] Quit and publish");

    print!("\nChoice: ");
    io::stdout().flush()?;

    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    let input = input.trim().to_lowercase();

    match input.as_str() {
        "c" | "" => Ok(UserAction::Continue),
        "r" => Ok(UserAction::Revise),
        "d" => {
            print!("Direction: ");
            io::stdout().flush()?;
            let mut direction = String::new();
            io::stdin().read_line(&mut direction)?;
            Ok(UserAction::Direct(direction.trim().to_string()))
        }
        "x" => Ok(UserAction::ContinueChapter),
        "q" => Ok(UserAction::Quit),
        _ => {
            println!("Unknown command, continuing...");
            Ok(UserAction::Continue)
        }
    }
}
