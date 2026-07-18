// The pure-function directive subset: lit's `map` and `when` are plain
// functions, and `classMap` reduces to a class string under the
// render-to-string commit.

import { nothing } from './runtime.js';

export const classMap = (info: Record<string, unknown>): string =>
    Object.keys(info)
        .filter((key) => Boolean(info[key]))
        .join(' ');

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

export const when = (
    condition: unknown,
    trueCase: () => unknown,
    falseCase?: () => unknown,
): unknown => (condition ? trueCase() : falseCase ? falseCase() : nothing);
