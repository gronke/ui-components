// The example page entry: one component in the web pane, the same one in
// the terminal pane, synced per property (or over the state channel where
// the config says so). The width slider drives both panes through one CSS
// variable; the terminal follows via its pane observer. Below the md
// breakpoint the panes become tabs.
import { InMemorySource } from './components/uic-connectors.js';

import { exampleConfig } from './example-config.js';
import { mountTuiPane } from './tui-pane.js';
import { mountWorkerPane } from './worker-pane.js';
import { wirePropertySync } from './sync.js';
import { wireThemeToggle } from './theme-mode.js';
import { wireStatePane } from './wiring.js';
import type { AppState } from './bridge.js';

function wireDebugBar(): (entry: unknown) => void {
    const log = document.getElementById('events')!;
    (window as any).__events = [];
    const record = (entry: unknown) => {
        (window as any).__events.push(entry);
        log.textContent += JSON.stringify(entry) + '\n';
    };
    const bar = document.getElementById('debug-bar')!;
    const toggle = document.getElementById('debug-toggle')!;
    toggle.addEventListener('click', () => {
        const collapsed = bar.classList.toggle('collapsed');
        toggle.textContent = collapsed ? '▴' : '▾';
        toggle.setAttribute('aria-expanded', String(!collapsed));
    });
    return record;
}

/** Below md the panes are tabs; above they sit side by side. */
function wireTabs(): void {
    const links = Array.from(document.querySelectorAll<HTMLElement>('[data-pane-tab]'));
    for (const link of links) {
        link.addEventListener('click', (e) => {
            e.preventDefault();
            for (const other of links) {
                other.classList.toggle('active', other === link);
            }
            for (const pane of document.querySelectorAll<HTMLElement>('.example-pane')) {
                pane.classList.toggle('pane-active', pane.id === link.dataset.paneTab);
            }
        });
    }
}

function wireSlider(): void {
    const slider = document.getElementById('pane-width') as HTMLInputElement | null;
    const panes = document.getElementById('panes');
    if (!slider || !panes) {
        return;
    }
    const bounds = () => {
        // Side-by-side halves minus the gap; the pane may not outgrow it.
        const max = Math.max(360, Math.floor((panes.clientWidth - 48) / 2));
        slider.max = String(max);
        if (!slider.value || Number(slider.value) > max) {
            slider.value = String(max);
        }
    };
    bounds();
    window.addEventListener('resize', bounds);
    slider.addEventListener('input', () => {
        panes.classList.add('resizing');
        panes.style.setProperty('--pane-width', `${slider.value}px`);
    });
}

/**
 * The editable word pool: the page plays the host answering the panes'
 * `query-changed` from the textarea's current words, the same
 * InMemorySource semantics the form's live pool uses (ADR 0014).
 */
function wirePool(pool: string[], element: any): {
    answerTui: (session: any, index: number, flush: () => void, json: string) => void;
} | null {
    const textarea = document.getElementById('word-pool') as HTMLTextAreaElement | null;
    if (!textarea) {
        return null;
    }
    textarea.value = pool.join('\n');
    const source = () =>
        InMemorySource.fromWords(
            textarea.value
                .split(/\s+/)
                .map((word) => word.trim())
                .filter((word) => word.length > 0),
        );
    element.addEventListener('query-changed', async (e: Event) => {
        element.suggestions = await source().query((e as CustomEvent).detail.value ?? '');
    });
    return {
        answerTui: (session, index, flush, json) => {
            const query = JSON.parse(json).value ?? '';
            // A macrotask hop: the notify callback runs inside the session
            // borrow, so the answer must not re-enter synchronously.
            setTimeout(async () => {
                const rows = await source().query(query);
                session.set_option_rows_json(index, 'suggestions', JSON.stringify(rows));
                flush();
            }, 0);
        },
    };
}

async function boot(): Promise<void> {
    const config = exampleConfig();
    const record = wireDebugBar();
    wireThemeToggle(document.getElementById('theme-toggle')!);
    wireTabs();
    wireSlider();

    const el = document.createElement(config.tag) as any;
    el.classList.add('d-block');
    if (config.foreign) {
        // A foreign element's own theme variables follow the page.
        el.style.setProperty('--background-color', 'var(--bs-body-bg)');
        el.style.setProperty('--color', 'var(--bs-body-color)');
    }
    for (const [name, value] of Object.entries(config.attrs ?? {})) {
        el.setAttribute(name, value);
    }
    for (const [name, value] of Object.entries(config.props ?? {})) {
        el[name] = value;
    }
    for (const [name, rows] of Object.entries(config.optionProps ?? {})) {
        el[name] = rows;
    }
    document.getElementById('web-pane')!.appendChild(el);
    await el.updateComplete;

    // The pool consumes query-changed; every other notify syncs the panes.
    const pool = config.pool?.length ? wirePool(config.pool, el) : null;
    const syncedNotify = pool
        ? (config.notify ?? []).filter((entry) => entry.event !== 'query-changed')
        : config.notify;

    // The sync target the notify callback reaches; assigned below once the
    // wiring exists (events only fire on interaction, long after boot).
    let sync: { fromSession: (event: string, json: string) => void } | null = null;
    let stateOut: { changed: (state: AppState) => void } | null = null;

    const tui = config.foreign
        ? await mountWorkerPane({
              config,
              pane: document.getElementById('tui-pane')!,
              terminal: document.getElementById('terminal')!,
          })
        : await mountTuiPane({
              config,
              pane: document.getElementById('tui-pane')!,
              terminal: document.getElementById('terminal')!,
              seed: (session, index) => {
                  if (config.channel) {
                      // The form's state property seeds from the settled DOM side.
                      session.set_prop_json(index, 'state', JSON.stringify(el.state ?? {}));
                  }
              },
              onNotify: (event, json) => {
                  if (pool && event === 'query-changed') {
                      const pane = tui as any;
                      pool.answerTui(pane.session, pane.index, pane.flush, json);
                  } else if (config.channel) {
                      stateOut?.changed(JSON.parse(json).value as AppState);
                  } else {
                      sync?.fromSession(event, json);
                  }
              },
          });
    if (!tui) {
        // No wasm bundle: the page degrades to the web pane alone.
        document.getElementById('tui-tab-item')?.classList.add('d-none');
        return;
    }
    document.getElementById('tui-pane')!.classList.remove('d-none');

    if (config.foreign) {
        // Foreign elements carry no notify contract; the panes render the
        // same seeds independently.
    } else if (config.channel) {
        // Whole-state snapshots over the broadcast channel: the form's
        // cross-tab story, unchanged from the original demo.
        stateOut = wireStatePane({
            boot: (el.state ?? {}) as AppState,
            apply: (state) => {
                tui.session.set_prop_json(tui.index, 'state', JSON.stringify(state));
                tui.flush();
            },
            src: 'tui',
            record,
        });
        const domPane = wireStatePane({
            boot: (el.state ?? {}) as AppState,
            apply: (state) => {
                el.state = state;
            },
            src: 'dom',
            record,
        });
        el.addEventListener('state-changed', (e: Event) => {
            domPane.changed((e as CustomEvent).detail.value as AppState);
        });
    } else {
        sync = wirePropertySync({
            element: el,
            session: tui.session,
            index: (tui as any).index ?? 0,
            notify: syncedNotify,
            flush: tui.flush,
            record,
        });
    }
    (window as any).__tui = tui.session;
}

boot();
