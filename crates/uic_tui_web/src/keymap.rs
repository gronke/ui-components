use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEventKind};

/// Translates a DOM pointer gesture name into the terminal mouse kind.
/// The pane only speaks the left button and the wheel, like the widgets.
pub fn translate_mouse(kind: &str) -> Option<MouseEventKind> {
    Some(match kind {
        "down" => MouseEventKind::Down(MouseButton::Left),
        "up" => MouseEventKind::Up(MouseButton::Left),
        "drag" => MouseEventKind::Drag(MouseButton::Left),
        "wheel-up" => MouseEventKind::ScrollUp,
        "wheel-down" => MouseEventKind::ScrollDown,
        _ => return None,
    })
}

/// Translates a DOM `KeyboardEvent` (its `key`, `ctrlKey`, `altKey`,
/// `shiftKey`) into the terminal key event the runtime understands.
/// Returns `None` for keys a terminal has no notion of, like bare modifiers.
pub fn translate_key(key: &str, ctrl: bool, alt: bool, shift: bool) -> Option<KeyEvent> {
    let mut modifiers = KeyModifiers::NONE;
    if shift {
        modifiers |= KeyModifiers::SHIFT;
    }
    if ctrl {
        modifiers |= KeyModifiers::CONTROL;
    }
    if alt {
        modifiers |= KeyModifiers::ALT;
    }
    let code = match key {
        "Enter" => KeyCode::Enter,
        "Escape" => KeyCode::Esc,
        "Backspace" => KeyCode::Backspace,
        "Delete" => KeyCode::Delete,
        "Insert" => KeyCode::Insert,
        "Home" => KeyCode::Home,
        "End" => KeyCode::End,
        "PageUp" => KeyCode::PageUp,
        "PageDown" => KeyCode::PageDown,
        "ArrowUp" => KeyCode::Up,
        "ArrowDown" => KeyCode::Down,
        "ArrowLeft" => KeyCode::Left,
        "ArrowRight" => KeyCode::Right,
        "Tab" if shift => KeyCode::BackTab,
        "Tab" => KeyCode::Tab,
        _ => {
            let mut chars = key.chars();
            match (chars.next(), chars.next()) {
                // A printable key arrives as its single character.
                (Some(c), None) => KeyCode::Char(c),
                _ => {
                    let n: u8 = key.strip_prefix('F')?.parse().ok()?;
                    if (1..=12).contains(&n) {
                        KeyCode::F(n)
                    } else {
                        return None;
                    }
                }
            }
        }
    };
    Some(KeyEvent::new(code, modifiers))
}
