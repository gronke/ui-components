# ADR 0008: A retained DOM for the TUI

## Decision

The terminal renders from a retained DOM: `uic_dom` owns the tree, templates compile once into part plans over it, components mount on it with LitElement's semantics, and layout, paint and input read it as the one source of truth.

### The tree is ours, on `indextree`

`Document<T>` wraps an arena; `NodeId` is copyable identity, generation-checked so ids of freed or recycled slots read as absent.
Every element carries a consumer payload `T`: the hook the runtime uses to attach widget state to nodes.
The mutation API is public and web-shaped: `create_element`, `append_child`, `insert_before`, `replace_child`, `detach` (removeChild semantics), `remove` (destroy and reclaim), `set_attribute`, the class list, text access, traversal.
Appending a parented node moves it, like the web.
A typed vocabulary (`html::Div`, `html::Input`, …) covers the tag slice the components use; the `ElementKind` trait leaves room for per-element typed attributes and a generated full vocabulary later.

### Parsing is html5ever through `TreeSink`

Raw HTML parses straight into the arena: no intermediate DOM.
`parse_fragment` uses the spec's fragment algorithm (no implied `html`/`head`/`body`); `<template>` children land in a separate contents fragment per the whatwg model, at every creation path.
The lit-flavored dialect rides through the parser as ordinary attributes and text: `?attr`, `.prop` and `@event` are legal attribute names, `${holes}` are plain text.
The parser lowercases attribute names per spec; the parts compiler recovers case-sensitive names from the template source by index, exactly like lit's `$lit$` side array.
Malformed input never fails; diagnostics collect on `Document::parse_errors`.
Serialization reuses html5ever's spec serializer (`outer_html`/`inner_html`): escaping, void elements and template contents come out browser-shaped.

### Events follow the whatwg dispatch subset a single light-DOM tree needs

