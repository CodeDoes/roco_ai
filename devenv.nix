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
  # GPU/Vulkan packages for Phase 4 — local RWKV inference via web-rwkv+wgpu.
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
    pkgs.nodejs_22
    pkgs.corepack_22    # enables pnpm via corepack
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

  # https://devenv.sh/processes/
  processes.roco-web.exec = "cd apps/web && pnpm dev";
  processes.roco-gateway.exec = "cargo run -p roco-gateway";
  processes.roco-viz.exec = "pnpm dev:visualizer";
  # processes.cargo-watch.exec = "cargo watch -x 'run --bin roco'";

  # https://devenv.sh/scripts/
  scripts.check.exec = "cargo check --workspace";
  scripts.test.exec = "cargo test --workspace";
  scripts.run.exec = "cargo run --bin roco";
  scripts.build-backends.exec = "cargo build --features http-backends";
  scripts.test-backends.exec = "cargo test --features http-backends";
  scripts.demo.exec = "cargo run --features http-backends";
  scripts.roco.exec = ''
    exec cargo run --bin roco -- "$@"
  '';
  scripts.gpu-check.exec = ''
    echo "=== Vulkan devices ==="
    vulkaninfo --summary 2>&1 | grep -E "(GPU[0-9]|deviceName|deviceType|driverID|driverInfo)" || true
    echo ""
    echo "=== RWKV model & vocab ==="
    ls -lh models/rwkv7-g1g-2.9b-20260526-ctx8192-converted.st 2>/dev/null || echo "convert .st model not found"
    ls -lh assets/vocab/rwkv_vocab_v20230424.json 2>/dev/null || echo "vocab not found"
    echo ""
    echo "=== To select a specific GPU at runtime ==="
    echo "  RWKV_ADAPTER=NVIDIA roco chat   # force NVIDIA GPU"
    echo "  RWKV_ADAPTER=AMD roco chat      # force AMD GPU"
    echo "  RWKV_ADAPTER=llvmpipe roco chat # force CPU software renderer"
  '';
  # web app scripts
  scripts.web-dev.exec = "cd apps/web && pnpm dev";
  scripts.web-build.exec = "cd apps/web && pnpm build";
  scripts.web-install.exec = "corepack enable && pnpm install";
  scripts.viz-dev.exec = "pnpm dev:visualizer";
  scripts.viz-build.exec = "pnpm build:visualizer";
  scripts.gateway.exec = "cargo run -p roco-gateway";
  scripts.napi-build.exec = "cd crates/napi && napi build --release";

  # https://devenv.sh/tasks/
  # "devenv:enterShell".after = "roco:setup";

  # Point to the system library path so the Vulkan loader can find
  # NVIDIA/Mesa driver .so files referenced by ICDs.
  env.LD_LIBRARY_PATH = "/usr/lib/x86_64-linux-gnu";

  # To force the NVIDIA Vulkan ICD (discrete GPU) instead of AMD iGPU:
  #   export VK_ICD_FILENAMES=/usr/share/vulkan/icd.d/nvidia_icd.json
  # To force the AMD RADV ICD (integrated GPU):
  #   export VK_ICD_FILENAMES=/usr/share/vulkan/icd.d/radeon_icd.json

  # https://devenv.sh/integrations/dotenv/
  dotenv.enable = true;

  # https://devenv.sh/tests/
  enterTest = ''
    echo "Running RoCo AI test suite"
    cargo test
  '';

  # https://devenv.sh/pre-commit-hooks/
  # pre-commit.hooks.shellcheck.enable = true;

  enterShell = ''
    # Use the SYSTEM's already-installed GTK3/WebKit (not Nix). devenv's own
    # PKG_CONFIG_PATH only lists Nix store paths, so it can't see the host's
    # webkit2gtk-4.1 / gtk3 / libsoup3 dev packages. Append the host pkg-config
    # search dirs. This keeps devenv's Nix-provided vulkan-loader path intact
    # (devenv sets PKG_CONFIG_PATH *before* enterShell runs) while also making
    # the host GUI libraries discoverable.
    export PKG_CONFIG_PATH="$PKG_CONFIG_PATH:/usr/lib/x86_64-linux-gnu/pkgconfig:/usr/lib/pkgconfig:/usr/share/pkgconfig:/usr/local/lib/x86_64-linux-gnu/pkgconfig:/usr/local/lib/pkgconfig:/usr/local/share/pkgconfig"

    # sccache: cache compiled Rust crate artifacts across builds so repeated
    # `cargo build`/`cargo check` (and the heavy first Dioxus compile) are fast.
    # This is what actually addresses slow cargo builds — cachix does not.
    export RUSTC_WRAPPER=sccache
    export SCCACHE_DIR="/home/kit/.cache/sccache"
    export SCCACHE_CACHE_SIZE="20G"

    echo "RoCo AI — devenv ready"
    echo ""
    echo "  ── Rust ──"
    echo "  cargo test --workspace          # run the full test suite"
    echo "  roco [args..]                   # run the CLI binary (GPU-backed)"
    echo "  roco chat -r                    # resume latest session"
    echo "  roco eval [NAME]                # run an eval suite"
    echo "  gpu-check                       # show Vulkan device + model status"
    echo ""
    echo "  ── Gateway ──"
    echo "  run gateway                     # start axum gateway on :3001"
    echo ""
    echo "  ── Web (Next.js) ──"
    echo "  run web-dev                     # start Next.js dev server :3000"
    echo "  run web-build                   # production build"
    echo ""
    echo "  ── Visualizer (React+Vite) ──"
    echo "  run viz-dev                     # start Vite dev server :5173"
    echo "  run viz-build                   # production build"
    echo ""
    echo "  ── Monorepo ──"
    echo "  pnpm install                    # install all npm deps"
    echo "  run napi-build                  # build napi-rs .node addon"
    echo ""
    echo "GPU: $(vulkaninfo --summary 2>/dev/null | grep -oP 'deviceName\s*=\s*\K.*' | head -1 || echo 'no Vulkan device found')"
  '';

  # See full reference at https://devenv.sh/reference/options/
}
