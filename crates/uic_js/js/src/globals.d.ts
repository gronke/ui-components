// The host natives the runtime modules call — implemented in Rust
// (src/natives.rs) and registered on the Boa context before any module
// evaluates. Ambient-only: the build strips types, editors get the shapes.
declare function __uic_commit(handle: number, html: string): void;
declare function __uic_get_attr(handle: number, name: string): string | null;
declare function __uic_set_attr(handle: number, name: string, value: string): void;
declare function __uic_has_attr(handle: number, name: string): boolean;
declare function __uic_remove_attr(handle: number, name: string): void;
declare function __uic_text(handle: number): string;
declare function __uic_query(handle: number, selector: string): number[];
declare function __uic_matches(handle: number, selector: string): boolean;
declare function __uic_contains(outer: number, inner: number): boolean;
declare function __uic_parent(handle: number): number;
declare function __uic_focused(): number;
declare function __uic_set_focused(handle: number): void;
declare function __uic_widget_value(handle: number): string | null;
declare function __uic_set_widget_value(handle: number, text: string): void;
declare function __uic_adopt_styles(tag: string, cssText: string): number;
declare function __uic_log(message: string): void;
// The storage feature's natives — absent without it; the runtime probes
// with typeof before installing localStorage.
declare function __uic_storage_get(key: string): string | null;
declare function __uic_storage_set(key: string, value: string): void;
declare function __uic_storage_remove(key: string): void;
declare function __uic_storage_clear(): void;
declare function __uic_storage_key(index: number): string | null;
declare function __uic_storage_length(): number;
// The dialogs feature's native — absent without it; the runtime probes
// with typeof before installing alert/confirm/prompt.
declare function __uic_dialog_request(
    id: number,
    kind: string,
    message: string,
    defaultValue: string | null,
): void;
