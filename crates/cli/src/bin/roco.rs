//! `roco` — unified CLI for RoCo AI.
//!
//! ════════════════════════════════════════════════════════════════════════════
//! FILE STATUS: EDITABLE (CLI binary). See EDIT_GUIDE.md for rules.
//! SIZE: ~1373 lines / 52 KB. Very large binary entry point.
//! KEY SECTIONS (in order):
//!   1. Helper functions (spawn_detached, default_detach_path) (lines 15-80)
//!   2. main() — subcommand dispatch (eval, bless, rwkv, grammar, gpu-check, server, gateway, gui, stop, story, interact) (lines 82-120)
//!   3. cmd_eval / cmd_bless (lines 500-700)
//!   4. cmd_server / cmd_gateway / cmd_gui (lines 120-500)
//!   5. Story pipeline (cmd_story) — outline → wiki → chapter ×3 → validation → correction → synopsis → publish (lines 700-1370)
//! ════════════════════════════════════════════════════════════════════════════
//!
//! Usage:
//!   roco eval [--output PATH]              run the RWKV eval suite
//!   roco bless [--snapshot PATH]           bless current outputs as new oracle
//!   roco rwkv                              smoke-test the RWKV backend
//!   roco grammar                           grammar-constrained decode
//!   roco gpu-check                         show Vulkan + model info

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

#[path = "../story_routes.rs"]
mod story_routes;

#[path = "../lsp.rs"]
mod lsp_handler;

#[path = "../interact.rs"]
mod interact_cli;

#[path = "../rich_output.rs"]
mod rich_output;

#[path = "../daemon.rs"]
mod daemon;

/// Spawn a detached child process for `roco server` or `roco gateway`.
/// The parent redirects stdio to a log file, writes a PID file, and exits.
fn spawn_detached(subcmd: &str, extra: &[&str], log_path: &Path, pid_path: &Path) {
    let exe = std::env::current_exe().expect("failed to get current exe path");

    // Build args for the child: subcommand + modified extras
    let mut child_args: Vec<String> = Vec::new();
    child_args.push(subcmd.to_string());

    let mut i = 0;
    while i < extra.len() {
        let a = extra[i];
        if a == "--detach" || a == "-d" {
            child_args.push(format!("--_child-{subcmd}"));
        } else if a == "--pid-file" || a == "--log-file" {
            child_args.push(a.to_string());
            if i + 1 < extra.len() {
                child_args.push(extra[i + 1].to_string());
                i += 1;
            }
        } else {
            child_args.push(a.to_string());
        }
        i += 1;
    }

    let log_file = fs::File::create(log_path)
        .unwrap_or_else(|e| panic!("failed to create log file {}: {e}", log_path.display()));
    let log_clone = log_file
        .try_clone()
        .expect("failed to clone log file handle");

    let child = Command::new(&exe)
        .args(&child_args)
        .stdin(fs::File::open("/dev/null").expect("no /dev/null"))
        .stdout(log_file)
        .stderr(log_clone)
        .spawn()
        .unwrap_or_else(|e| panic!("failed to spawn child: {e}"));

    let pid = child.id();
    fs::write(pid_path, pid.to_string())
        .unwrap_or_else(|e| panic!("failed to write pid file {}: {e}", pid_path.display()));

    println!("roco {subcmd} started (PID {pid})");
    println!("  log:      {}", log_path.display());
    println!("  pidfile:  {}", pid_path.display());
}

/// Compute a default path under `/tmp/roco/` for PID or log files.
fn default_detach_path(subcmd: &str, port: u16, ext: &str) -> PathBuf {
    let dir = std::env::temp_dir().join("roco");
    let _ = fs::create_dir_all(&dir);
    dir.join(format!("{subcmd}_{port}.{ext}"))
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let sub = args.get(1).map(|s| s.as_str()).unwrap_or("help");
    let extra: Vec<&str> = args.iter().skip(2).map(|s| s.as_str()).collect();

    match sub {
        "eval" => cmd_eval(&extra),
        "bless" => cmd_bless(&extra),
        "rwkv" => run_cargo(
            "run",
            &["-p", "roco-cli", "--example", "rwkv_test", "--release"],
            &extra,
        ),
        "grammar" => run_cargo(
            "run",
            &["-p", "roco-cli", "--example", "grammar_smoke", "--release"],
            &extra,
        ),
        "gpu-check" => cmd_gpu_check(&extra),
        "server" => cmd_server(&extra),
        "gateway" => cmd_gateway(&extra),
        "gui" => cmd_gui(&extra),
        "stop" => { crate::daemon::stop_all(); },
        "story" => cmd_story(&extra),
        "interact" => cmd_interact(&extra),
        _ => help(sub),
    }
}


