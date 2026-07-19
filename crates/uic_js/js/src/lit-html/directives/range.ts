// lit-html produces range: integers from start toward end by step.

export function* range(startOrEnd: number, end?: number, step = 1): Generator<number> {
    const start = end === undefined ? 0 : startOrEnd;
    const stop = end ?? startOrEnd;
    for (let value = start; step > 0 ? value < stop : value > stop; value += step) {
        yield value;
    }
}
