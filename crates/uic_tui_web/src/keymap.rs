use crossterm::event::{KeyEvent, MouseButton, MouseEventKind};
use uic_tui::KeyStroke;

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
/// `shiftKey`) into the terminal key event the runtime understands,
/// the shared vocabulary of `uic_tui::keys`. Returns `None` for keys a
/// terminal has no notion of, like bare modifiers.
pub fn translate_key(key: &str, ctrl: bool, alt: bool, shift: bool) -> Option<KeyEvent> {
    KeyStroke {
        key: key.to_string(),
        shift,
        ctrl,
        alt,
        meta: false,
    }
    .to_crossterm()
}
