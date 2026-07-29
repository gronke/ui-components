//! Browser dialogs for terminal hosts: one centered box painted after the
//! document (ratatui buffers are last-write-wins — the popup rule) and a
//! keystroke handler the host routes to first while the box shows. Alert
//! acknowledges, confirm decides, prompt collects a line. Hosts own the
//! modality — paint last, route first; this module is only the box.

use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, Padding, Paragraph, Wrap};

use crate::keys::KeyStroke;

/// What the dialog asks and how it answers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DialogKind {
    Alert,
    Confirm,
    Prompt,
}

/// Which button holds the keyboard.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DialogChoice {
    Ok,
    Cancel,
}

/// What a keystroke did: the box stays open, or it answered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DialogOutcome {
    Open,
    Ok,
    Cancel,
}

/// The box a host paints and routes to. The labels default to ok/cancel;
/// a host asking its own question puts its own words on them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Dialog {
    pub kind: DialogKind,
    pub title: String,
    pub message: String,
    /// The prompt's line of text; unused for alert and confirm.
    pub input: String,
    pub focus: DialogChoice,
    pub ok_label: String,
    pub cancel_label: String,
}

impl Dialog {
    fn new(kind: DialogKind, title: &str, message: impl Into<String>) -> Dialog {
        Dialog {
            kind,
            title: title.into(),
            message: message.into(),
            input: String::new(),
            focus: DialogChoice::Ok,
            ok_label: "ok".into(),
            cancel_label: "cancel".into(),
        }
    }

    pub fn alert(message: impl Into<String>) -> Dialog {
        Self::new(DialogKind::Alert, "alert", message)
    }

    pub fn confirm(message: impl Into<String>) -> Dialog {
        Self::new(DialogKind::Confirm, "confirm", message)
    }

    pub fn prompt(message: impl Into<String>, default: &str) -> Dialog {
        Dialog {
            input: default.into(),
            ..Self::new(DialogKind::Prompt, "prompt", message)
        }
    }

    /// Routes one keystroke: Enter answers with the focused button, Escape
    /// cancels, Tab and the horizontal arrows move the focus, confirm
    /// takes y/n shortcuts, and a prompt eats printables into its line.
    /// Modifier chords stay with the host (Ctrl+C keeps quitting there).
    pub fn key(&mut self, stroke: &KeyStroke) -> DialogOutcome {
        if stroke.ctrl || stroke.alt || stroke.meta {
            return DialogOutcome::Open;
        }
        match stroke.key.as_str() {
            "Escape" => DialogOutcome::Cancel,
            "Enter" => match self.focus {
                DialogChoice::Ok => DialogOutcome::Ok,
                DialogChoice::Cancel => DialogOutcome::Cancel,
            },
            "Tab" | "ArrowLeft" | "ArrowRight" if self.kind != DialogKind::Alert => {
                self.focus = match self.focus {
                    DialogChoice::Ok => DialogChoice::Cancel,
                    DialogChoice::Cancel => DialogChoice::Ok,
                };
                DialogOutcome::Open
            }
            "Backspace" if self.kind == DialogKind::Prompt => {
                self.input.pop();
                DialogOutcome::Open
            }
            // Prompt text first: y and n are letters there, shortcuts only
            // on confirm.
            key if self.kind == DialogKind::Prompt && printable(key) => {
                self.input.push_str(key);
                DialogOutcome::Open
            }
            "y" if self.kind == DialogKind::Confirm => DialogOutcome::Ok,
            "n" if self.kind == DialogKind::Confirm => DialogOutcome::Cancel,
            _ => DialogOutcome::Open,
        }
    }
}

/// One printable character — the DOM key name of a text key.
fn printable(key: &str) -> bool {
    let mut chars = key.chars();
    matches!((chars.next(), chars.next()), (Some(c), None) if !c.is_control())
}

/// Greedy word wrap, the line count `Wrap { trim: true }` will paint —
/// overlong words spill across lines like the renderer spills them.
fn wrapped_line_count(text: &str, width: usize) -> usize {
    if width == 0 {
        return 1;
    }
    let mut lines = 1usize;
    let mut column = 0usize;
    for word in text.split_whitespace() {
        let mut length = word.chars().count();
        if column > 0 && column + 1 + length > width {
            lines += 1;
            column = 0;
        }
        if column > 0 {
            column += 1;
        }
        while column + length > width {
            let taken = width - column;
            length -= taken;
            lines += 1;
            column = 0;
        }
        column += length;
    }
    lines
}

/// Paints the dialog centered over `area` — call it AFTER the document
/// paint; the buffer's last write wins, so the box overlays whatever lies
/// beneath (the widget popups' own mechanism).
pub fn paint_dialog(frame: &mut ratatui::Frame, area: Rect, dialog: &Dialog) {
    let message_width = dialog.message.chars().count();
    let buttons_width =
        dialog.ok_label.chars().count() + dialog.cancel_label.chars().count() + "[  ]   [  ]".len();
    let inner = message_width
        .max(buttons_width)
        .max(dialog.input.chars().count() + 1)
        .clamp(26, 56)
        .min(area.width.saturating_sub(6) as usize);
    let message_lines = wrapped_line_count(&dialog.message, inner);
    let prompt_lines = if dialog.kind == DialogKind::Prompt {
        2
    } else {
        0
    };
    let width = (inner + 4) as u16;
    let height = (2 + message_lines + 1 + prompt_lines + 1) as u16;
    let width = width.min(area.width);
    let height = height.min(area.height);
    let rect = Rect {
        x: area.x + (area.width.saturating_sub(width)) / 2,
        y: area.y + (area.height.saturating_sub(height)) / 2,
        width,
        height,
    };

    frame.render_widget(Clear, rect);
    let block = Block::bordered()
        .title(dialog.title.as_str())
        .padding(Padding::horizontal(1));
    let body = block.inner(rect);
    frame.render_widget(block, rect);

    let message = Rect {
        height: (message_lines as u16).min(body.height),
        ..body
    };
    frame.render_widget(
        Paragraph::new(dialog.message.as_str()).wrap(Wrap { trim: true }),
        message,
    );

    if dialog.kind == DialogKind::Prompt && body.height > message.height + 1 {
        let input = Rect {
            y: message.y + message.height + 1,
            height: 1,
            ..body
        };
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::raw(dialog.input.as_str()),
                Span::styled("▏", Style::new().dim()),
            ])),
            input,
        );
    }

    if body.height > 0 {
        let styled = |label: &str, focused: bool| {
            let text = format!("[ {label} ]");
            if focused {
                Span::styled(text, Style::new().reversed())
            } else {
                Span::raw(text)
            }
        };
        let mut spans = vec![styled(&dialog.ok_label, dialog.focus == DialogChoice::Ok)];
        if dialog.kind != DialogKind::Alert {
            spans.push(Span::raw("   "));
            spans.push(styled(
                &dialog.cancel_label,
                dialog.focus == DialogChoice::Cancel,
            ));
        }
        let buttons = Rect {
            y: body.y + body.height - 1,
            height: 1,
            ..body
        };
        frame.render_widget(Paragraph::new(Line::from(spans)).centered(), buttons);
    }
}
