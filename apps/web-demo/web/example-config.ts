// The example page's contract: one JSON script tag carries everything the
// shared boot needs — the component, its seeds, the terminal geometry and
// the notify wiring. build.rs writes it per page from the manifest.

export type OptionRow = { value: string; short?: string; label?: string };

export type ExampleConfig = {
    /** The custom element tag, e.g. `input-date`. */
    tag: string;
    /** Markup attributes, replayed on both panes. */
    attrs: Record<string, string>;
    /** Plain-valued property seeds (JSON-expressible). */
    props: Record<string, unknown>;
    /** Option-rows properties — their own data type, not plain arrays (ADR 0005). */
    optionProps: Record<string, OptionRow[]>;
    /** The notify events that sync the panes, each carrying one property. */
    notify: { event: string; prop: string }[];
    /** Terminal geometry; rows stay fixed, columns follow the pane width. */
    cols: number;
    rows: number;
    /** Set on the form example only: the cross-tab state channel. */
    channel?: string;
    /** Words answering `query-changed` through the page's pool textarea. */
    pool?: string[] | null;
    /** A foreign npm element for the Boa pane: its vendored package, ESM
     *  entry and module list (the page fetches and registers them). */
    foreign?: { package: string; entry: string; modules: string[] } | null;
};

export function exampleConfig(): ExampleConfig {
    const tag = document.getElementById('example-config');
    if (!tag) {
        throw new Error('example-config script tag missing');
    }
    return JSON.parse(tag.textContent ?? '{}') as ExampleConfig;
}
