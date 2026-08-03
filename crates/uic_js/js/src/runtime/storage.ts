// Web Storage over the host natives (src/storage.rs). Installed only when
// the storage feature registered them; without it the runtime is unchanged
// and `typeof localStorage` stays 'undefined', the same guard components
// already use for the browser's own storage-less modes.

export function installStorage(): void {
    if (typeof __uic_storage_get !== 'function') {
        return;
    }
    (globalThis as any).localStorage = {
        getItem: (key: unknown): string | null => __uic_storage_get(String(key)),
        setItem: (key: unknown, value: unknown): void =>
            __uic_storage_set(String(key), String(value)),
        removeItem: (key: unknown): void => __uic_storage_remove(String(key)),
        clear: (): void => __uic_storage_clear(),
        key: (index: unknown): string | null => __uic_storage_key(Number(index)),
        get length(): number {
            return __uic_storage_length();
        },
    };
}
