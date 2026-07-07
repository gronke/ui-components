# ADR 0001: The template expression language is closed

## Decision

Holes in templates take exactly one of:

- a property or computed-property name (`${value}`, `placeholder=${placeholder_text}`),
- a negated name in boolean positions (`?hidden=${!visible}`, `<template if=${!error_message}>`),
- a bare handler name in event positions (`@change=${on_change}`).

Conditionals are `<template if=${name}>…</template>`; nesting expresses AND.
The `|` character is reserved inside holes for future formatters and is rejected with a hint today.
Anything richer — parsing, formatting, arithmetic, branching on values — must become a named computed property or handler on the component.

## Why

Both render targets execute every template: the browser runs generated Lit TypeScript, the terminal interprets the IR directly.
A closed expression grammar is what keeps that guarantee cheap; arbitrary expressions would need a transpiler per target and would silently diverge.

## Consequences

The parser (`uic_template`) rejects richer expressions with messages that point to the escape hatch ("declare a computed property/handler").
Extending the grammar (formatters, inline ternaries) requires a new ADR and simultaneous support in `uic_codegen_web` and `uic_tui`.
