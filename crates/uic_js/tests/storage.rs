//! The storage feature through the runtime: the `localStorage` global
//! speaks Web Storage over the backend seam — string coercion, null past
//! the data, the native index guard — and a component reads it as early as
//! its constructor. Values come back as engine values and are read in
//! Rust; the backend semantics themselves are pinned by the pure-Rust
//! tests beside the trait (src/storage.rs).

use uic_js::JsHost;

/// A string-or-null expression, read off the engine value in Rust.
fn string_of(host: &mut JsHost, expr: &str) -> Option<String> {
    let value = host.eval(expr).unwrap();
    if value.is_null() {
        return None;
    }
    Some(
        value
            .as_string()
            .unwrap_or_else(|| panic!("{expr} is not a string"))
            .to_std_string_escaped(),
    )
}

fn number_of(host: &mut JsHost, expr: &str) -> f64 {
    host.eval(expr)
        .unwrap()
        .as_number()
        .unwrap_or_else(|| panic!("{expr} is not a number"))
}

#[test]
fn the_global_round_trips_and_misses_yield_null() {
    let mut host = JsHost::new().unwrap();
    host.eval("localStorage.setItem('uic-theme', 'dark')")
        .unwrap();
    assert_eq!(
        string_of(&mut host, "localStorage.getItem('uic-theme')").as_deref(),
        Some("dark")
    );
    assert_eq!(string_of(&mut host, "localStorage.getItem('absent')"), None);
}

#[test]
fn removal_and_clear_forget_through_the_natives() {
    let mut host = JsHost::new().unwrap();
    host.eval("localStorage.setItem('k', 'v')").unwrap();
    host.eval("localStorage.removeItem('k')").unwrap();
    assert_eq!(string_of(&mut host, "localStorage.getItem('k')"), None);

    host.eval("localStorage.setItem('a', '1')").unwrap();
    host.eval("localStorage.setItem('b', '2')").unwrap();
    host.eval("localStorage.clear()").unwrap();
    assert_eq!(number_of(&mut host, "localStorage.length"), 0.0);
}

// key(n) sorted and length come from the backend; the negative index is the
// native's own guard — the browser's unsigned coercion lands past the data.
#[test]
fn keys_enumerate_through_the_index_guard() {
    let mut host = JsHost::new().unwrap();
    host.eval("localStorage.setItem('banana', '2')").unwrap();
    host.eval("localStorage.setItem('apple', '1')").unwrap();

    assert_eq!(number_of(&mut host, "localStorage.length"), 2.0);
    assert_eq!(
        string_of(&mut host, "localStorage.key(0)").as_deref(),
        Some("apple")
    );
    assert_eq!(string_of(&mut host, "localStorage.key(2)"), None);
    assert_eq!(string_of(&mut host, "localStorage.key(-1)"), None);
}

#[test]
fn values_coerce_to_strings() {
    let mut host = JsHost::new().unwrap();
    host.eval("localStorage.setItem('count', 7)").unwrap();
    assert_eq!(
        string_of(&mut host, "localStorage.getItem('count')").as_deref(),
        Some("7")
    );
    host.eval("localStorage.setItem(42, 'answer')").unwrap();
    assert_eq!(
        string_of(&mut host, "localStorage.getItem('42')").as_deref(),
        Some("answer")
    );
}

#[test]
fn a_fresh_host_starts_empty() {
    let mut first = JsHost::new().unwrap();
    first.eval("localStorage.setItem('k', 'v')").unwrap();
    drop(first);

    let mut second = JsHost::new().unwrap();
    assert_eq!(number_of(&mut second, "localStorage.length"), 0.0);
    assert_eq!(string_of(&mut second, "localStorage.getItem('k')"), None);
}

const REMEMBERS: &str = r#"
import { html, LitElement } from 'lit';

class StoredGreeting extends LitElement {
    static properties = { note: {} };

    constructor() {
        super();
        this.note = localStorage.getItem('greeting') ?? 'unset';
    }

    render() {
        return html`<span>${this.note}</span>`;
    }
}

customElements.define('stored-greeting', StoredGreeting);
"#;

// The backend installs before the runtime's entry module evaluates, so a
// component can read storage as early as its constructor — the load path
// persistence rides on.
#[test]
fn a_component_reads_storage_in_its_constructor() {
    let mut host = JsHost::new().unwrap();
    host.eval("localStorage.setItem('greeting', 'hello again')")
        .unwrap();
    host.load_module("test:stored", REMEMBERS).unwrap();
    let node = host.mount("stored-greeting", &[]).unwrap();
    assert_eq!(host.prop_json(node, "note").unwrap(), "\"hello again\"");
}

// `with_storage` wires an app-selected backend into the runtime; the
// reopen semantics themselves are pinned beside the trait.
#[cfg(feature = "sqlite")]
#[test]
fn sqlite_carries_a_file_across_hosts() {
    let path = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("storage-across-hosts.db");
    let _ = std::fs::remove_file(&path);

    let backend = uic_js::SqliteBackend::open(&path).unwrap();
    let mut first = JsHost::with_storage(Box::new(backend)).unwrap();
    first
        .eval("localStorage.setItem('uic-todos', '[{\"id\":1}]')")
        .unwrap();
    drop(first);

    let backend = uic_js::SqliteBackend::open(&path).unwrap();
    let mut second = JsHost::with_storage(Box::new(backend)).unwrap();
    assert_eq!(
        string_of(&mut second, "localStorage.getItem('uic-todos')").as_deref(),
        Some(r#"[{"id":1}]"#)
    );
}
