# RoCo AI Quickstart

Welcome to RoCo AI! This guide covers the basics of installing, running a story, and basic troubleshooting.

## 1. Installation

To install and build RoCo AI, you will need a working Rust toolchain.
Run the following in your terminal:

```bash
# Clone the repository
git clone https://github.com/your-repo/roco-ai.git
cd roco-ai

# Build the workspace
cargo build --release
```

**Environment Setup:**
Before running RoCo, ensure you have the `RWKV_MODEL` and `RWKV_VOCAB` environment variables set correctly to point to your model and tokenizer files. Refer to `roco quickstart` in the CLI for more specific model setup details.

## 2. Running Your First Story

The RoCo CLI provides an automated story generation pipeline that handles outlining, wiki building, and drafting chapters.

```bash
# Start a new story with a premise
cargo run --release --bin roco -- story "A lighthouse keeper finds a mysterious message in the fog."
```
The generated stories are saved sequentially under `.roco/stories/`.

## 3. Working with Stories (`show_work` and `continue`)

If you want to view a running or completed workspace phase, or resume an interrupted process:

- **Continue/Resume**: If the story pipeline stops or you exit midway, you can resume exactly where you left off by running:
  ```bash
  cargo run --release --bin roco -- story --resume
  ```
- **Show Work**: All generated components (e.g., `01-OUTLINE.md`, `03-CHAPTER_1.md`) are saved directly in your current story's workspace directory. You can inspect these Markdown files directly in any text editor.

## 4. Common Troubleshooting

- **"Gateway did not become healthy"**: If the backend gateway fails to start, check if another process is using port 18000. Stop other daemon processes via `roco stop`.
- **Formatting Constraints Failing**: If JSON structure or grammar rules fail, try lowering the temperature: `--temperature 0.3`.
- **Missing Linker on Linux**: If you encounter `mold` linker errors, prepend commands with `RUSTFLAGS=""`.

