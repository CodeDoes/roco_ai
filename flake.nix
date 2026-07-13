{
  description = "RoCo AI — local RWKV inference (Rust + WGPU)";

  inputs = {
    nixpkgs.url = "nixpkgs";
    flake-utils.url = "github:numtide/flake-utils";
    rust-overlay.url = "github:oxalica/rust-overlay";
  };

  outputs = { self, nixpkgs, flake-utils, rust-overlay, ... }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = import nixpkgs {
          inherit system;
          overlays = [ (import rust-overlay) ];
        };
        rust = pkgs.rust-bin.stable.latest.default;
      in
      {
        devShells.default = pkgs.mkShell {
          packages = [
            rust
            pkgs.pkg-config
            pkgs.openssl
            pkgs.libiconv
          ];
          # Full GTK3 stack — dev outputs included automatically.
          buildInputs = with pkgs; [
            gtk3
            glib
            pango
            cairo
            gdk-pixbuf
            atk
            libxkbcommon
            libGL
          ];
          shellHook = ''
            export PKG_CONFIG_PATH="${pkgs.glib.dev}/lib/pkgconfig:${pkgs.gtk3.dev}/lib/pkgconfig:${pkgs.pango.dev}/lib/pkgconfig:${pkgs.cairo.dev}/lib/pkgconfig:${pkgs.gdk-pixbuf.dev}/lib/pkgconfig:${pkgs.atk.dev}/lib/pkgconfig:$PKG_CONFIG_PATH"
            echo "RoCo AI dev shell — GTK deps ready"
            echo "  cargo build --workspace"
            echo "  cargo run -p roco-cli"
          '';
        };
      }
    );
}
