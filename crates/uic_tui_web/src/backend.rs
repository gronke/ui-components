use std::cell::RefCell;
use std::fmt::Write as _;
use std::io;
use std::rc::Rc;

use ratatui::backend::{Backend, ClearType, WindowSize};
use ratatui::buffer::Cell;
use ratatui::layout::{Position, Size};
use ratatui::style::{Color, Modifier};

/// Shared handle onto the ANSI a backend produced; the session drains it
/// after each draw and hands it to `term.write`.
#[derive(Clone, Default)]
pub struct Output(Rc<RefCell<String>>);

impl Output {
    pub fn take(&self) -> String {
        std::mem::take(&mut self.0.borrow_mut())
    }
}

/// A ratatui backend that renders into an ANSI string for xterm.js instead
/// of a terminal device. It also keeps the symbols as a plain-text shadow
/// grid, so hosts and tests can assert on the screen without a terminal.
pub struct XtermBackend {
    cols: u16,
    rows: u16,
    out: Output,
    cursor: Position,
    screen: Vec<Vec<String>>,
}

impl XtermBackend {
    pub fn new(cols: u16, rows: u16) -> (Self, Output) {
        let out = Output::default();
        let backend = XtermBackend {
            cols,
            rows,
            out: out.clone(),
            cursor: Position::new(0, 0),
            screen: vec![vec![" ".to_string(); cols as usize]; rows as usize],
        };
        (backend, out)
    }

    /// Resizes the reported terminal size; the shadow grid resets blank
    /// (ratatui's autoresize clears and fully repaints on the next draw).
    pub fn resize(&mut self, cols: u16, rows: u16) {
        self.cols = cols.max(1);
        self.rows = rows.max(1);
        self.screen = vec![vec![" ".to_string(); self.cols as usize]; self.rows as usize];
    }

    /// The screen as text rows, trailing blanks trimmed.
    pub fn screen_text(&self) -> String {
        self.screen
            .iter()
            .map(|row| row.concat().trim_end().to_string())
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn push(&self, ansi: &str) {
        self.out.0.borrow_mut().push_str(ansi);
    }
}

impl Backend for XtermBackend {
    type Error = io::Error;

