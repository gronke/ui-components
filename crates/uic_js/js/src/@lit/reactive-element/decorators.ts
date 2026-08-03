// @lit/reactive-element produces the decorators upstream; `lit/decorators`
// re-exports them. The shape is dictated by the published artifacts, not by
// us: esbuild-compiled dists call __decorateClass with (prototype, name,
// descriptor) at runtime; the pinned fixture does. Dists built with the
// TC39 standard protocol get that shape when a fixture needs it.

import { accessorDescriptor, queryAllNodes } from '../../runtime.js';

function ensureDecorated(ctor: any): Record<string, any> {
    if (!Object.prototype.hasOwnProperty.call(ctor, '__uicDecorated')) {
        Object.defineProperty(ctor, '__uicDecorated', {
            value: {},
            configurable: true,
        });
    }
    return ctor.__uicDecorated;
}

export const property =
    (options: Record<string, unknown> = {}) =>
    (proto: any, name: string) => {
        ensureDecorated(proto.constructor)[name] = options;
        return accessorDescriptor(name);
    };

export const state = (options: Record<string, unknown> = {}) =>
    property({ ...options, attribute: false, state: true });

export const queryAll =
    (selector: string) =>
    (_proto: any, _name: string) => ({
        get(this: any) {
            return queryAllNodes(this, selector);
        },
        configurable: true,
    });
