.PHONY: all build test check fmt fix clean

# ─── Rust workspace ──────────────────────────────────────────────────────────

all: build

build:
	cargo build --workspace

test:
	cargo test --workspace

check:
	cargo check --workspace

fmt:
	cargo fmt --all

fix:
	cargo fix --workspace --allow-dirty

clean:
	cargo clean

# ─── RWKV inference ──────────────────────────────────────────────────────────

rwkv:
	cargo run -p roco-inference --example rwkv_test --release

grammar:
	cargo run -p roco-cli --example grammar_smoke --release

eval:
	cargo run -p roco-cli --example eval_suite --release -- --backend rwkv

chat:
	cargo run -p roco-cli --example chat --release

gpu-check:
	@echo "=== Vulkan devices ==="; vulkaninfo --summary 2>&1 | grep -E "(GPU[0-9]|deviceName|deviceType)" || true
	@echo "=== RWKV model & vocab ==="; ls -lh models/*.st 2>/dev/null || echo "no .st model found"; ls -lh assets/vocab/rwkv_vocab_v20230424.json 2>/dev/null || echo "vocab not found"

# ─── RoCo CLI ────────────────────────────────────────────────────────────────

## Run the roco binary via cargo run
roco:
	cargo run --bin roco -- $(ARGS)