fn cmd_gui(_extra: &[&str]) {
    use crate::daemon::{self, GATEWAY_PORT};
    use eframe::egui;
    use roco_app::AppContext;
    use roco_infer_client::RemoteBackend;
    use roco_ui::RocoDesktopApp;
    use std::sync::Arc;

    let exe = std::env::current_exe().expect("failed to get current exe path");

    // 1. Start gateway daemon if not running
    println!("Checking gateway daemon on port {}...", GATEWAY_PORT);
    let already_running = daemon::ensure_daemon(&exe, "gateway", GATEWAY_PORT, &["--detach"]);

    if !already_running {
        println!("Gateway starting...");
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("Failed to build Tokio runtime");
        rt.block_on(async {
            match daemon::wait_for_healthy(
                GATEWAY_PORT,
                std::time::Duration::from_secs(15),
                "Gateway",
            )
            .await
            {
                Ok(()) => println!("Gateway is ready."),
                Err(e) => {
                    eprintln!("Warning: {e}");
                    eprintln!("GUI will start without backend connection.");
                }
            }
        });
    } else {
        println!("Gateway already running.");
    }

    // 2. Construct the shared AppContext (Phase 3.1: single surface primitive).
    // AppContext::connect_remote wraps the same gateway URL the RemoteBackend
    // pushes to, so the GUI now shares workspace timeline, session binding,
    // model_state_generate, and future quality / revision ops with every
    // other surface (interact / story).
    let gateway_url = format!("http://127.0.0.1:{}", GATEWAY_PORT);
    let backend: Option<Arc<dyn roco_engine::ModelBackend>> = Some(Arc::new(RemoteBackend::new(
        gateway_url.clone(),
    ))
        as Arc<dyn roco_engine::ModelBackend>);
    let app_context = AppContext::connect_remote(&gateway_url);

    println!("Starting GUI (backend: {})...", gateway_url);
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1200.0, 800.0])
            .with_title("RoCo AI — Collaborative Story Writing"),
        ..Default::default()
    };

    let app = RocoDesktopApp::with_context(backend, Some(app_context));
    eframe::run_native(
        "RoCo AI Desktop",
        options,
        Box::new(|_cc| Ok(Box::new(app))),
    )
    .expect("GUI failed to start");
}

fn cmd_gateway(extra: &[&str]) {
    use roco_gateway::Gateway;

    let host = parse_opt("--host", extra).unwrap_or("127.0.0.1");
    let port_str = parse_opt("--port", extra).unwrap_or("8000");
    let port = port_str.parse::<u16>().unwrap_or(8000);
    let target = parse_opt("--target", extra).unwrap_or("http://127.0.0.1:8080");
    let limit_str = parse_opt("--rate-limit", extra).unwrap_or("60");
    let limit = limit_str.parse::<usize>().unwrap_or(60);

    let detach = extra.iter().any(|&a| a == "--detach" || a == "-d");
    let is_child = extra.iter().any(|&a| a == "--_child-gateway");
    let log_path = parse_opt("--log-file", extra)
        .map(PathBuf::from)
        .unwrap_or_else(|| default_detach_path("gateway", port, "log"));
    let pid_path = parse_opt("--pid-file", extra)
        .map(PathBuf::from)
        .unwrap_or_else(|| default_detach_path("gateway", port, "pid"));

    if detach && !is_child {
        spawn_detached("gateway", extra, &log_path, &pid_path);
        return;
    }

    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("Failed to build Tokio runtime");

    rt.block_on(async move {
        tracing_subscriber::fmt()
            .with_env_filter(
                tracing_subscriber::EnvFilter::try_from_default_env()
                    .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
            )
            .init();

        // Auto-start inference server if not running
        let exe = std::env::current_exe().expect("failed to get current exe path");
        crate::daemon::ensure_daemon(&exe, "server", crate::daemon::INFERENCE_PORT, &["--detach"]);

        let gateway = Gateway::new(host.to_string(), port, target.to_string(), limit);
        println!(
            "Starting API Gateway on {host}:{port} targeting {target} (limit: {limit}/min)..."
        );
        if let Err(e) = gateway.run().await {
            eprintln!("Gateway error: {e}");
        }
    });
}

fn cmd_server(extra: &[&str]) {
    use crate::story_routes::create_story_router;
    use roco_agent::story_engine::{StoryConfig, StoryEngine};
    use roco_infer_client::RemoteBackend;
    use roco_inference::RwkvBackend;
    use roco_server::{Server, ServerConfig};
    use std::sync::Arc;

    let host = parse_opt("--host", extra).unwrap_or("127.0.0.1");
    let port_str = parse_opt("--port", extra).unwrap_or("8080");
    let port = port_str.parse::<u16>().unwrap_or(8080);
    let story_mode = extra.iter().any(|&a| a == "--story" || a == "-s");
    let stdio_lsp = extra.iter().any(|&a| a == "--stdio-lsp");
    let inference_url = parse_opt("--inference-url", extra)
        .map(|s| s.to_string())
        .unwrap_or_else(|| {
            std::env::var("ROCO_API_URL").unwrap_or_else(|_| "http://127.0.0.1:8080".to_string())
        });

    // Detach mode
    let detach = extra.iter().any(|&a| a == "--detach" || a == "-d");
    let is_child = extra.iter().any(|&a| a == "--_child-server");
    let log_path = parse_opt("--log-file", extra)
        .map(PathBuf::from)
        .unwrap_or_else(|| default_detach_path("server", port, "log"));
    let pid_path = parse_opt("--pid-file", extra)
        .map(PathBuf::from)
        .unwrap_or_else(|| default_detach_path("server", port, "pid"));

    if detach && !is_child {
        spawn_detached("server", extra, &log_path, &pid_path);
        return;
    }

    // ── LSP front-end ──────────────────────────────────────────────────────
    // When spawned by an editor (e.g. Zed's `language_server_command`), run
    // only the LSP loop over stdin/stdout. The LSP is a thin client to the
    // singleton inference API server and does NOT load its own model. This
    // avoids binding a TCP port (which would conflict with the user's
    // manually-started server) and lets the process exit cleanly when the
    // editor closes stdin.
    if stdio_lsp {
        println!("Starting RoCo LSP (client → {inference_url})...");
        let rt = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("Failed to build Tokio runtime");
        rt.block_on(async move {
            tracing_subscriber::fmt()
                .with_env_filter(
                    tracing_subscriber::EnvFilter::try_from_default_env()
                        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
                )
                .init();
            let client = Arc::new(RemoteBackend::new(inference_url));
            match crate::lsp_handler::run_lsp(client).await {
                Ok(()) => tracing::info!("RoCo LSP session ended"),
                Err(e) => {
                    eprintln!("RoCo LSP error: {e}");
                    std::process::exit(1);
                }
            }
        });
        return;
    }

    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("Failed to build Tokio runtime");

    rt.block_on(async move {
        tracing_subscriber::fmt()
            .with_env_filter(
                tracing_subscriber::EnvFilter::try_from_default_env()
                    .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
            )
            .init();

        println!("Loading model...");
        let backend = match RwkvBackend::from_env() {
            Ok(b) => Arc::new(b),
            Err(e) => {
                eprintln!("Error loading backend: {e}");
                std::process::exit(1);
            }
        };
        println!("Model loaded successfully.");

        if story_mode {
            println!("Story mode enabled — initializing story engine...");
            let story_config = StoryConfig {
                interactive: true,
                validate_quality: true,
                ..Default::default()
            };
            let engine = match StoryEngine::new(story_config) {
                Ok(e) => e,
                Err(e) => {
                    eprintln!("Error creating story engine: {e}");
                    std::process::exit(1);
                }
            };

            let app = roco_server::routes::create_router(backend.clone())
                .merge(create_story_router(backend.clone(), engine));

            let addr = format!("{host}:{port}");
            println!("Starting story server on {addr}...");
            let listener = tokio::net::TcpListener::bind(&addr)
                .await
                .expect("Failed to bind TCP listener");
            if let Err(e) = axum::serve(listener, app).await {
                eprintln!("Server error: {e}");
            }
        } else {
            let config = ServerConfig {
                host: host.to_string(),
                port,
            };
            let server = Server::new(config, backend);
            println!("Starting server on {host}:{port}...");
            if let Err(e) = server.run().await {
                eprintln!("Server error: {e}");
            }
        }
    });
}

