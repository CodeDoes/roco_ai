#!/usr/bin/env python3
"""Entry point — tmux_roco emulator."""
import sys
from pathlib import Path
# Add scripts to path if needed
sys.path.insert(0, str(Path(__file__).parent))
from tmux_roco.cli import main

if __name__ == "__main__":
    main()
