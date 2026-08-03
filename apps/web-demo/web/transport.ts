// The message transport under the state bridge: posts raw state snapshots
// and delivers the peers'. One JSON object per message, no envelope.
import type { AppState } from './bridge.js';

export interface Transport {
  send(state: AppState): void;
  onMessage(cb: (state: AppState) => void): void;
}

// The demo's transport: a BroadcastChannel between the page and its
// terminal pane (and, on other tabs, their panes too).
export class BroadcastChannelTransport implements Transport {
  private channel: BroadcastChannel;

  constructor(name: string) {
    this.channel = new BroadcastChannel(name);
  }

  send(state: AppState): void {
    this.channel.postMessage(state);
  }

  onMessage(cb: (state: AppState) => void): void {
    this.channel.onmessage = (ev: MessageEvent) => cb(ev.data as AppState);
  }
}

// The WebSocket variant slots in here: send as JSON.stringify(state),
// deliver JSON.parse(ev.data), the identical snapshots toward a native
// TUI behind an axum server. The bridge and its dedupe stay unchanged.
