//! The clipboard feature end-to-end: an installed backend surfaces as the
//! mocked `navigator.clipboard`, whose readText/writeText round-trip
//! through it; without a backend the reads come back empty, the way a
//! browser without permission does.

use std::cell::RefCell;
use std::rc::Rc;

use uic_js::{ClipboardBackend, JsHost};

/// An in-memory backend standing in for the system clipboard.
#[derive(Default)]
struct FakeClipboard(RefCell<Option<String>>);

impl ClipboardBackend for FakeClipboard {
    fn read(&self) -> Option<String> {
        self.0.borrow().clone()
    }

    fn write(&self, text: &str) -> bool {
        *self.0.borrow_mut() = Some(text.to_string());
        true
    }
}

fn text_of(host: &mut JsHost, expr: &str) -> String {
    host.eval(&format!("String({expr})"))
        .unwrap()
        .as_string()
        .map(|s| s.to_std_string_escaped())
        .unwrap_or_default()
}

#[test]
fn navigator_clipboard_reads_and_writes_through_the_backend() {
    let mut host = JsHost::new().unwrap();
    let clipboard = Rc::new(FakeClipboard(RefCell::new(Some("copied text".into()))));
    host.install_clipboard(clipboard.clone());

    // readText resolves the backend's text; the host reads the same.
    host.eval("void navigator.clipboard.readText().then((t) => { globalThis.got = t; });")
        .unwrap();
    host.run_jobs().unwrap();
    assert_eq!(text_of(&mut host, "got"), "copied text");
    assert_eq!(host.clipboard_read().as_deref(), Some("copied text"));

    // writeText lands in the backend, visible to the host read.
    host.eval("void navigator.clipboard.writeText('from the page');")
        .unwrap();
    host.run_jobs().unwrap();
    assert_eq!(host.clipboard_read().as_deref(), Some("from the page"));
    assert_eq!(clipboard.read().as_deref(), Some("from the page"));
}

#[test]
fn without_a_backend_the_clipboard_reads_empty() {
    let mut host = JsHost::new().unwrap();
    // navigator.clipboard still installs (the natives exist), but no backend
    // is behind it: readText resolves empty, the host read is None.
    host.eval("void navigator.clipboard.readText().then((t) => { globalThis.got = `[${t}]`; });")
        .unwrap();
    host.run_jobs().unwrap();
    assert_eq!(text_of(&mut host, "got"), "[]");
    assert_eq!(host.clipboard_read(), None);
}

#[test]
fn a_fresh_host_forgets_the_previous_backend() {
    let first = JsHost::new().unwrap();
    first.install_clipboard(Rc::new(FakeClipboard(RefCell::new(Some("stale".into())))));
    drop(first);

    let second = JsHost::new().unwrap();
    assert_eq!(second.clipboard_read(), None);
}
