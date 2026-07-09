use crossterm::event::{KeyCode, KeyEventKind, KeyModifiers};
use uic_tui_web::translate_key;

#[test]
fn printables_arrive_as_chars_with_modifiers() {
    let event = translate_key("A", false, false, true).unwrap();
    assert_eq!(event.code, KeyCode::Char('A'));
    assert_eq!(event.modifiers, KeyModifiers::SHIFT);
    assert_eq!(event.kind, KeyEventKind::Press);

    let space = translate_key(" ", false, false, false).unwrap();
    assert_eq!(space.code, KeyCode::Char(' '));

    let ctrl_c = translate_key("c", true, false, false).unwrap();
    assert_eq!(ctrl_c.code, KeyCode::Char('c'));
    assert_eq!(ctrl_c.modifiers, KeyModifiers::CONTROL);
}

#[test]
fn named_keys_map_to_their_terminal_codes() {
    for (name, code) in [
        ("Enter", KeyCode::Enter),
        ("Escape", KeyCode::Esc),
        ("Backspace", KeyCode::Backspace),
        ("Delete", KeyCode::Delete),
        ("Insert", KeyCode::Insert),
        ("Home", KeyCode::Home),
        ("End", KeyCode::End),
        ("PageUp", KeyCode::PageUp),
        ("PageDown", KeyCode::PageDown),
        ("ArrowUp", KeyCode::Up),
        ("ArrowDown", KeyCode::Down),
        ("ArrowLeft", KeyCode::Left),
        ("ArrowRight", KeyCode::Right),
        ("Tab", KeyCode::Tab),
        ("F4", KeyCode::F(4)),
        ("F12", KeyCode::F(12)),
    ] {
        assert_eq!(
            translate_key(name, false, false, false).unwrap().code,
            code,
            "{name}"
        );
    }
}

#[test]
fn shift_tab_is_back_tab() {
    let event = translate_key("Tab", false, false, true).unwrap();
    assert_eq!(event.code, KeyCode::BackTab);
    assert_eq!(event.modifiers, KeyModifiers::SHIFT);
}

#[test]
fn keys_without_a_terminal_notion_are_dropped() {
    for name in [
        "Shift",
        "Control",
        "Alt",
        "Meta",
        "CapsLock",
        "Dead",
        "Unidentified",
        "F13",
    ] {
        assert!(translate_key(name, false, false, false).is_none(), "{name}");
    }
}

#[test]
fn pointer_gestures_map_to_terminal_mouse_kinds() {
    use crossterm::event::{MouseButton, MouseEventKind};
    use uic_tui_web::translate_mouse;

    for (name, kind) in [
        ("down", MouseEventKind::Down(MouseButton::Left)),
        ("up", MouseEventKind::Up(MouseButton::Left)),
        ("drag", MouseEventKind::Drag(MouseButton::Left)),
        ("wheel-up", MouseEventKind::ScrollUp),
        ("wheel-down", MouseEventKind::ScrollDown),
    ] {
        assert_eq!(translate_mouse(name), Some(kind), "{name}");
    }
    assert_eq!(translate_mouse("hover"), None);
}