fn cmd_eval(extra: &[&str]) {
    let output = parse_opt("--output", extra).unwrap_or("evals/results/latest.json");
    let exit_code = run_cargo_get_code(
        "run",
        &[
            "-p",
            "roco-cli",
            "--example",
            "eval_suite",
            "--release",
            "--",
            "--backend",
            "rwkv",
        ],
        extra,
    );

    let snapshot_path = snapshot_path(output);
    if let Ok(report) = std::fs::read_to_string(output) {
        if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&report) {
            if let Some(results) = parsed["results"].as_array() {
                let mut snap = serde_json::Map::new();
                for r in results {
                    let name = r["name"].as_str().unwrap_or("");
                    let out = r["output"].as_str().unwrap_or("").trim();
                    if !name.is_empty() {
                        snap.insert(name.to_string(), serde_json::Value::String(out.to_string()));
                    }
                }
                let snap_json = serde_json::Value::Object(snap);
                if let Ok(json_str) = serde_json::to_string_pretty(&snap_json) {
                    let _ = std::fs::write(&snapshot_path, &json_str);
                    eprintln!("Snapshot saved to: {}", snapshot_path.display());
                }
            }
        }
    }
    std::process::exit(exit_code);
}

fn cmd_bless(extra: &[&str]) {
    let snapshot = parse_opt("--snapshot", extra)
        .map(PathBuf::from)
        .unwrap_or_else(|| snapshot_path("evals/results/latest.json"));

    let snap: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(&snapshot)
            .expect("snapshot file not found — run `roco eval` first"),
    )
    .expect("invalid snapshot JSON");
    let obj = snap.as_object().expect("snapshot must be a JSON object");

    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap_or_else(|_| ".".into());
    let source_candidates = [
        PathBuf::from(&manifest_dir).join("src/engine/eval.rs"),
        PathBuf::from(&manifest_dir).join("crates/engine/src/eval.rs"),
        PathBuf::from(&manifest_dir).join("src/engine/cases.rs"),
        PathBuf::from(&manifest_dir).join("crates/engine/src/cases.rs"),
    ];
    let source_paths: Vec<PathBuf> = source_candidates
        .iter()
        .filter(|p| p.exists())
        .cloned()
        .collect();

    if source_paths.is_empty() {
        eprintln!("eval source files not found");
        return;
    }

    let mut total_changed = 0;
    for source_path in &source_paths {
        let content = std::fs::read_to_string(source_path).expect("source not found");
        let mut lines: Vec<String> = content.lines().map(|l| l.to_string()).collect();
        let mut changed = 0;

        for (name, out_val) in obj {
            let out_str = out_val.as_str().unwrap_or("");
            if let Some(name_line) = lines
                .iter()
                .position(|l| l.trim() == &format!("name: \"{}\".into(),", name))
            {
                let mut oracle_line = None;
                for i in name_line..lines.len() {
                    let trimmed = lines[i].trim();
                    if trimmed.starts_with("oracle: Some(") || trimmed.starts_with("oracle: None,")
                    {
                        oracle_line = Some(i);
                        break;
                    }
                    if (trimmed.starts_with("category:") || trimmed.starts_with("name:"))
                        && i != name_line
                    {
                        break;
                    }
                }
                if let Some(oi) = oracle_line {
                    let escaped = out_str
                        .replace('\\', "\\\\")
                        .replace('"', "\\\"")
                        .replace('\n', "\\n");
                    let indent = &lines[oi][..lines[oi].len() - lines[oi].trim_start().len()];
                    lines[oi] = format!("{indent}oracle: Some(\"{escaped}\".into()),");
                    changed += 1;
                    eprintln!("  blessed {name}: \"{escaped}\"");
                } else {
                    eprintln!("  skipping {name}: no oracle field found");
                }
            } else {
                eprintln!("  skipping {name}: eval case not found");
            }
        }

        if changed > 0 {
            std::fs::write(source_path, lines.join("\n") + "\n")
                .expect("failed to write source file");
            eprintln!("\nBlessed {changed} oracle(s). Rebuild to pick up changes.");
        } else {
            eprintln!("No oracles blessed.");
        }
        total_changed += changed;
    }

    if total_changed > 0 {
        eprintln!("\nTotal blessed: {total_changed}");
    }
}

