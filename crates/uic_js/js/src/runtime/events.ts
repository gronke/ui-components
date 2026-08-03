// The event polyfill: bubbling dispatch over the retained tree, merging a
// component's addEventListener registrations with the render's template
// listener markers. The import cycle with element.js is deferred-call-only
// (ESM-legal, pinned by the boa_quirks cycle canary).

import { instances, listenerFns } from './state.js';
import { wrapNode } from './element.js';

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
        data: init.data ?? null,
        inputType: init.inputType ?? '',
        target: wrapNode(targetHandle),
        currentTarget: null,
        relatedTarget: init.relatedTarget ?? null,
        bubbles: init.bubbles !== false,
        defaultPrevented: false,
        // Two flags, the platform's distinction: stopPropagation lets the
        // remaining listeners on the SAME node run and only blocks the
        // ancestors; stopImmediatePropagation silences both.
        __stopped: false,
        __stoppedNow: false,
        preventDefault(this: any) {
            this.defaultPrevented = true;
        },
        stopPropagation(this: any) {
            this.__stopped = true;
        },
        stopImmediatePropagation(this: any) {
            this.__stopped = true;
            this.__stoppedNow = true;
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
            if (event.__stoppedNow) {
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
