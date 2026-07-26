# RoCo AI

AI-assisted collaborative writing tool with local LLM (RWKV-7 2.9B).

## Quick Start

```bash
# Generate story from premise
./start.sh "A lighthouse keeper discovers a hidden message in the fog"

# Desktop GUI
./run_desktop.sh

# Run tests
./run_tests.sh
```

## Key Commands

- `./scout.sh` - Project overview
- `roco gui` - Desktop GUI
- `roco interact` - Interactive chat
- `roco server` - HTTP server for plugins

## Environment

- Model: RWKV-7 2.9B (set `RWKV_MODEL` or auto-detected)
- Config: `.roco/config.toml` or `~/.config/roco/config.toml`
- Rust: Edition 2021

## Notes

- Run tests after every edit: `./run_tests.sh`
- For debug GPU hangs: `RWKV_ADAPTER=llvmpipe`
- See `./scout.sh` for live project details