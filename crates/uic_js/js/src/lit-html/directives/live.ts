// lit-html produces live, a set-against-the-live-DOM guard. The
// serialize commit writes every frame, so the value passes through.

export const live = (value: unknown): unknown => value;
