// The focus polyfill. focus(): the previous node blurs with a focusout,
// the next one gains a focusin; relatedTarget carries the counterpart,
// WHATWG order.

import { deliver } from './events.js';
import { wrapNode } from './element.js';

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
