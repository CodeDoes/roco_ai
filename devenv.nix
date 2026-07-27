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
  ];

  languages.rust = {
    enable = true;
    channel = "nixpkgs";
    components = [ "rustfmt" "clippy" "rust-analyzer" ];
  };

  scripts.roco.exec = "cargo watch -x \"run -p roco-cli -- \\\"$@\\\"\"";

  dotenv.enable = true;

  enterTest = ''
    mkdir -p .roco/tests
    ROCO_USE_MOCK_BACKEND=1 cargo test --workspace >> .roco/tests/latest.log 2>&1 || true
    echo "=== Test summary ==="  | tee -a .roco/tests/latest.log
    grep -E "^(test result|running|passed|failed|ignored)" .roco/tests/latest.log >> /dev/null && echo "See .roco/tests/latest.log for full output." || true
  '';

  env.LD_LIBRARY_PATH = "/usr/lib/x86_64-linux-gnu";
  env.CARGO_INCREMENTAL = "";
  env.SCCACHE_DIR = "$HOME/.cache/sccache";
  env.SCCACHE_CACHE_SIZE = "20G";

  enterShell = ''
    echo "RoCo AI — devenv ready"
  '';
}
