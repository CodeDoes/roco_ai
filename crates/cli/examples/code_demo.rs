//! Coder mode — `roco code`
//!
//! AI coding assistant in the terminal. Acts as a knowledgeable programming
//! assistant, writing code, explaining concepts, debugging, and suggesting
//! improvements. Maintains conversation history for context.
//!
//! # Usage (CLI)
//!
//! ```bash
//! # Start interactive coding session
//! roco code
//!
//! # Ask a specific question with initial prompt
//! roco code "how do I use async/await in Rust?"
//!
//! # Specify a language focus
//! roco code "write a binary search in Python" --lang python
//! roco code --lang typescript "explain React hooks"
//! roco code --lang rust "implement a state machine"
//! ```
//!
//! # Example session
//!
//! ```text
//! $ roco code "build a simple HTTP server in Rust" --lang rust
//!
//! ═══════════════════════════════════════════
//!   RoCo AI — Coder Mode
//! ═══════════════════════════════════════════
//!   Language focus: rust
//!   Ask coding questions.  :h for help, :q to quit.
//!
//! ────────────────────────────────────────────
//! You
//! ────────────────────────────────────────────
//! build a simple HTTP server in Rust
//!
//! ────────────────────────────────────────────
//! Assistant
//! ────────────────────────────────────────────
//! Here's a minimal HTTP server using just the standard library:
//!
//! ```rust
//! use std::io::{Read, Write};
//! use std::net::{TcpListener, TcpStream};
//! use std::thread;
//!
//! fn handle_client(mut stream: TcpStream) {
//!     let mut buffer = [0; 1024];
//!     stream.read(&mut buffer).unwrap();
//!     let response = "HTTP/1.1 200 OK\r\nContent-Length: 12\r\n\r\nHello World!";
//!     stream.write_all(response.as_bytes()).unwrap();
//! }
//!
//! fn main() {
//!     let listener = TcpListener::bind("127.0.0.1:7878").unwrap();
//!     for stream in listener.incoming() {
//!         thread::spawn(|| handle_client(stream.unwrap()));
//!     }
//! }
//! ```
//!
//! Key points:
//! - Uses `TcpListener` to accept connections...
//! ```
//!
//! 💻 > how do I add concurrency with tokio?
//! ```
//!
//! # Running this example
//!
//! ```bash
//! RWKV_MODEL=/path/to/model.st cargo run --example code_demo -p roco-cli
//! ```

use roco_app::daemon;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("═══ RoCo AI — Coder Mode Demo ═══");
    println!();
    println!("This example demonstrates the coder command structure.");
    println!("Run the CLI directly to use it:");
    println!();
    println!("  roco code");
    println!("  roco code \"explain monads in Rust\"");
    println!("  roco code --lang python \"write a decorator\"");
    println!("  roco code --lang typescript \"React custom hook\"");
    println!();
    println!("Coder mode features:");
    println!("  - Maintains conversation history across turns");
    println!("  - Language-specific code generation");
    println!("  - Explains reasoning before showing code");
    println!("  - Complete, runnable code examples");
    println!("  - Debugging with step-by-step reasoning");
    println!("  - Best practices, error handling, and tests");
    println!();
    println!("In-session commands:");
    println!("  :h /help    — Show help");
    println!("  :q /quit    — Quit");
    println!("  :hist       — Show conversation history");
    println!("  :clear      — Clear history and start fresh");
    println!();

    if std::env::var("RWKV_MODEL").is_err() {
        println!("[demo] No RWKV_MODEL set — showing coder usage only.");
        println!("[demo] Set RWKV_MODEL to use the coding assistant:");
        println!("[demo]   RWKV_MODEL=models/rwkv-v7.st roco code --lang rust");
        return Ok(());
    }

    let _backend = daemon::ensure_sync_backend();
    println!("[demo] Backend ready. Use `roco code` to start coding!");
    Ok(())
}
