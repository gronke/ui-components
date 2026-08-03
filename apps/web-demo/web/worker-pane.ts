// The foreign terminal pane: the worker host (crates/uic_worker) runs the
// component on the browser's own engine; this side wires its client facade
// into the shared scaffolding. Frames arrive pushed; early ones buffer
// until the terminal exists.
import { connectWorkerSession } from './client.js';
import { wireTerminalPane } from './pane-scaffold.js';
import { effectiveTheme } from './theme-mode.js';
import type { ExampleConfig } from './example-config.js';
import type { Terminal } from '@xterm/xterm';

export type WorkerPane = {
    term: Terminal;
    flush: () => void;
};

export async function mountWorkerPane(options: {
    config: ExampleConfig;
    pane: HTMLElement;
    terminal: HTMLElement;
}): Promise<WorkerPane | null> {
    const { config } = options;
    const foreign = config.foreign!;
    // Absent bundle degrades to the web pane alone.
    const boot = await fetch('./tui/web_demo_tui.js', { method: 'HEAD' }).catch(() => null);
    if (!boot || !boot.ok) {
        return null;
    }

    const pending: string[] = [];
    let write: ((data: string) => void) | null = null;
    const session = connectWorkerSession({
        workerUrl: './tui-worker.js',
        init: {
            cols: config.cols,
            rows: config.rows,
            tag: config.tag,
            attrs: config.attrs ?? {},
            props: config.props ?? {},
            entry: `./tui-worker/modules/${foreign.package}/${foreign.entry}`,
            theme: effectiveTheme(),
        },
        onAnsi: (data) => {
            if (write) {
                write(data);
            } else {
                pending.push(data);
            }
        },
        onError: (message) => console.error('[tui-worker]', message),
    });

    const { term, flush } = wireTerminalPane({
        session,
        pane: options.pane,
        terminal: options.terminal,
        cols: config.cols,
        rows: config.rows,
    });
    write = (data) => term.write(data);
    for (const data of pending.splice(0)) {
        term.write(data);
    }
    return { term, flush };
}
