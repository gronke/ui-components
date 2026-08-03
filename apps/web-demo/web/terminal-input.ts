// Feeds DOM keyboard and pointer events into the session as terminal
// events, and writes the resulting ANSI back into the terminal.
import type { Terminal } from '@xterm/xterm';

export function wireTerminalInput(options: {
  term: Terminal;
  session: any;
  screen: HTMLElement;
}): void {
  const { term, session, screen } = options;

  // Paste chords stay with the browser: without preventDefault the native
  // paste lands on xterm's textarea, whose own clipboard handler routes the
  // text through term.onData below. Ctrl+V therefore means the OS clipboard
  // here, not rat's in-process one.
  const isPasteChord = (ev: KeyboardEvent) =>
    ((ev.ctrlKey || ev.metaKey) && !ev.altKey && ev.key.toLowerCase() === 'v') ||
    (ev.shiftKey && !ev.ctrlKey && !ev.altKey && ev.key === 'Insert');

  term.attachCustomKeyEventHandler((ev: KeyboardEvent) => {
    if (ev.type !== 'keydown') return false;
    // Shift+Tab walks the focus backward inside the pane (the keymap turns
    // it into BackTab); the browser keeps its function keys except the F4
    // picker.
    if (/^F\d+$/.test(ev.key) && ev.key !== 'F4') return false;
    if (isPasteChord(ev)) return false;
    term.write(session.key(ev.key, ev.ctrlKey, ev.altKey, ev.shiftKey));
    if (session.take_quit()) term.blur();
    ev.preventDefault();
    return false;
  });

  // With every key preventDefault()ed above and no terminal modes enabled,
  // onData only carries pastes (chords, context menu, middle click) and
  // ESC-prefixed query auto-replies, never typed text. xterm folds the
  // pasted line breaks to \r; the session normalizes them.
  term.onData((data: string) => {
    if (!data || data.includes('\u001b')) return;
    if (typeof session.paste === 'function') term.write(session.paste(data));
  });

  // Pointer gestures skip terminal mouse protocols: pane pixels convert to
  // cells and feed the session directly. Clicks focus and pick, drags select
  // text, the wheel browses an open list, and losing the pane blurs the
  // focused widget (commit, ring and caret gone), like the browser.
  // The live term.cols/rows keep the pixel-to-cell math true across
  // resizes; captured values would go stale with the first slider move.
  const pointer = (kind: string, ev: MouseEvent) => {
    const rect = screen.getBoundingClientRect();
    const col = Math.min(
      term.cols - 1,
      Math.max(0, Math.floor(((ev.clientX - rect.left) * term.cols) / rect.width)),
    );
    const row = Math.min(
      term.rows - 1,
      Math.max(0, Math.floor(((ev.clientY - rect.top) * term.rows) / rect.height)),
    );
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
