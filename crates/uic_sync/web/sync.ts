// Root-component attachment: the listed reactive properties travel as one
// canonical snapshot (ADR 0013's envelope-less protocol) whenever the root
// announces a change, and inbound snapshots assign straight back onto it.
//
// One shared `last` slot dedupes both directions; the ready/applying flags
// keep boot and echo quiet. Exactly one side greets: the party holding the
// canonical state announces it on open (a server, a pairing host) and the
// other waits — two greeters would swap states and settle crossed.

import { decode, encode } from './codec.js';
import type { Wire } from './wire.js';

export interface AttachOptions {
    /** The reactive property names the snapshot mirrors. */
    fields: string[];
    wire: Wire;
    /** Announce our snapshot on open (default: wait for the far side's). */
    greet?: boolean;
    /** The root's announcement event (default 'state-changed'). */
    event?: string;
}

export interface Attachment {
    detach(): void;
}

export function attach(root: any, options: AttachOptions): Attachment {
    const { fields, wire } = options;
    const eventName = options.event ?? 'state-changed';
    let last = '';
    let ready = false;
    let applying = false;
    let detached = false;

    const snapshot = (): string => {
        const state: Record<string, unknown> = {};
        for (const field of fields) {
            state[field] = root[field];
        }
        return encode(state);
    };

    wire.onMessage(async (text) => {
        if (detached || text === last) {
            return;
        }
        last = text;
        applying = true;
        Object.assign(root, decode(text) as Record<string, unknown>);
        await root.updateComplete;
        applying = false;
        ready = true;
    });

    if (options.greet) {
        wire.onOpen(() => {
            if (detached) {
                return;
            }
            last = snapshot();
            wire.send(last);
            ready = true;
        });
    }

    const announce = (): void => {
        if (detached || !ready || applying) {
            return;
        }
        const state = snapshot();
        if (state === last) {
            return;
        }
        last = state;
        wire.send(state);
    };
    root.addEventListener(eventName, announce);

    return {
        detach(): void {
            detached = true;
            root.removeEventListener(eventName, announce);
        },
    };
}
