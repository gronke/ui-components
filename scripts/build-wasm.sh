#!/usr/bin/env sh
# Builds the browser TUI: wasm + JS glue into apps/web-demo/web-tui/, which
# the demo serves under /tui. Re-run after Rust changes, then restart the
# demo server.
set -eu
cd "$(dirname "$0")/.."

rustup target add wasm32-unknown-unknown
cargo build -p uic_tui_web --target wasm32-unknown-unknown --profile wasm

# The wasm-bindgen CLI must match the locked crate version exactly.
VERSION="$(cargo pkgid wasm-bindgen)"
VERSION="${VERSION##*@}"
if ! command -v wasm-bindgen >/dev/null 2>&1; then
    cargo install wasm-bindgen-cli --version "$VERSION" --locked
fi
if ! wasm-bindgen --version | grep -qF "$VERSION"; then
    echo "the wasm-bindgen CLI does not match the locked crate version $VERSION:" >&2
    echo "  cargo install wasm-bindgen-cli --version $VERSION --locked --force" >&2
    exit 1
fi

wasm-bindgen --target web --no-typescript \
    --out-dir apps/web-demo/web-tui \
    target/wasm32-unknown-unknown/wasm/uic_tui_web.wasm

# The registry must survive the linker: a bundle whose data section lost the
# component tags boots into "unknown custom element" (see ui_components::link).
if ! grep -aq 'input-date' apps/web-demo/web-tui/uic_tui_web_bg.wasm; then
    echo "the bundle lost the component registry (linker dropped the registrations)" >&2
    exit 1
fi
echo "browser TUI built into apps/web-demo/web-tui (served as /tui)"
