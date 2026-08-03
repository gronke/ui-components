// The dedicated worker hosting a foreign lit element on the browser's own
// JS engine: the wasm DomSession carries the retained document, cascade
// and paint; the unchanged mocked-lit runtime and the component run here
// natively, resolving through the rewritten worker module tree; import
// maps do not reach workers. ANSI crosses to the page per message.

type InitMessage = {
    type: 'init';
    cols: number;
    rows: number;
    tag: string;
    attrs: Record<string, string>;
    props: Record<string, unknown>;
    entry: string;
    theme: string;
};

type InputMessage =
    | { type: 'key'; key: string; ctrl?: boolean; alt?: boolean; shift?: boolean }
    | { type: 'paste'; text: string }
    | { type: 'mouse'; kind: string; col: number; row: number }
    | { type: 'resize'; cols: number; rows: number }
    | { type: 'theme'; theme: string };

let session: any = null;
let rows = 0;
let rootHandle = -1;

const post = (ansi: string) => {
    if (ansi) {
        (self as any).postMessage({ type: 'ansi', data: ansi });
    }
};

// Lit schedules updates on the microtask queue; give them one turn before
// painting: the browser's own job draining, where Boa needed run_jobs().
const settled = () => new Promise((resolve) => setTimeout(resolve, 0));

async function init(message: InitMessage): Promise<void> {
    const glue = await import('./tui/uic_tui_web.js');
    await (glue as any).default();
    session = new (glue as any).DomSession(message.cols, message.rows);
    rows = message.rows;

    const s = session;
    const g = globalThis as any;
    g.__uic_commit = (h: number, html: string) => s.commit(h, html);
    g.__uic_get_attr = (h: number, n: string) => s.get_attr(h, n) ?? null;
    g.__uic_set_attr = (h: number, n: string, v: string) => s.set_attr(h, n, String(v));
    g.__uic_has_attr = (h: number, n: string) => s.has_attr(h, n);
    g.__uic_remove_attr = (h: number, n: string) => s.remove_attr(h, n);
    g.__uic_text = (h: number) => s.text(h);
    g.__uic_query = (h: number, sel: string) => Array.from(s.query(h, sel));
    g.__uic_matches = (h: number, sel: string) => s.matches(h, sel);
    g.__uic_contains = (o: number, i: number) => s.contains(o, i);
    g.__uic_parent = (h: number) => s.parent(h);
    g.__uic_focused = () => s.focused();
    g.__uic_set_focused = (h: number) => s.set_focused(h);
    g.__uic_adopt_styles = (t: string, css: string) => s.adopt_styles(t, css);
    // Optional-chained: a stale wasm glue without the widget surface
    // degrades to plain nodes instead of throwing.
    g.__uic_widget_value = (h: number) => s.widget_value?.(h) ?? null;
    g.__uic_set_widget_value = (h: number, t: string) => s.set_widget_value?.(h, String(t));
    g.__uic_log = (m: string) => console.log('[tui-worker]', m);

    // The runtime publishes customElements and the __uic* entry points.
    await import('./tui-worker/modules/main.js');
    // The foreign component defines its tag against the mocked lit.
    await import(message.entry);

    // One settled turn after each entry call: the drain points the Boa
    // host has with run_jobs(); the component must render before focusin
    // reaches its own handlers.
    rootHandle = session.create_root(message.tag, JSON.stringify(message.attrs ?? {}));
    (globalThis as any).__uicMount(message.tag, rootHandle);
    await settled();
    for (const [name, value] of Object.entries(message.props ?? {})) {
        (globalThis as any).__uicSetProp(rootHandle, name, value);
        await settled();
    }
    (globalThis as any).__uicFocus(rootHandle);
    await settled();
    post(session.set_theme(message.theme));
}

async function input(message: InputMessage): Promise<void> {
    if (!session) {
        return;
    }
    switch (message.type) {
        case 'key': {
            const focused = session.focused();
            if (focused >= 0) {
                // The full modifier state travels, the same keydown
                // contract the native Boa host delivers.
                const prevented = (globalThis as any).__uicDeliver(focused, 'keydown', {
                    key: message.key,
                    shiftKey: Boolean(message.shift),
                    ctrlKey: Boolean(message.ctrl),
                    altKey: Boolean(message.alt),
                });
                // An uncancelled keydown runs the focused widget as the
                // editing default action; a text change delivers `input`.
                // Capability-checked: a stale wasm glue skips the widgets.
                if (
                    !prevented &&
                    typeof session.widget_key === 'function' &&
                    session.widget_key(
                        message.key,
                        Boolean(message.shift),
                        Boolean(message.ctrl),
                        Boolean(message.alt),
                    )
                ) {
                    (globalThis as any).__uicDeliver(focused, 'input', {});
                }
                await settled();
            }
            break;
        }
        case 'paste': {
            const focused = session.focused();
            // One bulk insert through the widget's paste handling, then the
            // single `input` a browser paste fires. Capability-checked like
            // the key path: a stale wasm glue skips it.
            if (
                focused >= 0 &&
                typeof session.widget_paste === 'function' &&
                session.widget_paste(message.text)
            ) {
                (globalThis as any).__uicDeliver(focused, 'input', {});
                await settled();
            }
            break;
        }
        case 'mouse': {
            if (message.kind === 'down') {
                const target = session.hit_test(message.col, message.row);
                if (target >= 0) {
                    // A widget node takes focus and the caret lands under
                    // the pointer: the browser's click-into-an-input.
                    if (typeof session.widget_at === 'function' && session.widget_at(target)) {
                        (globalThis as any).__uicFocus(target);
                        session.place_caret(target, message.col, message.row);
                    }
                    (globalThis as any).__uicDeliver(target, 'click', {});
                    await settled();
                }
            }
            break;
        }
        case 'resize': {
            post(session.resize(message.cols, message.rows));
            rows = message.rows;
            return;
        }
        case 'theme': {
            post(session.set_theme(message.theme));
            return;
        }
    }
    post(session.draw());
}

self.onmessage = (event: MessageEvent) => {
    const message = event.data as InitMessage | InputMessage;
    const work = message.type === 'init' ? init(message) : input(message);
    work.catch((error) => {
        (self as any).postMessage({ type: 'error', message: String(error) });
    });
};