Capture, at-target and bubble phases run over the ancestor path, `stop_propagation` versus `stop_immediate_propagation`, `prevent_default` honored only on cancelable events outside passive listeners, and `capture`/`once`/`passive` listener options.
Listeners register directly per node (lit's EventPart model, no delegation) and receive the document mutably, so the public DOM API works inside handlers.
Named constructors encode the native table: `input` and `change` bubble without cancelation, `submit` bubbles and cancels, `focus`/`blur` do not bubble.
Retargeting and `composed` are omitted: there are no shadow boundaries in a light-DOM tree.

### `<slot>` is lowered, not implemented natively

Slot assignment only functions inside a ShadowRoot, and the components render light DOM, so a literal `<slot>` would project nothing in the browser.
The one slot in the model is the chrome seam: a chrome template (`wraps_src`) carries exactly one `<slot/>`, and `uic_template::splice` replaces it with the wrapped component's template, so both targets consume one spliced template and no runtime slot machinery exists.

### Templates compile to parts

The parts engine (`uic_dom::parts`) is value-agnostic and compiles once: `CompiledTemplate::compile(source)` parses raw HTML into a prototype `Document` and walks it into a part plan, and `CompiledTemplate::from_template` builds the same prototype straight from a component's (chrome-spliced) `uic_template::Template`, authored names preserved by construction.
Text holes split into comment-marker nodes, the part's stable anchor; real node identity replaces lit's start/end marker pair.
Bound attributes classify by lit's prefixes (`.` property, `?` boolean, `@` event, plain attribute with static chunks around multiple holes) and are removed from the prototype; their case-sensitive names recover from the source by index, with a hard error when source and tree disagree.
`<template if=${…}>` is the conditional part: the template element itself is the anchor, its body compiles into a branch plan and the anchor's contents link is severed, so instances clone an empty anchor and the body clones from the branch on demand.
True becomes instantiate-or-patch, false tears the branch down, recursively across nested branches.
`<template for=…>` is the `Repeat` part beside `Conditional`, using the child part's single-anchor seam for committed node sequences; `PartValue::List` carries a repeat's resolved rows and `CompiledTemplate::repeats()` exposes the repeat tree (the iteration form is ADR 0001's).
Instantiation clones: `Document::import_node` (the DOM's `importNode`, deep) copies the prototype under a target parent and returns the id map that rebinds the plan to the copy, so several instances of one template coexist independently.
Commit patches only what changed: one `PartValue` per hole (`Text`, `Bool`, `Value`, `Nothing`, `NoChange`, `List`), dirty-checked per part with lit's semantics.
`NoChange` keeps the committed state, a single-hole `Nothing` removes the attribute while multi-hole values render it empty, booleans toggle presence.
The tree owns what the tree can hold: child, attribute and boolean parts commit into the document; property parts come back as `PropertyWrite` data and event bindings surface at instantiation (plus per-commit for freshly rendered branches) through `CommitEffects`.
Applying them against component instances is the runtime glue's job, which keeps `uic_dom` free of the component model.
The engine resolves nothing: holes carry their raw expression text and the caller supplies values per commit, so the expression language (ADR 0001) stays outside `uic_dom`.

### Components mount on the DOM

`uic_tui::dom` mounts registered custom elements onto the document, one `Mount` per component beside its element node; compiled templates cache per tag and every instance clones from the shared prototype.
Holes resolve as template expressions: `ident` reads the store or dispatches to a computed getter, `!ident` negates truthiness, member holes read the loop scope; null and undefined become the engine's `Nothing`, so absent state clears attributes and branches the way lit's `nothing` does.
The update cycle is ReactiveElement's, hook order byte-comparable with the browser (ADR 0002): the trigger collects the batch with old values, `will_update` joins it, notify events emit, reflected properties land on the host attributes, the commit patches the parts and syncs widgets and children, `updated` observes the committed state and its writes drive a converging follow-up cycle.
Data flows down through the tree itself.
Child custom tags mount recursively wherever they appear (including inside conditional branches the moment they render) and receive state two ways: attribute parts commit onto the child's node and the child syncs its observed attributes from it (additions, changes and removals, so a cleared `?disabled` arrives as absence), while `.prop` writes from `CommitEffects` apply straight to the child's store.
Property writes onto plain elements map to widget state.
Events flow up two ways at once: a child's notify events route into the parent's `@event` template bindings as handler calls (the statically-resolved analog of the listener the template declares), and every notify event also dispatches as a bubbling DOM event from the component's node, so document-level listeners hear `value-changed` from anywhere below, like the browser.

### Layout, paint and input read the document

- **Widget state lives in the node payload.**
  `DomDocument = uic_dom::Document<WidgetPayload>`; every widget-bearing element (a plain `input`/`textarea`/`select` by element type or an explicit `data-tui` kind, ADR 0026) carries its rat widget in the payload, created idempotently whenever nodes appear (fresh instantiation or a conditional branch).
  `.value`/`.options` property writes from the parts engine sync the widget with the lit-style dirty check, so uncommitted typing survives unrelated updates; node identity is the whole bookkeeping.
- **Attributes are the runtime's stylesheet selectors.**
  Placeholders and `disabled` are committed attributes on the widget node, and component state reaches paint through reflection in the glue: `reflect` properties land on the host element as attributes during the update cycle (ReactiveElement's reflection), so the error outline reads `[error]` off the component exactly like the browser's stylesheet, and a `seamless` component's group renders borderless through the same mechanism (the sheets are ADR 0021's).
- **Layout and paint walk the document.**
  `dom::layout` builds the taffy tree from the nodes and their computed styles (cascaded per ADR 0021), keeps comment markers and conditional anchors invisible, and stacks mounted roots with a one-row flow margin; `dom::render` carries the paint semantics: borders, hints, focus ring, placeholder and resting-alignment overpaints, the select's closed label, the caret, and the overlays (calendar, option list) painted after all content off the focused node's widget.
- **Focus is a node.**
  The host walks the widget-bearing elements in document order; disabled widgets are skipped, and unrendered conditional branches are unfocusable by construction: their nodes do not exist.
- **Commits are events on the tree.**
  A widget commit routes into the `@change` binding its template declares (descending into the owning child mount) and dispatches a bubbling DOM `change` event: both halves of the browser's change-on-commit.
- **The pointer travels the tree.**
  Hit-testing resolves clicks against the widget areas rat records at paint, keyed by node; clicks focus, place the caret and pick from overlays via published geometry (rat's own mouse path stays unused: its click arming reads the system clock, absent on wasm32), drags select, the wheel pages, and a click into nothing blurs with change-on-blur.

## Why

The goal is UI state as a DOM-like structure with public element operations, mirroring LitElement's semantics closely enough that browser and terminal stay behaviorally identical, and terminal UIs designed and loaded from real HTML.
A retained tree gives plain elements runtime identity: nodes are addressable between frames, hold per-node state, receive events along the tree, and change structure dynamically.

The ecosystem shaped the hand-rolled approach:

- lit-html itself does not parse templates; it injects sentinel markers and feeds the result to the browser's real HTML parser, then walks the tree once and patches only the bound "parts" afterwards.
  Adopting a real HTML parser is therefore porting lit faithfully, not deviating from it.
- No crate offers a mutable DOM with consumer payloads on nodes: `dom_query` has a closed node type, `kuchikiki` sits on ancient servo crates, `markup5ever_rcdom` is test-only, and `blitz-dom` hard-requires stylo (the full Firefox CSS engine, 47 dependencies, MPL-2.0).
  The ecosystem practice is to hand-roll the retained tree over an arena (gpui, Masonry, Floem); html5ever deliberately ships no DOM and expects consumers to bring their own via `TreeSink`.
- blitz-dom's `DocumentMutator` and Ink's fake DOM define a proven mutation vocabulary; dioxus's `WriteMutations` shows the same shape driven from a declarative layer.

Our dialect differs from lit's tagged literals in one convenient way: holes are named (`${ident}`), not positional values, so the raw source parses as-is and the holes are self-marking.
No marker-injection pass is needed; only text holes need reifying into addressable nodes after the parse.

## Consequences

- `uic_tui::App` is the one application host, on the OS event loop natively and driven by the wasm `TuiSession` in the browser; mounts address roots by index.
- The host is single-threaded by design (the template cache is thread-local), matching the TUI runtime and the wasm host.
- The wasm CI job lints `uic_dom` for `wasm32-unknown-unknown` alongside the browser TUI; the html5ever chain (markup5ever, tendril, web_atoms, string_cache) is wasm-clean and single-thread safe.
- Rendered output carries `<!--uic-part-->` markers, like lit's comment markers in the browser DOM; the serializer shows them, which the tests use as structure assertions.
- Keyed list reconciliation is deliberately absent: a repeat rebuilds its rows when the resolved list changes, dirty-checked at the list level (ADR 0001).
- `Document<T>` panics on structural mutation through stale ids and on cycle-creating appends (the web throws `HierarchyRequestError` there); read access degrades to `None`.
- The full TestBackend suite (render, select, nested, range, mouse, lifecycle, ANSI) runs against this pipeline; the lifecycle order's mid-cycle observer is a DOM event listener, because notify events dispatch as bubbling events during the update cycle: the browser's timing.
