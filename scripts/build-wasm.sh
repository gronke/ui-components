#!/usr/bin/env sh
# Builds the browser TUI: wasm + JS glue into apps/web-demo/web-tui/, which
# the demo serves under /tui. Re-run after Rust changes, then restart the
# demo server.
set -eu
cd "$(dirname "$0")/.."

rustup target add wasm32-unknown-unknown
cargo build -p web-demo-tui --target wasm32-unknown-unknown --profile wasm

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
    target/wasm32-unknown-unknown/wasm/web_demo_tui.wasm

# The registry must survive the linker: a bundle whose data section lost the
# component tags boots into "unknown custom element" (see link_catalog). Guard
# a catalog input, app-root (registered in ui_components_demo), and the widget
# twins tab-bar and suggestion-input (registered in ui_components_tui).
for tag in input-date app-root tab-bar suggestion-input; do
    if ! grep -aq "$tag" apps/web-demo/web-tui/web_demo_tui_bg.wasm; then
        echo "the bundle lost $tag (linker dropped the registrations)" >&2
        exit 1
    fi
done
echo "browser TUI built into apps/web-demo/web-tui (served as /tui)"
