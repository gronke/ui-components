//! The native-engine host boundary, driven natively: the same calls the
//! worker's shimmed `__uic_*` globals make.

use uic_tui_web::DomSession;

fn session() -> (DomSession, u32) {
    let mut session = DomSession::new(40, 8).unwrap();
    let root = session
        .create_root("x-demo", r#"{"data-kind": "demo"}"#)
        .unwrap();
    (session, root)
}

#[test]
fn a_commit_paints_and_attributes_read_back() {
    let (mut session, root) = session();
    assert_eq!(session.get_attr(root, "data-kind").as_deref(), Some("demo"));
    session.commit(root, "<span class='fw-bold'>hello host</span>");
    assert!(session.dirty());
    let _ = session.draw().unwrap();
    assert!(
        session.screen_text().contains("hello host"),
        "the committed subtree paints:\n{}",
        session.screen_text()
    );
    assert!(!session.dirty(), "the draw clears the dirty flag");
}

#[test]
fn queries_return_handles_and_matches_sees_focus() {
    let (mut session, root) = session();
    session.commit(
        root,
        "<ul><li data-path='a'>one</li><li data-path='b'>two</li></ul>",
    );
    let rows = session.query(root, "[data-path]").unwrap();
    assert_eq!(rows.len(), 2, "both rows match the attribute selector");
    let first = rows[0];
    assert_eq!(session.text(first), "one");
    assert!(session.matches(first, "[data-path=\"a\"]").unwrap());
    assert!(!session.matches(first, ":focus").unwrap());
    session.set_focused(first as i32);
    assert!(session.matches(first, ":focus").unwrap());
    assert_eq!(session.focused(), first as i32);
    assert!(session.contains(root, first));
    let parent = session.parent(first);
    assert!(parent >= 0, "the li has an element parent (the ul)");
    assert_ne!(parent, root as i32, "the ul sits between li and root");
}

#[test]
fn a_paste_bulk_inserts_into_the_focused_widget() {
    let (mut session, root) = session();
    session.commit(root, "<input data-path='p' value=''>");
    let input = session.query(root, "input").unwrap()[0];
    session.set_focused(input as i32);

    assert!(session.widget_paste("hi there"));
    assert_eq!(session.widget_value(input).as_deref(), Some("hi there"));
    // A second paste continues from the caret — insert, not replace.
    assert!(session.widget_paste("!"));
    assert_eq!(session.widget_value(input).as_deref(), Some("hi there!"));
    // Nothing focused: the paste has no target.
    session.set_focused(-1);
    assert!(!session.widget_paste("lost"));
}

#[test]
fn focus_survives_a_commit_by_data_path() {
    let (mut session, root) = session();
    session.commit(root, "<div data-path='keep'>row</div>");
    let row = session.query(root, "[data-path=\"keep\"]").unwrap()[0];
    session.set_focused(row as i32);
    // The swap destroys the node; the same data-path re-resolves.
    session.commit(
        root,
        "<section><div data-path='keep'>row again</div></section>",
    );
    let focused = session.focused();
    assert!(focused >= 0, "focus re-resolved");
    assert_eq!(
        session.get_attr(focused as u32, "data-path").as_deref(),
        Some("keep")
    );
}

#[test]
fn a_resize_fully_repaints() {
    let (mut session, root) = session();
    session.commit(root, "<span>resizable content line</span>");
    let _ = session.draw().unwrap();
    let ansi = session.resize(60, 8).unwrap();
    assert!(ansi.contains("\x1b[2J"), "the resize clears: {ansi:?}");
    // Inline-flow words paint as separately positioned runs, so the
    // stream carries each word, not the spaced sentence.
    assert!(
        ansi.contains("resizable") && ansi.contains("line"),
        "{ansi:?}"
    );
    assert!(session.screen_text().contains("resizable content line"));
}
