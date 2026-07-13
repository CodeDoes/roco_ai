.PHONY: all build test check fmt fix clean rwkv grammar eval gpu-check

# ─── Rust (local RWKV inference) ─────────────────────────────────────────────

all: build

## Build the workspace (just crates/core now)
build:
	cargo build --workspace

## Run all Rust tests
test:
	cargo test --workspace

## Type-check the workspace
check:
	cargo check --workspace

## Format all Rust code
fmt:
	cargo fmt --all

## Fix Rust warnings
fix:
	cargo fix --workspace --allow-dirty

# ─── RWKV backend ────────────────────────────────────────────────────────────

## Smoke-test the RWKV backend (requires a .st model; --release for GPU)
rwkv:
	cargo run -p roco-core --features grammar-rwkv --example rwkv_test --release

## Grammar-constrained decode smoke test
grammar:
	cargo run -p roco-core --features grammar-rwkv --example grammar_smoke --release

## Run the rwkv eval suite
eval:
	cargo run -p roco-core --features grammar-rwkv --example eval_suite --release -- --backend rwkv

## Show Vulkan device + model/vocab status
gpu-check:
	@echo "=== Vulkan devices ==="; vulkaninfo --summary 2>&1 | grep -E "(GPU[0-9]|deviceName|deviceType)" || true
	@echo "=== RWKV model & vocab ==="; ls -lh models/*.st 2>/dev/null || echo "no .st model found"; ls -lh assets/vocab/rwkv_vocab_v20230424.json 2>/dev/null || echo "vocab not found"

# ─── Utilities ───────────────────────────────────────────────────────────────

## Clean build artifacts
clean:
	cargo clean
