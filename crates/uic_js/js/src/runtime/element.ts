// The node facade polyfill: plain objects delegating to the host natives,
// one wrapper identity per node; components resolve through `instances`
// first, so an upgraded element is its own facade.

import { instances } from './state.js';
import { focusNode } from './focus.js';

const nodeWrappers = new Map<number, any>();

export function wrapNode(handle: number): any {
    if (instances.has(handle)) {
        return instances.get(handle);
    }
    if (nodeWrappers.has(handle)) {
        return nodeWrappers.get(handle);
    }
    const node = {
        __node: handle,
        getAttribute(name: string) {
            return __uic_get_attr(handle, name);
        },
        setAttribute(name: string, value: unknown) {
            __uic_set_attr(handle, name, String(value));
        },
        hasAttribute(name: string) {
            return __uic_has_attr(handle, name);
        },
        removeAttribute(name: string) {
            __uic_remove_attr(handle, name);
        },
        matches(selector: string) {
            return __uic_matches(handle, selector);
        },
        closest(selector: string) {
            return closestFrom(handle, selector);
        },
        contains(other: any) {
            return Boolean(other && __uic_contains(handle, other.__node));
        },
        focus() {
            focusNode(handle);
        },
        get tabIndex() {
            const value = __uic_get_attr(handle, 'tabindex');
            return value === null ? -1 : Number(value);
        },
        set tabIndex(value: number) {
            __uic_set_attr(handle, 'tabindex', String(value));
        },
        // The input facade: a mounted terminal widget answers with its live
        // text (the browser's `target.value` idiom); plain nodes fall back
        // to the value attribute.
        get value() {
            const text = __uic_widget_value(handle);
            return text !== null ? text : (__uic_get_attr(handle, 'value') ?? '');
        },
        set value(text: unknown) {
            __uic_set_widget_value(handle, String(text));
        },
        get dataset() {
            return datasetProxy(this);
        },
        get innerText() {
            return __uic_text(handle);
        },
        get textContent() {
            return __uic_text(handle);
        },
    };
    nodeWrappers.set(handle, node);
    return node;
}

function datasetProxy(node: any): any {
    // json-viewer reads `dataset.path`; a read-through object suffices.
    return {
        get path() {
            return node.getAttribute('data-path');
        },
    };
}

export function queryAllNodes(el: any, selector: string): any[] {
    if (el.__node < 0) {
        return [];
    }
    return __uic_query(el.__node, selector).map((handle) => wrapNode(handle));
}

// The nearest self-or-ancestor matching the selector, the DOM's closest().
// Event targets can be text nodes here (the retained tree lays them), so
// click discrimination wants the ancestor walk, not a bare matches().
export function closestFrom(handle: number, selector: string): any {
    let current = handle;
    while (current >= 0) {
        if (__uic_matches(current, selector)) {
            return wrapNode(current);
        }
        current = __uic_parent(current);
    }
    return null;
}
