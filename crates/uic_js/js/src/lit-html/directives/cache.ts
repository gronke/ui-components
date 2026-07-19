// lit-html produces cache, a DOM cache across template switches. The
// subtree-swap commit rebuilds either way, so the value passes through.

export const cache = (value: unknown): unknown => value;
