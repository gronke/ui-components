# 8. A retained DOM for the TUI

Date: 2026-07-09

## Status

Accepted

## Context

The TUI re-expands the static template IR into a throwaway render tree every frame.
Only custom-element instances and their widget slots persist, and slots attach by counting template order — positional identity that survives conditionals only through careful bookkeeping.
Plain elements have no runtime identity at all: nothing can address them between frames, hold per-node state, receive events along the tree, or change structure dynamically (the template dialect deliberately has no list primitive, which is why select options travel as data).

The goal is to express UI state as a DOM-like structure with public element operations, mirroring LitElement's semantics closely enough that browser and terminal stay behaviorally identical — and eventually to design and load terminal UIs from real HTML.

Research (2026-07-09) shaped the approach:

- lit-html itself does not parse templates; it injects sentinel markers and feeds the result to the browser's real HTML parser, then walks the tree once and patches only the bound "parts" afterwards.
  Adopting a real HTML parser is therefore porting lit faithfully, not deviating from it.
- No crate offers a mutable DOM with consumer payloads on nodes: `dom_query` has a closed node type, `kuchikiki` sits on ancient servo crates, `markup5ever_rcdom` is test-only, and `blitz-dom` hard-requires stylo (the full Firefox CSS engine, 47 dependencies, MPL-2.0).
  The ecosystem practice is to hand-roll the retained tree over an arena (gpui, Masonry, Floem); html5ever deliberately ships no DOM and expects consumers to bring their own via `TreeSink`.
- blitz-dom's `DocumentMutator` and Ink's fake DOM define a proven mutation vocabulary; dioxus's `WriteMutations` shows the same shape driven from a declarative layer.

## Decision

A new foundation crate, `uic_dom`, owns the retained tree; the template parts compiler, the reactive update lifecycle and the TUI integration will build on it in follow-up arcs.

- **The tree is ours, on `indextree`.**
  `Document<T>` wraps an arena; `NodeId` is copyable identity, generation-checked so ids of freed or recycled slots read as absent.
  Every element carries a consumer payload `T` — the hook the TUI runtime will use to attach widget state where today a positional slot index stands in.
- **The mutation API is public and web-shaped**: `create_element`, `append_child`, `insert_before`, `replace_child`, `detach` (removeChild semantics), `remove` (destroy and reclaim), `set_attribute`, the class list, text access, traversal.
  Appending a parented node moves it, like the web.
  A typed vocabulary (`html::Div`, `html::Input`, …) covers the tag slice the components use; the `ElementKind` trait leaves room for per-element typed attributes and a generated full vocabulary later.
- **Parsing is html5ever through `TreeSink`**, straight into the arena — no intermediate DOM.
  `parse_fragment` uses the spec's fragment algorithm (no implied `html`/`head`/`body`); `<template>` children land in a separate contents fragment per the whatwg model, at every creation path.
  The lit-flavored dialect rides through the parser as ordinary attributes and text: `?attr`, `.prop` and `@event` are legal attribute names, `${holes}` are plain text.
  The parser lowercases attribute names per spec; the parts compiler will recover case-sensitive names from the template source by index, exactly like lit's `$lit$` side array.
  Malformed input never fails — diagnostics collect on `Document::parse_errors`.
- **Events follow the whatwg dispatch subset a single light-DOM tree needs**: capture, at-target and bubble phases over the ancestor path, `stop_propagation` versus `stop_immediate_propagation`, `prevent_default` honored only on cancelable events outside passive listeners, and `capture`/`once`/`passive` listener options.
  Listeners register directly per node (lit's EventPart model — no delegation) and receive the document mutably, so the public DOM API works inside handlers.
  Named constructors encode the native table: `input` and `change` bubble without cancelation, `submit` bubbles and cancels, `focus`/`blur` do not bubble.
  Retargeting and `composed` are omitted: there are no shadow boundaries in a light-DOM tree.
- **Serialization reuses html5ever's spec serializer** (`outer_html`/`inner_html`): escaping, void elements and template contents come out browser-shaped.

## Consequences

- The wasm CI job lints `uic_dom` for `wasm32-unknown-unknown` alongside the browser TUI; the html5ever chain (markup5ever, tendril, web_atoms, string_cache) is wasm-clean and single-thread safe.
- `<slot>` gets lowered, not implemented natively: slot assignment only functions inside a ShadowRoot, and the generated components render light DOM, so a literal `<slot>` would project nothing in the browser.
  The shared model for both targets is Vue-shaped — slots as named renderable values with the outlet expression providing fallback — and lands with the parts compiler.
- Follow-up arcs, in rough order: the lit-parts template compiler (instantiate once, patch bound parts, keyed list reconciliation), the ReactiveElement update lifecycle in `Behavior` (`request_update` collecting old values, batched `should_update → will_update → update → first_updated → updated`), TUI rendering and layout reading the retained tree (widget identity as nodes, tree-routed events), the `uic_macros` parser swap, and `query_selector` via the standalone `selectors` crate if needed.
- `Document<T>` panics on structural mutation through stale ids and on cycle-creating appends (the web throws `HierarchyRequestError` there); read access degrades to `None`.
