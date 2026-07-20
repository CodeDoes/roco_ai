#!/bin/bash
# run_desktop.sh — Launch the desktop GUI (egui)
# Note: The desktop app is the future primary surface (see roadmap/README.md).
# Currently requires a binary target or running the ui crate directly.

set -euo pipefail

echo "========================================"
echo "  RoCo AI — Desktop GUI"
echo "========================================"
echo ""
echo "The desktop GUI lives in crates/ui/ and is built with egui."
echo ""
echo "To build and run the desktop app:"
echo "  cargo run --release -p roco-ui --bin roco-desktop"
echo ""
echo "Note: As of the current build, the desktop binary is still in"
echo "development (see crates/ui/src/desktop_app.rs). If the binary"
echo "is missing, you can still explore the code:"
echo "  cargo doc -p roco-ui --open"
echo "  open crates/ui/src/lib.rs"
echo ""
echo "For now, the recommended user entry point is the CLI:"
echo "  ./start.sh"
echo ""
