.PHONY: all build test test-% check check-% fmt fix clean

# Directories for artefact output.
mkdir_test := mkdir -p .roco/tests
mkdir_lint := mkdir -p .roco/lints
LOG_TEST   := .roco/tests/latest.log
LOG_LINT   := .roco/lints/latest.log

# ─── Rust workspace ──────────────────────────────────────────────────────────

all: build

build:
	cargo build --workspace

# Full test suite — captures all output to LOG_TEST for inspection.
test:
	$(mkdir_test)
	cargo test --workspace > $(LOG_TEST) 2>&1 || true

# Single-crate test: make test-agent, make test-engine, etc.
test-%:
	$(mkdir_test)
	cargo test -p roco-$* > $(LOG_TEST) 2>&1 || true

# Workspace-wide compile check → LOG_LINT.
check:
	$(mkdir_lint)
	cargo check --workspace > $(LOG_LINT) 2>&1 || true

# Single-crate compile check: make check-agent, make check-engine, etc.
check-%:
	$(mkdir_lint)
	cargo check -p roco-$* > $(LOG_LINT) 2>&1 || true

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
