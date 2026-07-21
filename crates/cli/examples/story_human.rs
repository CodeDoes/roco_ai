//! Story Human — human-centered story writing experience.
//!
//! This example focuses on the actual human experience:
//! - Clear output paths
//! - Easy to understand
//! - Works for different use cases
//! - Shows what's happening
//! - Makes it easy to give feedback
//!
//! Usage:
//!   RWKV_MODEL=... cargo run --release --example story_human -p roco-cli
//!
//! Or with a premise:
//!   RWKV_MODEL=... cargo run --release --example story_human -p roco-cli \
//!     "Write a dark fantasy about a fallen knight"

use std::io::{self, Write};

use roco_agent::interaction::{HumanAction, InteractionMode};
use roco_agent::natural_feedback::FeedbackParser;
use roco_agent::outline_editing::OutlineEditor;
use roco_agent::story_direction::StoryDirection;
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
        prompt("What kind of story do you want to write?")?
    };

    println!("\n✨ Let's write a story together!\n");

    // Phase 1: Set the direction
    println!("🎨 First, let's set the tone for your story.");
    println!("   (Press Enter to skip any question)\n");

    let tone = prompt("Tone (dark, light, humorous, serious):")?;
    let style = prompt("Style (literary, pulp, minimalist):")?;
    let themes = prompt("Themes (comma-separated, e.g., redemption, revenge):")?;
    let pacing = prompt("Pacing (fast, slow, building):")?;

    let mut direction = StoryDirection::new();
    if !tone.is_empty() {
        direction = direction.with_tone(&tone);
    }
    if !style.is_empty() {
        direction = direction.with_style(&style);
    }
    if !themes.is_empty() {
        for theme in themes.split(',') {
            direction = direction.with_theme(theme.trim());
        }
    }
    if !pacing.is_empty() {
        direction = direction.with_pacing(&pacing);
    }

    if !direction.is_empty() {
        println!("\n📋 Direction set:");
        println!("   {}", direction.summary());
    }

    // Phase 2: Create the engine
    let config = StoryConfig {
        interactive: true,
        interaction_mode: InteractionMode::FullControl,
        track_plot_state: true,
        validate_quality: true,
        ..Default::default()
    };

    println!("\n📝 Generating outline based on your premise...");
    let backend = RwkvBackend::from_env()?;
    let mut engine = StoryEngine::new(config)?;
    engine.generate_outline(&backend, &premise)?;

    // Phase 3: Show and edit outline
    println!("\n📋 Here's the outline I came up with:\n");
    for ch in engine.outline() {
        println!("  {}. {}", ch.number, ch.title);
        println!("     {}", ch.summary);
    }

    println!("\n📝 You can edit the outline:");
    println!("   - Type 'add 2 Title: Summary' to add a chapter");
    println!("   - Type 'remove 2' to remove a chapter");
    println!("   - Type 'move 1 to 3' to move a chapter");
    println!("   - Type 'done' when you're happy with the outline");
    println!("   - Type 'skip' to use the outline as-is");

    let mut outline_editor = OutlineEditor::new(engine.outline().to_vec());
    loop {
        let input = prompt("\nOutline command:")?;
        let lower = input.trim().to_lowercase();

        if lower == "done" || lower == "skip" || lower.is_empty() {
            break;
        }

        if let Some(cmd) = outline_editor.parse_command(&input) {
            let result = outline_editor.execute(cmd);
            if result.success {
                println!("✅ {}", result.message);
                println!("\nUpdated outline:");
                for ch in result.outline.iter() {
                    println!("  {}. {}", ch.number, ch.title);
                    println!("     {}", ch.summary);
                }
            } else {
                println!("❌ {}", result.message);
            }
        } else {
            println!("❓ Unknown command. Try: add, remove, move, done");
        }
    }

    // Phase 4: Generate chapters
    println!("\n✍️  Let's start writing!\n");

    let mut chapter_num = 0;
    loop {
        if chapter_num >= engine.outline().len() {
            println!("\n📝 We've reached the end of the outline.");
            println!("   [c] Continue the story (expand outline)");
            println!("   [d] Done - publish the story");

            let choice = prompt("Choice:")?;
            if choice.to_lowercase() == "d" || choice.is_empty() {
                break;
            }

            println!("\n📝 Expanding outline...");
            let expanded = engine.expand_outline(&backend)?;
            if !expanded {
                println!("🎯 The story arc feels complete.");
                break;
            }
        }

        chapter_num += 1;
        let chapter_info = &engine.outline()[chapter_num - 1];

        println!(
            "✍️  Writing Chapter {}: {}",
            chapter_num, chapter_info.title
        );
        println!("   {}\n", chapter_info.summary);

        let chapter = engine.generate_chapter(&backend)?;

        // Show preview
        println!("📄 Here's what I wrote:\n");
        let preview: String = chapter.chars().take(500).collect();
        println!("{}...\n", preview);

        // Ask for feedback
        println!("What do you think?");
        println!("  [Enter] Good, continue");
        println!("  [r] Revise based on quality check");
        println!("  [f] Give feedback");
        println!("  [s] Skip to next chapter");
        println!("  [q] Stop and publish");

        let choice = prompt("Choice:")?;
        let lower = choice.trim().to_lowercase();

        if lower == "q" {
            break;
        } else if lower == "r" {
            println!("\n🔍 Checking quality...");
            match engine.evaluate_chapter_quality(&backend, chapter_num) {
                Ok(critique) => {
                    println!("📊 Quality: {:.1}/10", critique.scores.overall);
                    if critique.should_revise {
                        println!("\n⚠️  Issues found:");
                        for (i, rev) in critique.priority_revisions.iter().enumerate() {
                            println!("  {}. {}", i + 1, rev);
                        }
                        println!("\n🔄 Revising...");
                        let revised = engine.revise_chapter(&backend, chapter_num, &critique)?;
                        println!("✅ Revised ({} chars)\n", revised.len());
                    } else {
                        println!("✅ Quality is good!\n");
                    }
                }
                Err(e) => {
                    println!("⚠️  Couldn't check quality: {e}\n");
                }
            }
        } else if lower == "f" {
            let feedback = prompt("Your feedback:")?;
            if !feedback.is_empty() {
                // Parse feedback
                if let Some(parsed) = FeedbackParser::quick_parse(&feedback) {
                    match parsed.intent {
                        roco_agent::natural_feedback::FeedbackIntent::Continue => {
                            println!("Continuing...\n");
                        }
                        roco_agent::natural_feedback::FeedbackIntent::Skip => {
                            println!("Skipping...\n");
                            continue;
                        }
                        roco_agent::natural_feedback::FeedbackIntent::Stop => {
                            break;
                        }
                        _ => {}
                    }
                } else {
                    // Use model to parse
                    println!("💭 Processing your feedback...");
                    // TODO: Use model to parse and apply feedback
                    println!("Noted! I'll keep that in mind.\n");
                }
            }
        } else if lower == "s" {
            println!("Skipping...\n");
            continue;
        }
    }

    // Phase 5: Publish
    println!("\n📦 Publishing your story...");
    let story = engine.publish()?;
    let workspace = engine.workspace_path();
    let story_path = workspace.join("06-STORY.md");

    println!("\n✨ Story published!");
    println!("   📁 Workspace: {}", workspace.display());
    println!("   📄 Story: {}", story_path.display());
    println!("   📊 Chapters: {}", engine.chapters().len());
    println!("   📝 Words: {}", story.split_whitespace().count());

    // Offer to open
    println!("\n💡 To view your story:");
    println!("   cat {}", story_path.display());
    println!("   open {}", story_path.display());

    // Offer to resume later
    println!("\n💡 To resume later:");
    println!(
        "   cargo run --release --example story_human -p roco-cli --resume {}",
        workspace.display()
    );

    Ok(())
}

fn prompt(message: &str) -> Result<String, io::Error> {
    print!("{} ", message);
    io::stdout().flush()?;
    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    Ok(input.trim().to_string())
}
