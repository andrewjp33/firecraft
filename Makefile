# FireCraft build shortcuts. Run `make help` for the list; plain `make` builds the Linux and
# Windows debug apps into dist/.
#
# Requirements: Rust (rustup) for everything; `sudo apt install mingw-w64` for Windows builds;
# `cargo install trunk` + `rustup target add wasm32-unknown-unknown` for the web build.
# Optional: `cargo install sccache` (or the distro package) caches compiled crates outside
# target/, so even `make distclean` doesn't mean recompiling every dependency; it's used
# automatically when installed.

# rustup installs into ~/.cargo/bin; make it work even if that isn't on the shell's PATH.
export PATH := $(HOME)/.cargo/bin:$(PATH)
ifneq ($(shell command -v sccache 2>/dev/null),)
export RUSTC_WRAPPER ?= sccache
endif
CARGO ?= cargo
WIN_TARGET := x86_64-pc-windows-gnu
WASM_TARGET := wasm32-unknown-unknown
# FireCraft's own packages: `make clean` removes only these, keeping compiled dependencies.
OWN_PACKAGES := firecraft firecraft-web firecraft-ui-egui firecraft-engine firecraft-io firecraft-doc firecraft-raster firecraft-fwpng
# Fireworks documents to compare against (make parity).
REF_DIR ?= ../ref_fw_images
# Each `make parity` run gets its own folder here (date, time, commit), with index.html pages.
PARITY_RUNS ?= ../parity_runs
# The latest run is also copied here (replaced each run).
PARITY_OUT ?= target/parity
# How many of the newest runs keep their comparison images (older runs keep their tables).
PARITY_KEEP ?= 10
DIST := dist

.PHONY: all help build release run windows windows-release web web-serve test lint check parity clean distclean size

all: build windows

help:
	@echo "FireCraft make targets (binaries are copied to $(DIST)/):"
	@echo "  make                  Linux + Windows debug builds"
	@echo "  make build            Linux debug build      -> $(DIST)/linux/firecraft"
	@echo "  make windows          Windows debug build    -> $(DIST)/windows/firecraft.exe"
	@echo "  make release          Linux optimized build  -> $(DIST)/linux-release/firecraft"
	@echo "  make windows-release  Windows optimized      -> $(DIST)/windows-release/firecraft.exe"
	@echo "  make web              Browser build (trunk)  -> $(DIST)/web/"
	@echo "  make web-debug        Browser debug build (dev profile, function names; larger)"
	@echo "  make web-serve        Serve the browser build at http://127.0.0.1:8766"
	@echo "  make run [FILE=x.png] Build and run (debug), optionally opening a file"
	@echo "  make test             Run all tests"
	@echo "  make lint             Clippy (lint) check"
	@echo "  make check            Tests + lint + Windows/web compile checks"
	@echo "  make parity           Compare renders with Fireworks for files in REF_DIR ($(REF_DIR))"
	@echo "  make clean            Remove FireCraft's own build outputs, keep compiled dependencies"
	@echo "  make distclean        Remove everything (target/ and $(DIST)/): next build is a full one"
	@echo "  make size             Show how much space the build outputs use"

build:
	$(CARGO) build -p firecraft
	@mkdir -p $(DIST)/linux
	@cp target/debug/firecraft $(DIST)/linux/firecraft
	@echo "Built: $(DIST)/linux/firecraft"

release:
	$(CARGO) build --release -p firecraft
	@mkdir -p $(DIST)/linux-release
	@cp target/release/firecraft $(DIST)/linux-release/firecraft
	@echo "Built: $(DIST)/linux-release/firecraft"

run: build
	./$(DIST)/linux/firecraft $(FILE)

windows:
	scripts/build-windows.sh
	@mkdir -p $(DIST)/windows
	@cp target/$(WIN_TARGET)/debug/firecraft.exe $(DIST)/windows/firecraft.exe
	@echo "Built: $(DIST)/windows/firecraft.exe (copy it to a Windows PC; no other files needed)"

windows-release:
	scripts/build-windows.sh --release
	@mkdir -p $(DIST)/windows-release
	@cp target/$(WIN_TARGET)/release/firecraft.exe $(DIST)/windows-release/firecraft.exe
	@echo "Built: $(DIST)/windows-release/firecraft.exe"

web:
	@command -v trunk >/dev/null || { echo "trunk not found: cargo install trunk"; exit 1; }
	cd apps/firecraft-web && trunk build --release

web-debug:
	@command -v trunk >/dev/null || { echo "trunk not found: cargo install trunk"; exit 1; }
	cd apps/firecraft-web && trunk build

web-serve:
	@command -v trunk >/dev/null || { echo "trunk not found: cargo install trunk"; exit 1; }
	cd apps/firecraft-web && trunk serve --release

test:
	$(CARGO) test --workspace

lint:
	$(CARGO) clippy --workspace

check: test lint
	$(CARGO) check -p firecraft --target $(WIN_TARGET)
	$(CARGO) check -p firecraft-web --target $(WASM_TARGET)

parity:
	@mkdir -p $(PARITY_RUNS)
	@commit=$$(git rev-parse --short HEAD 2>/dev/null || echo nogit); \
	if [ -n "$$(git status --porcelain --untracked-files=no 2>/dev/null)" ]; then commit="$$commit+changes"; fi; \
	run=$$(date +%Y-%m-%d_%H%M)_$$commit; \
	prev=$$(ls -d $(PARITY_RUNS)/*/ 2>/dev/null | grep -v "/$$run/" | sort | tail -1); \
	PARITY_LABEL="$$(date '+%Y-%m-%d %H:%M'), commit $$commit" PARITY_PREV="$$prev" PARITY_KEEP=$(PARITY_KEEP) \
	PARITY_LATEST=$(PARITY_OUT) \
	$(CARGO) run -q --release -p firecraft-io --example parity_report -- $(PARITY_RUNS)/$$run $(REF_DIR)/*.png; \
	echo "All runs: $$(cd $(PARITY_RUNS) && pwd)/index.html"

# Only FireCraft's packages, for every target that has been built; third-party crates stay
# compiled, so the next build takes about a minute instead of a full rebuild.
clean:
	$(CARGO) clean $(addprefix -p ,$(OWN_PACKAGES))
	@for t in $(WIN_TARGET) $(WASM_TARGET); do \
		if [ -d target/$$t ]; then $(CARGO) clean --target $$t $(addprefix -p ,$(OWN_PACKAGES)); fi; \
	done
	@echo "Kept compiled dependencies; 'make distclean' removes everything."

distclean:
	$(CARGO) clean
	@if [ -d $(DIST) ]; then rm -r $(DIST); fi

size:
	@du -sh target $(DIST) 2>/dev/null || true
	@du -sh target/*/ 2>/dev/null | sort -h || true
