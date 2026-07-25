# lit-demo

One hand-written Lit todo app — `<todo-app>` composing `<todo-item>` rows, plain `lit` and nothing else — run by two hosts from one crate:

```sh
cargo run -p uic_lit_demo             # the app in this terminal (uic_js/Boa + ratatui)
cargo run -p uic_lit_demo -- serve    # the same sources on real lit, http://127.0.0.1:8090
cargo run -p uic_lit_demo -- live     # both at once, one shared state (ADR 0024)
```

Both targets speak the same keys: type to draft a new entry and Enter adds it, Space toggles the selected row, Enter edits the selected row in place (typing changes it live, Enter finishes, emptying the text deletes the row), ArrowUp/ArrowDown select, Shift+ArrowUp/Down reorders (F5/F6 in the terminal, translated to the same shifted arrows before delivery).
The checkbox — a real one in the browser, the `[x]` span in the terminal — is the only pointer toggle; a plain click selects, and a double click (or double tap) opens the row for inline editing, Enter's pointer twin (the native terminal synthesizes `dblclick` from two quick clicks on one node).
The browser also reorders by drag & drop (terminal drag stays a possible follow-up).
Delete removes the selected row (the browser also offers a close button per row), and an edit emptied of its text deletes too.
A caret marks the insertion point in both hosts; the browser blinks it exactly while keys land in the list.
Esc quits the terminal.

The components render light DOM and carry Bootstrap classes — the house style: the browser shows a regular Bootstrap card and list group, and the terminal maps the same classes through its filtered Bootstrap sheet (the card's border, the list rows, the active highlight).
The components' `static styles` are the terminal-only layer — real lit never adopts them without a shadow root — adding what the map cannot say, like the `[x]`/`[ ]` markers as generated content.

## One tree, two hosts

`build.rs` compiles `web/src/*.ts` once into an npm-shaped package (`$OUT_DIR/npm/@schuhkarton/lit-todo`):

- the terminal host loads it like any installed package (`uic_js::JsHost::load_package`) and runs it against the mocked lit;
- the browser build takes the same tree as a source root beside the Tera-rendered page, with the real lit family vendored from `web/package.json` — vendoring is not transitive, so the manifest names lit's own channels too.

The app sticks to the idioms both engines serve (see `crates/uic_js/README.md`, Runtime mechanics): composition data flows down as attributes, keyboard input is a plain `keydown` listener on the host element, and template event values are method references with row context in a `data-*` attribute.
Module names must not shadow the terminal runtime's specifiers — never name app files `main.ts`, `runtime.ts` or `lit*.ts`.

## Two sync harnesses around the same app

The app itself knows nothing about either; the glue rides `@schuhkarton/uic-sync` (ADR 0024), baked beside the app's tree.

**`live` — the terminal is the server.** Every browser mirrors the terminal's state over `/ws`: type in one place and the letters land everywhere, including the terminal running the process.
The terminal shows the join URL as a scannable QR pane beside the app (dropped on narrow terminals — the status line keeps the URL) and listens on `0.0.0.0`, so phones on the network join by scanning; mind that this exposes the shared list to the LAN.
The page probes `/live` first, so the same page stays quiet under plain `serve`.

**`/p2p` — no server carries the state.** The pairing lives in a `<pair-wizard>` lit element that shows one step at a time — create an invite, send it, done — says what happens next at every turn, keeps a start-over button in reach from the first step, and wears a badge that turns green while the wire stands and red when it drops.
An invite is one link (`#s=<payload>&via=<topic>`, QR-sized): whoever opens it answers automatically through a one-time topic on a public rendezvous relay (ntfy.sh, ADR 0025), so a single scan or opened link connects both sides — the reply payload is the only thing the relay ever carries, the todo state stays peer-to-peer.
Pairing is symmetric underneath (ADR 0024): both sides create WebRTC offers and each synthesizes the peer's answer locally with fingerprint-derived DTLS roles, so nobody has to be "first"; a link opened in a browser that already has a waiting tab hands the connection over via BroadcastChannel, and the waiting tab answers through the link's topic.
The manual exchange stays first-class behind "exchange by hand" — the raw token copies with a click, the paste box parses tokens and full links alike, and camera scanning appears where `BarcodeDetector` exists (secure contexts) — and the wizard falls back to it with an explicit hint whenever the relay is unreachable.
The page passes a public STUN server to the pairing (the library default is none); the repo ships no TURN relay, and unreachable peers fail with a message instead of hanging.
Two localStorage knobs tune a browser: `uic-broker` (`off` for the fully manual flow, or the URL of a self-hosted ntfy) and `uic-ice` (a JSON `RTCIceServer` array appended to the STUN default — a TURN server with credentials reaches across hostile NATs; coturn mints long-lived credentials from its static secret with username = a future unix timestamp and credential = base64 of HMAC-SHA1(secret, username)).
Links are consumed exactly once (a reload lands on the clean invite page) and the important pairing events log to the browser console.
Each swap pairs exactly two browsers; links carrying the whole peer set, so a connected tab can keep inviting others, is the natural next step.
The baked dist is a plain static site, so the project's GitHub Pages serves it under `/lit-demo/` — the pairing page at `/lit-demo/p2p/` needs no server at all, and the HTTPS context enables the camera scanner there; the server-backed `live` mode naturally stays a `cargo run` affair.

## Knobs

- `UIC_LIT_DEMO_ADDR=host:port` moves the server (default `127.0.0.1:8090`; `live` defaults to `0.0.0.0:8090`).
- `WEB_MODULES_EMBEDDED=1` forces the fully-embedded dist (no filesystem reads).
- Dev serving recompiles `web/pages/` live; a change under `web/src/` re-bakes through cargo — restart the server.
