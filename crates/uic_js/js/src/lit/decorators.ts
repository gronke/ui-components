// The runtime decorators in the legacy convention — the shape esbuild's
// __decorateClass helper calls with (prototype, name, descriptor).

import { accessorDescriptor, queryAllNodes } from '../runtime.js';

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
