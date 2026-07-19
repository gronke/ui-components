// lit-html produces choose: the first matching case renders.

export const choose = <T, V>(
    value: T,
    cases: Array<[T, () => V]>,
    defaultCase?: () => V,
): V | undefined => {
    for (const entry of cases) {
        if (entry[0] === value) {
            return entry[1]();
        }
    }
    return defaultCase ? defaultCase() : undefined;
};