fn run_cargo(cmd: &str, args: &[&str], extra: &[&str]) {
    let code = run_cargo_get_code(cmd, args, extra);
    std::process::exit(code);
}

fn run_cargo_get_code(cmd: &str, args: &[&str], extra: &[&str]) -> i32 {
    let mut c = Command::new("cargo");
    c.arg(cmd);
    c.args(args);
    c.args(extra);
    c.status().map(|s| s.code().unwrap_or(1)).unwrap_or(1)
}

fn cmd_gpu_check(extra: &[&str]) {
    let json_mode = extra.iter().any(|&a| a == "--json" || a == "-j");
    let model_path = "models/rwkv7-g1h-2.9b-20260710-ctx10240-f16.st";
    let vocab_path = "assets/vocab/rwkv_vocab_v20230424.json";

    // Gather info
    let vulkan_ok = Command::new("vulkaninfo")
        .arg("--summary")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false);

    let model_exists = std::path::Path::new(model_path).exists();
    let vocab_exists = std::path::Path::new(vocab_path).exists();

    if json_mode {
        let info = serde_json::json!({
            "vulkan": {
                "available": vulkan_ok,
            },
            "model": {
                "path": model_path,
                "exists": model_exists,
            },
            "vocab": {
                "path": vocab_path,
                "exists": vocab_exists,
            },
        });
        println!("{}", serde_json::to_string_pretty(&info).unwrap());
    } else {
        println!("=== Vulkan devices ===");
        let _ = Command::new("vulkaninfo").arg("--summary").status();
        if !vulkan_ok {
            eprintln!("(vulkaninfo not available — GPU check may be limited)");
        }
        println!();
        println!("=== RWKV model ===");
        if model_exists {
            let _ = Command::new("ls").args(["-lh", model_path]).status();
        } else {
            eprintln!("Model not found at {model_path}");
        }
        println!("=== RWKV vocab ===");
        if vocab_exists {
            let _ = Command::new("ls").args(["-lh", vocab_path]).status();
        } else {
            eprintln!("Vocab not found at {vocab_path}");
        }
    }
}

fn help(sub: &str) {
    eprintln!("Usage: roco <subcommand> [args]\n");
    eprintln!("  eval [--output PATH]              Run evals, save snapshot");
    eprintln!("  bless [--snapshot PATH]            Bless snapshot as new oracle");
    eprintln!("  rwkv                              Smoke-test the RWKV backend");
    eprintln!("  grammar                           Grammar-constrained decode");
    eprintln!("  gpu-check [--json|-j]              Show Vulkan + model info (--json for machine-readable)");
    eprintln!("  server [--host ADDR] [--port PORT] [--story] [--detach|-d] Run the local HTTP server; --story adds story API");
    eprintln!(
        "                                  [--log-file PATH] [--pid-file PATH]  detach options"
    );
    eprintln!("  server --stdio-lsp [--inference-url URL]        Run the editor LSP client (no model load; talks to the");
    eprintln!(
        "                                  inference API server at ROCO_API_URL / --inference-url)"
    );
    eprintln!(
        "  gateway [--host ADDR] [--port PORT] [--target URL] [--rate-limit L] Run the API gateway"
    );
    eprintln!(
        "                                  [--detach|-d] [--log-file PATH] [--pid-file PATH]"
    );
    eprintln!("  gui                               Start the desktop GUI application");
    eprintln!("                                  (auto-starts gateway on :8000, which auto-starts inference on :8080)");
    eprintln!("  stop                              Stop background inference + gateway");
    eprintln!("  story <prompt> [--strategy S] [--max-tokens T] Generate a structured short story");
    eprintln!("  interact [--interactive] [--prompt PROMPT] [--resume SESSION] [--pace MODE]");
    eprintln!(
        "                                  Interactive CLI with pacing control, session resume"
    );
    eprintln!("  interact --list-sessions         List saved sessions");
    std::process::exit(if sub == "help" { 0 } else { 1 });
}

fn parse_opt<'a>(name: &str, args: &'a [&str]) -> Option<&'a str> {
    args.windows(2)
        .find_map(|w| if w[0] == name { Some(w[1]) } else { None })
}

fn snapshot_path(output: &str) -> PathBuf {
    let p = Path::new(output);
    let mut s = p.to_path_buf();
    s.set_extension("snapshot.json");
    s
}

// ═════════════════════════════════════════════════════════════════════════════
// Story Subcommand & Pipeline
// ═════════════════════════════════════════════════════════════════════════════

use roco_agent::mechanistic::{
    HandlerResult, MechanisticAgent, Plan as MechPlan, RepairConfig, Task,
};
use roco_engine::{CompletionRequest, ModelBackend};
use roco_grammar::{Schema, StrategyKind, StrategySelector};
use roco_tools::{ReadTool, Tool, WriteTool};
use roco_workspace::{Workspace, WorkspaceKind};
use serde::Deserialize;
use serde_json::json;
use std::collections::HashMap;

#[derive(Debug, Deserialize)]
struct StoryOutline {
    title: String,
    genre: String,
    tone: String,
    chapters: Vec<StoryChapterInfo>,
}

#[derive(Debug, Deserialize)]
struct StoryChapterInfo {
    number: u64,
    title: String,
    summary: String,
}

impl StoryOutline {
    fn schema() -> Schema {
        Schema::object()
            .prop("title", Schema::string())
            .prop("genre", Schema::string())
            .prop("tone", Schema::string())
            .prop(
                "chapters",
                Schema::array(
                    Schema::object()
                        .prop("number", Schema::integer())
                        .prop("title", Schema::string())
                        .prop("summary", Schema::string())
                        .build(),
                ),
            )
            .build()
    }
}

