//! Structured story pipeline — `roco story`
//!
//! Runs a formal pipeline: outline → wiki → chapters → validation →
//! synopsis → published story in a sandbox workspace.
//!
//! # Usage (CLI)
//!
//! ```bash
//! # Generate a three-chapter story
//! roco story "A lighthouse keeper discovers a message in a bottle"
//!
//! # Specify strategy and token budget
//! roco story "A dragon and a knight become friends" --strategy meticulous --max-tokens 4096
//!
//! # Use collaborative mode (writer + editor loop)
//! roco story "An AI wakes up in a abandoned data center" --strategy collaborative
//!
//! # Export the finished story
//! roco export my-story --format md --output story.md
//! roco export my-story --format html --output story.html
//! ```
//!
//! # Pipeline stages
//!
//! 1. **Outline** — genre, tone, characters, 3-chapter structure
//! 2. **Wiki** — world-building: locations, lore, characters
//! 3. **Chapters** — 3 chapters with prose generation
//! 4. **Validation** — quality check, revision if needed
//! 5. **Synopsis** — summary of the complete story
//! 6. **Publish** — save to workspace
//!
//! # Example output
//!
//! ```text
//! $ roco story "A quantum physicist builds a door to a parallel universe"
//!
//! ────────────────────────────────────────────
//!  STORY PIPELINE
//! ────────────────────────────────────────────
//!
//! Title: The Echo Chamber
//! Genre: Science Fiction
//! Tone: Contemplative, wonder-filled
//!
//! ── Phase 1/6: Generating outline...
//! ✓ Outline complete (3 chapters)
//!
//! Chapter 1: The Anomaly
//!   Dr. Elara Voss detects a quantum fluctuation that shouldn't exist...
//!
//! Chapter 2: The Threshold
//!   Building the door requires a particle accelerator at max energy...
//!
//! Chapter 3: What Lies Beyond
//!   Elara steps through into a version of reality where she never existed...
//!
//! ── Phase 2/6: Building world wiki...
//! ✓ Wiki: 5 entries created
//!
//! ── Phase 3/6: Writing chapters...
//! ✓ Chapter 1 complete (2,450 words)
//! ✓ Chapter 2 complete (3,100 words)
//! ✓ Chapter 3 complete (2,800 words)
//!
//! ── Phase 4/6: Validating quality...
//! ✓ Quality check passed
//!
//! ── Phase 5/6: Writing synopsis...
//! ✓ Synopsis complete
//!
//! ── Phase 6/6: Publishing...
//! ✓ Story published to workspace: .roco/stories/echo_chamber/
//!
//! Story saved. Read it with:
//!   cat .roco/stories/echo_chamber/chapter_1.md
//! ```
//!
//! # Running this example
//!
//! ```bash
//! RWKV_MODEL=/path/to/model.st cargo run --example story_demo -p roco-cli
//! ```

use roco_app::daemon;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("═══ RoCo AI — Story Pipeline Demo ═══");
    println!();
    println!("This example demonstrates the story command structure.");
    println!("Run the CLI directly to generate a structured story:");
    println!();
    println!("  roco story \"A lighthouse keeper finds a message in a bottle\"");
    println!("  roco story \"A dragon and knight become friends\" --strategy collaborative");
    println!("  roco story \"AI wakes up in an abandoned data center\" --max-tokens 4096");
    println!();
    println!("Pipeline stages (run sequentially):");
    println!("  1. Outline — genre, tone, 3-chapter structure");
    println!("  2. Wiki — world-building entries");
    println!("  3. Chapters — full prose generation");
    println!("  4. Validation — quality check + revision");
    println!("  5. Synopsis — complete story summary");
    println!("  6. Publish — save to sandbox workspace");
    println!();
    println!("After generation, export the story:");
    println!("  roco export <story-dir> --format md --output story.md");
    println!("  roco export <story-dir> --format html --output story.html");
    println!();

    if std::env::var("RWKV_MODEL").is_err() {
        println!("[demo] No RWKV_MODEL set — showing story pipeline usage only.");
        println!("[demo] Set RWKV_MODEL to generate a story:");
        println!("[demo]   RWKV_MODEL=models/rwkv-v7.st roco story \"your premise\"");
        return Ok(());
    }

    let _backend = daemon::ensure_sync_backend();
    println!("[demo] Backend ready. Use `roco story` to write a structured story!");
    Ok(())
}
