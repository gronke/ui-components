// The reactive-property polyfill: merged property options along the
// prototype chain, lit's attribute converters, and the accessor
// installation the element base and the decorators share.

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
// binding; see the boa_quirks canary.
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
