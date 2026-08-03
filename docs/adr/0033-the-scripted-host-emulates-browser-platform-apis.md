# ADR 0033: The scripted host emulates browser platform APIs

## Decision

The scripted host (`uic_js`, Boa running real npm lit on the terminal runtime, ADR 0007) synthesizes the browser platform APIs a plain lit app reaches for beyond the DOM: Web Storage, the `alert`/`confirm`/`prompt` dialogs, and the clipboard.
An app that persists to `localStorage`, awaits `confirm(…)` or reads `navigator.clipboard` runs unchanged on the terminal, the way ADR 0026 lets it use a real `<input>`: the same bargain one layer out, from the component's DOM to the window's globals.

Each capability is one pattern in three layers:

- a JS shim in `crates/uic_js/js/src/runtime/` (`storage.ts`, `dialogs.ts`, `clipboard.ts`) that assigns the browser global, compiled into the shared runtime for every host and self-gating on its native's presence: a `typeof __uic_… !== 'function'` guard early-returns, so the shim is inert wherever the native was not registered;
- a flat `__uic_*` Boa native (`crates/uic_js/src/natives.rs`), registered only when its cargo feature is on;
- a thread-local seam in Rust (`storage.rs`, `dialogs.rs`, `clipboard.rs`) that the embedding app fills with a backend.

The shim is always present in the JS; the native and its backend are the feature-gated half.
Where the feature is off (or the host is the browser worker, ADR 0007, which registers only the DOM natives), the native is absent, the shim's guard fires, and the global never appears, exactly as a real environment lacking the API leaves it.
A Worker has no `localStorage` either, and the app's own `typeof localStorage === 'undefined'` guard skips persistence.
So the emulation lives only under Boa; the browser main thread uses the genuine globals, and one runtime source serves both.

**Web Storage** (`storage.ts`, feature `storage`) is `localStorage` alone: `getItem`, `setItem`, `removeItem`, `clear`, `key` and `length`, synchronous like the real API.
The `__uic_storage_*` natives route through a `StorageBackend` trait: `MemoryBackend`, a sorted `BTreeMap`, is the default, and `SqliteBackend` (feature `sqlite`, one `kv` table) persists across runs.
A refused `set` throws into JS, the way a browser's quota would.
The demo chooses the backend with `--backend memory://|sqlite://<path>` and the todo app keys its rows `uic-todos`, the same key the page's own `localStorage` serves when it runs on real lit (the synchronized state is a separate wire, ADR 0013; this is per-device persistence).

**Dialogs** (`dialogs.ts`, feature `dialogs`) put `alert`, `confirm` and `prompt` on the global, returning Promises where the browser blocks: `await confirm(…)` is the one spelling that reads identically on both hosts, because a scripted host cannot spin a nested event loop to suspend.
The `__uic_dialog_request` native pushes onto a one-way queue rather than a backend trait (a dialog is a question, not a store), and the host drains it, paints a terminal overlay (`uic_tui::dialog`), and answers back through the JS-side `__uicDialogAnswer` resolver.

**Clipboard** (`clipboard.ts`, feature `clipboard`) is `navigator.clipboard` with `readText`/`writeText`, Promise-wrapped over a synchronous `ClipboardBackend`.
The demo installs an arboard-backed `SystemClipboard` in the pairing mode only (`--clipboard`, opt-in), so the emulated `navigator.clipboard` and the pairing loop's own watch (ADR 0028) share one system connection.
On a headless host arboard fails to open and the backend is inert (reads resolve to `''`, writes no-op) while the manual paste path stays.

## Why

Two hosts render one app from one definition, and the browser globals are where a plain app's I/O lives once it steps past the DOM: a list persists to `localStorage`, a delete asks `confirm`, a share reads the clipboard.
Rather than fork the app per host, the scripted host answers those globals, so the app stays ordinary browser code and the terminal supplies the platform, the ADR 0026 arrangement (native widgets for native markup) carried up to the window.

The split (shim always in the JS, native and backend feature-gated in Rust) keeps the runtime one byte-identical source across hosts while letting a binary compile in only the capabilities it wants: the worker takes none of them (it wraps DOM ops only), a minimal terminal app takes none, and the lit-demo takes all four.
The self-gating guard makes "feature off" indistinguishable from "API absent," which is the honest browser behavior: code that feature-detects `localStorage` or `navigator.clipboard` already handles both.

Dialogs return Promises because blocking is the one browser semantic a scripted host cannot honor: `window.confirm` suspends the event loop, and Boa has no nested loop to suspend.
Async is the smallest faithful shape, and `await` makes the terminal and the browser read the same; storage and the clipboard are synchronous underneath, so their Promise wrapping only matches the browser's typed surface.

The backends live in the app, not the library, for the same reason the WebRTC stack does (ADR 0028): `uic_js` owns the seam (the trait, the thread-local and the native), the embedder the policy (memory or SQLite, a real clipboard or none).

## Consequences

- The capabilities are additive cargo features (`storage`, `sqlite`, `dialogs`, `clipboard`), all off by default; a consumer enables what its app calls, and the JS shim carries no weight when the native is absent.
- The clipboard backend is what gives the terminal a clipboard the pairing UI (ADR 0029) can use: the paste affordance and the credential watch both read through the one arboard connection, and a headless run degrades to the always-present manual path.
- `localStorage` is the terminal's per-device store; the `uic-ice` knob (ADR 0028) and the todo rows are ordinary consumers, while a browser page bypasses the shim entirely for its native store.
- Dialogs are answered by the host's own UI, so their look is the terminal's (an overlay), not the browser's chrome; the contract is the return value, not the presentation.
- Only `localStorage` is emulated, not `sessionStorage` or the `Storage` constructor, and only the three dialog functions: the surface an app actually reaches for, extended when one more is needed.
- The worker host (ADR 0007) omits these natives deliberately: `localStorage` stays `undefined` there, matching a real Worker scope, and the app's feature-detection skips persistence; the emulation is a terminal affordance, not a browser one.
