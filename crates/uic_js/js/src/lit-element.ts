// The lit-element channel: the mocked LitElement base (property accessors
// scheduling microtask updates, converter-aware attribute sync, and the
// render-to-string commit through the host natives).

import {
    attributeName,
    beginRender,
    closestFrom,
    collectProps,
    endRender,
    focusNode,
    fromAttribute,
    hasPrototypeAccessor,
    installAccessor,
    installAccessors,
    nothing,
    queryAllNodes,
    releaseListeners,
    renderToString,
    upgradeDescendants,
} from './runtime.js';

export class LitElement {
    static properties: Record<string, any> = {};

    __node = -1;
    __pending = false;
    __hasUpdated = false;
    __values = new Map<string, unknown>();
    __listeners: { type: string; listener: Function; options?: unknown }[] = [];
    __renderListeners: number[] = [];

    constructor() {
        installAccessors(this, collectProps(this.constructor));
    }

    // Class fields use define semantics, so a subclass field clobbers the
    // constructor-installed accessor with a plain data property. Lift only
    // those: reinstall the accessor, replay the value through the setter.
    // Accessor own-properties are already live and stay.
    __upgradeOwnProperties(): void {
        for (const name of Object.keys(collectProps(this.constructor))) {
            const own = Object.getOwnPropertyDescriptor(this, name);
            if (own && !own.get && !own.set) {
                delete (this as any)[name];
                if (!hasPrototypeAccessor(this, name)) {
                    installAccessor(this, name);
                }
                (this as any)[name] = own.value;
            }
        }
    }

    // Initial attribute values flow into declared properties through the
    // converters, the upgrade half of attributeChangedCallback.
    __syncFromAttributes(): void {
        const props = collectProps(this.constructor);
        for (const name of Object.keys(props)) {
            const attr = attributeName(name, props[name]);
            if (attr !== null && this.hasAttribute(attr)) {
                (this as any)[name] = fromAttribute(this.getAttribute(attr), props[name]);
            }
        }
    }

    get renderRoot(): this {
        return this;
    }

    get shadowRoot(): this {
        return this;
    }

    get updateComplete(): Promise<boolean> {
        return Promise.resolve(true);
    }

    getAttribute(name: string): string | null {
        return this.__node < 0 ? null : __uic_get_attr(this.__node, name);
    }

    setAttribute(name: string, value: unknown): void {
        if (this.__node < 0) {
            return;
        }
        __uic_set_attr(this.__node, name, String(value));
        const props = collectProps(this.constructor);
        for (const propName of Object.keys(props)) {
            if (attributeName(propName, props[propName]) === name) {
                (this as any)[propName] = fromAttribute(String(value), props[propName]);
            }
        }
    }

    hasAttribute(name: string): boolean {
        return this.__node < 0 ? false : __uic_has_attr(this.__node, name);
    }

    removeAttribute(name: string): void {
        if (this.__node >= 0) {
            __uic_remove_attr(this.__node, name);
        }
    }

    get innerText(): string {
        return this.__node < 0 ? '' : __uic_text(this.__node);
    }

    get textContent(): string {
        return this.innerText;
    }

    matches(selector: string): boolean {
        return this.__node >= 0 && __uic_matches(this.__node, selector);
    }

    closest(selector: string): unknown {
        return this.__node >= 0 ? closestFrom(this.__node, selector) : null;
    }

    contains(other: any): boolean {
        return Boolean(
            other && other.__node >= 0 && this.__node >= 0 && __uic_contains(this.__node, other.__node),
        );
    }

    querySelector(selector: string): unknown {
        return queryAllNodes(this, selector)[0] ?? null;
    }

    querySelectorAll(selector: string): unknown[] {
        return queryAllNodes(this, selector);
    }

    focus(): void {
        if (this.__node >= 0) {
            focusNode(this.__node);
        }
    }

    get tabIndex(): number {
        const value = this.getAttribute('tabindex');
        return value === null ? -1 : Number(value);
    }

    set tabIndex(value: number) {
        this.setAttribute('tabindex', String(value));
    }

    scrollIntoView(): void {}

    addEventListener(type: string, listener: Function, options?: unknown): void {
        this.__listeners.push({ type, listener, options });
    }

    removeEventListener(type: string, listener: Function): void {
        this.__listeners = this.__listeners.filter(
            (entry) => entry.type !== type || entry.listener !== listener,
        );
    }

    requestUpdate(): void {
        if (this.__pending || this.__node < 0) {
            return;
        }
        this.__pending = true;
        Promise.resolve().then(() => {
            this.__pending = false;
            this.performUpdate();
        });
    }

    performUpdate(): void {
        const previous = this.__renderListeners;
        beginRender(this);
        const result = this.render();
        const markup = renderToString(result);
        this.__renderListeners = endRender();
        __uic_commit(this.__node, markup);
        upgradeDescendants(this.__node);
        releaseListeners(previous);
        if (!this.__hasUpdated) {
            this.__hasUpdated = true;
            if (typeof (this as any).firstUpdated === 'function') {
                (this as any).firstUpdated(new Map());
            }
        }
        if (typeof (this as any).updated === 'function') {
            (this as any).updated(new Map());
        }
    }

    connectedCallback(): void {
        this.requestUpdate();
    }

    render(): unknown {
        return nothing;
    }
}
