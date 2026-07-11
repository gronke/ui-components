// Feeds DOM keyboard and pointer events into the session as terminal
// events, and writes the resulting ANSI back into the terminal.
import type { Terminal } from '@xterm/xterm';

export function wireTerminalInput(options: {
  term: Terminal;
  session: any;
  screen: HTMLElement;
  cols: number;
  rows: number;
}): void {
  const { term, session, screen, cols, rows } = options;

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
  const pointer = (kind: string, ev: MouseEvent) => {
    const rect = screen.getBoundingClientRect();
    const col = Math.min(cols - 1, Math.max(0, Math.floor(((ev.clientX - rect.left) * cols) / rect.width)));
    const row = Math.min(rows - 1, Math.max(0, Math.floor(((ev.clientY - rect.top) * rows) / rect.height)));
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
}
