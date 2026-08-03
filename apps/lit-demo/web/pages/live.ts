// The live bridge: one shared state between this page, other tabs and the
// terminal running the server. The probe keeps plain `serve` mode silent —
// no `/live` route, no connection attempt. The server greets with the
// canonical state, so this side waits (greet stays false).

import { STATE_FIELDS } from './@gronke/lit-todo/todo-app.js';
import { attach } from './@gronke/uic-sync/sync.js';
import { WebSocketWire } from './@gronke/uic-sync/wire.js';

async function connect(): Promise<void> {
    const probe = await fetch('live').catch(() => null);
    if (!probe || !probe.ok) {
        return;
    }
    await customElements.whenDefined('todo-app');
    const el = document.querySelector('todo-app');
    if (!el) {
        return;
    }
    const url = new URL('ws', location.href);
    url.protocol = url.protocol === 'https:' ? 'wss:' : 'ws:';
    attach(el, { fields: STATE_FIELDS, wire: new WebSocketWire(url) });
}

connect();
