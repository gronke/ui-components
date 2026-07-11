# ADR 0013: App state is an object property, synchronized over a broadcast channel

## Decision

`uic_core::ObjectMap` (a newtype over `BTreeMap<String, Value>`) joins the closed object-valued set of ADR 0005: `JsType::Object`, `Value::Object`, browser type `Record<string, unknown>`, default `{}`.
Object properties follow the Options shape rules — plain `ObjectMap` (never `Option`), property-only, no `default`; the generated Lit declaration is `{ attribute: false }` with reference change detection, while the TUI's `PropertyStore` suppresses deeply equal writes through `PartialEq`.

`<app-root>` (`crates/ui_components/src/demo/`) applies the variant: the demo form as one component around a `state` object with one member per field.
Members reach the children through computed properties (`.value=${date}` where `date` reads `state["date"]`) — the template grammar stays closed to `ident`/`!ident` (ADR 0001), so member access lives in behavior, not in the expression language.
Each computed falls back to the child's own default when the member is absent, which makes a sparse state push no changes.
Child commits fold back through `@value-changed` handlers that clone the map, skip equal members, and set `state`; every real change notifies `state-changed` with the whole snapshot.

The transport is a raw full-state snapshot per message — no envelope.
In the demo a `BroadcastChannel` named `uic-app-state` (`apps/web-demo/web/bridge.ts`) links the DOM `<app-root>` and the TUI pane's session: `state-changed` posts the snapshot, `onmessage` applies it via `.state =` / `set_prop_json`.
Dedupe is by canonical serialization — top-level keys sorted, which `ObjectMap` (BTreeMap) and `bridge.ts`'s `canon()` produce identically — remembered as the last state seen anywhere; the page seeds the initial state and pushes it into the session before any listener attaches, so booting produces no traffic.

JSON ⇄ `Value` conversion lives in `uic_core::json` behind the `json` feature (`value_to_json`/`value_from_json`), shared by the wasm session's `set_prop_json`/`on_notify` and any future native transport.
The conversion is deliberately lossy-asymmetric: `Zoned` flattens to its ISO string, option lists to their data rows, and arrays are rejected on the way in — option lists are data (ADR 0006), and nothing state-shaped is array-valued.

## Why

The two render targets showed the same form with no shared data; a state object that trickles down and events that trickle up are the missing halves of one application over one definition.
Three echo brakes make the loop safe without protocol machinery: the member-equality guard in the handlers (Lit's reference dirty check would otherwise re-fire on every child echo), the canonical-string dedupe in the bridge (applying a received state re-fires the local `state-changed`), and the TUI's `PartialEq` suppression.
A raw snapshot with last-writer-wins is the simplest protocol that a WebSocket variant (a native TUI behind an axum server, the planned follow-up) can reuse byte-for-byte.

## Consequences

- The state contract is flat string-keyed scalars; nested objects convert but weaken the sorted-key dedupe to top level, and arrays do not travel.
- Concurrent edits resolve last-writer-wins; a server-side arbiter would be a transport concern, not a component one.
- Passing focus through an untouched `allow-null` input commits null into the state (the change-on-blur contract), so a pass-through Tab can produce a legitimate state change.
- `app-root` registers like any catalog component but carries `dist = false`: the dev server and the runtimes see it, the published npm package does not.
- `uic_core` gains an optional `serde_json` dependency (feature `json`), enabled by `uic_tui_web`.
