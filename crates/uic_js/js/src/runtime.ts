// The mocked lit's shared runtime: the registries, the node facades, the
// property machinery and the render-to-string commit. Every other module
// builds on these singletons — ESM modules instantiate once, so this is the
// state the old bootstrap closure carried.

export const registry = new Map<string, any>();
export const instances = new Map<number, any>();

// Listener table: template `@event` bindings register the function here and
// the rendered HTML carries a `data-uic-l` marker per element.
export const listenerFns = new Map<number, { event: string; fn: Function; host: any }>();
let listenerId = 0;
let renderListenerSink: number[] | null = null;
let renderHost: any = null;

export const nothing = Object.freeze({ __litNothing: true });

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
// exactly like the parts compiler does (ADR 0010).
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
        // to its attribute, everything else is dropped here (the per-part
        // commit path is the recorded follow-up).
        if (name === 'hidden' && value) {
            out.text += ' hidden=""';
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

// ---- property machinery (shared by the element base and the decorators) ----

// Merged property options along the prototype chain: `static properties`
// and decorator registrations both land here.
export function collectProps(ctor: any): Record<string, any> {
    if (!ctor || ctor === Function.prototype) {
        return {};
    }
    const inherited = collectProps(Object.getPrototypeOf(ctor));
    const own = Object.prototype.hasOwnProperty.call(ctor, 'properties') ? ctor.properties : {};
    const decorated = Object.prototype.hasOwnProperty.call(ctor, '__uicDecorated')
        ? ctor.__uicDecorated
        : {};
    return { ...inherited, ...own, ...decorated };
}

function defaultFromAttribute(value: string | null, type: unknown): unknown {
    if (type === Boolean) {
        return value !== null;
    }
    if (type === Number) {
        return value === null ? null : Number(value);
    }
    if (type === Object || type === Array) {
        return value === null ? null : JSON.parse(value);
    }
    return value;
}

export function attributeName(name: string, options: any): string | null {
    if (options && typeof options.attribute === 'string') {
        return options.attribute;
    }
    if (options && options.attribute === false) {
        return null;
    }
    return name.toLowerCase();
}

export function fromAttribute(value: string | null, options: any): unknown {
    const converter = options && options.converter;
    if (converter && typeof converter.fromAttribute === 'function') {
        return converter.fromAttribute(value, options.type);
    }
    if (typeof converter === 'function') {
        return converter(value, options.type);
    }
    return defaultFromAttribute(value, options ? options.type : undefined);
}

// Does any prototype own an accessor for the name (a decorator put it
// there)? A module-level function on purpose: Boa 0.21 panics when a
// closure created inside a class constructor captures a local lexical
// binding — see the boa_quirks canary.
export function hasPrototypeAccessor(el: any, name: string): boolean {
    let proto = Object.getPrototypeOf(el);
    while (proto) {
        const desc = Object.getOwnPropertyDescriptor(proto, name);
        if (desc) {
            return Boolean(desc.get || desc.set);
        }
        proto = Object.getPrototypeOf(proto);
    }
    return false;
}

export function accessorDescriptor(name: string): PropertyDescriptor {
    return {
        get(this: any) {
            return this.__values.get(name);
        },
        set(this: any, value: unknown) {
            this.__values.set(name, value);
            this.requestUpdate();
        },
        configurable: true,
        enumerable: true,
    };
}

export function installAccessor(el: any, name: string): void {
    Object.defineProperty(el, name, accessorDescriptor(name));
}

export function installAccessors(el: any, props: Record<string, any>): void {
    for (const name of Object.keys(props)) {
        if (!hasPrototypeAccessor(el, name)) {
            installAccessor(el, name);
        }
    }
}

// ---- node facades: plain objects delegating to the host natives ----

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

// The nearest self-or-ancestor matching the selector — the DOM's closest().
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

// ---- events: bubbling dispatch over the retained tree ----

function listenersAt(handle: number, type: string): Function[] {
    const found: Function[] = [];
    const instance = instances.get(handle);
    if (instance) {
        for (const entry of instance.__listeners) {
            if (entry.type === type) {
                found.push(entry.listener);
            }
        }
    }
    const marker = __uic_get_attr(handle, `data-uic-l-${type}`);
    if (marker !== null) {
        const bound = listenerFns.get(Number(marker));
        if (bound) {
            found.push(bound.host ? bound.fn.bind(bound.host) : bound.fn);
        }
    }
    return found;
}

function makeEvent(type: string, init: any, targetHandle: number): any {
    return {
        type,
        key: init.key,
        shiftKey: Boolean(init.shiftKey),
        ctrlKey: Boolean(init.ctrlKey),
        altKey: Boolean(init.altKey),
        metaKey: Boolean(init.metaKey),
        target: wrapNode(targetHandle),
        currentTarget: null,
        relatedTarget: init.relatedTarget ?? null,
        bubbles: init.bubbles !== false,
        defaultPrevented: false,
        __stopped: false,
        preventDefault(this: any) {
            this.defaultPrevented = true;
        },
        stopPropagation(this: any) {
            this.__stopped = true;
        },
        stopImmediatePropagation(this: any) {
            this.__stopped = true;
        },
    };
}

export function deliver(targetHandle: number, type: string, init: any = {}): boolean {
    const event = makeEvent(type, init, targetHandle);
    let current = targetHandle;
    while (current >= 0 && !event.__stopped) {
        event.currentTarget = wrapNode(current);
        for (const listener of listenersAt(current, type)) {
            listener.call(event.currentTarget, event);
            if (event.__stopped) {
                break;
            }
        }
        if (event.bubbles === false) {
            break;
        }
        current = __uic_parent(current);
    }
    return event.defaultPrevented;
}

// ---- custom element upgrades over the retained tree ----

// The host creates the element node (with its markup attributes) before
// instantiating: attribute reads in constructors and connectedCallback see
// the real initial state.
export function mountAt(tag: string, handle: number): number {
    const cls = registry.get(tag);
    if (!cls) {
        throw new Error(`unknown custom element <${tag}>`);
    }
    const el = new cls();
    el.__node = handle;
    el.__upgradeOwnProperties();
    el.__syncFromAttributes();
    instances.set(handle, el);
    el.connectedCallback();
    return handle;
}

// A committed subtree may nest custom elements; whatever the registry
// knows upgrades once per node, and the upgrade recurses through each
// child's own commit. A parent re-commit swaps its subtree: replaced
// children upgrade fresh from their attributes (data flows down through
// attribute bindings — the serialize commit drops `.prop=`), while the
// stranded old instances stay in `instances` and render into detached
// nodes as no-ops.
export function upgradeDescendants(handle: number): void {
    for (const tag of registry.keys()) {
        for (const child of __uic_query(handle, tag)) {
            if (!instances.has(child)) {
                mountAt(tag, child);
            }
        }
    }
}

// focus(): the previous node blurs with a focusout, the next one gains a
// focusin — relatedTarget carries the counterpart, WHATWG order.
export function focusNode(handle: number): void {
    const previous = __uic_focused();
    if (previous === handle) {
        return;
    }
    __uic_set_focused(handle);
    if (previous >= 0) {
        deliver(previous, 'focusout', { relatedTarget: wrapNode(handle) });
    }
    deliver(handle, 'focusin', {
        relatedTarget: previous >= 0 ? wrapNode(previous) : null,
    });
}
