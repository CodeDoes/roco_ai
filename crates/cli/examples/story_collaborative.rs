//! Collaborative Story Writing — human-AI partnership.
//!
//! This example demonstrates a conversational, collaborative approach to story writing.
//! The human and AI work together as partners, with the human in control.
//!
//! Philosophy: The human is the author. The AI is the tool.
//!
//! Usage:
//!   RWKV_MODEL=... cargo run --release --example story_collaborative -p roco-cli \
//!     "I want to write a dark fantasy about a fallen knight"

use std::io::{self, Write};

use roco_agent::quality::QualityAnalyzer;
use roco_agent::story_engine::{StoryConfig, StoryEngine};
use roco_inference::RwkvBackend;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let args: Vec<String> = std::env::args().collect();
    let premise = if args.len() > 1 {
        args[1..].join(" ")
    } else {
        "I want to write a short story about a lighthouse keeper who discovers a message in a bottle.".to_string()
    };

    println!("Loading model...");
    let backend = RwkvBackend::from_env()?;
    println!("Model ready.\n");

    // Create story engine with human-friendly defaults
    let config = StoryConfig {
        interactive: true,
        track_plot_state: true,
        validate_quality: true,
        quality_threshold: 6.0,
        max_revisions: 2,
        ..Default::default()
    };

    let mut engine = StoryEngine::new(config)?;
    println!("📁 Workspace: {}\n", engine.workspace_path().display());

    // Start the collaborative process
    println!("✨ Let's write a story together!");
    println!("   You're the author. I'm here to help.\n");

    // Phase 1: Understand the vision
    println!("📝 Your premise: {}", premise);
    println!();

    // Generate initial outline
    println!("🤔 Let me think about how to structure this...");
    engine.generate_outline(&backend, &premise)?;

    println!("📋 Here's what I'm thinking for the outline:\n");
    for ch in engine.outline() {
        println!("  Chapter {}: {}", ch.number, ch.title);
        println!("    {}", ch.summary);
    }
    println!();

    // Ask for feedback on outline
    println!("What do you think? We can:");
    println!("  [y] Looks good, let's start writing");
    println!("  [e] I'd like to edit the outline");
    println!("  [r] Let me rethink the premise");
    println!();

    print!("Your choice: ");
    io::stdout().flush()?;

    let mut input = String::new();
    io::stdin().read_line(&mut input)?;

    match input.trim().to_lowercase().as_str() {
        "y" | "" => {
            println!("\nGreat! Let's start writing.\n");
        }
        "e" => {
            println!("\n📝 Outline editing coming soon!");
            println!("   For now, let's proceed with this outline.\n");
        }
        "r" => {
            println!("\nNo problem! What would you like to change about the premise?");
            print!("New premise: ");
            io::stdout().flush()?;
            let mut new_premise = String::new();
            io::stdin().read_line(&mut new_premise)?;
            println!("\n🔄 Regenerating outline with new premise...");
            engine.generate_outline(&backend, new_premise.trim())?;
            println!("📋 New outline:\n");
            for ch in engine.outline() {
                println!("  Chapter {}: {}", ch.number, ch.title);
                println!("    {}", ch.summary);
            }
            println!();
        }
        _ => {
            println!("\nLet's proceed with this outline.\n");
        }
    }

    // Phase 2: Write chapters collaboratively
    let mut chapter_num = 0;
    loop {
        // Check if we need more chapters
        if chapter_num >= engine.outline().len() {
            println!("\n🤔 We've reached the end of the current outline.");
            println!("   Should we continue the story?");
            println!("  [y] Yes, let's see what happens next");
            println!("  [n] No, let's wrap it up");
            println!();

            print!("Your choice: ");
            io::stdout().flush()?;
            let mut input = String::new();
            io::stdin().read_line(&mut input)?;

            match input.trim().to_lowercase().as_str() {
                "y" | "" => {
                    println!("\n📝 Expanding the outline...");
                    let expanded = engine.expand_outline(&backend)?;
                    if expanded {
                        println!("✅ I've added more chapters to the outline.\n");
                    } else {
                        println!("🎯 The story arc feels complete. Let's wrap up.\n");
                        break;
                    }
                }
                _ => {
                    println!("\nOkay, let's finish up.\n");
                    break;
                }
            }
        }

        // Generate next chapter
        chapter_num += 1;
        let chapter_info = &engine.outline()[chapter_num - 1];

        println!(
            "✍️  Writing Chapter {}: {}",
            chapter_num, chapter_info.title
        );
        println!("   Summary: {}", chapter_info.summary);
        println!();

        let chapter = engine.generate_chapter(&backend)?;

        // Show preview
        println!("📄 Here's what I wrote:\n");
        let preview: String = chapter.chars().take(500).collect();
        println!("{}...", preview);
        println!();

        // Ask for feedback
        println!("What do you think?");
        println!("  [g] Good, continue to next chapter");
        println!("  [r] I'd like some revisions");
        println!("  [d] I have direction for what comes next");
        println!("  [x] I want to extend this chapter");
        println!("  [q] Let's stop here and publish");
        println!();

        print!("Your choice: ");
        io::stdout().flush()?;
        let mut input = String::new();
        io::stdin().read_line(&mut input)?;

        match input.trim().to_lowercase().as_str() {
            "g" | "" => {
                println!("\nGreat! Moving on to the next chapter.\n");
            }
            "r" => {
                println!("\n🔍 Let me evaluate the chapter for quality...");
                match engine.evaluate_chapter_quality(&backend, chapter_num) {
                    Ok(critique) => {
                        println!("📊 Quality: {:.1}/10", critique.scores.overall);
                        if critique.should_revise {
                            println!("\n⚠️  I found some issues:");
                            for (i, rev) in critique.priority_revisions.iter().enumerate() {
                                println!("  {}. {}", i + 1, rev);
                            }
                            println!("\n🔄 Revising...");
                            let revised =
                                engine.revise_chapter(&backend, chapter_num, &critique)?;
                            println!("✅ Chapter revised ({} chars)\n", revised.len());
                        } else {
                            println!("✅ The chapter looks good!\n");
                        }
                    }
                    Err(e) => {
                        println!("⚠️  Couldn't evaluate: {e}\n");
                    }
                }
            }
            "d" => {
                println!("\n📝 What direction would you like for the next chapter?");
                print!("Direction: ");
                io::stdout().flush()?;
                let mut direction = String::new();
                io::stdin().read_line(&mut direction)?;
                println!("\nNoted! I'll keep that in mind for the next chapter.\n");
                // TODO: Store direction and apply to next chapter
            }
            "x" => {
                println!("\n✍️  Extending the chapter...");
                let continued = engine.continue_chapter(
                    &backend,
                    chapter_num,
                    "Continue the scene naturally",
                )?;
                println!("✅ Chapter extended ({} chars total)\n", continued.len());
            }
            "q" => {
                println!("\n🛑 Let's wrap up and publish.\n");
                break;
            }
            _ => {
                println!("\nContinuing...\n");
            }
        }
    }

    // Phase 3: Publish
    println!("\n📦 Publishing your story...");
    let story = engine.publish()?;
    println!("✅ Story published! ({} chars)\n", story.len());

    // Show quality summary
    let avg_quality = engine.average_quality();
    if avg_quality > 0.0 {
        println!("📊 Quality Summary:");
        println!("  Average score: {:.1}/10", avg_quality);
        println!("  Chapters: {}", engine.chapters().len());
        println!("  Revisions: {}", engine.revisions().len());
        println!();
    }

    // Show workspace
    println!("📁 Your story is saved in:");
    println!("   {}", engine.workspace_path().display());
    println!();
    println!("   Files:");
    let ws_path = engine.workspace_path();
    if let Ok(entries) = std::fs::read_dir(ws_path) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
            println!("     {} ({} bytes)", name, size);
        }
    }

    println!("\n✨ Thank you for writing with me!");
    println!("   Your story: {}/06-STORY.md", ws_path.display());
    println!("\n   To continue later:");
    println!(
        "   cargo run --release --example story_full -p roco-cli --resume {}",
        ws_path.display()
    );

    Ok(())
}
