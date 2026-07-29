// Browser dialogs over the host native (src/dialogs.rs). The promises and
// their resolvers live here; the native only queues the question and the
// host answers through __uicDialogAnswer. Installed only when the dialogs
// feature registered the native; without it the globals stay undefined.
// In a real browser these are native and synchronous, so `await confirm(…)`
// is the one spelling with identical semantics in both hosts.

export function installDialogs(): void {
    if (typeof __uic_dialog_request !== 'function') {
        return;
    }
    let nextId = 1;
    const pending = new Map<number, (value: unknown) => void>();
    (globalThis as any).__uicDialogAnswer = (id: number, value: unknown): void => {
        const resolve = pending.get(id);
        pending.delete(id);
        resolve?.(value);
    };
    const request = (kind: string, message: string, fallback: string | null): Promise<unknown> => {
        const id = nextId++;
        const answer = new Promise((resolve) => pending.set(id, resolve));
        __uic_dialog_request(id, kind, message, fallback);
        return answer;
    };
    (globalThis as any).alert = (message?: unknown): Promise<void> =>
        request('alert', String(message ?? ''), null) as Promise<void>;
    (globalThis as any).confirm = (message?: unknown): Promise<boolean> =>
        request('confirm', String(message ?? ''), null) as Promise<boolean>;
    (globalThis as any).prompt = (message?: unknown, fallback?: unknown): Promise<string | null> =>
        request(
            'prompt',
            String(message ?? ''),
            fallback == null ? '' : String(fallback),
        ) as Promise<string | null>;
}
