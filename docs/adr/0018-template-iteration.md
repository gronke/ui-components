# ADR 0018: Template iteration over data rows

## Context

ADR 0006 deferred a general `<template for>` until something structurally needed repetition.
Rendering a variable-length table — many rows, several columns each — is that need: it cannot be modelled as select options (a single-label list fed to one widget) and it cannot be a fixed emission like `<select>`.

The expression language is closed (ADR 0001), the parts engine resolves nothing and takes one value per hole (ADR 0010), and the slot model requires widget counts to be static in template position (ADR 0006).
Iteration must fit all three.

## Decision

A new template construct repeats a body once per element of an array-valued property.

```html
<template for=${rows} as=row>
  <tr>
    <td>${row.name}</td>
    <td>${row.role}</td>
  </tr>
</template>
```

- `for=${expr}` names the array; `as=name` binds each element to a loop variable.
- Inside the body, a hole may reference a loop variable's member with `${name.field}` — the one grammar extension, scoped to the loop variables in scope.
- Bare `${ident}` and `${!ident}` still resolve against the component's properties and computeds, so component values remain reachable inside the body.

Backing data is a new value type: `Value::Array(Vec<Value>)`, `JsType::Array`, browser type `Record<string, unknown>[]` (rows are objects, so `${row.field}` typechecks).
Rows are ordinary objects (`Value::Object`), so `${row.field}` reads a member exactly like the state pattern (ADR 0013), one level deep.

### The grammar extension

`uic_template` gains:

- `Node::For { each: Expr, item: String, body: Vec<Node> }`, parsed from `<template for=${each} as=item>…</template>`.
- `Expr::Member { base: String, field: String }`, parsed from `${base.field}`.
  A member hole is valid only where `base` is a loop variable in scope; the parser records the scope as it descends and rejects an unbound base with the closed-grammar hint.

Nesting composes: a `for` body may contain `if` and further `for`, extending the scope with each `as` binding.
Both render targets already walk the IR recursively, so nesting is structural, not special-cased.

### The static-widget invariant holds

A loop body renders **data**: text, holes and plain elements.
It may not contain a custom element or a `data-tui` widget, because those mount stateful instances whose count the slot model needs fixed.
The derive and the TUI lint reject a widget or custom tag inside a `for` body, pointing at options-as-data (ADR 0006) for lists of widgets.

### Web emission

The generator emits the proven `.map` shape it already uses for select options (ADR 0006):

```ts
${rows.map((row) => html`<tr><td>${row.name}</td><td>${row.role}</td></tr>`)}
```

Member holes emit as `row.field`; the closed expression compiler gains one case for `Expr::Member`.

### Terminal: a repeat part in the parts engine

`uic_dom::parts` gains a `Repeat` part beside `Conditional`, using the child part's single-anchor seam ADR 0010 reserved for committed node sequences.
The engine still resolves nothing.
`PartValue::List(Vec<Vec<PartValue>>)` carries the fully resolved body holes, one inner vector per row, in body-hole order.
`CompiledTemplate` exposes each repeat's `item` variable and its body-hole expressions, so the caller resolves the array, then resolves each row's body holes against that row, and hands the engine the `List`.

`uic_tui`'s resolver reads a member hole `${name.field}` against a scope stack of loop variables layered over the store: `name` resolves to the innermost matching loop variable's row (a `Value::Object`), `.field` reads its member; a bare ident still resolves against the store or a computed.

Repeats nest: `CompiledTemplate::repeats()` returns the repeat **tree**, a nested repeat's `each` (a member of the outer variable, `${card.rows}`) resolves under the outer scope, and its rows resolve with both variables in scope — the resolved lists nest inside the outer rows at the nested repeat's body slot.
The commit path needs nothing extra: a nested repeat part committed inside an instantiated row receives that row's values, where its list already sits.
The web generator emits a nested source with a runtime-shape cast (`(card.rows as Record<string, unknown>[]).map(…)`), because a member of a row is `unknown` to the type system.

The implementation rebuilds the row instances whenever the resolved `List` changes, dirty-checked at the list level.
Keyed reconciliation stays a later optimization; correctness does not depend on it.

### Transport

`uic_core::json` gains array support in both directions, replacing the previous "arrays are unsupported" error.
`Value::Array` round-trips through the state/JSON layer, so an app can push rows as state.

## Consequences

- The closed grammar grows by exactly one form (`${var.field}`), gated to loop scope, and it is added to `uic_codegen_web` and `uic_tui` in the same change (ADR 0001).
- `Value` gains an array variant; truthiness and display follow the object rules (an array is an object: always truthy, empty in text position).
- Lists of **widgets** remain out of scope by construction; options-as-data stays the way to render a variable list of interactive items (ADR 0006).
- The parts engine keeps its "resolve nothing" contract; scope resolution lives in the runtime glue, next to the existing hole resolver.
