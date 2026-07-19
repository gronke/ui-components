// The terminal pane of an example page: one session, one xterm, both sized
// by the pane. The terminal opens lazily on the first nonzero width (a
// hidden tab pane cannot be measured), the first render calibrates the
// cell width, and every later width — slider, tab switch, window resize —
// funnels through one ResizeObserver into the column resize.
import { Terminal } from '@xterm/xterm';

import { bootstrapTheme } from './theme.js';
import { effectiveTheme, onThemeChange } from './theme-mode.js';
import { wireTerminalInput } from './terminal-input.js';
import type { ExampleConfig } from './example-config.js';

export type TuiPane = {
    session: any;
    term: Terminal;
    index: number;
    /** Repaints after session mutations (property sync). */
    flush: () => void;
};

export async function mountTuiPane(options: {
    config: ExampleConfig;
    /** The pane wrapper whose width governs the terminal columns. */
    pane: HTMLElement;
    /** The screen host, `#terminal`. */
    terminal: HTMLElement;
    /** Runs between mount and the first paint — late seeds. */
    seed?: (session: any, index: number) => void;
    /** One callback per notify event, delivered outside the session borrow. */
    onNotify?: (event: string, json: string) => void;
}): Promise<TuiPane | null> {
    let glue: any;
    try {
        glue = await import('./tui/uic_tui_web.js');
        await glue.default();
    } catch {
        return null;
    }
    const { config } = options;
    const session = new glue.TuiSession(config.cols, config.rows);
    const index = session.mount(config.tag);
    for (const [name, value] of Object.entries(config.attrs ?? {})) {
        session.set_attr(index, name, value);
    }
    for (const [name, value] of Object.entries(config.props ?? {})) {
        session.set_prop_json(index, name, JSON.stringify(value ?? null));
    }
    for (const [name, rows] of Object.entries(config.optionProps ?? {})) {
        session.set_option_rows_json(index, name, JSON.stringify(rows));
    }
    options.seed?.(session, index);
    for (const { event } of config.notify ?? []) {
        session.on_notify(index, event, (json: string) => options.onNotify?.(event, json));
    }

    const term = new Terminal({
        cols: config.cols,
        rows: config.rows,
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
        const cols = Math.max(40, Math.min(160, Math.floor((px - overhead) / cellWidth)));
        if (cols === term.cols) {
            return;
        }
        // The session first: its full-repaint ANSI targets the new size, so
        // the terminal must match before the write — and a session error
        // leaves the terminal untouched.
        const ansi = session.resize(cols, config.rows);
        term.resize(cols, config.rows);
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
        session,
        term,
        index,
        flush: () => {
            if (opened) {
                term.write(session.draw());
            }
        },
    };
}
