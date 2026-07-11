// The terminal pane: the same page, run by the TUI runtime compiled to
// wasm. One <app-root> mounts into the session, seeded with the DOM
// element's state; the shared wiring keeps both sides on the same snapshot.
// Without the wasm assets (scripts/build-wasm.sh) the pane stays hidden.
import { Terminal } from '@xterm/xterm';

import { bootstrapTheme } from './theme.js';
import { wireTerminalInput } from './terminal-input.js';
import { wireStatePane } from './wiring.js';
import type { AppState } from './bridge.js';

const COLS = 72;
// The form ends around row 43 with a one-line state (the controls carry
// their mb-4 margins); the slack covers the textarea growing to its
// max-lines and the wrapping state line.
const ROWS = 50;

async function boot(): Promise<void> {
  let glue: any;
  try {
    glue = await import('/tui/uic_tui_web.js');
    await glue.default();
  } catch {
    return;
  }
  const log = document.getElementById('events')!;
  const events = (window as any).__events as unknown[];
  const session = new glue.TuiSession(COLS, ROWS);

  // One root: the same <app-root> the page hosts, seeded with the DOM
  // element's SETTLED state (children normalize members during the first
  // update) BEFORE any listener attaches — booting is not news.
  const root = document.querySelector('app-root') as any;
  await root.updateComplete;
  const index = session.mount('app-root');
  session.set_prop_json(index, 'state', JSON.stringify(root.state ?? {}));

  const term = new Terminal({
    cols: COLS,
    rows: ROWS,
    fontSize: 13,
    scrollback: 0,
    cursorBlink: true,
    theme: bootstrapTheme(document.getElementById('terminal')!),
  });
  term.open(document.getElementById('terminal')!);
  term.write(session.draw());

  const record = (entry: unknown) => {
    events.push(entry);
    log.textContent += '[tui] ' + JSON.stringify(entry) + '\n';
  };
  // BroadcastChannel delivery is asynchronous (its own task), so applying
  // the state re-enters the session safely.
  const pane = wireStatePane({
    boot: (root.state ?? {}) as AppState,
    apply: (state) => {
      session.set_prop_json(index, 'state', JSON.stringify(state));
      term.write(session.draw());
    },
    src: 'tui',
    record,
  });
  // Forward-only: the callback runs inside a session borrow, so it must not
  // call back into the session — it hands the snapshot to the wiring.
  session.on_notify(index, 'state-changed', (json: string) => {
    pane.changed(JSON.parse(json).value as AppState);
  });

  wireTerminalInput({
    term,
    session,
    screen: document.querySelector('#terminal .xterm-screen') as HTMLElement,
    cols: COLS,
    rows: ROWS,
  });
  document.getElementById('tui-pane')!.classList.remove('d-none');
  (window as any).__tui = session;
}

boot();
