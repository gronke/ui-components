// The terminal pane: the same page, run by the TUI runtime compiled to
// wasm. One <app-root> mounts into the session, seeded with the DOM
// element's state; the bridge keeps both sides on the same snapshot.
// Without the wasm assets (scripts/build-wasm.sh) the pane stays hidden.
import { Terminal } from '@xterm/xterm';

import { StateBridge } from './bridge.js';
import type { AppState } from './bridge.js';

const COLS = 72;
// The form ends around row 36 with a one-line state; the slack covers the
// textarea growing to its max-lines and the wrapping state line.
const ROWS = 42;

// The terminal palette from Bootstrap's own variables: the runtime speaks
// plain ANSI colors (a real terminal keeps the user's scheme), and this pane
// maps those slots to the stylesheet's custom properties AS RESOLVED ON THE
// SCREEN ELEMENT — both theme variants are always on offer, and the screen
// picks one by wearing data-bs-theme (dark here, while the site wears
// light; flip either and its colors follow).
function bootstrapTheme(screen: HTMLElement): Record<string, string> {
  const bs = (name: string) => getComputedStyle(screen).getPropertyValue(name).trim();
  return {
    background: bs('--bs-body-bg'),
    foreground: bs('--bs-body-color'),
    cursor: bs('--bs-body-color'),
    red: bs('--bs-danger'),
    green: bs('--bs-success'),
    yellow: bs('--bs-warning'),
    blue: bs('--bs-primary'),
    cyan: bs('--bs-info'),
    brightBlack: bs('--bs-secondary'),
    brightRed: bs('--bs-danger-text-emphasis'),
    brightGreen: bs('--bs-success-text-emphasis'),
    brightYellow: bs('--bs-warning-text-emphasis'),
    brightBlue: bs('--bs-primary-text-emphasis'),
    brightCyan: bs('--bs-info-text-emphasis'),
  };
}

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
  const bridge = new StateBridge();
  bridge.remember(root.state ?? {});

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

  // Forward-only: the callback runs inside a session borrow, so it must not
  // call back into the session — it hands the snapshot to the channel.
  session.on_notify(index, 'state-changed', (json: string) => {
    const state = JSON.parse(json).value as AppState;
    if (bridge.send(state)) {
      const entry = { src: 'tui', type: 'state-changed', state };
      events.push(entry);
      log.textContent += '[tui] ' + JSON.stringify(entry) + '\n';
    }
  });
  // BroadcastChannel delivery is asynchronous (its own task), so applying
  // the state re-enters the session safely.
  bridge.onState((state) => {
    session.set_prop_json(index, 'state', JSON.stringify(state));
    term.write(session.draw());
    const entry = { src: 'tui', type: 'state-applied', state };
    events.push(entry);
    log.textContent += '[tui] ' + JSON.stringify(entry) + '\n';
  });

  term.attachCustomKeyEventHandler((ev: KeyboardEvent) => {
    if (ev.type !== 'keydown') return false;
    // Shift+Tab walks the focus backward inside the pane (the keymap turns
    // it into BackTab); the browser keeps its function keys except the F4
    // picker.
    if (/^F\d+$/.test(ev.key) && ev.key !== 'F4') return false;
    term.write(session.key(ev.key, ev.ctrlKey, ev.altKey, ev.shiftKey));
    if (session.take_quit()) term.blur();
    ev.preventDefault();
    return false;
  });
  // Pointer gestures skip terminal mouse protocols: pane pixels convert to
  // cells and feed the session directly. Clicks focus and pick, drags select
  // text, the wheel browses an open list, and losing the pane blurs the
  // focused widget (commit, ring and caret gone), like the browser.
  const screen = document.querySelector('#terminal .xterm-screen') as HTMLElement;
  const pointer = (kind: string, ev: MouseEvent) => {
    const rect = screen.getBoundingClientRect();
    const col = Math.min(COLS - 1, Math.max(0, Math.floor(((ev.clientX - rect.left) * COLS) / rect.width)));
    const row = Math.min(ROWS - 1, Math.max(0, Math.floor(((ev.clientY - rect.top) * ROWS) / rect.height)));
    term.write(session.mouse(kind, col, row));
  };
  screen.addEventListener('mousedown', (ev) => pointer('down', ev));
  screen.addEventListener('mouseup', (ev) => pointer('up', ev));
  screen.addEventListener('mousemove', (ev) => {
    if (ev.buttons & 1) pointer('drag', ev);
  });
  screen.addEventListener('wheel', (ev) => {
    pointer(ev.deltaY < 0 ? 'wheel-up' : 'wheel-down', ev);
    ev.preventDefault();
  }, { passive: false });
  term.textarea?.addEventListener('blur', () => term.write(session.blur()));
  document.getElementById('tui-pane')!.classList.remove('d-none');
  (window as any).__tui = session;
}

boot();
