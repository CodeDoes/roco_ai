fn main() {
    if std::env::var("RUSTFLAGS")
        .or_else(|_| std::env::var("CARGO_ENCODED_RUSTFLAGS"))
        .map(|f| f.contains("fuse-ld"))
        .unwrap_or(false)
    {
        return;
    }

    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    if target_os != "linux" {
        return;
    }

    let chosen = if probe_linker("mold") {
        Some("mold")
    } else if probe_linker("lld") {
        Some("lld")
    } else {
        None
    };

    if let Some(linker) = chosen {
        println!("cargo:rustc-link-arg=-fuse-ld={linker}");
    }
}

fn probe_linker(name: &str) -> bool {
    std::process::Command::new("which")
        .arg(name)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}
