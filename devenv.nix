{ pkgs, lib, config, inputs, ... }:

{
  # https://devenv.sh/basics/
  env.ROCO_PROJECT = "roco_ai";

  # Build artifacts land on a local ext4 cache rather than the NTFS external
  # drive (avoids symlink/permission issues when compiling from EXTHD).
  env.CARGO_TARGET_DIR = "/home/kit/.cache/roco_target";

  # https://devenv.sh/packages/
  packages = [
    pkgs.git
    pkgs.cargo-watch # `cargo watch` for edit-reload dev loop
    pkgs.pkg-config
    pkgs.jq # handy for inspecting model/*.json configs
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
  # processes.cargo-watch.exec = "cargo watch -x 'run --bin roco'";

  # https://devenv.sh/scripts/
  scripts.check.exec = "cargo check";
  scripts.test.exec = "cargo test";
  scripts.run.exec = "cargo run --bin roco";
  scripts.build-backends.exec = "cargo build --features http-backends";
  scripts.test-backends.exec = "cargo test --features http-backends";
  scripts.demo.exec = "cargo run --features http-backends";

  # https://devenv.sh/tasks/
  # "devenv:enterShell".after = "roco:setup";

  # https://devenv.sh/tests/
  enterTest = ''
    echo "Running RoCo AI test suite"
    cargo test
  '';

  # https://devenv.sh/pre-commit-hooks/
  # pre-commit.hooks.shellcheck.enable = true;

  enterShell = ''
    echo "RoCo AI — devenv ready"
    echo "  cargo test        # run the suite"
    echo "  cargo run --bin roco   # smoke test (mock backend)"
    echo "  cargo build --features http-backends  # include NVIDIA/Kilo backends"
  '';

  # See full reference at https://devenv.sh/reference/options/
}
