// The serializer polyfill: lit's template tags and the render-to-string
// commit, recovering the binding prefixes (`.prop=`, `@event=`, `?attr=`)
// from the static strings exactly like the parts compiler does (ADR 0008).

import { listenerFns, nothing } from './state.js';

let listenerId = 0;
let renderListenerSink: number[] | null = null;
let renderHost: any = null;

const escapeHtml = (text: unknown): string =>
    String(text).replace(
        /[&<>"']/g,
        (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' })[c]!,
    );

export interface TemplateResult {
    __litTemplate: true;
    strings: TemplateStringsArray;
    values: unknown[];
}

export const html = (strings: TemplateStringsArray, ...values: unknown[]): TemplateResult => ({
    __litTemplate: true,
    strings,
    values,
});

export const css = (strings: TemplateStringsArray, ...values: unknown[]) => ({
    __litCss: true,
    cssText: String.raw({ raw: strings.raw }, ...values),
});

// ---- the render lifecycle bracket (listener markers scope per render) ----

export function beginRender(host: any): void {
    renderListenerSink = [];
    renderHost = host;
}

export function endRender(): number[] {
    const created = renderListenerSink ?? [];
    renderListenerSink = null;
    renderHost = null;
    return created;
}

export function releaseListeners(ids: number[]): void {
    for (const id of ids) {
        listenerFns.delete(id);
    }
}

// ---- render to string, recovering lit's binding prefixes ----

// Handles the tail of the accumulated output before a hole: lit's `.prop=`,
// `@event=` and `?attr=` prefixes are recovered from the static strings
// exactly like the parts compiler does (ADR 0008).
const BINDING_TAIL = /([.@?])([a-zA-Z][\w-]*)=("|')?$/;

interface Out {
    text: string;
}

export function renderToString(value: unknown): string {
    const out: Out = { text: '' };
    appendValue(out, value);
    return out.text;
}

function appendTemplate(out: Out, template: TemplateResult): void {
    const { strings, values } = template;
    let swallow: string | null = null;
    for (let i = 0; i < strings.length; i += 1) {
        let part = strings[i]!;
        if (swallow && part.startsWith(swallow)) {
            part = part.slice(swallow.length);
        }
        swallow = null;
        if (i >= values.length) {
            out.text += part;
            continue;
        }
        const binding = BINDING_TAIL.exec(part);
        if (binding) {
            out.text += part.slice(0, binding.index);
            swallow = commitBinding(out, binding, values[i]);
        } else {
            out.text += part;
            appendValue(out, values[i]);
        }
    }
}

// The unterminated open tag at the tail of the accumulated output — where
// a binding necessarily sits. `.value=` serializes as the value attribute
// only on the browser's value-carrying elements (lit-SSR's rule), so a
// custom element with a `value` property stays untouched.
const OPEN_TAG_TAIL = /<([a-zA-Z][\w-]*)[^<>]*$/;

function valueCarryingTag(text: string): boolean {
    const open = OPEN_TAG_TAIL.exec(text);
    if (!open) {
        return false;
    }
    const tag = open[1]!.toLowerCase();
    return tag === 'input' || tag === 'textarea' || tag === 'select';
}

function commitBinding(out: Out, binding: RegExpExecArray, value: unknown): string | null {
    const kind = binding[1];
    const name = binding[2]!;
    const quote = binding[3];
    if (kind === '@') {
        if (typeof value === 'function') {
            listenerId += 1;
            // Template listeners run with `this` bound to the host
            // component, lit's EventPart contract.
            listenerFns.set(listenerId, { event: name, fn: value, host: renderHost });
            renderListenerSink?.push(listenerId);
            out.text += ` data-uic-l-${name}="${listenerId}"`;
        }
    } else if (kind === '?') {
        if (value) {
            out.text += ` ${name}=""`;
        }
    } else if (kind === '.') {
        // Property bindings do not exist in serialized HTML; `hidden` maps
        // to its attribute, `.value` on a value-carrying element becomes
        // the value attribute (the host commit syncs the terminal widget
        // from it, echo-skipped), everything else is dropped here (the
        // per-part commit path is the recorded follow-up).
        if (name === 'hidden' && value) {
            out.text += ' hidden=""';
        } else if (name === 'value' && valueCarryingTag(out.text)) {
            out.text += ` value="${escapeHtml(String(value ?? ''))}"`;
        }
    }
    return quote ?? null;
}

function appendValue(out: Out, value: unknown): void {
    if (value === null || value === undefined || value === nothing || value === false) {
        return;
    }
    if (typeof value === 'string') {
        out.text += escapeHtml(value);
        return;
    }
    if (typeof value === 'number' || typeof value === 'boolean') {
        out.text += String(value);
        return;
    }
    if (Array.isArray(value)) {
        for (const item of value) {
            appendValue(out, item);
        }
        return;
    }
    if (typeof value === 'object' && (value as TemplateResult).__litTemplate) {
        appendTemplate(out, value as TemplateResult);
        return;
    }
    if (typeof value === 'object' && typeof (value as any)[Symbol.iterator] === 'function') {
        for (const item of value as Iterable<unknown>) {
            appendValue(out, item);
        }
        return;
    }
    if (typeof value === 'function') {
        return;
    }
    out.text += escapeHtml(String(value));
}
