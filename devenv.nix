{ pkgs, lib, config, inputs, ... }:

{
  packages = [
    pkgs.git
    pkgs.cargo-watch
    pkgs.pkg-config
    pkgs.jq
    pkgs.vulkan-loader
    pkgs.vulkan-tools
    pkgs.sccache
    # GTK3 stack (dev outputs) — needed so `cargo clippy --all-features`
    # (desktop/gui features → glib-sys/gobject-sys) builds inside the
    # devenv shell. Mirrors flake.nix devShell.buildInputs.
    pkgs.glib.dev
    pkgs.gtk3.dev
    pkgs.pango.dev
    pkgs.cairo.dev
    pkgs.gdk-pixbuf.dev
    pkgs.atk.dev
    pkgs.libxkbcommon
    pkgs.libGL
  ];

  languages.rust = {
    enable = true;
    # Pinned deliberately — must match rust-toolchain.toml and flake.nix.
    # "nixpkgs"/bare "stable" drift (its rust version moves with the nixpkgs
    # snapshot), which caused E0514 and clippy-lint whiplash between devenv
    # (1.95), local rustup (1.96) and CI (1.97).
    channel = "stable";
    version = "1.97.1";
    components = [ "rustfmt" "clippy" "rust-analyzer" ];
  };

  scripts.roco.exec = ''
    # Use direct release binary if available, otherwise cargo run
    TARGET_BIN="${CARGO_TARGET_DIR:-$(pwd)/target}/release/roco"
    if [ -f "$TARGET_BIN" ]; then
      exec "$TARGET_BIN" "$@"
    else
      exec cargo run -p roco-cli -- "$@"
    fi
  '';

  dotenv.enable = true;

  enterTest = ''
    mkdir -p .roco/tests
    ROCO_USE_MOCK_BACKEND=1 cargo test --workspace >> .roco/tests/latest.log 2>&1 || true
    echo "=== Test summary ==="  | tee -a .roco/tests/latest.log
    grep -E "^(test result|running|passed|failed|ignored)" .roco/tests/latest.log >> /dev/null && echo "See .roco/tests/latest.log for full output." || true
  '';

  env.PKG_CONFIG_PATH =
    "${pkgs.glib.dev}/lib/pkgconfig:${pkgs.gtk3.dev}/lib/pkgconfig:${pkgs.pango.dev}/lib/pkgconfig:${pkgs.cairo.dev}/lib/pkgconfig:${pkgs.gdk-pixbuf.dev}/lib/pkgconfig:${pkgs.atk.dev}/lib/pkgconfig:${pkgs.libxkbcommon}/lib/pkgconfig:${pkgs.libGL.dev}/lib/pkgconfig";
  # Nix RPATH/RUNPATH is non-transitive, so toolchain binaries (rustfmt/clippy)
  # fail to load libz.so.1 from libLLVM at runtime. Point LD_LIBRARY_PATH at
  # the NIX zlib (NOT the host /usr/lib — that breaks Nix binaries on CI
  # runners with an older host glibc, GLIBC_2.42 not found).
  env.LD_LIBRARY_PATH = "${pkgs.zlib}/lib";
  env.CARGO_INCREMENTAL = "";
  env.SCCACHE_DIR = "$HOME/.cache/sccache";
  env.SCCACHE_CACHE_SIZE = "20G";

  enterShell = ''
    echo "RoCo AI — devenv ready"
  '';
}
