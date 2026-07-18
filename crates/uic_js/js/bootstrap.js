// The mocked lit runtime (exploration #65). Evaluated as a script before any
// component module loads; the `lit` module specifiers re-export from
// `globalThis.__uicLit`. Rust exposes flat natives under `__uic_*`.
'use strict';

(() => {
    const registry = new Map();
    const instances = new Map();
    // Listener table: template `@event` bindings register the function here
    // and the rendered HTML carries a `data-uic-l` marker per element.
    const listenerFns = new Map();
    let listenerId = 0;
    let renderListenerSink = null;
    let renderHost = null;

    const nothing = Object.freeze({ __litNothing: true });

    const escapeHtml = (text) =>
        String(text).replace(
            /[&<>"']/g,
            (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' })[c],
        );

    const html = (strings, ...values) => ({ __litTemplate: true, strings, values });
    const css = (strings, ...values) => ({
        __litCss: true,
        cssText: String.raw({ raw: strings }, ...values),
    });
    const svg = html;

    // ---- directives (the pure-function subset json-viewer uses) ----

    const classMap = (info) =>
        Object.keys(info)
            .filter((key) => Boolean(info[key]))
            .join(' ');

    function* map(items, f) {
        if (items === undefined || items === null) {
            return;
        }
        let index = 0;
        for (const value of items) {
            yield f(value, index);
            index += 1;
        }
    }

    const when = (condition, trueCase, falseCase) =>
        condition ? trueCase() : falseCase ? falseCase() : nothing;

    // ---- property model ----

    // Merged property options along the prototype chain: `static properties`
    // and decorator registrations both land here.
    function collectProps(ctor) {
        if (!ctor || ctor === Function.prototype) {
            return {};
        }
        const inherited = collectProps(Object.getPrototypeOf(ctor));
        const own = Object.prototype.hasOwnProperty.call(ctor, 'properties')
            ? ctor.properties
            : {};
        const decorated = Object.prototype.hasOwnProperty.call(ctor, '__uicDecorated')
            ? ctor.__uicDecorated
            : {};
        return { ...inherited, ...own, ...decorated };
    }

    function defaultFromAttribute(value, type) {
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

    function attributeName(name, options) {
        if (options && typeof options.attribute === 'string') {
            return options.attribute;
        }
        if (options && options.attribute === false) {
            return null;
        }
        return name.toLowerCase();
    }

    function fromAttribute(value, options) {
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
    // there)? Hoisted plain function — see the Boa canary in boa_quirks.rs.
    function hasPrototypeAccessor(el, name) {
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

    function accessorDescriptor(name) {
        return {
            get() {
                return this.__values.get(name);
            },
            set(value) {
                this.__values.set(name, value);
                this.requestUpdate();
            },
            configurable: true,
            enumerable: true,
        };
    }

    function installAccessor(el, name) {
        Object.defineProperty(el, name, accessorDescriptor(name));
    }

    function installAccessors(el, props) {
        for (const name of Object.keys(props)) {
            if (!hasPrototypeAccessor(el, name)) {
                installAccessor(el, name);
            }
        }
    }

    // ---- decorators (legacy convention, the esbuild __decorateClass shape) ----

    function ensureDecorated(ctor) {
        if (!Object.prototype.hasOwnProperty.call(ctor, '__uicDecorated')) {
            Object.defineProperty(ctor, '__uicDecorated', {
                value: {},
                configurable: true,
            });
        }
        return ctor.__uicDecorated;
    }

    const property = (options = {}) => (proto, name) => {
        ensureDecorated(proto.constructor)[name] = options;
        return accessorDescriptor(name);
    };

    const state = (options = {}) => property({ ...options, attribute: false, state: true });

    const queryAll = (selector) => (proto, name) => ({
        get() {
            return __uicQueryAll(this, selector);
        },
        configurable: true,
    });

    // Selector queries resolve against the committed subtree via a native.
    globalThis.__uicQueryAll = (el, selector) => {
        if (el.__node < 0) {
            return [];
        }
        const handles = __uic_query(el.__node, selector);
        return handles.map((handle) => wrapNode(handle));
    };

    // ---- node facades: plain objects delegating to natives ----

    const nodeWrappers = new Map();

    function wrapNode(handle) {
        if (instances.has(handle)) {
            return instances.get(handle);
        }
        if (nodeWrappers.has(handle)) {
            return nodeWrappers.get(handle);
        }
        const node = {
            __node: handle,
            getAttribute(name) {
                return __uic_get_attr(handle, name);
            },
            setAttribute(name, value) {
                __uic_set_attr(handle, name, String(value));
            },
            hasAttribute(name) {
                return __uic_has_attr(handle, name);
            },
            removeAttribute(name) {
                __uic_remove_attr(handle, name);
            },
            matches(selector) {
                return __uic_matches(handle, selector);
            },
            contains(other) {
                return Boolean(other && __uic_contains(handle, other.__node));
            },
            focus() {
                __uicFocus(handle);
            },
            get tabIndex() {
                const value = __uic_get_attr(handle, 'tabindex');
                return value === null ? -1 : Number(value);
            },
            set tabIndex(value) {
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

    function datasetProxy(node) {
        // json-viewer reads `dataset.path`; a read-through object suffices.
        return {
            get path() {
                return node.getAttribute('data-path');
            },
        };
    }

    // ---- the render-to-string commit with lit binding recovery ----

    // Handles the tail of the accumulated output before a hole: lit's
    // `.prop=`, `@event=` and `?attr=` prefixes are recovered from the
    // static strings exactly like the parts compiler does (ADR 0010).
    const BINDING_TAIL = /([.@?])([a-zA-Z][\w-]*)=("|')?$/;

    function renderToString(value) {
        const out = { text: '' };
        appendValue(out, value, null);
        return out.text;
    }

    function appendTemplate(out, template) {
        const { strings, values } = template;
        let swallow = null;
        for (let i = 0; i < strings.length; i += 1) {
            let part = strings[i];
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

    function commitBinding(out, binding, value) {
        const [, kind, name, quote] = binding;
        if (kind === '@') {
            if (typeof value === 'function') {
                listenerId += 1;
                // Template listeners run with `this` bound to the host
                // component, lit's EventPart contract.
                listenerFns.set(listenerId, { event: name, fn: value, host: renderHost });
                if (renderListenerSink) {
                    renderListenerSink.push(listenerId);
                }
                out.text += ` data-uic-l-${name}="${listenerId}"`;
            }
        } else if (kind === '?') {
            if (value) {
                out.text += ` ${name}=""`;
            }
        } else if (kind === '.') {
            // Property bindings do not exist in serialized HTML; `hidden`
            // maps to its attribute, everything else is dropped here (the
            // per-part commit path is the recorded follow-up).
            if (name === 'hidden' && value) {
                out.text += ' hidden=""';
            }
        }
        return quote ?? null;
    }

    function appendValue(out, value) {
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
        if (typeof value === 'object' && value.__litTemplate) {
            appendTemplate(out, value);
            return;
        }
        if (typeof value === 'object' && typeof value[Symbol.iterator] === 'function') {
            for (const item of value) {
                appendValue(out, item);
            }
            return;
        }
        if (typeof value === 'function') {
            return;
        }
        out.text += escapeHtml(String(value));
    }

    // ---- the element base ----

    class LitElement {
        static properties = {};

        constructor() {
            this.__node = -1;
            this.__pending = false;
            this.__values = new Map();
            this.__listeners = [];
            installAccessors(this, collectProps(this.constructor));
        }

        // Class fields use define semantics, so a subclass field clobbers
        // the constructor-installed accessor with a plain data property.
        // Lift only those: reinstall the accessor, replay the value through
        // the setter. Accessor own-properties are already live and stay.
        __upgradeOwnProperties() {
            for (const name of Object.keys(collectProps(this.constructor))) {
                const own = Object.getOwnPropertyDescriptor(this, name);
                if (own && !own.get && !own.set) {
                    delete this[name];
                    if (!hasPrototypeAccessor(this, name)) {
                        installAccessor(this, name);
                    }
                    this[name] = own.value;
                }
            }
        }

        // Initial attribute values flow into declared properties through the
        // converters, the upgrade half of attributeChangedCallback.
        __syncFromAttributes() {
            const props = collectProps(this.constructor);
            for (const name of Object.keys(props)) {
                const attr = attributeName(name, props[name]);
                if (attr !== null && this.hasAttribute(attr)) {
                    this[name] = fromAttribute(this.getAttribute(attr), props[name]);
                }
            }
        }

        get renderRoot() {
            return this;
        }

        get shadowRoot() {
            return this;
        }

        get updateComplete() {
            return Promise.resolve(true);
        }

        getAttribute(name) {
            return this.__node < 0 ? null : __uic_get_attr(this.__node, name);
        }

        setAttribute(name, value) {
            if (this.__node < 0) {
                return;
            }
            __uic_set_attr(this.__node, name, String(value));
            const props = collectProps(this.constructor);
            for (const propName of Object.keys(props)) {
                if (attributeName(propName, props[propName]) === name) {
                    this[propName] = fromAttribute(String(value), props[propName]);
                }
            }
        }

        hasAttribute(name) {
            return this.__node < 0 ? false : __uic_has_attr(this.__node, name);
        }

        removeAttribute(name) {
            if (this.__node >= 0) {
                __uic_remove_attr(this.__node, name);
            }
        }

        get innerText() {
            return this.__node < 0 ? '' : __uic_text(this.__node);
        }

        get textContent() {
            return this.innerText;
        }

        addEventListener(type, listener, options) {
            this.__listeners.push({ type, listener, options });
        }

        matches(selector) {
            return this.__node >= 0 && __uic_matches(this.__node, selector);
        }

        contains(other) {
            return Boolean(
                other && other.__node >= 0 && this.__node >= 0 && __uic_contains(this.__node, other.__node),
            );
        }

        focus() {
            if (this.__node >= 0) {
                __uicFocus(this.__node);
            }
        }

        get tabIndex() {
            const value = this.getAttribute('tabindex');
            return value === null ? -1 : Number(value);
        }

        set tabIndex(value) {
            this.setAttribute('tabindex', String(value));
        }

        scrollIntoView() {}

        removeEventListener(type, listener) {
            this.__listeners = this.__listeners.filter(
                (entry) => entry.type !== type || entry.listener !== listener,
            );
        }

        requestUpdate() {
            if (this.__pending || this.__node < 0) {
                return;
            }
            this.__pending = true;
            Promise.resolve().then(() => {
                this.__pending = false;
                this.performUpdate();
            });
        }

        performUpdate() {
            const previous = this.__renderListeners ?? [];
            const created = [];
            renderListenerSink = created;
            renderHost = this;
            const result = this.render();
            const markup = renderToString(result);
            renderListenerSink = null;
            renderHost = null;
            __uic_commit(this.__node, markup);
            for (const id of previous) {
                listenerFns.delete(id);
            }
            this.__renderListeners = created;
            if (typeof this.updated === 'function') {
                this.updated(new Map());
            }
        }

        connectedCallback() {
            this.requestUpdate();
        }

        render() {
            return nothing;
        }
    }

    globalThis.customElements = {
        define(tag, cls) {
            registry.set(tag, cls);
        },
        get(tag) {
            return registry.get(tag);
        },
    };

    // The host created the element node (with its markup attributes) before
    // instantiating: attribute reads in constructors and connectedCallback
    // see the real initial state.
    globalThis.__uicMount = (tag, handle) => {
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
    };

    globalThis.__uicSetProp = (handle, name, value) => {
        const el = instances.get(handle);
        if (!el) {
            throw new Error(`no instance for handle ${handle}`);
        }
        el[name] = value;
    };

    globalThis.__uicGetProp = (handle, name) => {
        const el = instances.get(handle);
        return el ? el[name] : undefined;
    };

    // ---- events: bubbling dispatch over the retained tree ----

    function listenersAt(handle, type) {
        const found = [];
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

    function makeEvent(type, init, targetHandle) {
        return {
            type,
            key: init.key,
            target: wrapNode(targetHandle),
            currentTarget: null,
            relatedTarget: init.relatedTarget ?? null,
            bubbles: init.bubbles !== false,
            defaultPrevented: false,
            __stopped: false,
            preventDefault() {
                this.defaultPrevented = true;
            },
            stopPropagation() {
                this.__stopped = true;
            },
            stopImmediatePropagation() {
                this.__stopped = true;
            },
        };
    }

    globalThis.__uicDeliver = (targetHandle, type, init = {}) => {
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
    };

    // focus(): the previous node blurs with a focusout, the next one gains a
    // focusin — relatedTarget carries the counterpart, WHATWG order.
    globalThis.__uicFocus = (handle) => {
        const previous = __uic_focused();
        if (previous === handle) {
            return;
        }
        __uic_set_focused(handle);
        if (previous >= 0) {
            __uicDeliver(previous, 'focusout', { relatedTarget: wrapNode(handle) });
        }
        __uicDeliver(handle, 'focusin', {
            relatedTarget: previous >= 0 ? wrapNode(previous) : null,
        });
    };

    globalThis.__uicLit = {
        html,
        svg,
        css,
        nothing,
        LitElement,
        property,
        state,
        queryAll,
        classMap,
        map,
        when,
    };
})();
