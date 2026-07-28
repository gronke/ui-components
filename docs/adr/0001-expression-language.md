# ADR 0001: The template expression language is closed

## Decision

Holes in templates take exactly one of:

- a property or computed-property name (`${value}`, `placeholder=${placeholder_text}`),
- a negated name in boolean positions (`?hidden=${!visible}`, `<template if=${!error_message}>`),
- a loop variable's member (`${row.field}`) inside a `<template for>` body,
- a bare handler name in event positions (`@change=${on_change}`).

Conditionals are `<template if=${name}>…</template>`; nesting expresses AND.
The `|` character is reserved inside holes for future formatters and is rejected with a hint today.
Anything richer — parsing, formatting, arithmetic, branching on values — must become a named computed property or handler on the component.

### The one iteration form

`<template for=${rows} as=row>…</template>` repeats its body once per element of an array-valued property, binding each element to the loop variable.

```html
<template for=${rows} as=row>
  <tr>
    <td>${row.name}</td>
    <td>${row.role}</td>
  </tr>
</template>
```

- `for=${expr}` names the array; `as=name` binds each element to a loop variable.
- A member hole `${name.field}` is valid only where `name` is a loop variable in scope; the parser records the scope as it descends and rejects an unbound base (and a negated member).
- Bare `${ident}` and `${!ident}` still resolve against the component's properties and computeds, so component values remain reachable inside the body.
- Nesting composes: a `for` body may contain `if` and further `for`, extending the scope with each `as` binding, and a nested `for`'s array may itself be a member of the outer variable (`${card.rows}`).

Backing data is `Value::Array(Vec<Value>)`, `JsType::Array`, browser type `Record<string, unknown>[]` (rows are objects, so `${row.field}` typechecks).
Rows are ordinary objects (`Value::Object`), so `${row.field}` reads a member exactly like the state pattern (ADR 0013), one level deep.
`uic_core::json` carries arrays in both directions, so an app can push rows as state.

### A loop body renders data, not widgets

The body holds text, holes and plain elements.
It may not contain a custom element or a widget-implying element (a `data-tui` kind or a plain `input`/`textarea`/`select`), because those mount stateful instances whose count must stay static in template position.
The derive and the TUI lint reject a widget or custom tag inside a `for` body; options-as-data stays the way to render a variable list of interactive items (ADR 0005).

### Execution per target

The web generator emits the `.map` shape, with member holes emitted as `row.field`:

```ts
${rows.map((row) => html`<tr><td>${row.name}</td><td>${row.role}</td></tr>`)}
```

A nested repeat's source carries a runtime-shape cast (`(card.rows as Record<string, unknown>[]).map(…)`), because a member of a row is `unknown` to the type system.
The terminal executes iteration through the `Repeat` part of the parts engine (ADR 0008): the engine resolves nothing, `PartValue::List` carries the fully resolved body holes — one inner vector per row, in body-hole order — and `CompiledTemplate::repeats()` exposes the repeat tree so the runtime glue resolves each row's holes under a scope stack of loop variables layered over the store.
Row instances rebuild whenever the resolved list changes, dirty-checked at the list level; keyed reconciliation stays a later optimization, and correctness does not depend on it.

## Why

Both render targets execute every template: the browser runs generated Lit TypeScript, the terminal interprets the IR directly.
A closed expression grammar is what keeps that guarantee cheap; arbitrary expressions would need a transpiler per target and would silently diverge.
Member holes gated to loop scope are the grammar's one composite form: without them iteration could not render a row's columns, and with more the language would stop being closed.

## Consequences

The parser (`uic_template`) rejects richer expressions: the reserved `|` errors with the escape-hatch hint ("declare a computed property on the component"), and an unbound member base points at the enclosing `<template for>` it needs.
Extending the grammar (formatters, inline ternaries) requires a new ADR and simultaneous support in `uic_codegen_web` and `uic_tui`.
`Value::Array` follows the object rules for truthiness and display: always truthy, empty in text position.
Lists of widgets remain out of scope by construction; a variable list of interactive items is options-as-data (ADR 0005).
The parts engine keeps its "resolve nothing" contract (ADR 0008); scope resolution lives in the runtime glue, next to the existing hole resolver.