    fn draw<'a, I>(&mut self, content: I) -> Result<(), Self::Error>
    where
        I: Iterator<Item = (u16, u16, &'a Cell)>,
    {
        let out = self.out.0.clone();
        let mut out = out.borrow_mut();
        let mut last: Option<(u16, u16)> = None;
        let mut style: Option<(Color, Color, Modifier)> = None;
        for (x, y, cell) in content {
            // The terminal cursor advanced past the previous cell; move it
            // only when this cell is not the adjacent one.
            if last != Some((x.wrapping_sub(1), y)) {
                let _ = write!(out, "\x1b[{};{}H", y + 1, x + 1);
            }
            let cell_style = (cell.fg, cell.bg, cell.modifier);
            if style != Some(cell_style) {
                write_sgr(&mut out, cell);
                style = Some(cell_style);
            }
            out.push_str(cell.symbol());
            if let Some(row) = self.screen.get_mut(y as usize) {
                if let Some(slot) = row.get_mut(x as usize) {
                    *slot = cell.symbol().to_string();
                }
            }
            last = Some((x, y));
        }
        out.push_str("\x1b[0m");
        Ok(())
    }

    fn hide_cursor(&mut self) -> Result<(), Self::Error> {
        self.push("\x1b[?25l");
        Ok(())
    }

    fn show_cursor(&mut self) -> Result<(), Self::Error> {
        self.push("\x1b[?25h");
        Ok(())
    }

    fn get_cursor_position(&mut self) -> Result<Position, Self::Error> {
        Ok(self.cursor)
    }

    fn set_cursor_position<P: Into<Position>>(&mut self, position: P) -> Result<(), Self::Error> {
        self.cursor = position.into();
        let ansi = format!("\x1b[{};{}H", self.cursor.y + 1, self.cursor.x + 1);
        self.push(&ansi);
        Ok(())
    }

    fn clear(&mut self) -> Result<(), Self::Error> {
        self.push("\x1b[2J");
        for row in &mut self.screen {
            row.fill(" ".to_string());
        }
        Ok(())
    }

    fn clear_region(&mut self, clear_type: ClearType) -> Result<(), Self::Error> {
        self.push(match clear_type {
            ClearType::All => "\x1b[2J",
            ClearType::AfterCursor => "\x1b[J",
            ClearType::BeforeCursor => "\x1b[1J",
            ClearType::CurrentLine => "\x1b[2K",
            ClearType::UntilNewLine => "\x1b[K",
        });
        Ok(())
    }

    fn size(&self) -> Result<Size, Self::Error> {
        Ok(Size::new(self.cols, self.rows))
    }

    fn window_size(&mut self) -> Result<WindowSize, Self::Error> {
        Ok(WindowSize {
            columns_rows: Size::new(self.cols, self.rows),
            // Cell pixel sizes are unknown in the browser; the trait allows
            // zero here.
            pixels: Size::new(0, 0),
        })
    }

    fn flush(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }
}

/// Reset-then-set: one SGR sequence per style run, no incremental diffing.
fn write_sgr(out: &mut String, cell: &Cell) {
    out.push_str("\x1b[0");
    let modifier = cell.modifier;
    for (flag, code) in [
        (Modifier::BOLD, "1"),
        (Modifier::DIM, "2"),
        (Modifier::ITALIC, "3"),
        (Modifier::UNDERLINED, "4"),
        (Modifier::SLOW_BLINK, "5"),
        (Modifier::RAPID_BLINK, "6"),
        (Modifier::REVERSED, "7"),
        (Modifier::HIDDEN, "8"),
        (Modifier::CROSSED_OUT, "9"),
    ] {
        if modifier.contains(flag) {
            out.push(';');
            out.push_str(code);
        }
    }
    write_color(out, cell.fg, true);
    write_color(out, cell.bg, false);
    out.push('m');
}

/// SGR color parameters; the reset (39/49) is implied by the leading `0`.
fn write_color(out: &mut String, color: Color, foreground: bool) {
    let base: u16 = if foreground { 30 } else { 40 };
    let simple = |offset: u16| base + offset;
    let bright = |offset: u16| base + 60 + offset;
    let code = match color {
        Color::Reset => return,
        Color::Black => simple(0),
        Color::Red => simple(1),
        Color::Green => simple(2),
        Color::Yellow => simple(3),
        Color::Blue => simple(4),
        Color::Magenta => simple(5),
        Color::Cyan => simple(6),
        Color::Gray => simple(7),
        Color::DarkGray => bright(0),
        Color::LightRed => bright(1),
        Color::LightGreen => bright(2),
        Color::LightYellow => bright(3),
        Color::LightBlue => bright(4),
        Color::LightMagenta => bright(5),
        Color::LightCyan => bright(6),
        Color::White => bright(7),
        Color::Rgb(r, g, b) => {
            let _ = write!(out, ";{};2;{};{};{}", base + 8, r, g, b);
            return;
        }
        Color::Indexed(index) => {
            let _ = write!(out, ";{};5;{}", base + 8, index);
            return;
        }
    };
    let _ = write!(out, ";{code}");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sgr(cell: &Cell) -> String {
        let mut out = String::new();
        write_sgr(&mut out, cell);
        out
    }

    #[test]
    fn sgr_resets_then_sets_modifiers_and_colors() {
        let mut cell = Cell::EMPTY;
        cell.fg = Color::Red;
        cell.modifier = Modifier::BOLD;
        assert_eq!(sgr(&cell), "\x1b[0;1;31m");

        cell.fg = Color::Reset;
        cell.bg = Color::Rgb(1, 2, 3);
        cell.modifier = Modifier::DIM | Modifier::REVERSED;
        assert_eq!(sgr(&cell), "\x1b[0;2;7;48;2;1;2;3m");

        cell.bg = Color::Indexed(105);
        cell.modifier = Modifier::empty();
        assert_eq!(sgr(&cell), "\x1b[0;48;5;105m");
    }
}
