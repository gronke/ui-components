# ADR 0010: Templates compile to parts

## Context

ADR 0008 queued the parts compiler as the first layer on the retained DOM: lit-html's architecture, where a template turns into DOM once and updates patch only the bound positions, instead of re-expanding everything per frame.
lit prepares its templates by injecting sentinel markers into the joined strings, parsing with the real HTML parser, and walking the result once into a cached part plan; attribute-name case survives in a side array because parsers lowercase.

Our dialect differs from lit's tagged literals in one convenient way: holes are named (`${ident}`), not positional values — so the raw source parses as-is (proven by the sink's dialect-survival tests) and the holes are self-marking.
No marker-injection pass is needed; only text holes need reifying into addressable nodes after the parse.

## Decision

`uic_dom::parts` implements the engine, value-agnostic:

- **Compile once**: `CompiledTemplate::compile(source)` parses the raw source into a prototype `Document` and walks it into a part plan.
  Text holes split into comment-marker nodes — the part's stable anchor; real node identity replaces lit's start/end marker pair.
  Bound attributes classify by lit's prefixes (`.` property, `?` boolean, `@` event, plain attribute with static chunks around multiple holes) and are removed from the prototype; their case-sensitive names recover from the source by index, lit's side-array technique, with a hard error when source and tree disagree.
- **`<template if=${…}>` is the conditional part.**
  The template element itself is the anchor; its body compiles into a branch plan and the anchor's contents link is severed, so instances clone an empty anchor and the body clones from the branch on demand — true becomes instantiate-or-patch, false tears the branch down.
- **Instantiate by cloning**: the new `Document::import_node` (the DOM's `importNode`, deep) copies the prototype under a target parent and returns the id map that rebinds the plan to the copy.
  Several instances of one template coexist independently.
- **Commit patches only what changed**: one `PartValue` per hole (`Text`, `Bool`, `Value`, `Nothing`, `NoChange`), dirty-checked per part with lit's semantics — `NoChange` keeps the committed state, a single-hole `Nothing` removes the attribute while multi-hole values render it empty, booleans toggle presence.
- **The tree owns what the tree can hold.**
  Child, attribute and boolean parts commit into the document; property parts come back as `PropertyWrite` data and event bindings surface at instantiation (plus per-commit for freshly rendered branches) — applying them against component instances is the runtime glue's job, which keeps `uic_dom` free of the component model.

## Consequences

- The engine resolves nothing: holes carry their raw expression text and the caller supplies values per commit, so the existing expression language (and any future formatter syntax) stays outside `uic_dom`.
- Rendered output carries `<!--uic-part-->` markers, like lit's comment markers in the browser DOM; the serializer shows them, which the tests use as structure assertions.
- Lists (`repeat`-style keyed reconciliation) are deliberately absent until the dialect grows a list primitive; the child part's single-anchor design has room for committed node sequences.
- Next in the arc: the runtime glue — resolve holes against `PropertyStore`/`Behavior` per update cycle, apply `PropertyWrite`s through child bindings, wire `EventBinding`s to handlers — and then TUI layout/paint reading the retained tree.
