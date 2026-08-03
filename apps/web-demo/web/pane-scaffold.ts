// The terminal-pane scaffolding shared by the catalog session (/tui) and
// the worker session: lazy open on the first nonzero width, cell
// calibration once the canvas measures plausibly, chrome overhead
// subtracted from the width→columns math, one debounced observer for
// slider, tab and window widths, and theme following.
import { Terminal } from '@xterm/xterm';

import { bootstrapTheme } from './theme.js';
import { effectiveTheme, onThemeChange } from './theme-mode.js';
import { wireTerminalInput } from './terminal-input.js';

/** The session surface both wasm hosts expose. */
export type PaneSession = {
    draw(): string;
    resize(cols: number, rows: number): string;
    set_theme(theme: string): string;
    key(key: string, ctrl: boolean, alt: boolean, shift: boolean): string;
    paste(text: string): string;
    mouse(kind: string, col: number, row: number): string;
    blur(): string;
    take_quit(): boolean;
};

export function wireTerminalPane(options: {
    session: PaneSession;
    /** The pane wrapper whose width governs the terminal columns. */
    pane: HTMLElement;
    /** The screen host, `#terminal`. */
    terminal: HTMLElement;
    cols: number;
    rows: number;
}): { term: Terminal; flush: () => void } {
    const { session, cols, rows } = options;
    const term = new Terminal({
        cols,
        rows,
        fontSize: 13,
        scrollback: 0,
        cursorBlink: true,
        theme: bootstrapTheme(options.terminal),
    });

    let cellWidth = 0;
    // The pane chrome around the canvas (the screen padding): without
    // subtracting it, pane width feeds back into columns and the terminal
    // grows a few columns per observation until the clamp.
    let overhead = 0;
    let opened = false;
    const open = () => {
        if (opened) {
            return;
        }
        opened = true;
        term.open(options.terminal);
        term.write(session.draw());
        term.write(session.set_theme(effectiveTheme()));
        const screen = options.terminal.querySelector('.xterm-screen') as HTMLElement;
        wireTerminalInput({ term, session, screen });
        // The renderer sizes its canvas a frame later; calibrate once the
        // measurement is plausible (at least four pixels per cell) and let
        // the observer's next width flow through the then-valid resize.
        const calibrate = () => {
            const width = screen.getBoundingClientRect().width;
            if (width >= term.cols * 4) {
                cellWidth = width / term.cols;
                overhead = Math.max(0, options.pane.getBoundingClientRect().width - width);
                return;
            }
            requestAnimationFrame(calibrate);
        };
        calibrate();
    };

    const resizeToWidth = (px: number) => {
        if (!opened || cellWidth <= 0 || px <= 0) {
            return;
        }
        const next = Math.max(40, Math.min(160, Math.floor((px - overhead) / cellWidth)));
        if (next === term.cols) {
            return;
        }
        // The session first: its full-repaint ANSI targets the new size, so
        // the terminal must match before the write, and a session error
        // leaves the terminal untouched.
        const ansi = session.resize(next, rows);
        term.resize(next, rows);
        term.write(ansi);
    };

    // The observer sees the pane, not the screen: xterm sizes its own
    // canvas, so observing the screen would chase the terminal's tail.
    let timer: number | undefined;
    const observer = new ResizeObserver((entries) => {
        const width = entries[entries.length - 1]?.contentRect.width ?? 0;
        if (width <= 0) {
            return;
        }
        if (!opened) {
            open();
            resizeToWidth(width);
            return;
        }
        clearTimeout(timer);
        timer = setTimeout(() => resizeToWidth(width), 100) as unknown as number;
    });
    observer.observe(options.pane);

    onThemeChange((theme) => {
        if (!opened) {
            return;
        }
        term.options.theme = bootstrapTheme(options.terminal);
        term.write(session.set_theme(theme));
    });

    return {
        term,
        flush: () => {
            if (opened) {
                term.write(session.draw());
            }
        },
    };
}
