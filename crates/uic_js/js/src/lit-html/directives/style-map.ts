// lit-html produces styleMap: the style attribute string. Custom
// properties keep their names; camelCase converts to kebab-case.

export const styleMap = (info: Record<string, unknown>): string =>
    Object.entries(info)
        .filter(([, value]) => value !== null && value !== undefined && value !== false)
        .map(([name, value]) => {
            const property = name.startsWith('--')
                ? name
                : name.replace(/[A-Z]/g, (upper) => '-' + upper.toLowerCase());
            return `${property}: ${value}`;
        })
        .join('; ');
