// The host entry: evaluated once at engine startup, it publishes the
// customElements registry and the flat entry points the Rust host calls by
// name (mount, property writes, event delivery, focus).

import {
    deliver,
    focusNode,
    installClipboard,
    installDialogs,
    installStorage,
    instances,
    mountAt,
    queryAllNodes,
    registry,
} from './runtime.js';

// `static styles` flattens like lit's finalize: nested arrays of css``
// results (or raw strings) concatenate into one sheet per tag.
function collectStyles(styles: unknown): string {
    if (!styles) {
        return '';
    }
    if (Array.isArray(styles)) {
        return styles.map(collectStyles).join('\n');
    }
    if (typeof styles === 'object' && (styles as any).cssText) {
        return String((styles as any).cssText);
    }
    if (typeof styles === 'string') {
        return styles;
    }
    return '';
}

(globalThis as any).customElements = {
    define(tag: string, cls: any) {
        registry.set(tag, cls);
        const cssText = collectStyles(cls.styles);
        if (cssText.trim().length > 0) {
            __uic_adopt_styles(tag, cssText);
        }
    },
    get(tag: string) {
        return registry.get(tag);
    },
};

(globalThis as any).__uicMount = (tag: string, handle: number): number => mountAt(tag, handle);

(globalThis as any).__uicSetProp = (handle: number, name: string, value: unknown): void => {
    const el = instances.get(handle);
    if (!el) {
        throw new Error(`no instance for handle ${handle}`);
    }
    el[name] = value;
};

(globalThis as any).__uicGetProp = (handle: number, name: string): unknown => {
    const el = instances.get(handle);
    return el ? el[name] : undefined;
};

(globalThis as any).__uicDeliver = deliver;
(globalThis as any).__uicFocus = focusNode;
(globalThis as any).__uicQueryAll = queryAllNodes;

installStorage();
installDialogs();
installClipboard();
