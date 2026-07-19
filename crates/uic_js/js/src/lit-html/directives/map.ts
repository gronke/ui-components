// lit-html produces map: a plain generator over the items.

export function* map<T>(
    items: Iterable<T> | undefined | null,
    f: (value: T, index: number) => unknown,
): Generator<unknown> {
    if (items === undefined || items === null) {
        return;
    }
    let index = 0;
    for (const value of items) {
        yield f(value, index);
        index += 1;
    }
}
