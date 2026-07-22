.PHONY: all build build-inc build-cold build-dyn run-dyn build-all build-desktop build-net build-inferd build-release build-dist check check-leaf watch check-all check-% \
        test test-% fmt fix clean timings cache-stats cache-zero

# Directories for artefact output.
mkdir_test := mkdir -p .roco/tests
mkdir_lint := mkdir -p .roco/lints
LOG_TEST   := .roco/tests/latest.log
LOG_LINT   := .roco/lints/latest.log

# ── Compile-speed environment ────────────────────────────────────────────────
# DEFAULT = edit-loop: rustc incremental ON, sccache OFF.
# mold is always on via .cargo/config.toml.
# Cold/CI: `make build-cold` or `source scripts/compile_env.sh cold`.
# Override jobs on low-RAM hosts: `make build JOBS=1`
JOBS ?=
export CARGO_INCREMENTAL ?= 1
# Do not auto-enable sccache — it disables incremental and slows edit loops.
export SCCACHE_CACHE_SIZE ?= 10G
ifdef JOBS
  export CARGO_BUILD_JOBS := $(JOBS)
endif

CARGO ?= cargo

# ─── Rust workspace ──────────────────────────────────────────────────────────

all: build

# Fast default: CLI only, incremental + mold (edit-loop optimized).
build:
	CARGO_INCREMENTAL=1 RUSTC_WRAPPER= $(CARGO) build

# Alias kept for muscle memory.
build-inc: build

# Cold/clean rebuild with sccache (no incremental).
build-cold:
	@if [ -x "$(HOME)/.cargo/bin/sccache" ]; then \
	  CARGO_INCREMENTAL=0 RUSTC_WRAPPER=$(HOME)/.cargo/bin/sccache $(CARGO) build; \
	elif command -v sccache >/dev/null 2>&1; then \
	  CARGO_INCREMENTAL=0 RUSTC_WRAPPER=sccache $(CARGO) build; \
	else \
	  CARGO_INCREMENTAL=0 RUSTC_WRAPPER= $(CARGO) build; \
	fi

# Dynamic libstd only (see scripts/run_dyn.sh). Does NOT dylib workspace crates.
build-dyn:
	./scripts/run_dyn.sh build -p roco-cli
run-dyn:
	./scripts/run_dyn.sh run -p roco-cli -- $(ARGS)

# Local GPU inference daemon (wgpu / web-rwkv). Not in default build.
build-inferd:
	$(CARGO) build -p roco-inferd

# Everything including desktop UI + inferd.
build-all:
	$(CARGO) build --workspace -p roco-cli --features desktop

# Desktop GUI binary (enables eframe/egui on roco-cli).
build-desktop:
	$(CARGO) build -p roco-cli --features desktop -p roco_ui

# HTTP server/gateway surface (axum) without GPU.
build-net:
	$(CARGO) build -p roco-cli --features net -p roco-server -p roco-gateway

# Day-to-day optimized binary (thin LTO).
build-release:
	$(CARGO) build --release

# Shipping profile (fat LTO, CGU=1) — slow; use sparingly.
build-dist:
	$(CARGO) build --profile dist -p roco-cli --features desktop

# Full test suite — captures all output to LOG_TEST for inspection.
test:
	$(mkdir_test)
	$(CARGO) test --workspace > $(LOG_TEST) 2>&1 || true

# Single-crate test: make test-agent, make test-engine, etc.
test-%:
	$(mkdir_test)
	$(CARGO) test -p roco-$* > $(LOG_TEST) 2>&1 || true

# Fast compile check (default-members / CLI). Incremental.
check:
	CARGO_INCREMENTAL=1 RUSTC_WRAPPER= $(CARGO) check

# Leaf-crate check — use while editing agent/engine without rebuilding CLI.
check-leaf:
	CARGO_INCREMENTAL=1 RUSTC_WRAPPER= $(CARGO) check -p roco-agent -p roco-engine -p roco-grammar

# Watch a crate (requires cargo-watch). Example: make watch CRATE=roco-agent
CRATE ?= roco-cli
watch:
	CARGO_INCREMENTAL=1 RUSTC_WRAPPER= cargo watch -x "check -p $(CRATE)"

# Full workspace check including UI.
check-all:
	$(mkdir_lint)
	$(CARGO) check --workspace > $(LOG_LINT) 2>&1 || true

# Single-crate compile check: make check-agent, make check-engine, etc.
check-%:
	$(mkdir_lint)
	$(CARGO) check -p roco-$* > $(LOG_LINT) 2>&1 || true

# Emit rustc self-profiler summary for the next build (html in target/cargo-timings).
timings:
	$(CARGO) build -p roco-cli -Z unstable-options --timings=html 2>/dev/null \
		|| $(CARGO) build -p roco-cli --timings

fmt:
	$(CARGO) fmt --all

fix:
	$(CARGO) fix --workspace --allow-dirty

clean:
	$(CARGO) clean
	@if command -v sccache >/dev/null 2>&1; then sccache --stop-server 2>/dev/null || true; fi

cache-stats:
	@command -v sccache >/dev/null 2>&1 && sccache --show-stats || echo "sccache not installed"

cache-zero:
	@command -v sccache >/dev/null 2>&1 && sccache --zero-stats || true

# ─── RWKV inference ──────────────────────────────────────────────────────────

rwkv:
	$(CARGO) run -p roco-inference --example rwkv_test --release

grammar:
	$(CARGO) run -p roco-inference --example grammar_smoke --release

eval:
	$(CARGO) run -p roco-engine --example eval_suite --release -- --backend rwkv 2>/dev/null || $(CARGO) run -p roco-cli -- eval -- --backend rwkv

chat:
	$(CARGO) run -p roco-cli -- interact

gpu-check:
	@echo "=== Vulkan devices ==="; vulkaninfo --summary 2>&1 | grep -E "(GPU[0-9]|deviceName|deviceType)" || true
	@echo "=== RWKV model & vocab ==="; ls -lh models/*.st 2>/dev/null || echo "no .st model found"; ls -lh assets/vocab/rwkv_vocab_v20230424.json 2>/dev/null || echo "vocab not found"

# ─── RoCo CLI ────────────────────────────────────────────────────────────────

## Run the roco binary via cargo run
roco:
	$(CARGO) run --bin roco -- $(ARGS)

## Desktop GUI
gui:
	$(CARGO) run -p roco-cli --features desktop -- gui