#[derive(Debug, Deserialize)]
struct StoryWiki {
    characters: Vec<StoryCharacter>,
    setting: String,
}

#[derive(Debug, Deserialize)]
struct StoryCharacter {
    name: String,
    description: String,
}

impl StoryWiki {
    fn schema() -> Schema {
        Schema::object()
            .prop(
                "characters",
                Schema::array(
                    Schema::object()
                        .prop("name", Schema::string())
                        .prop("description", Schema::string())
                        .build(),
                ),
            )
            .prop("setting", Schema::string())
            .build()
    }
}

#[derive(Debug, Deserialize)]
struct StoryChapter {
    title: String,
    content: String,
}

impl StoryChapter {
    fn schema() -> Schema {
        Schema::object()
            .prop("title", Schema::string())
            .prop("content", Schema::string())
            .build()
    }
}

#[derive(Debug, Deserialize)]
struct StoryValidation {
    quality: String,
    issues: String,
    suggestion: String,
}

impl StoryValidation {
    fn schema() -> Schema {
        Schema::object()
            .prop(
                "quality",
                Schema::enum_values(vec![
                    serde_json::json!("pass"),
                    serde_json::json!("fail"),
                    serde_json::json!("needs-work"),
                ]),
            )
            .prop("issues", Schema::string())
            .prop("suggestion", Schema::string())
            .build()
    }
}

#[derive(Debug, Deserialize)]
struct StorySynopsis {
    summary: String,
}

impl StorySynopsis {
    fn schema() -> Schema {
        Schema::object().prop("summary", Schema::string()).build()
    }
}

fn structured_complete_with_strategy<T>(
    backend: &dyn ModelBackend,
    system: &str,
    prompt: &str,
    strategy: &StrategySelector,
    temperature: f32,
    max_tokens: usize,
) -> Result<T, String>
where
    T: serde::de::DeserializeOwned,
{
    let text = futures::executor::block_on(backend.complete(CompletionRequest {
        system: system.to_string(),
        prompt: prompt.to_string(),
        grammar: if strategy.grammar().is_empty() {
            None
        } else {
            Some(strategy.grammar())
        },
        temperature,
        max_tokens,
        ..Default::default()
    }))
    .map_err(|e| format!("model error: {e}"))?
    .text;

    strategy.parse(&text)
}

fn cmd_interact(extra: &[&str]) {
    use crate::daemon;
    use crate::interact_cli::{self, InteractMode, PacingChoice};

    // Check for --list-sessions
    if extra.iter().any(|&a| a == "--list-sessions" || a == "-l") {
        interact_cli::list_sessions();
        return;
    }

    // Determine mode
    let prompt_arg = parse_opt("--prompt", extra);
    let resume = parse_opt("--resume", extra);
    let interactive = extra.iter().any(|&a| a == "--interactive" || a == "-i");
    let pace_str = parse_opt("--pace", extra).unwrap_or("careful");
    let first_arg = extra.first().map(|s| *s).unwrap_or("");

    let pacing = match pace_str {
        "planning" | "plan" => PacingChoice::Planning,
        "careful" | "full" => PacingChoice::Careful,
        "rolling" | "batch" => PacingChoice::Rolling,
        "auto" | "auto-accept" => PacingChoice::AutoAccept,
        _ => PacingChoice::Careful,
    };

    let mode = if let Some(p) = prompt_arg {
        if p.is_empty() {
            eprintln!("Error: --prompt requires a non-empty prompt");
            std::process::exit(1);
        }
        InteractMode::Prompt {
            prompt: p.to_string(),
        }
    } else if let Some(session_id) = resume {
        InteractMode::Resume {
            session_id: session_id.to_string(),
        }
    } else if interactive || extra.is_empty() {
        let initial = if first_arg.is_empty() {
            None
        } else {
            Some(first_arg.to_string())
        };
        InteractMode::Interactive {
            pacing,
            prompt: initial,
        }
    } else {
        InteractMode::Interactive {
            pacing,
            prompt: Some(first_arg.to_string()),
        }
    };

    let backend = daemon::ensure_sync_backend();

    if let Err(e) = interact_cli::run(mode, &*backend) {
        eprintln!("Session error: {e}");
        std::process::exit(1);
    }
}

