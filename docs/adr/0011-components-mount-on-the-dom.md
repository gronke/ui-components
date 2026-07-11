# ADR 0011: Components mount on the DOM

## Context

ADR 0010 shipped the value-agnostic parts engine and named its consumer: the runtime glue that resolves holes against component state, applies property writes through child bindings, and wires event bindings to handlers.
This is the layer where the terminal runtime starts speaking LitElement's language over the retained tree — data passing down a real (faked) DOM, events coming back up it — ahead of the paint migration.

## Decision

`uic_tui::dom::DomHost` mounts registered custom elements onto a `uic_dom::Document`, one `Mount` per component beside its element node.

- **Templates compile from the parsed IR.**
  `CompiledTemplate::from_template` builds the prototype straight from the component's (chrome-spliced) `uic_template::Template` — no HTML re-parse, authored names preserved by construction; the html5ever path stays for loading templates from raw HTML.
  Compiled templates cache per tag and every instance clones from the shared prototype.
- **Holes resolve like template expressions always have**: `ident` reads the store or dispatches to a computed getter, `!ident` negates truthiness; null and undefined become the engine's `Nothing`, so absent state clears attributes and branches the way lit's `nothing` does.
- **The update cycle is ReactiveElement's**, the same order the widget runtime runs: the trigger collects the batch with old values, `will_update` joins it, notify events emit, the commit patches the parts, `updated` observes the committed state and its writes drive a converging follow-up cycle.
- **Data flows down through the tree itself.**
  Child custom tags mount recursively wherever they appear — including inside conditional branches the moment they render — and receive state two ways: attribute parts commit onto the child's node and the child syncs its observed attributes from it (additions, changes and removals, so a cleared `?disabled` arrives as absence), while `.prop` writes from `CommitEffects` apply straight to the child's store.
  Writes onto plain elements wait for the paint migration, where they map to widget state.
- **Events flow up two ways at once.**
  A child's notify events route into the parent's `@event` template bindings as handler calls (the statically-resolved analog of the listener the template declares), and every notify event also dispatches as a bubbling DOM event from the component's node — so document-level listeners hear `value-changed` from anywhere below, like the browser.
- Branch teardown became recursive in the engine: a nested conditional's insertions are siblings beside its anchor, which the enclosing branch's node list does not cover.

## Consequences

- The composite story runs end to end on the DOM: an `input-date-range` mount pushes decomposed values into its `input-date` children (their own lifecycles run — invalid dates raise the child error rows in the child DOM), and a value committed inside a child routes up through `on_start_changed`, clamps, and lands in the other child.
- The render pass is untouched: the retained tree runs beside the per-frame expansion.
  The paint migration — layout and paint reading the `Document`, widget state in the node payload, geometric hit-testing replaced by tree events — is the next arc, after which the expansion retires.
- `DomHost` is single-threaded by design (the template cache is thread-local), matching the TUI runtime and the wasm host.
