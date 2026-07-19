// lit-html produces join: the joiner (a value or an index function)
// interleaves the items.

export function* join<I, J>(
    items: Iterable<I> | undefined | null,
    joiner: J | ((index: number) => J),
): Generator<I | J> {
    if (items === undefined || items === null) {
        return;
    }
    let index = 0;
    for (const value of items) {
        if (index > 0) {
            yield typeof joiner === 'function' ? (joiner as (index: number) => J)(index - 1) : joiner;
        }
        yield value;
        index += 1;
    }
}
