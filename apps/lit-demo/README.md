# lit-demo

One hand-written Lit todo app — `<todo-app>` composing `<todo-item>` rows, plain `lit` and nothing else — run by two hosts from one crate:

```sh
cargo run -p uic_lit_demo             # the app in this terminal (uic_js/Boa + ratatui)
cargo run -p uic_lit_demo -- serve    # the same sources on real lit, http://127.0.0.1:8090
cargo run -p uic_lit_demo -- live     # both at once, one shared state (ADR 0013)
cargo run -p uic_lit_demo -- p2p      # a serverless WebRTC peer: generate an invite (ADR 0028)
cargo run -p uic_lit_demo -- p2p LINK # open someone's invite link or pairing code
```

Text entry is a plain `<input type="text">` in both hosts: the browser gives it the native caret, selection and focus outline, and the terminal mounts its rat widget twin by element type (ADR 0026) with the hardware cursor — mid-text editing, Home/End and selection chords work in both.
The draft input holds the keyboard from the start (autofocus), so load-and-type just works; Enter adds the draft as a row, Enter on an empty draft opens the selected row for editing in place (a real input again, typing changes it live letter by letter, Enter finishes, emptying the text deletes the row).
ArrowUp/ArrowDown select and Shift+ArrowUp/Down reorder even while the input holds focus (single-line inputs pass them through; F5/F6 in the terminal, translated to the same shifted arrows); Space toggles and Delete removes exactly while no text is in play — an empty draft, no row edit — otherwise they stay editing keys, the cancelable-keydown contract both hosts honor.
The checkbox — a real one in the browser, the `[x]` span in the terminal — is the only pointer toggle; a plain click selects (clicking an input places the caret under the pointer), and a double click (or double tap) opens the row for inline editing, Enter's pointer twin (the native terminal synthesizes `dblclick` from two quick clicks on one node).
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

The app sticks to the idioms both engines serve (see `crates/uic_js/README.md`, Runtime mechanics): composition data flows down as attributes, text entry is a plain `<input>` whose bubbling `input` events carry the live text (`target.value`), a host-level `keydown` listener keeps the list chrome, and template event values are method references with row context in a `data-*` attribute.
Module names must not shadow the terminal runtime's specifiers — never name app files `main.ts`, `runtime.ts`, `runtime/*` or `lit*.ts`.

## Two sync harnesses around the same app

The app itself knows nothing about either; the glue rides `@schuhkarton/uic-sync` (ADR 0013), baked beside the app's tree.

**`live` — the terminal is the server.** Every browser mirrors the terminal's state over `/ws`: type in one place and the letters land everywhere, including the terminal running the process.
The terminal shows the join URL as a scannable QR pane beside the app (dropped on narrow terminals — the status line keeps the URL) and listens on `0.0.0.0`, so phones on the network join by scanning; mind that this exposes the shared list to the LAN.
The page probes `/live` first, so the same page stays quiet under plain `serve`.