fn cmd_story(extra: &[&str]) {
    use crate::daemon;

    let prompt = extra.first().cloned().unwrap_or(
        "Write a short story about a lighthouse keeper who discovers a message in a bottle.",
    );

    let strategy_str = parse_opt("--strategy", extra).unwrap_or("loose");
    let strategy_kind = StrategyKind::from_str(strategy_str).unwrap_or(StrategyKind::LooseJson);

    let max_tok_str = parse_opt("--max-tokens", extra).unwrap_or("600");
    let max_tokens = max_tok_str.parse::<usize>().unwrap_or(600);

    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("Failed to build Tokio runtime");

    let backend = daemon::ensure_backend();

    rt.block_on(async move {
        tracing_subscriber::fmt()
            .with_env_filter(tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")))
            .init();

        println!("Generating story...");

        let mut agent = MechanisticAgent::new()
            .with_repair(RepairConfig {
                max_retries: 2,
                temperature: 0.7,
                temperature_delta: 0.2,
                temperature_floor: 0.3,
                max_tokens,
                token_decay: 128,
                min_tokens: 128,
            })
            .with_fallback_threshold(0.3);

        agent.add_route("storyTeller", vec![
            ("compose", "outline"),
            ("compose", "wiki"),
            ("write", "chapter"),
            ("write", "synopsis"),
            ("validate", "chapter"),
            ("publish", "chapter"),
        ]);

        // Strategy selectors
        let outline_strategy = StrategySelector::new(strategy_kind, StoryOutline::schema(), "");
        let wiki_strategy = StrategySelector::new(strategy_kind, StoryWiki::schema(), "");
        let chapter_strategy = StrategySelector::new(strategy_kind, StoryChapter::schema(), "");
        let val_strategy = StrategySelector::new(strategy_kind, StoryValidation::schema(), "");
        let synopsis_strategy = StrategySelector::new(strategy_kind, StorySynopsis::schema(), "");

        // ── compose/outline ──────────────────────────────────────────────
        let outline_strategy_clone = outline_strategy;
        agent.register("compose", "outline", Box::new(move |task, backend, ws| {
            let premise = task.spec.get("premise")
                .and_then(|v| v.as_str())
                .unwrap_or("a short story");

            let outline: StoryOutline = structured_complete_with_strategy(
                backend,
                "You are a story outliner. Output valid JSON only.",
                &format!(
                    "Outline a short story with 3 chapters based on this premise:\n{premise}\n\n\
                     Output JSON matching the schema: title, genre, tone, chapters \
                     (array of 3 objects with number, title, summary)",
                ),
                &outline_strategy_clone,
                0.6,
                300,
            ).unwrap_or_else(|e| StoryOutline {
                title: "Untitled".into(),
                genre: "Unknown".into(),
                tone: "Unknown".into(),
                chapters: (1..=3).map(|i| StoryChapterInfo {
                    number: i,
                    title: format!("Chapter {i}"),
                    summary: format!("Error generating outline: {e}"),
                }).collect(),
            });

            // Render to markdown
            let mut md = format!("Title: {}\nGenre: {}\nTone: {}\n\n", outline.title, outline.genre, outline.tone);
            for ch in &outline.chapters {
                md.push_str(&format!("Chapter {}: {}\n{}\n\n", ch.number, ch.title, ch.summary));
            }

            let path = ws.resolve("01-OUTLINE.md").unwrap();
            let _ = WriteTool.call(json!({"path": path.to_string_lossy(), "content": &md}));

            HandlerResult {
                task: task.clone(),
                output: md,
                files: HashMap::new(),
                pass: true,
            }
        }));

        // ── compose/wiki ────────────────────────────────────────────────
        let wiki_strategy_clone = wiki_strategy;
        agent.register("compose", "wiki", Box::new(move |task, backend, ws| {
            let premise = task.spec.get("premise")
                .and_then(|v| v.as_str())
                .unwrap_or("a short story");
            let outline = task.spec.get("outline")
                .and_then(|v| v.as_str())
                .unwrap_or("");

            let wiki: StoryWiki = structured_complete_with_strategy(
                backend,
                "You are a worldbuilding assistant. Output valid JSON only.",
                &format!(
                    "Based on this premise and outline, create character bios and setting lore:\n\n\
                     Premise: {premise}\nOutline: {outline}\n\n\
                     Output JSON matching the schema: characters (array of objects with name, description), \
                     setting (string)",
                ),
                &wiki_strategy_clone,
                0.7,
                400,
            ).unwrap_or_else(|e| StoryWiki {
                characters: vec![StoryCharacter {
                    name: "Unknown".into(),
                    description: format!("Error generating wiki: {e}"),
                }],
                setting: "Unknown".into(),
            });

            // Render to markdown
            let mut md = String::from("Characters:\n");
            for ch in &wiki.characters {
                md.push_str(&format!("  - {}: {}\n", ch.name, ch.description));
            }
            md.push('\n');
            md.push_str(&format!("Setting:\n  - {}\n", wiki.setting));

            let path = ws.resolve("02-WIKI.md").unwrap();
            let _ = WriteTool.call(json!({"path": path.to_string_lossy(), "content": &md}));

            HandlerResult {
                task: task.clone(),
                output: md,
                files: HashMap::new(),
                pass: true,
            }
        }));

        // ── write/chapter ────────────────────────────────────────────────
        let chapter_strategy_clone = chapter_strategy;
        agent.register("write", "chapter", Box::new(move |task, backend, ws| {
            let chapter_num: usize = task.spec.get("number")
                .and_then(|v| v.as_u64())
                .unwrap_or(1) as usize;
            let chapter_label = task.spec.get("label")
                .and_then(|v| v.as_str())
                .unwrap_or("Chapter 1");
            let outline = task.spec.get("outline")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let previous = task.spec.get("previous")
                .and_then(|v| v.as_str())
                .unwrap_or("");

            let directive = if chapter_num == 1 {
                format!(
                    "Write {chapter_label}. Introduce the main character and setting. ~400 words.\n\n\
                     Outline context:\n{outline}\n\n\
                     Output JSON with: title (string), content (string with the chapter prose)",
                )
            } else {
                format!(
                    "Write {chapter_label}. Continue from where the previous chapter left off. \
                     Advance the plot. ~400 words.\n\n\
                     Previous chapter recap:\n{previous}\n\n\
                     Outline context:\n{outline}\n\n\
                     Output JSON with: title (string), content (string with the chapter prose)",
                )
            };

            let chapter: StoryChapter = structured_complete_with_strategy(
                backend,
                "You are a fiction writer. Write vivid, engaging prose. Output valid JSON only.",
                &directive,
                &chapter_strategy_clone,
                0.8,
                600,
            ).unwrap_or_else(|e| StoryChapter {
                title: chapter_label.into(),
                content: format!("Error writing chapter: {e}"),
            });

            let md = format!("# {}\n\n{}", chapter.title, chapter.content);

            let filename = format!("03-CHAPTER_{}.md", chapter_num);
            let path = ws.resolve(&filename).unwrap();
            let _ = WriteTool.call(json!({"path": path.to_string_lossy(), "content": &md}));

            HandlerResult {
                task: task.clone(),
                output: md,
                files: HashMap::new(),
                pass: true,
            }
        }));

        // ── validate/chapter ────────────────────────────────────────────
        let val_strategy_clone = val_strategy;
        agent.register("validate", "chapter", Box::new(move |task, backend, ws| {
            let chapter_text = task.spec.get("text")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let chapter_num = task.spec.get("number")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);

            let entry = if chapter_text.trim().is_empty() {
                format!("\n## Chapter {chapter_num}\n[validation skipped — chapter is empty]\n")
            } else {
                structured_complete_with_strategy::<StoryValidation>(
                    backend,
                    "You are a quality reviewer. Be strict. Output valid JSON only.",
                    &format!(
                        "Review this chapter and check for:\n\
                         1. Does it read like a coherent story (not meta-commentary)?\n\
                         2. Is the prose engaging?\n\n\
                         Chapter:\n{chapter_text}\n\n\
                         Output JSON matching the schema: quality (\"pass\" | \"fail\" | \"needs-work\"), \
                         issues (string), suggestion (string)",
                    ),
                    &val_strategy_clone,
                    0.3,
                    200,
                ).map(|v: StoryValidation| {
                    format!("\n## Chapter {chapter_num}\nQuality: {}\nIssues: {}\nSuggestion: {}\n",
                            v.quality, v.issues, v.suggestion)
                }).unwrap_or_else(|e| {
                    format!("\n## Chapter {chapter_num}\nQuality: fail\nIssues: Model error: {e}\nSuggestion: Retry\n")
                })
            };

            // Append to VALIDATION.md
            let path = ws.resolve("04-VALIDATION.md").unwrap();
            let existing = ReadTool
                .call(json!({"path": path.to_string_lossy()}))
                .ok()
                .and_then(|v| v.get("content").and_then(|c| c.as_str().map(String::from)))
                .unwrap_or_default();
            let _ = WriteTool.call(json!({"path": path.to_string_lossy(), "content": existing + &entry}));

            HandlerResult {
                task: task.clone(),
                output: entry,
                files: HashMap::new(),
                pass: true,
            }
        }));

        // ── write/synopsis ──────────────────────────────────────────────
        let synopsis_strategy_clone = synopsis_strategy;
        agent.register("write", "synopsis", Box::new(move |task, backend, ws| {
            let chapters = task.spec.get("chapters")
                .and_then(|v| v.as_str())
                .unwrap_or("");

            let synopsis: StorySynopsis = structured_complete_with_strategy(
                backend,
                "You are a literary summarizer. Output valid JSON only.",
                &format!(
                    "Write a one-paragraph synopsis of the complete story based on these chapters:\n\n\
                     {chapters}\n\n\
                     Output JSON matching the schema: summary (string, one paragraph, ~100 words)",
                ),
                &synopsis_strategy_clone,
                0.5,
                200,
            ).unwrap_or_else(|e| StorySynopsis {
                summary: format!("Error writing synopsis: {e}"),
            });

            let md = format!("Synopsis:\n\n{}", synopsis.summary);

            let path = ws.resolve("05-SYNOPSIS.md").unwrap();
            let _ = WriteTool.call(json!({"path": path.to_string_lossy(), "content": &md}));

            HandlerResult {
                task: task.clone(),
                output: md,
                files: HashMap::new(),
                pass: true,
            }
        }));

        // ── publish/chapter ────────────────────────────────────────────
        agent.register("publish", "chapter", Box::new(|_task, _backend, ws| {
            let read_file = |name: &str| -> String {
                ReadTool
                    .call(json!({"path": ws.root().join(name).to_string_lossy()}))
                    .ok()
                    .and_then(|v| v.get("content").and_then(|c| c.as_str().map(String::from)))
                    .unwrap_or_default()
            };
            let outline = read_file("01-OUTLINE.md");
            let wiki = read_file("02-WIKI.md");
            let mut story = format!("# {}\n\n", extract_title(&outline));

            if !wiki.is_empty() {
                story.push_str("## Characters & Setting\n\n");
                story.push_str(&wiki);
                story.push_str("\n\n---\n\n");
            }

            for i in 1..=3 {
                let ch = ReadTool
                    .call(json!({"path": ws.root().join(format!("03-CHAPTER_{i}.md")).to_string_lossy()}))
                    .ok()
                    .and_then(|v| v.get("content").and_then(|c| c.as_str().map(String::from)))
                    .unwrap_or_default();
                if !ch.is_empty() {
                    story.push_str(&ch);
                    story.push_str("\n\n---\n\n");
                }
            }

            let synopsis = read_file("05-SYNOPSIS.md");
            if !synopsis.is_empty() {
                story.push_str("## Synopsis\n\n");
                story.push_str(&synopsis);
                story.push_str("\n");
            }

            let path = ws.resolve("06-STORY.md").unwrap();
            let _ = WriteTool.call(json!({"path": path.to_string_lossy(), "content": &story}));

            HandlerResult {
                task: Task {
                    r#type: "publish".into(),
                    domain: "chapter".into(),
                    spec: serde_json::json!({"status": "published"}),
                },
                output: format!("published {} bytes", story.len()),
                files: HashMap::new(),
                pass: true,
            }
        }));

        // Build execution plan
        let plan = MechPlan {
            tasks: vec![
                Task {
                    r#type: "compose".into(),
                    domain: "outline".into(),
                    spec: serde_json::json!({"premise": prompt}),
                },
            ],
        };

        let ws = create_story_workspace(&prompt).unwrap();
        let workspace_path = ws.root().to_string_lossy().to_string();

        println!("\nWorkspace: {workspace_path}\n");
        println!("Pipeline: outline → worldbuilding → chapter×3 (with validation & correction) → synopsis → publish\n");

        // Phase 1: outline
        println!("📝 Outline...");
        let outline_result = agent.dispatch_single(backend.as_ref(), &plan.tasks[0], &ws)
            .expect("outline failed");
        let outline_text = &outline_result.output;

        // Phase 2: wiki
        println!("📚 Worldbuilding...");
        let wiki_plan = MechPlan {
            tasks: vec![Task {
                r#type: "compose".into(),
                domain: "wiki".into(),
                spec: serde_json::json!({"premise": prompt, "outline": outline_text}),
            }],
        };
        let wiki_result = agent.dispatch_single(backend.as_ref(), &wiki_plan.tasks[0], &ws)
            .expect("wiki failed");

        // Phase 3: chapters ×3
        let mut chapter_texts = Vec::new();
        for i in 1..=3 {
            let chapter_label = format!("Chapter {i}");
            let previous = chapter_texts.last().cloned().unwrap_or_default();
            println!("✍️  {}...", &chapter_label);

            let ch_task = Task {
                r#type: "write".into(),
                domain: "chapter".into(),
                spec: serde_json::json!({
                    "number": i,
                    "label": chapter_label,
                    "outline": outline_text,
                    "previous": previous,
                }),
            };
            let ch_result = agent.dispatch_single(backend.as_ref(), &ch_task, &ws)
                .expect("chapter failed");
            chapter_texts.push(ch_result.output.clone());

            println!("🔍 Validating {}...", &chapter_label);
            let val_task = Task {
                r#type: "validate".into(),
                domain: "chapter".into(),
                spec: serde_json::json!({
                    "number": i,
                    "text": ch_result.output,
                }),
            };
            let _val_result = agent.dispatch_single(backend.as_ref(), &val_task, &ws)
                .expect("validation failed");

            // Self-correction loop
            let val_path = ws.root().join("04-VALIDATION.md");
            if let Some(val_content) = ReadTool
                .call(json!({"path": val_path.to_string_lossy()}))
                .ok()
                .and_then(|v| v.get("content").and_then(|c| c.as_str().map(String::from)))
            {
                let chapter_header = format!("## Chapter {i}");
                let needs_revision = if let Some(start_idx) = val_content.find(&chapter_header) {
                    let segment = &val_content[start_idx..];
                    let next_chapter_header = format!("## Chapter {}", i + 1);
                    let segment = if let Some(end_idx) = segment.find(&next_chapter_header) {
                        &segment[..end_idx]
                    } else {
                        segment
                    };
                    segment.contains("Quality: fail") || segment.contains("Quality: needs-work") || segment.contains("needs-work")
                } else {
                    false
                };

                if needs_revision {
                    println!("⚠️  {} needs revision — retrying...", &chapter_label);

                    let retry_task = Task {
                        r#type: "write".into(),
                        domain: "chapter".into(),
                        spec: serde_json::json!({
                            "number": i,
                            "label": chapter_label,
                            "outline": outline_text,
                            "previous": previous,
                            "retry": true,
                        }),
                    };
                    let retry_result = agent.dispatch_single(backend.as_ref(), &retry_task, &ws)
                        .unwrap_or(ch_result);

                    let filename = format!("03-CHAPTER_{}.md", i);
                    let path = ws.resolve(&filename).unwrap();
                    let _ = WriteTool.call(json!({"path": path.to_string_lossy(), "content": &retry_result.output}));
                    chapter_texts[i - 1] = retry_result.output;
                }
            }
        }

        // Phase 4: synopsis
        println!("📋 Synopsis...");
        let all_chapters = chapter_texts.iter()
            .enumerate()
            .map(|(i, t)| format!("## Chapter {}\n{}", i + 1, t))
            .collect::<Vec<_>>()
            .join("\n\n");
        let synopsis_task = Task {
            r#type: "write".into(),
            domain: "synopsis".into(),
            spec: serde_json::json!({"chapters": all_chapters}),
        };
        let _synopsis_result = agent.dispatch_single(backend.as_ref(), &synopsis_task, &ws)
            .expect("synopsis failed");

        // Phase 5: publish
        println!("📦 Publishing...");
        let publish_task = Task {
            r#type: "publish".into(),
            domain: "chapter".into(),
            spec: serde_json::json!({}),
        };
        let publish_result = agent.dispatch_single(backend.as_ref(), &publish_task, &ws)
            .expect("publish failed");

        let outcome = agent.commit(plan.clone(), vec![
            outline_result, wiki_result, publish_result,
        ], &ws).unwrap();

        println!("✅ Done! {} files in workspace:\n", outcome.workspace_files.len());
        let mut filenames: Vec<_> = outcome.workspace_files.keys().collect();
        filenames.sort();
        for fname in &filenames {
            let size = outcome.workspace_files[*fname].len();
            println!("  📄 {} ({} bytes)", fname, size);
        }

        println!("\nStory successfully published to 06-STORY.md inside the workspace: {}", outcome.workspace_path);
    });
}

fn extract_title(outline: &str) -> String {
    for line in outline.lines() {
        if line.starts_with("Title:") {
            return line.trim_start_matches("Title:").trim().to_string();
        }
    }
    "Untitled Story".to_string()
}

fn sanitize_story_dirname(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect::<String>()
        .to_lowercase()
}

fn create_story_workspace(prompt: &str) -> Result<Workspace, anyhow::Error> {
    let base = std::env::current_dir()?.join(".roco").join("workspaces");
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let name = if prompt.trim().is_empty() {
        format!("story_ts_{ts}")
    } else {
        let words: Vec<&str> = prompt.split_whitespace().take(4).collect();
        format!("story_{}", sanitize_story_dirname(&words.join("_")))
    };
    let dir = base.join(format!("{name}_{ts}"));
    std::fs::create_dir_all(&dir)?;
    let ws = Workspace::from_existing(dir, WorkspaceKind::Agent)?;
    Ok(ws.with_name(name))
}
