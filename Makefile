# FireCraft build shortcuts. Run `make help` for the list.
#
# Requirements: Rust (rustup) for everything; `sudo apt install mingw-w64` for Windows builds;
# `cargo install trunk` + `rustup target add wasm32-unknown-unknown` for the web build.

CARGO ?= cargo
WIN_TARGET := x86_64-pc-windows-gnu
# Fireworks documents to compare against (make parity).
REF_DIR ?= ../ref_fw_images
PARITY_OUT ?= target/parity

.PHONY: help build release run windows windows-release web web-serve test lint check parity clean

help:
	@echo "FireCraft make targets:"
	@echo "  make build            Linux debug build      -> target/debug/firecraft"
	@echo "  make release          Linux optimized build  -> target/release/firecraft"
	@echo "  make run [FILE=x.png] Build and run (debug), optionally opening a file"
	@echo "  make windows          Windows debug build    -> target/$(WIN_TARGET)/debug/firecraft.exe"
	@echo "  make windows-release  Windows optimized build"
	@echo "  make web              Browser build (trunk)  -> dist/web/"
	@echo "  make web-serve        Serve the browser build at http://127.0.0.1:8766"
	@echo "  make test             Run all tests"
	@echo "  make lint             Clippy (lint) check"
	@echo "  make check            Tests + lint + Windows/web compile checks"
	@echo "  make parity           Compare renders with Fireworks for files in REF_DIR ($(REF_DIR))"
	@echo "  make clean            Remove build outputs (cargo clean)"

build:
	$(CARGO) build -p firecraft
	@echo "Built: target/debug/firecraft"

release:
	$(CARGO) build --release -p firecraft
	@echo "Built: target/release/firecraft"

run: build
	./target/debug/firecraft $(FILE)

windows:
	scripts/build-windows.sh

windows-release:
	scripts/build-windows.sh --release

web:
	@command -v trunk >/dev/null || { echo "trunk not found: cargo install trunk"; exit 1; }
	cd apps/firecraft-web && trunk build --release

web-serve:
	@command -v trunk >/dev/null || { echo "trunk not found: cargo install trunk"; exit 1; }
	cd apps/firecraft-web && trunk serve --release

test:
	$(CARGO) test --workspace

lint:
	$(CARGO) clippy --workspace

check: test lint
	$(CARGO) check -p firecraft --target $(WIN_TARGET)
	$(CARGO) check -p firecraft-web --target wasm32-unknown-unknown

parity:
	@mkdir -p $(PARITY_OUT)
	$(CARGO) run --release -p firecraft-io --example parity_report -- $(PARITY_OUT) $(REF_DIR)/*.png
	@echo "Side-by-side comparisons: $(PARITY_OUT)/*.cmp.png"

clean:
	$(CARGO) clean
