//! HTML live-preview mode — `roco html`
//!
//! The agent responds in HTML instead of plain text, served through a local
//! HTTP server with live browser rendering. Type in the terminal, see richly
//! styled HTML output in your browser.
//!
//! # Usage (CLI)
//!
//! ```bash
//! # Start HTML session on default port (8080)
//! roco html
//!
//! # Start with an initial prompt
//! roco html "build a personal dashboard page"
//! roco html "create a landing page for a coffee shop"
//!
//! # Custom port
//! roco html --port 9090
//! ```
//!
//! # How it works
//!
//! 1. `roco html` starts a local HTTP server on the given port.
//! 2. Open `http://localhost:<port>` in your browser.
//! 3. Type prompts in the terminal; the agent responds in HTML.
//! 4. The browser page auto-refreshes after each response.
//! 5. Type `:q` in the terminal to quit.
//!
//! # Example session
//!
//! ```text
//! $ roco html "design a dashboard for a weather app"
//!
//! ═══════════════════════════════════════════
//!   RoCo AI — HTML Canvas
//! ═══════════════════════════════════════════
//!   Agent responds in HTML. Open http://127.0.0.1:8080 in your browser.
//!   Type your next request.  :q to quit.
//!
//! [SERVER] Serving HTTP on http://127.0.0.1:8080
//!
//! > design a dashboard for a weather app
//! [AGENT] Generated 1350 chars of HTML/CSS/JS
//! [SERVER] Refresh your browser to see the update
//! ```
//!
//! # Running this example
//!
//! ```bash
//! RWKV_MODEL=/path/to/model.st cargo run --example html_demo -p roco-cli
//! ```

use roco_app::daemon;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("═══ RoCo AI — HTML Mode Demo ═══");
    println!();
    println!("This example demonstrates the HTML command structure.");
    println!("Run the CLI directly to use it:");
    println!();
    println!("  roco html");
    println!("  roco html \"build a dashboard\"");
    println!("  roco html --port 9090");
    println!();
    println!("How it works:");
    println!("  1. Starts a local HTTP server on port 8080 (or custom)");
    println!("  2. Open http://localhost:8080 in your browser");
    println!("  3. Type prompts in the terminal");
    println!("  4. Agent responds with HTML/CSS/JS");
    println!("  5. Page auto-refreshes in browser");
    println!("  6. Type ':q' to quit");
    println!();
    println!("The agent can produce styled HTML pages, dashboards,");
    println!("landing pages, data visualizations, and more.");
    println!("Each response is rendered as rich HTML in the browser.");
    println!();

    if std::env::var("RWKV_MODEL").is_err() {
        println!("[demo] No RWKV_MODEL set — showing HTML usage only.");
        println!("[demo] Set RWKV_MODEL to use it:");
        println!("[demo]   RWKV_MODEL=models/rwkv-v7.st roco html");
        return Ok(());
    }

    let _backend = daemon::ensure_sync_backend();
    println!("[demo] Backend ready. Use `roco html` to start an HTML session!");
    Ok(())
}
