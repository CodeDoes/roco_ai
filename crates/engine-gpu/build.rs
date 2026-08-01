//! Emits `WEB_RWKV_VERSION` from the workspace `Cargo.lock` so the actor can
//! stamp saved state blobs with the exact web-rwkv version used to build it.
//! (Previously hardcoded — see AGENTS.md §10.)

use std::env;
use std::fs;
use std::path::Path;

fn main() {
    println!("cargo:rerun-if-changed=../../Cargo.lock");
    let manifest_dir = env::var("CARGO_MANIFEST_DIR").unwrap();
    let lock_path = Path::new(&manifest_dir).join("../../Cargo.lock");
    let Ok(lock) = fs::read_to_string(&lock_path) else {
        println!(
            "cargo:warning=engine-gpu: Cargo.lock not found at {}; WEB_RWKV_VERSION unset",
            lock_path.display()
        );
        return;
    };

    // Cargo.lock TOML: `[[package]]` blocks with `name = "..."` / `version = "..."`.
    let mut in_pkg = false;
    let mut name = String::new();
    for line in lock.lines() {
        let line = line.trim();
        if line == "[[package]]" {
            in_pkg = true;
            name.clear();
            continue;
        }
        if !in_pkg {
            continue;
        }
        if line.is_empty() || line.starts_with('[') {
            in_pkg = false;
            continue;
        }
        if let Some(rest) = line.strip_prefix("name = ") {
            name = rest.trim_matches('"').to_string();
        } else if let Some(rest) = line.strip_prefix("version = ") {
            if name == "web-rwkv" {
                let version = rest.trim_matches('"');
                println!("cargo:rustc-env=WEB_RWKV_VERSION={version}");
                return;
            }
        }
    }
    println!("cargo:warning=engine-gpu: web-rwkv not found in Cargo.lock; WEB_RWKV_VERSION unset");
}
