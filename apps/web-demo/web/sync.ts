// Per-property sync between the two panes of an example: each notify event
// carries one property, and a canonical-JSON brake per property stops the
// echo: the state bridge's dedupe trick, one string per property instead
// of one per snapshot.
import type { ExampleConfig } from './example-config.js';

export function wirePropertySync(options: {
    element: any;
    session: any;
    index: number;
    notify: ExampleConfig['notify'];
    /** Repaints the terminal after a session write. */
    flush: () => void;
    record: (entry: unknown) => void;
}): { fromSession: (event: string, json: string) => void } {
    const last = new Map<string, string>();
    const canon = (value: unknown) => JSON.stringify(value ?? null);

    for (const { event, prop } of options.notify ?? []) {
        options.element.addEventListener(event, (e: Event) => {
            const value = (e as CustomEvent).detail?.value;
            const s = canon(value);
            if (last.get(prop) === s) {
                return;
            }
            last.set(prop, s);
            options.session.set_prop_json(options.index, prop, JSON.stringify(value ?? null));
            options.flush();
            options.record({ src: 'dom', type: event, value });
        });
    }

    return {
        // Lit batches property writes into an async update, so the DOM-side
        // notify fires outside the session borrow this callback runs in.
        fromSession: (event, json) => {
            const entry = (options.notify ?? []).find((n) => n.event === event);
            if (!entry) {
                return;
            }
            const value = JSON.parse(json).value;
            const s = canon(value);
            if (last.get(entry.prop) === s) {
                return;
            }
            last.set(entry.prop, s);
            options.element[entry.prop] = value;
            options.record({ src: 'tui', type: event, value });
        },
    };
}
