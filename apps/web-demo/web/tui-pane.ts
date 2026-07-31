// The catalog terminal pane: the /tui session mounts a registered
// component, replays its seeds and notify wiring, and hands the terminal
// scaffolding to the shared pane module.
import { wireTerminalPane } from './pane-scaffold.js';
import type { ExampleConfig } from './example-config.js';
import type { Terminal } from '@xterm/xterm';

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
        glue = await import('./tui/web_demo_tui.js');
        await glue.default();
        // Bind this demo's catalog into the reusable, catalog-agnostic host.
        glue.link_catalog();
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

    const { term, flush } = wireTerminalPane({
        session,
        pane: options.pane,
        terminal: options.terminal,
        cols: config.cols,
        rows: config.rows,
    });
    return { session, term, index, flush };
}