**`/p2p` — no server carries the state.** The pairing lives in a `<pair-wizard>` lit element that renders the shared `<pair-panel>` (ADR 0029): it shows the invite to share and the box to open theirs, says what happens next at every turn, keeps a start-over button in reach, and wears a badge that turns green while the wire stands and red when it drops.
Pairing is a mutual exchange with no third party (ADR 0028): each side creates an invite and opens the other's, and the two connect once each has applied the other's payload.
An invite is one link — the payload IS the fragment (`#<payload>`, bare base64url, ADR 0028), staying in the hash (never sent to a server) and linkifying whole — shown once per host: the browser card carries a compact "🔗 copy link" and the scannable `<qr-code>` (ADR 0029, the same component the terminal renders), the terminal the full link as wrapped, selectable text.
Pairing is symmetric underneath (ADR 0028): both sides create WebRTC offers and each synthesizes the peer's answer locally with fingerprint-derived DTLS roles, so nobody has to be "first"; a link opened in a browser that already has a waiting tab hands the connection over via BroadcastChannel — a same-browser handover, not a server.
A session lives in its tab: a return link carries a digest of the invite it answers, so the handover routes each reply to the exact tab that invited — several tabs can hold pairings with different recipients at once — and a reply to a tab that was closed or reloaded says so plainly (WebRTC state cannot outlive its page) instead of hanging.
A session can still move: opening a reply link beside a standing session offers to take it over, and the takeover re-signals a fresh pairing through the session's own wire (ADR 0032) — the new tab connects, the old one retires with a note, and the remote side (browser or terminal alike) re-pairs without noticing more than a blink.
The cross-tab machinery — which tab owns a session, claim routing, the takeover handshake, the `uicc1.` control plane — is `@schuhkarton/uic-sync`'s `session.js`, framework-free and ready to become a package of its own.
Opening a peer takes any form: the paste box parses tokens and full links alike, and camera scanning appears where `BarcodeDetector` exists (secure contexts).
The page passes a public STUN server to the pairing (the library default is none); the repo ships no TURN relay, and unreachable peers fail with a message instead of hanging.
One localStorage knob tunes a browser: `uic-ice` (a JSON `RTCIceServer` array appended to the STUN default — a TURN server with credentials reaches across hostile NATs; coturn mints long-lived credentials from its static secret with username = a future unix timestamp and credential = base64 of HMAC-SHA1(secret, username)).
Links are consumed exactly once (a load reads the payload, then strips it — a reload lands on the clean invite page and nothing lingers in history) and the important pairing events log to the browser console.
Each swap pairs exactly two browsers; links carrying the whole peer set, so a connected tab can keep inviting others, is the natural next step.
The baked dist is a plain static site, so the project's GitHub Pages serves it under `/lit-demo/` — the pairing page at `/lit-demo/p2p/` needs no server at all, and the HTTPS context enables the camera scanner there; the server-backed `live` mode naturally stays a `cargo run` affair.

**The terminal is a peer too (ADR 0028), rendering the same pairing UI (ADR 0029).** `cargo run -p uic_lit_demo -- p2p` mounts the shared `<pair-panel>` beside the todo list — the same component the browser renders — showing the invite link wrapped across lines (`overflow-wrap: anywhere`, honored by the terminal's text layout), a scannable QR (a native terminal widget, ADR 0029), a peer-paste box, and the renew ("start over") and connect controls, with a live badge.
The QR draws black on white whatever the terminal theme (a camera wants dark modules on a light ground) and docks to the right of both panes on a wide terminal, wrapping below them under about 200 columns — real flexbox from the p2p deck's stylesheet, not host rect math.
`cargo run -p uic_lit_demo -- p2p '<link-or-token>'` opens an invite the other way — decode the browser's link or its pairing code to connect, then send your own token back (paste it into the browser's panel) so the other side connects too.
The panel is presentation only, driven by properties the host sets and signalling button intent back through a `command` property the terminal loop polls (Boa has no events); the transport differs per host — the browser's `pair-wizard` owns the swap and the clipboard, the terminal's native `webrtc-rs` peer carries the codec in Rust (`uic_sync::pair`, one byte contract with the page).
The terminal runs as an ICE-lite agent (host candidates only, so it pairs on a shared network — the same LAN, a personal hotspot — while crossing NATs stays out of scope), and `UIC_LIT_DEMO_ICE_DEBUG=1` traces the candidates when a pairing will not come up.

## Knobs

- `UIC_LIT_DEMO_ADDR=host:port` moves the server (default `127.0.0.1:8090`; `live` defaults to `0.0.0.0:8090`).
- `WEB_MODULES_EMBEDDED=1` forces the fully-embedded dist (no filesystem reads).
- Dev serving recompiles `web/pages/` live; a change under `web/src/` re-bakes through cargo — restart the server.
- `UIC_LIT_DEMO_P2P_PAGE=url` sets the page a terminal invite links to (default the published `/lit-demo/p2p/`; point it at a dev server to pair locally).
- `UIC_LIT_DEMO_ICE_DEBUG=1` traces the terminal peer's ICE candidates and connection state.
