//! Interactive chat mode — `roco interact`
//!
//! This is the default mode. The agent responds to natural language prompts
//! with vivid prose. Supports pacing control, session persistence, and resume.
//!
//! # Usage (CLI)
//!
//! ```bash
//! # Start interactive chat with a premise
//! roco "A lone astronaut discovers a derelict alien ship"
//!
//! # Same, explicit
//! roco interact --interactive "A lone astronaut..."
//!
//! # One-shot prompt (generates, saves session, exits)
//! roco interact --prompt "Write a haiku about Rust"
//!
//! # Resume a previous session
//! roco interact --resume interact_20260722_120000
//!
//! # List saved sessions
//! roco interact --list-sessions
//!
//! # Control pacing mode
//! roco interact --pace planning   # agent runs to completion
//! roco interact --pace careful    # one task at a time (default)
//! roco interact --pace rolling    # review batches of 3
//! roco interact --pace auto       # fast, auto-accept
//! ```
//!
//! # Example
//!
//! ```text
//! $ roco "Tell me about the cyberpunk city of Neo-Tokyo-3"
//!
//! ═══════════════════════════════════════════
//!   RoCo AI — Interactive
//! ═══════════════════════════════════════════
//!   Just type your story idea.  :h for help, :q to quit.
//!
//! ── 1243 characters ──
//! The rain fell in sheets across Neo-Tokyo-3, each drop a needle
//! of data piercing the neon haze. From her perch in the Spire's
//! 247th floor, Kaito watched the city breathe...
//!
//! ── [a]ccept  [s]kip  [r]evise  [q]uit ──
//! > /pace rolling
//! ✓ Pacing: Rolling (review batches)
//! ```
//!
//! # Running this example
//!
//! ```bash
//! RWKV_MODEL=/path/to/model.st cargo run --example interact_demo -p roco-cli
//! ```

use roco_app::daemon;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("═══ RoCo AI — Interact Mode Demo ═══");
    println!();
    println!("This example demonstrates the interact command structure.");
    println!("Run the CLI directly to use it:");
    println!();
    println!("  roco \"your story premise here\"");
    println!("  roco interact --interactive");
    println!("  roco interact --resume <session-id>");
    println!("  roco interact --list-sessions");
    println!("  roco interact --pace auto");
    println!();
    println!("Interact modes:");
    println!("  --interactive   Full REPL with pacing control");
    println!("  --prompt TEXT   One-shot generation, saves session, exits");
    println!("  --resume ID     Load and continue a session");
    println!("  --pace MODE     planning | careful | rolling | auto");
    println!();
    println!("In-session commands:");
    println!("  /accept  /skip  /stop  /revise  /undo  /redo");
    println!("  /pace <mode>  /save  /list  /help  /quit");
    println!();
    println!("Sessions auto-save after each exchange to .roco/sessions/");
    println!();

    if std::env::var("RWKV_MODEL").is_err() {
        println!("[demo] No RWKV_MODEL set — showing interact usage only.");
        println!("[demo] Set RWKV_MODEL to run a real session.");
        return Ok(());
    }

    // With a model, you'd use interact_cli::run() directly.
    // For now, just confirm the backend is reachable.
    let _backend = daemon::ensure_sync_backend();
    println!("[demo] Backend ready. Use `roco interact` for the full experience.");
    Ok(())
}
