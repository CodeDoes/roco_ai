//! Workspace management subcommand: `roco workspace`
//!
//! Provides explicit workspace lifecycle management:
//! - `roco workspace new` — create a new workspace and print its ID
//! - `roco workspace list` — list all workspaces
//! - `roco workspace show <id>` — show workspace contents
//! - `roco workspace delete <id>` — delete a workspace

use std::path::PathBuf;

const WORKSPACES_DIR: &str = ".roco/workspaces";

/// Entry point for `roco workspace` subcommand.
pub fn cmd_workspace(extra: &[&str]) {
    let sub = extra.first().copied().unwrap_or("list");
    let args: Vec<&str> = extra[if extra.first().map(|s| *s == sub).unwrap_or(false) {
        1..
    } else {
        0..
    }]
    .to_vec();

    match sub {
        "new" => cmd_workspace_new(&args),
        "list" => cmd_workspace_list(),
        "show" => cmd_workspace_show(&args),
        "delete" => cmd_workspace_delete(&args),
        _ => {
            eprintln!("Usage:");
            eprintln!("  roco workspace new              Create a new workspace");
            eprintln!("  roco workspace list             List all workspaces");
            eprintln!("  roco workspace show <id>        Show workspace contents");
            eprintln!("  roco workspace delete <id>      Delete a workspace");
            std::process::exit(1);
        }
    }
}

/// Create a new workspace and print its ID.
fn cmd_workspace_new(_args: &[&str]) {
    let timestamp = chrono::Utc::now().timestamp();
    let workspace_id = format!("{}_{}", timestamp, "default");
    let workspace_path = get_workspaces_dir().join(&workspace_id);

    std::fs::create_dir_all(&workspace_path)
        .unwrap_or_else(|e| {
            eprintln!("Error: Failed to create workspace: {e}");
            std::process::exit(1);
        });

    println!("Created workspace: {}", workspace_id);
    println!("Path: {}", workspace_path.display());
    println!();
    println!("Use it with:");
    println!("  roco session <session_id> -p \"Use the workspace {}\"", workspace_id);
}

/// List all workspaces.
fn cmd_workspace_list() {
    let workspaces_dir = get_workspaces_dir();
    if !workspaces_dir.exists() {
        println!("No workspaces found.");
        return;
    }

    let mut workspaces: Vec<_> = std::fs::read_dir(&workspaces_dir)
        .expect("Failed to read workspaces directory")
        .filter_map(|entry| {
            let entry = entry.ok()?;
            let path = entry.path();
            if path.is_dir() {
                Some(path)
            } else {
                None
            }
        })
        .collect();

    workspaces.sort_by_key(|p| p.file_name().unwrap().to_string_lossy().to_string());

    if workspaces.is_empty() {
        println!("No workspaces found.");
        return;
    }

    println!("Workspaces ({} total):\n", workspaces.len());
    for path in &workspaces {
        let id = path.file_name().unwrap().to_string_lossy();
        let count = std::fs::read_dir(path)
            .map(|d| d.count())
            .unwrap_or(0);
        println!("  {:<40} {} files", id, count);
    }
}

/// Show workspace contents.
fn cmd_workspace_show(args: &[&str]) {
    let workspace_id = match args.first() {
        Some(id) => id,
        None => {
            eprintln!("Usage: roco workspace show <workspace-id>");
            std::process::exit(1);
        }
    };

    let workspace_path = get_workspaces_dir().join(workspace_id);
    if !workspace_path.exists() {
        eprintln!("Error: Workspace '{}' not found", workspace_id);
        std::process::exit(1);
    }

    println!("Workspace: {}\n", workspace_id);
    println!("Contents:\n");

    for entry in std::fs::read_dir(&workspace_path).expect("Failed to read workspace") {
        let entry = entry.expect("Failed to read entry");
        let path = entry.path();
        let name = path.file_name().unwrap().to_string_lossy();
        let metadata = entry.metadata().expect("Failed to read metadata");
        let size = metadata.len();
        println!("  {:<30} {} bytes", name, size);
    }
}

/// Delete a workspace.
fn cmd_workspace_delete(args: &[&str]) {
    let workspace_id = match args.first() {
        Some(id) => id,
        None => {
            eprintln!("Usage: roco workspace delete <workspace-id>");
            std::process::exit(1);
        }
    };

    let workspace_path = get_workspaces_dir().join(workspace_id);
    if !workspace_path.exists() {
        eprintln!("Error: Workspace '{}' not found", workspace_id);
        std::process::exit(1);
    }

    std::fs::remove_dir_all(&workspace_path)
        .unwrap_or_else(|e| {
            eprintln!("Error: Failed to delete workspace: {e}");
            std::process::exit(1);
        });

    println!("Deleted workspace: {}", workspace_id);
}

/// Get the workspaces directory path.
fn get_workspaces_dir() -> PathBuf {
    std::env::current_dir()
        .unwrap_or_default()
        .join(WORKSPACES_DIR)
}
