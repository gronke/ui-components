// lit-html produces classMap; under the render-to-string commit it
// reduces to the joined class string.

export const classMap = (info: Record<string, unknown>): string =>
    Object.keys(info)
        .filter((key) => Boolean(info[key]))
        .join(' ');
