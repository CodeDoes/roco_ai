//! Build script that probes for a fast linker (mold → lld → system default).
//!
//! This replaces the hardcoded `-fuse-ld=mold` in `.cargo/config.toml` so
//! that `cargo build` works seamlessly on systems without mold installed.

fn main() {
    // Skip if the user already specified a linker via RUSTFLAGS env var.
    if std::env::var("RUSTFLAGS")
        .or_else(|_| std::env::var("CARGO_ENCODED_RUSTFLAGS"))
        .map(|f| f.contains("fuse-ld"))
        .unwrap_or(false)
    {
        return;
    }

    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    // Only relevant on Linux (mold/lld are Linux-first).
    if target_os != "linux" {
        return;
    }

    // Probe order: mold → lld → (nothing, use system default).
    let chosen = if probe_linker("mold") {
        Some("mold")
    } else if probe_linker("lld") {
        Some("lld")
    } else {
        None
    };

    if let Some(linker) = chosen {
        println!("cargo:rustc-link-arg=-fuse-ld={linker}");
        println!("cargo:warning=using linker: {linker}");
    } else {
        println!("cargo:warning=no fast linker (mold/lld) found — using system default");
    }
}

/// Check if `name` is available on PATH.
fn probe_linker(name: &str) -> bool {
    std::process::Command::new("which")
        .arg(name)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}
