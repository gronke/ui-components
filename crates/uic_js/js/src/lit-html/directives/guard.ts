// lit-html produces guard, a render memo. The mock recomputes every
// render — the same output, none of the caching.

export const guard = (_deps: unknown, render: () => unknown): unknown => render();
