// The state bridge of the demo: raw state snapshots over a transport
// (BroadcastChannel here, a WebSocket toward a native TUI next), deduped by
// canonical serialization so applied states do not echo back out.
import { BroadcastChannelTransport } from './transport.js';
import type { Transport } from './transport.js';

export type AppState = Record<string, unknown>;

// Canonical serialization: top-level keys sorted, matching the Rust side's
// ObjectMap order; the string is the dedupe identity of a state.
export function canon(state: AppState): string {
  return JSON.stringify(
    Object.fromEntries(Object.entries(state).sort(([a], [b]) => (a < b ? -1 : a > b ? 1 : 0))),
  );
}

export class StateBridge {
  private transport: Transport;
  // The last state seen anywhere (sent, received, or seeded): applying a
  // state re-fires the local state-changed, and this string stops the echo.
  private last = '';

  constructor(transport: Transport = new BroadcastChannelTransport('uic-app-state')) {
    this.transport = transport;
  }

  // Seeds the dedupe with a state applied out-of-band (the boot state).
  remember(state: AppState): void {
    this.last = canon(state);
  }

  // Posts when the state is news; false means a suppressed echo.
  send(state: AppState): boolean {
    const s = canon(state);
    if (s === this.last) return false;
    this.last = s;
    this.transport.send(state);
    return true;
  }

  // Delivers received states that are news; echoes die here.
  onState(cb: (state: AppState) => void): void {
    this.transport.onMessage((state) => {
      const s = canon(state);
      if (s === this.last) return;
      this.last = s;
      cb(state);
    });
  }
}
