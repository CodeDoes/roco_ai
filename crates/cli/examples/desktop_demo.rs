//! Desktop GUI mode — `roco gui` (requires `--features desktop`)
//!
//! Full egui desktop application with chat, story editing, wiki, link graph,
//! file tree, session management, and change timeline.
//!
//! # Usage (CLI)
//!
//! ```bash
//! # Build and run with desktop features
//! cargo run --bin roco --features desktop -- gui
//!
//! # The GUI auto-starts the gateway daemon and connects to the inference backend.
//! # A window opens with:
//! #   - Left panel: file tree + session browser
//! #   - Center: chat + pacing controls
//! #   - Right panel (toggle): editor, wiki, link graph, timeline
//! ```
//!
//! # Widgets
//!
//! | Widget | Panel | Purpose |
//! |---|---|---|
//! | Chat | Center | Send messages, see responses |
//! | Pacing | Center | Accept/skip/revise, mode control |
//! | Editor | Right | Markdown document editor |
//! | File Tree | Left | Browse project files |
//! | Wiki Browser | Right | World-building reference |
//! | Link Graph | Right | Character/location relationship graph |
//! | Session Browser | Left | Load/save/delete sessions |
//! | Change Timeline | Right | History of edits and snapshots |
//!
//! # Controls
//!
//! - **Pacing**: FullControl (careful), ModerateControl (rolling), GoHam (auto)
//! - **Chat**: Send, Retry, Stop, Clear, Undo, Copy
//! - **Sessions**: New, Save, Load, Delete, Refresh
//! - **File Tree**: Open, Select, Delete, Rename, Refresh
//! - **Link Graph**: Zoom, Pan, Select nodes, Add nodes
//! - **Timeline**: Undo/Redo, Snapshots, Rollback
//!
//! # Running this example
//!
//! ```bash
//! RWKV_MODEL=/path/to/model.st cargo run --bin roco --features desktop -- gui
//! ```

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("═══ RoCo AI — Desktop GUI Demo ═══");
    println!();
    println!("This example demonstrates the desktop GUI structure.");
    println!("Run the CLI directly to launch it:");
    println!();
    println!("  cargo run --bin roco --features desktop -- gui");
    println!();
    println!("The GUI provides a full writing workspace:");
    println!("  - Chat interface with pacing controls");
    println!("  - Right panel: editor, wiki, link graph, timeline");
    println!("  - Left panel: file tree, session browser");
    println!("  - Session save/load/resume");
    println!("  - Windows and Linux supported");
    println!();
    println!("Desktop features are behind the 'desktop' feature flag");
    println!("to keep the default CLI build fast (~19s).");
    println!();
    println!("Desktop pet (always-on-top companion):");
    println!("  cargo run --bin roco --features desktop -- pet");
    println!("  cargo run --bin roco --features desktop -- pet --hide");
    println!("  cargo run --bin roco --features desktop -- pet stop");
    println!();

    Ok(())
}
