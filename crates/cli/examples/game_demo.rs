//! Adventure game mode — `roco game`
//!
//! The LLM acts as a text-adventure game master, maintaining a world state,
//! tracking player inventory, and responding to free-form actions with
//! vivid prose and state transitions.
//!
//! # Usage (CLI)
//!
//! ```bash
//! # Start a game in a fantasy world (default)
//! roco game
//!
//! # Start with a custom scenario
//! roco game "a cyberpunk Tokyo in 2099, ruled by megacorps"
//! roco game "an underwater research station, something went wrong"
//! roco game "the ancient library of Alexandria, you are a scribe"
//! ```
//!
//! # In-game commands
//!
//! ```text
//! :h, /help    — Show help
//! :q, /quit    — Quit the game
//! look, l      — Describe current area
//! inventory, i — List carried items
//! ```
//!
//! # Example session
//!
//! ```text
//! $ roco game "a haunted Victorian mansion"
//!
//! ═══════════════════════════════════════════
//!   RoCo AI — Adventure Game
//! ═══════════════════════════════════════════
//!   Scenario: a haunted Victorian mansion
//!   Type actions in natural language.  :h for help, :q to quit.
//!
//! The hallway stretches before you, lit by guttering gaslight...
//! A grandfather clock stands against the wall, its pendulum still.
//! Three doors line the corridor: oak (left), iron (right), and
//! a small service door (center).
//!
//! What do you do?
//!
//! ⚔️ [1] > examine the grandfather clock
//! ```
//!
//! # Running this example
//!
//! ```bash
//! RWKV_MODEL=/path/to/model.st cargo run --example game_demo -p roco-cli
//! ```

use roco_app::daemon;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("═══ RoCo AI — Adventure Game Mode Demo ═══");
    println!();
    println!("This example demonstrates the game command structure.");
    println!("Run the CLI directly to play:");
    println!();
    println!("  roco game");
    println!("  roco game \"a cyberpunk Tokyo dystopia\"");
    println!("  roco game \"an abandoned space station\"");
    println!();
    println!("Game features:");
    println!("  - LLM acts as game master: vivid prose, consistent state");
    println!("  - Implicit tracking of health, items, location, NPCs");
    println!("  - Type any natural language action (no rigid parser)");
    println!("  - 'look' / 'l' to re-describe the area");
    println!("  - 'inventory' / 'i' to check carried items");
    println!("  - ':q' to quit, ':h' for help");
    println!();
    println!("The game master maintains a consistent world across turns.");
    println!("Actions succeed or fail based on context and creativity.");
    println!();

    if std::env::var("RWKV_MODEL").is_err() {
        println!("[demo] No RWKV_MODEL set — showing game usage only.");
        println!("[demo] Set RWKV_MODEL to play. Example:");
        println!("[demo]   RWKV_MODEL=models/rwkv-v7.st roco game");
        return Ok(());
    }

    let _backend = daemon::ensure_sync_backend();
    println!("[demo] Backend ready. Use `roco game` to start playing!");
    Ok(())
}
