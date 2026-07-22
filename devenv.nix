{ pkgs, lib, config, inputs, ... }:

{
  # NOTE: cachix for the Nix layer needs no manual config here — devenv
  # auto-wires `devenv.cachix.org` for the `github:cachix/devenv-nixpkgs`
  # input used in devenv.yaml (see the "Configuring cachix" step on shell
  # entry). Cachix only caches Nix *derivations*; it does NOT speed up
  # `cargo build`. That's what sccache (below) is for.
  # https://devenv.sh/basics/
  env.ROCO_PROJECT = "roco_ai";

  # Build artifacts land on a local ext4 cache rather than the NTFS external
  # drive (avoids symlink/permission issues when compiling from EXTHD).
  env.CARGO_TARGET_DIR = "/home/kit/.cache/roco_target";

  # https://devenv.sh/packages/
  # GPU/Vulkan packages for local RWKV inference via web-rwkv+wgpu.
  # The system NVIDIA driver provides libGLX_nvidia.so.0 at /usr/lib/x86_64-linux-gnu/,
  # which the Vulkan loader discovers via the ICD at /usr/share/vulkan/icd.d/nvidia_icd.json.
  # LD_LIBRARY_PATH is needed so the dynamic linker can find those driver libraries.
  # NOTE: GTK/WebKit are NOT pulled from Nix. This box already has them
  # installed system-wide (apt: libwebkit2gtk-4.1-dev, libgtk-3-dev, etc.),
  # and devenv's Nix-only PKG_CONFIG_PATH was hiding them. We append the host
  # pkg-config dirs in `enterShell` so the already-installed GTK3/WebKit are
  # discovered — no Nix rebuild of the GTK stack.
  packages = [
    pkgs.git
    pkgs.cargo-watch # `cargo watch` for edit-reload dev loop
    pkgs.pkg-config
    pkgs.jq # handy for inspecting model/*.json configs
    pkgs.vulkan-loader # libvulkan.so for wgpu → web-rwkv
    pkgs.vulkan-tools   # vulkaninfo, vkcube for debugging GPU setup
    pkgs.sccache        # Rust compile cache (RUSTC_WRAPPER) — speeds up cargo builds
  ];

  # https://devenv.sh/languages/
  languages.rust = {
    enable = true;
    # Use the prebuilt nixpkgs Rust toolchain (binary, no rust-overlay build).
    # For a rustup-managed pinned toolchain instead, switch to
    # `channel = "stable"` and add the oxalica/rust-overlay input.
    channel = "nixpkgs";
    components = [
      "rustfmt"
      "clippy"
      "rust-analyzer"
    ];
  };

  # Test/lint output lands here for inspection; never prints to terminal.
  # After running: cat .roco/tests/latest.log or cat .roco/lints/latest.log
  scripts.check.exec = "mkdir -p .roco/lints && cargo check --workspace > .roco/lints/latest.log 2>&1 || true";
  scripts.test.exec = "mkdir -p .roco/tests && cargo test --workspace > .roco/tests/latest.log 2>&1 || true";
  scripts.build.exec = "cargo build --workspace";
  scripts.rwkv.exec = "cargo run -p roco-inference --example rwkv_test --release";
  scripts.grammar.exec = "cargo run -p roco-inference --example grammar_smoke --release";
  scripts.eval.exec = "cargo run -p roco-cli -- eval";
  scripts.chat.exec = "cargo run -p roco-cli -- interact";
  scripts.agent.exec = "cargo run -p roco-cli -- interact --";
  scripts.daemon.exec = "cargo run -p roco-server --example daemon --release";
  scripts.quant-analyze.exec = "cargo run -p roco-inference --example quant_analyze --release";
  scripts.style-stress.exec = "cargo run -p roco-inference --example style_stress --release";
  scripts.gpu-check.exec = ''
    echo "=== Vulkan devices ==="
    vulkaninfo --summary 2>&1 | grep -E "(GPU[0-9]|deviceName|deviceType|driverID|driverInfo)" || true
    echo ""
    echo "=== RWKV model & vocab ==="
    ls -lh models/rwkv7-g1h-2.9b-20260710-ctx10240-f16.st 2>/dev/null || echo ".st model not found"
    ls -lh assets/vocab/rwkv_vocab_v20230424.json 2>/dev/null || echo "vocab not found"
    echo ""
    echo "=== To select a specific GPU at runtime ==="
    echo "  RWKV_ADAPTER=NVIDIA roco eval   # force NVIDIA GPU"
    echo "  RWKV_ADAPTER=AMD roco eval      # force AMD GPU"
    echo "  RWKV_ADAPTER=llvmpipe roco eval # force CPU software renderer"
  '';

  # https://devenv.sh/integrations/dotenv/
  dotenv.enable = true;

  # https://devenv.sh/tests/
  # CI: devenv test runner uses workspace-level script which redirects to file.
  enterTest = ''
    mkdir -p .roco/tests
    cargo test --workspace >> .roco/tests/latest.log 2>&1 || true
    echo "=== Test summary ==="  | tee -a .roco/tests/latest.log
    grep -E "^(test result|running|passed|failed|ignored)" .roco/tests/latest.log >> /dev/null && echo "See .roco/tests/latest.log for full output." || true
  '';

  # Point to the system library path so the Vulkan loader can find
  # NVIDIA/Mesa driver .so files referenced by ICDs.
  env.LD_LIBRARY_PATH = "/usr/lib/x86_64-linux-gnu";

  # To force the NVIDIA Vulkan ICD (discrete GPU) instead of AMD iGPU:
  #   export VK_ICD_FILENAMES=/usr/share/vulkan/icd.d/nvidia_icd.json
  # To force the AMD RADV ICD (integrated GPU):
  #   export VK_ICD_FILENAMES=/usr/share/vulkan/icd.d/radeon_icd.json

  # https://devenv.sh/reference/options/
  enterShell = ''
    # Use the SYSTEM's already-installed GTK3/WebKit (not Nix). devenv's own
    # PKG_CONFIG_PATH only lists Nix store paths, so it can't see the host's
    # webkit2gtk-4.1 / gtk3 / libsoup3 dev packages. Append the host pkg-config
    # search dirs. This keeps devenv's Nix-provided vulkan-loader path intact
    # (devenv sets PKG_CONFIG_PATH *before* enterShell runs) while also making
    # the host GUI libraries discoverable.
    export PKG_CONFIG_PATH="$PKG_CONFIG_PATH:/usr/lib/x86_64-linux-gnu/pkgconfig:/usr/lib/pkgconfig:/usr/share/pkgconfig:/usr/local/lib/x86_64-linux-gnu/pkgconfig:/usr/local/lib/pkgconfig:/usr/local/share/pkgconfig"

    # Add the roco CLI binary to PATH after a cargo build.
    export PATH="$PATH:$CARGO_TARGET_DIR/release"

    # sccache: cache compiled Rust crate artifacts across builds so repeated
    # `cargo build`/`cargo check` are fast (sccache warms across builds).
    # This is what actually addresses slow cargo builds — cachix does not.
    # Edit-loop default: incremental ON. sccache is opt-in (see make build-cold).
    export CARGO_INCREMENTAL="''${CARGO_INCREMENTAL:-1}"
    # Keep sccache available but do not wrap rustc by default.
    export SCCACHE_DIR="/home/kit/.cache/sccache"
    # Uncomment to force sccache (disables incremental):
    # export RUSTC_WRAPPER=sccache
    # export CARGO_INCREMENTAL=0
    export SCCACHE_CACHE_SIZE="20G"

    echo "RoCo AI — devenv ready"
    echo ""
    echo "  ── Rust (local RWKV inference) ──"
    echo "  cargo test --workspace              # run the full test suite"
    echo "  roco eval                           # run the rwkv eval suite (--release)"
    echo "  roco rwkv                           # smoke-test the RWKV backend"
    echo "  roco grammar                        # grammar-constrained decode smoke test"
    echo "  roco chat                           # interactive chat REPL"
    echo "  roco daemon                         # background daemon + RPC"
    echo "  roco quant-analyze                  # RWKVQuant proxy analysis"
    echo "  roco style-stress                   # prompt style stress test"
    echo "  gpu-check                          # show Vulkan device + model status"
    echo ""
    echo "GPU: $(vulkaninfo --summary 2>/dev/null | grep -oP 'deviceName\s*=\s*\K.*' | head -1 || echo 'no Vulkan device found')"
  '';
}
