//! CLI subcommand: `roco vector-search`
//!
//! Provides CLI interaction with the local vector embedding and similarity search index.

use roco_agent::embeddings::VectorStore;
use std::path::PathBuf;

/// Entry point for `roco vector-search` subcommand.
pub fn cmd_vector_search(extra: &[&str]) {
    let sub = extra.first().copied().unwrap_or("status");
    let args: Vec<&str> = extra[if extra.first().map(|s| *s == sub).unwrap_or(false) {
        1..
    } else {
        0..
    }]
    .to_vec();

    match sub {
        "init" => cmd_vector_search_init(&args),
        "add" => cmd_vector_search_add(&args),
        "query" => cmd_vector_search_query(&args),
        "status" => cmd_vector_search_status(&args),
        _ => {
            eprintln!("Usage:");
            eprintln!(
                "  roco vector-search init [--index PATH] [--dimensions NUM]    Initialize index"
            );
            eprintln!(
                "  roco vector-search add <text> [--id ID] [--index PATH] [--meta JSON] Add text"
            );
            eprintln!(
                "  roco vector-search query <query> [--limit LIMIT] [--index PATH] Query index"
            );
            eprintln!(
                "  roco vector-search status [--index PATH]                     Show index details"
            );
            std::process::exit(1);
        }
    }
}

/// Helper to parse custom index path. Defaults to `.roco/vector_store.json`.
fn get_index_path(args: &[&str]) -> PathBuf {
    let path_str = crate::parse_opt("--index", args).unwrap_or(".roco/vector_store.json");
    PathBuf::from(path_str)
}

/// Helper to extract positional argument excluding option keys and values.
fn get_positional_arg(args: &[&str]) -> Option<String> {
    let mut i = 0;
    while i < args.len() {
        if args[i].starts_with('-') {
            if args[i].contains('=') {
                i += 1;
            } else {
                if i + 1 < args.len() && !args[i + 1].starts_with('-') {
                    i += 2;
                } else {
                    i += 1;
                }
            }
        } else {
            return Some(args[i].to_string());
        }
    }
    None
}

fn cmd_vector_search_init(args: &[&str]) {
    let path = get_index_path(args);
    let dims = crate::parse_opt("--dimensions", args)
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(128);

    if path.exists() {
        println!("⚠️  Index already exists at: {}", path.display());
        println!("Overwriting index.");
    }

    let store = VectorStore::with_dimensions(dims);
    store.save_to_file(&path).unwrap_or_else(|e| {
        eprintln!("Error: Failed to initialize index: {e}");
        std::process::exit(1);
    });

    println!("✓ Successfully initialized empty vector index.");
    println!("Path: {}", path.display());
    println!("Dimensions: {}", dims);
}

fn cmd_vector_search_add(args: &[&str]) {
    let path = get_index_path(args);
    let text = match get_positional_arg(args) {
        Some(t) => t,
        None => {
            eprintln!("Error: Missing text content to index.");
            eprintln!("Usage: roco vector-search add <text> [--id ID] [--index PATH]");
            std::process::exit(1);
        }
    };

    let id = crate::parse_opt("--id", args).map(|s| s.to_string());
    let meta_str = crate::parse_opt("--meta", args);
    let metadata = match meta_str {
        Some(s) => serde_json::from_str(s).unwrap_or_else(|e| {
            eprintln!("Warning: Failed to parse metadata JSON: {e}. Defaulting to empty object.");
            serde_json::json!({})
        }),
        None => serde_json::json!({}),
    };

    let mut store = VectorStore::load_from_file(&path).unwrap_or_else(|e| {
        eprintln!("Error: Failed to load index: {e}");
        std::process::exit(1);
    });

    let added_id = store.add(id, &text, metadata);
    store.save_to_file(&path).unwrap_or_else(|e| {
        eprintln!("Error: Failed to save index: {e}");
        std::process::exit(1);
    });

    println!("✓ Text successfully embedded and added to index.");
    println!("ID: {added_id}");
    println!("Index Path: {}", path.display());
}

fn cmd_vector_search_query(args: &[&str]) {
    let path = get_index_path(args);
    let query = match get_positional_arg(args) {
        Some(q) => q,
        None => {
            eprintln!("Error: Missing search query string.");
            eprintln!("Usage: roco vector-search query <query> [--limit LIMIT] [--index PATH]");
            std::process::exit(1);
        }
    };

    let limit = crate::parse_opt("--limit", args)
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(5);

    let store = VectorStore::load_from_file(&path).unwrap_or_else(|e| {
        eprintln!("Error: Failed to load index: {e}");
        std::process::exit(1);
    });

    let results = store.search(&query, limit);
    println!(
        "Search Results for: \"{query}\" ({} matches found):\n",
        results.len()
    );

    for (idx, r) in results.iter().enumerate() {
        println!("{}. [{:.4}] (ID: {})", idx + 1, r.score, r.entry.id);
        println!("   Text: {}", r.entry.text);
        if r.entry.metadata != serde_json::json!({}) {
            println!("   Meta: {}", r.entry.metadata);
        }
        println!();
    }
}

fn cmd_vector_search_status(args: &[&str]) {
    let path = get_index_path(args);
    if !path.exists() {
        println!("Index path does not exist: {}", path.display());
        println!("Run 'roco vector-search init' to create a new index.");
        return;
    }

    let store = VectorStore::load_from_file(&path).unwrap_or_else(|e| {
        eprintln!("Error: Failed to load index: {e}");
        std::process::exit(1);
    });

    println!("Vector Index Status:");
    println!("  Path:        {}", path.display());
    println!("  Entries:     {}", store.entries.len());
    println!("  Dimensions:  {}", store.dimensions);
}
