.PHONY: all build test cli viz gateway web clean

# ─── Rust ────────────────────────────────────────────────────────────────────

all: build

## Build all Rust crates
build:
	cargo build --workspace

## Run all Rust tests
test:
	cargo test --workspace

## Run CLI demos A–F
cli: build
	cargo run -p roco-cli

## Run trace visualizer (produces .roco/traces/*.html + *.json)
viz: build
	cargo run -p roco-cli -- viz

## List saved traces
trace-list: build
	cargo run -p roco-cli -- trace list

## Diff two traces
trace-diff:
	cargo run -p roco-cli -- trace diff $(ID1) $(ID2)

## Run a task from a JSON input file
run-input:
	cargo run -p roco-cli -- run-input $(FILE)

## Fix Rust warnings
fix:
	cargo fix --workspace --allow-dirty

# ─── Gateway ──────────────────────────────────────────────────────────────────

## Start the axum gateway server (listens on 0.0.0.0:3001)
gateway: build
	cargo run -p roco-gateway

# ─── Web App ──────────────────────────────────────────────────────────────────

## Install web app dependencies
web-install:
	cd apps/web && pnpm install

## Start the Next.js dev server (listens on localhost:3000)
web-dev:
	cd apps/web && pnpm dev

## Build the web app for production
web-build:
	cd apps/web && pnpm build

## Start the production web server
web-start:
	cd apps/web && pnpm start

# ─── napi addon ───────────────────────────────────────────────────────────────

## Build the napi-rs .node addon
napi-build:
	cd crates/napi && napi build --release

# ─── Utilities ────────────────────────────────────────────────────────────────

## Format all Rust code
fmt:
	cargo fmt --all

## Check for Rust compiler warnings
check:
	cargo check --workspace

## Clean build artifacts
clean:
	cargo clean
	rm -rf apps/web/.next

## Show full stack startup instructions
help:
	@echo "RoCo AI — Make targets"
	@echo ""
	@echo "Rust:"
	@echo "  make build       Build all crates"
	@echo "  make test        Run all tests (80 expected)"
	@echo "  make cli         Run CLI demos A-F"
	@echo "  make viz         Generate trace HTML+JSON"
	@echo "  make gateway     Start gateway on :3001"
	@echo ""
	@echo "Web:"
	@echo "  make web-install Install dependencies"
	@echo "  make web-dev     Start dev server on :3000"
	@echo "  make web-build   Production build"
	@echo ""
	@echo "Full stack:"
	@echo "  Terminal 1: make gateway"
	@echo "  Terminal 2: make web-dev"
	@echo "  Open http://localhost:3000"

# ─── Visualizer (React + Vite) ─────────────────────────────────────────────

## Install visualizer dependencies
viz-install:
	pnpm --filter @roco/visualizer install

## Start the React visualizer dev server (localhost:5173)
viz-dev:
	pnpm dev:visualizer

## Build the visualizer for production
viz-build:
	pnpm build:visualizer
