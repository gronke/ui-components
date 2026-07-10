// The state transport of the demo: a BroadcastChannel carrying raw state
// snapshots between the DOM <app-root> and the TUI pane on this page (and,
// on other tabs, their panes too). One JSON object per message, no
// envelope — a WebSocket variant can speak the identical protocol.
export type AppState = Record<string, unknown>;

// Canonical serialization: top-level keys sorted, matching the Rust side's
// ObjectMap order — the string is the dedupe identity of a state.
export function canon(state: AppState): string {
  return JSON.stringify(
    Object.fromEntries(Object.entries(state).sort(([a], [b]) => (a < b ? -1 : a > b ? 1 : 0))),
  );
}

export class StateBridge {
  private channel: BroadcastChannel;
  // The last state seen anywhere (sent, received, or seeded): applying a
  // state re-fires the local state-changed, and this string stops the echo.
  private last = '';

  constructor(name = 'uic-app-state') {
    this.channel = new BroadcastChannel(name);
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
    this.channel.postMessage(state);
    return true;
  }

  // Delivers received states that are news; echoes die here.
  onState(cb: (state: AppState) => void): void {
    this.channel.onmessage = (ev: MessageEvent) => {
      const state = ev.data as AppState;
      const s = canon(state);
      if (s === this.last) return;
      this.last = s;
      cb(state);
    };
  }
}
