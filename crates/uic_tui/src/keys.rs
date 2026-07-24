//! One keystroke vocabulary for every host: DOM `KeyboardEvent.key` names
//! beside crossterm's `KeyCode`, translated from one table in both
//! directions. Native hosts read crossterm and speak DOM names to the JS
//! runtimes; the browser hosts read DOM names and speak crossterm to the
//! terminal runtime.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

/// A DOM-shaped keystroke: the `KeyboardEvent.key` name plus the modifier
/// flags — the shared currency between crossterm and every JS-facing host.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyStroke {
    pub key: String,
    pub shift: bool,
    pub ctrl: bool,
    pub alt: bool,
    pub meta: bool,
}

/// The named keys both sides know — one table, both directions. Function
/// keys and printable characters translate programmatically.
const NAMED: &[(&str, KeyCode)] = &[
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
];

impl KeyStroke {
    /// A plain, unmodified stroke.
    pub fn new(key: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            shift: false,
            ctrl: false,
            alt: false,
            meta: false,
        }
    }

    /// A stroke carrying the shift flag — reorder chords and friends.
    pub fn shifted(key: impl Into<String>) -> Self {
        Self {
            shift: true,
            ..Self::new(key)
        }
    }

    /// The DOM name and modifiers of a terminal key event; `None` for keys
    /// the DOM contract has no name for. Printable characters keep
    /// `shift` false — the character itself already carries the case.
    /// `BackTab` reads as shifted `Tab`, the DOM's spelling of it.
    pub fn from_crossterm(event: &KeyEvent) -> Option<Self> {
        let shift = event.modifiers.contains(KeyModifiers::SHIFT);
        let base = Self {
            key: String::new(),
            shift,
            ctrl: event.modifiers.contains(KeyModifiers::CONTROL),
            alt: event.modifiers.contains(KeyModifiers::ALT),
            meta: event
                .modifiers
                .intersects(KeyModifiers::META | KeyModifiers::SUPER),
        };
        Some(match event.code {
            KeyCode::Char(c) => Self {
                key: c.to_string(),
                shift: false,
                ..base
            },
            KeyCode::BackTab => Self {
                key: "Tab".into(),
                shift: true,
                ..base
            },
            KeyCode::F(n) => Self {
                key: format!("F{n}"),
                ..base
            },
            code => Self {
                key: NAMED
                    .iter()
                    .find(|(_, named)| *named == code)?
                    .0
                    .to_string(),
                ..base
            },
        })
    }

    /// The terminal event of this stroke; `None` for names a terminal has
    /// no notion of, like bare modifiers. Shifted `Tab` folds into
    /// `BackTab` while the SHIFT modifier stays set — the shape the
    /// browser sessions always produced.
    pub fn to_crossterm(&self) -> Option<KeyEvent> {
        let mut modifiers = KeyModifiers::NONE;
        if self.shift {
            modifiers |= KeyModifiers::SHIFT;
        }
        if self.ctrl {
            modifiers |= KeyModifiers::CONTROL;
        }
        if self.alt {
            modifiers |= KeyModifiers::ALT;
        }
        if self.meta {
            modifiers |= KeyModifiers::META;
        }
        let code = if self.key == "Tab" && self.shift {
            KeyCode::BackTab
        } else if let Some((_, code)) = NAMED.iter().find(|(name, _)| *name == self.key) {
            *code
        } else {
            let mut chars = self.key.chars();
            match (chars.next(), chars.next()) {
                // A printable key arrives as its single character.
                (Some(c), None) => KeyCode::Char(c),
                _ => {
                    let n: u8 = self.key.strip_prefix('F')?.parse().ok()?;
                    if (1..=12).contains(&n) {
                        KeyCode::F(n)
                    } else {
                        return None;
                    }
                }
            }
        };
        Some(KeyEvent::new(code, modifiers))
    }

    /// The two universal quit chords every host honors: Escape, Ctrl+C.
    pub fn is_quit(&self) -> bool {
        self.key == "Escape" || (self.ctrl && self.key == "c")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_table_round_trips_both_ways() {
        for (name, code) in NAMED {
            let stroke = KeyStroke::new(*name);
            let event = stroke.to_crossterm().expect("named key translates");
            assert_eq!(event.code, *code, "{name}");
            let back = KeyStroke::from_crossterm(&event).expect("and comes back");
            assert_eq!(back, stroke, "{name}");
        }
    }

    #[test]
    fn characters_carry_case_instead_of_shift() {
        let upper =
            KeyStroke::from_crossterm(&KeyEvent::new(KeyCode::Char('A'), KeyModifiers::SHIFT))
                .unwrap();
        assert_eq!(upper.key, "A");
        assert!(!upper.shift);
        assert_eq!(
            KeyStroke::new("a").to_crossterm().unwrap().code,
            KeyCode::Char('a')
        );
    }

    #[test]
    fn function_keys_translate_within_the_terminal_range() {
        assert_eq!(
            KeyStroke::new("F5").to_crossterm().unwrap().code,
            KeyCode::F(5)
        );
        assert!(KeyStroke::new("F13").to_crossterm().is_none());
        let f6 =
            KeyStroke::from_crossterm(&KeyEvent::new(KeyCode::F(6), KeyModifiers::NONE)).unwrap();
        assert_eq!(f6.key, "F6");
    }

    #[test]
    fn shifted_tab_is_backtab_with_shift_still_set() {
        let event = KeyStroke::shifted("Tab").to_crossterm().unwrap();
        assert_eq!(event.code, KeyCode::BackTab);
        assert!(event.modifiers.contains(KeyModifiers::SHIFT));
        let back = KeyStroke::from_crossterm(&event).unwrap();
        assert_eq!(back.key, "Tab");
        assert!(back.shift);
    }

    #[test]
    fn bare_modifiers_translate_to_nothing() {
        for name in ["Shift", "Control", "Alt", "Meta", "CapsLock", "Dead"] {
            assert!(KeyStroke::new(name).to_crossterm().is_none(), "{name}");
        }
    }

    #[test]
    fn the_quit_chords() {
        assert!(KeyStroke::new("Escape").is_quit());
        let mut ctrl_c = KeyStroke::new("c");
        ctrl_c.ctrl = true;
        assert!(ctrl_c.is_quit());
        assert!(!KeyStroke::new("c").is_quit());
        assert!(!KeyStroke::shifted("Tab").is_quit());
    }
}
