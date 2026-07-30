// navigator.clipboard over the host natives (src/clipboard.rs). The backend
// is synchronous; these wrap it in resolved promises so the same component
// code — `await navigator.clipboard.readText()` — runs in a real browser
// and here alike. Installed only when the clipboard feature registered the
// natives; without it navigator.clipboard stays absent.

export function installClipboard(): void {
    if (typeof __uic_clipboard_read !== 'function') {
        return;
    }
    const nav = ((globalThis as any).navigator ??= {});
    nav.clipboard = {
        readText: (): Promise<string> => Promise.resolve(__uic_clipboard_read() ?? ''),
        writeText: (text: unknown): Promise<void> => {
            __uic_clipboard_write(String(text));
            return Promise.resolve();
        },
    };
}
