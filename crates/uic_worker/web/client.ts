// The page-side client of the worker host: a session facade over
// postMessage whose surface matches the wasm sessions', so the same pane
// scaffolding drives a worker-hosted terminal. Methods return empty
// strings; frames arrive pushed through `onAnsi`.

export type WorkerInit = {
    cols: number;
    rows: number;
    tag: string;
    attrs: Record<string, string>;
    props: Record<string, unknown>;
    entry: string;
    theme: string;
};

export type WorkerSession = {
    draw(): string;
    resize(cols: number, rows: number): string;
    set_theme(theme: string): string;
    key(key: string, ctrl: boolean, alt: boolean, shift: boolean): string;
    paste(text: string): string;
    mouse(kind: string, col: number, row: number): string;
    blur(): string;
    take_quit(): boolean;
};

export function connectWorkerSession(options: {
    workerUrl: string;
    init: WorkerInit;
    onAnsi: (data: string) => void;
    onError?: (message: string) => void;
}): WorkerSession {
    const worker = new Worker(options.workerUrl, { type: 'module' });
    worker.onmessage = (event: MessageEvent) => {
        const message = event.data as { type: string; data?: string; message?: string };
        if (message.type === 'ansi' && message.data) {
            options.onAnsi(message.data);
        } else if (message.type === 'error') {
            options.onError?.(message.message ?? 'worker error');
        }
    };
    worker.postMessage({ type: 'init', ...options.init });
    return {
        draw: () => '',
        resize: (cols, rows) => {
            worker.postMessage({ type: 'resize', cols, rows });
            return '';
        },
        set_theme: (theme) => {
            worker.postMessage({ type: 'theme', theme });
            return '';
        },
        key: (key, ctrl, alt, shift) => {
            worker.postMessage({ type: 'key', key, ctrl, alt, shift });
            return '';
        },
        paste: (text) => {
            worker.postMessage({ type: 'paste', text });
            return '';
        },
        mouse: (kind, col, row) => {
            worker.postMessage({ type: 'mouse', kind, col, row });
            return '';
        },
        blur: () => '',
        take_quit: () => false,
    };
}
