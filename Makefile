# Single entry point for building and checking Prchum.
#
# The Rust core builds first and regenerates the C header into the Swift
# package; SwiftPM does not track the static library as a dependency, so
# `build` removes the stale binary to force a relink.

export MACOSX_DEPLOYMENT_TARGET := 14.0

SWIFT_PKG := --package-path macos
APP_BUNDLE := dist/Prchum.app

.PHONY: all core build run test smoke header-check check app docs docs-serve clean

all: build

core:
	cargo build --release --manifest-path core/Cargo.toml

build: core
	rm -f macos/.build/debug/Prchum
	swift build $(SWIFT_PKG)

run: build
	macos/.build/debug/Prchum $(ARGS)

test:
	cargo test --manifest-path core/Cargo.toml

smoke: build
	macos/.build/debug/Prchum --smoke-test

header-check:
	git diff --exit-code macos/Sources/CPrchum/include/prchum.h

check: test smoke header-check

app: core
	rm -f macos/.build/release/Prchum
	swift build -c release $(SWIFT_PKG)
	rm -rf $(APP_BUNDLE)
	mkdir -p $(APP_BUNDLE)/Contents/MacOS $(APP_BUNDLE)/Contents/Resources
	cp macos/.build/release/Prchum $(APP_BUNDLE)/Contents/MacOS/
	cp macos/Info.plist $(APP_BUNDLE)/Contents/
	plutil -replace CFBundleShortVersionString \
	  -string "$$(git describe --tags --always --dirty 2>/dev/null || echo development)" \
	  $(APP_BUNDLE)/Contents/Info.plist
	rm -rf dist/Prchum.iconset
	mkdir -p dist/Prchum.iconset
	for size in 16 32 128 256 512; do \
	  sips -z $$size $$size macos/AppIcon/icon-1024.png \
	    --out dist/Prchum.iconset/icon_$${size}x$${size}.png >/dev/null; \
	  sips -z $$((size*2)) $$((size*2)) macos/AppIcon/icon-1024.png \
	    --out dist/Prchum.iconset/icon_$${size}x$${size}@2x.png >/dev/null; \
	done
	iconutil -c icns dist/Prchum.iconset -o $(APP_BUNDLE)/Contents/Resources/Prchum.icns

docs:
	python3 -m venv .docs-venv 2>/dev/null || true
	.docs-venv/bin/pip install -q -r docs/requirements.txt
	.docs-venv/bin/mkdocs build --strict

docs-serve:
	.docs-venv/bin/mkdocs serve

clean:
	cargo clean --manifest-path core/Cargo.toml
	rm -rf macos/.build dist site .docs-venv
