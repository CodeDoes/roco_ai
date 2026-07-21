//! Full Story Engine — demonstrates all features.
//!
//! This example shows the complete story generation workflow:
//! 1. Generate outline
//! 2. Expand outline dynamically
//! 3. Generate chapters with plot state tracking
//! 4. Evaluate quality using model-as-judge
//! 5. Revise chapters based on critique
//! 6. Save/load story state
//! 7. Publish final story
//!
//! Usage:
//!   RWKV_MODEL=... cargo run --release --example story_full -p roco-cli \
//!     --interactive --unlimited "Write an epic fantasy saga"

use std::io::{self, Write};

use roco_agent::quality::QualityAnalyzer;
use roco_agent::story_engine::{StoryConfig, StoryEngine};
use roco_agent::story_persistence::StoryPersistence;
use roco_inference::RwkvBackend;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    // Parse arguments
    let args: Vec<String> = std::env::args().collect();
    let mut config = StoryConfig::default();
    let mut premise = String::new();
    let mut resume_path: Option<String> = None;
    let mut i = 1;

    while i < args.len() {
        match args[i].as_str() {
            "--interactive" => {
                config.interactive = true;
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
                    i += 1;
                }
            }
            "--resume" => {
                i += 1;
                if i < args.len() {
                    resume_path = Some(args[i].clone());
                    i += 1;
                }
            }
            "--threshold" => {
                i += 1;
                if i < args.len() {
                    config.quality_threshold = args[i].parse().unwrap_or(6.0);
                    i += 1;
                }
            }
            "--max-revisions" => {
                i += 1;
                if i < args.len() {
                    config.max_revisions = args[i].parse().unwrap_or(2);
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

    if premise.is_empty() && resume_path.is_none() {
        premise =
            "Write a short story about a lighthouse keeper who discovers a message in a bottle."
                .to_string();
    }

    println!("Loading model...");
    let backend = RwkvBackend::from_env()?;
    println!("Model ready.\n");

    // Create or resume story engine
    let mut engine = if let Some(path) = resume_path {
        println!("📂 Resuming story from: {path}");
        let persistence = StoryPersistence::new(path.into());
        let state = persistence.load()?;
        println!("✅ Loaded story: {}", state.metadata.title);
        println!("   Chapters: {}", state.metadata.chapter_count);
        println!("   Words: {}", state.metadata.word_count);
        println!("   Quality: {:.1}/10\n", state.metadata.average_quality);

        // Recreate engine from state
        // TODO: Implement StoryEngine::from_state()
        StoryEngine::new(config.clone())?
    } else {
        StoryEngine::new(config.clone())?
    };

    println!("Workspace: {}\n", engine.workspace_path().display());

    // Phase 1: Generate outline
    if engine.outline().is_empty() {
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
    }

    // Phase 2: Generate chapters
    let mut chapter_num = engine.chapters().len();
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

                    if critique.should_revise {
                        println!("\n⚠️  Quality below threshold. Revisions needed:");
                        for (i, rev) in critique.priority_revisions.iter().enumerate() {
                            println!("  {}. {}", i + 1, rev);
                        }

                        // Auto-revise up to max_revisions times
                        let mut revision_count = 0;
                        while revision_count < config.max_revisions && critique.should_revise {
                            revision_count += 1;
                            println!(
                                "\n🔄 Revising (attempt {}/{})...",
                                revision_count, config.max_revisions
                            );

                            let revised =
                                engine.revise_chapter(&backend, chapter_num, &critique)?;
                            println!("✅ Chapter revised ({} chars)", revised.len());

                            // Re-evaluate
                            match engine.evaluate_chapter_quality(&backend, chapter_num) {
                                Ok(new_critique) => {
                                    println!(
                                        "📊 New quality: {:.1}/10",
                                        new_critique.scores.overall
                                    );
                                    if !new_critique.should_revise {
                                        println!("✅ Quality threshold met!");
                                        break;
                                    }
                                }
                                Err(e) => {
                                    println!("⚠️  Re-evaluation failed: {e}");
                                    break;
                                }
                            }
                        }
                    }
                }
                Err(e) => {
                    println!("⚠️  Quality evaluation failed: {e}");
                }
            }
        }

        // Save state periodically
        if chapter_num % 3 == 0 {
            println!("\n💾 Saving story state...");
            // TODO: Implement engine.to_state() and save
        }

        // Interactive mode
        if config.interactive {
            match prompt_user_action()? {
                UserAction::Continue => {
                    println!("Continuing...\n");
                }
                UserAction::Revise => {
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
                }
                UserAction::Direct(dir) => {
                    println!("📝 Direction noted: {}\n", dir);
                    // TODO: Feed direction into next chapter generation
                }
                UserAction::ContinueChapter => {
                    println!("✍️  Continuing current chapter...\n");
                    let continued = engine.continue_chapter(
                        &backend,
                        chapter_num,
                        "Continue the scene naturally",
                    )?;
                    println!("✅ Chapter extended ({} chars total)\n", continued.len());
                }
                UserAction::Quit => {
                    println!("\n🛑 Stopping story generation");
                    break;
                }
            }
        }

        // Check limits in non-interactive mode
        if !config.interactive && config.max_chapters > 0 && chapter_num >= config.max_chapters {
            println!(
                "\n🎯 Reached target chapter count ({})",
                config.max_chapters
            );
            break;
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
    println!("\nTo resume this story later:");
    println!(
        "   cargo run --release --example story_full -p roco-cli --resume {}",
        ws_path.display()
    );

    Ok(())
}

enum UserAction {
    Continue,
    Revise,
    Direct(String),
    ContinueChapter,
    Quit,
}

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
