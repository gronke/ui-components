// lit-html produces keyed. The subtree-swap commit rebuilds either way,
// so the key degrades to its value.

export const keyed = (_key: unknown, value: unknown): unknown => value;
