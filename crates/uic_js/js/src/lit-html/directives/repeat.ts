// lit-html produces repeat. Under the subtree-swap commit keys cannot pin
// DOM identity, so repeat degrades to an unkeyed map; focus survives the
// swaps by data-path re-resolution instead.

export function* repeat<T>(
    items: Iterable<T> | undefined | null,
    keyFnOrTemplate: (item: T, index: number) => unknown,
    template?: (item: T, index: number) => unknown,
): Generator<unknown> {
    const render = template ?? keyFnOrTemplate;
    if (items === undefined || items === null) {
        return;
    }
    let index = 0;
    for (const value of items) {
        yield render(value, index);
        index += 1;
    }
}
