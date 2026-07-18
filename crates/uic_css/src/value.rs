//! Typed values of the closed dialect, and the cell-unit conversion.
//!
//! The calibration is the repo's own: one column = 0.75rem = 12px = 1ch,
//! one row = 1.5rem = 24px = 1lh — Bootstrap's body line-height, and the
//! only mapping that reproduces the hardcoded class map's choices.

use cssparser::{match_ignore_ascii_case, Parser, ParserInput, Token};

pub const PX_PER_COLUMN: f32 = 12.0;
pub const PX_PER_ROW: f32 = 24.0;
const PX_PER_REM: f32 = 16.0;

/// The axis a length converts against — terminal cells are not square.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Axis {
    Horizontal,
    Vertical,
}

/// A parsed CSS length in the dialect.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Length {
    Px(f32),
    Rem(f32),
    Em(f32),
    Ch(f32),
    Lh(f32),
    Percent(f32),
    Zero,
}

impl Length {
    /// Converts to cells: round half away from zero; `separator` floors a
    /// nonzero length to one cell (borders and gaps must not vanish — the
    /// documented gap-2 rationale).
    pub fn to_cells(self, axis: Axis, separator: bool) -> Option<f32> {
        let px_per_cell = match axis {
            Axis::Horizontal => PX_PER_COLUMN,
            Axis::Vertical => PX_PER_ROW,
        };
        let px = match self {
            Length::Px(px) => px,
            Length::Rem(rem) | Length::Em(rem) => rem * PX_PER_REM,
            Length::Ch(ch) => ch * PX_PER_COLUMN,
            Length::Lh(lh) => lh * PX_PER_ROW,
            Length::Zero => 0.0,
            Length::Percent(_) => return None,
        };
        let cells = (px / px_per_cell + 0.5).floor().max(0.0);
        if separator && px > 0.0 && cells < 1.0 {
            return Some(1.0);
        }
        Some(cells)
    }

    pub fn percent(self) -> Option<f32> {
        match self {
            Length::Percent(unit) => Some(unit),
            _ => None,
        }
    }
}

/// The dialect's color space: the ANSI palette (so our sheets can hit exact
/// ratatui variants), 24-bit values for foreign sheets, and the CSS system
/// color `Highlight` standing in for reverse video.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Color {
    Ansi(AnsiColor),
    Rgb(u8, u8, u8),
    Highlight,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnsiColor {
    Black,
    Red,
    Green,
    Yellow,
    Blue,
    Magenta,
    Cyan,
    Gray,
    DarkGray,
    LightRed,
    LightGreen,
    LightYellow,
    LightBlue,
    LightMagenta,
    LightCyan,
    White,
}

/// One typed declaration value.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Length(Length),
    Color(Color),
    Keyword(String),
    /// `content: "…"` strings.
    Text(String),
    /// Up to four lengths (margin/padding shorthands).
    Sides(Vec<Length>),
}

/// Parses a raw declaration value into the dialect's typed form.
/// `None` = not expressible (the declaration is skipped at computed time).
pub fn parse_value(name: &str, raw: &str) -> Option<Value> {
    let mut input = ParserInput::new(raw);
    let mut parser = Parser::new(&mut input);
    match name {
        "color" | "background-color" => parse_color(&mut parser).map(Value::Color),
        "margin" | "padding" => {
            let mut sides = Vec::new();
            while let Ok(length) = parse_length(&mut parser) {
                sides.push(length);
            }
            if sides.is_empty() || sides.len() > 4 || !parser.is_exhausted() {
                return None;
            }
            Some(Value::Sides(sides))
        }
        "margin-top" | "margin-right" | "margin-bottom" | "margin-left" | "padding-top"
        | "padding-right" | "padding-bottom" | "padding-left" | "border-width" | "row-gap"
        | "column-gap" | "width" | "height" | "min-width" | "min-height" | "max-width"
        | "max-height" | "font-size" => {
            let length = parse_length_or_keyword(&mut parser, name)?;
            parser.is_exhausted().then_some(length)
        }
        "gap" => {
            let first = parse_length(&mut parser).ok()?;
            let second = parse_length(&mut parser).ok();
            if !parser.is_exhausted() {
                return None;
            }
            Some(Value::Sides(match second {
                Some(second) => vec![first, second],
                None => vec![first],
            }))
        }
        "content" => {
            let token = parser.next().ok()?.clone();
            match token {
                Token::QuotedString(text) => {
                    parser.is_exhausted().then(|| Value::Text(text.to_string()))
                }
                _ => None,
            }
        }
        _ => {
            // Keyword properties: display, flex-*, align-*, justify-content,
            // font-weight/style, text-align, text-decoration.
            let token = parser.next().ok()?.clone();
            match token {
                Token::Ident(ident) => {
                    let keyword = ident.to_ascii_lowercase();
                    parser.is_exhausted().then_some(Value::Keyword(keyword))
                }
                Token::Number { value, .. } if name == "flex-grow" || name == "flex-shrink" => {
                    parser
                        .is_exhausted()
                        .then(|| Value::Keyword(format!("{value}")))
                }
                Token::Dimension { .. } | Token::Percentage { .. } => {
                    // e.g. `font-weight: 500` handled above; dimensions in
                    // keyword slots are out of dialect.
                    None
                }
                Token::Number { value, .. } if name == "font-weight" => parser
                    .is_exhausted()
                    .then(|| Value::Keyword(format!("{value}"))),
                _ => None,
            }
        }
    }
}

fn parse_length_or_keyword(parser: &mut Parser, name: &str) -> Option<Value> {
    if let Ok(length) = parser.try_parse(|p| parse_length(p)) {
        return Some(Value::Length(length));
    }
    let token = parser.next().ok()?.clone();
    match token {
        Token::Ident(ident) => {
            let keyword = ident.to_ascii_lowercase();
            // `width: auto`, `font-size: smaller` and friends.
            let _ = name;
            Some(Value::Keyword(keyword))
        }
        _ => None,
    }
}

fn parse_length<'i>(parser: &mut Parser<'i, '_>) -> Result<Length, cssparser::ParseError<'i, ()>> {
    let location = parser.current_source_location();
    let token = parser.next()?.clone();
    match token {
        Token::Dimension { value, unit, .. } => {
            let length = match_ignore_ascii_case! { &unit,
                "px" => Length::Px(value),
                "rem" => Length::Rem(value),
                "em" => Length::Em(value),
                "ch" => Length::Ch(value),
                "lh" => Length::Lh(value),
                _ => return Err(location.new_custom_error(())),
            };
            Ok(length)
        }
        Token::Percentage { unit_value, .. } => Ok(Length::Percent(unit_value)),
        Token::Number { value: 0.0, .. } => Ok(Length::Zero),
        _ => Err(location.new_custom_error(())),
    }
}

fn parse_color(parser: &mut Parser) -> Option<Color> {
    let token = parser.next().ok()?.clone();
    let color = match token {
        Token::Ident(ident) => {
            let named = match_ignore_ascii_case! { &ident,
                "black" => Color::Ansi(AnsiColor::Black),
                "red" => Color::Ansi(AnsiColor::Red),
                "green" => Color::Ansi(AnsiColor::Green),
                "yellow" => Color::Ansi(AnsiColor::Yellow),
                "blue" => Color::Ansi(AnsiColor::Blue),
                "magenta" => Color::Ansi(AnsiColor::Magenta),
                "cyan" => Color::Ansi(AnsiColor::Cyan),
                "gray" | "grey" => Color::Ansi(AnsiColor::Gray),
                "white" => Color::Ansi(AnsiColor::White),
                "highlight" => Color::Highlight,
                _ => return None,
            };
            named
        }
        Token::Function(name) => {
            let name = name.to_ascii_lowercase();
            match name.as_str() {
                // The dialect's escape hatch to exact ANSI variants:
                // `ansi(light-red)`, `ansi(dark-gray)`, …
                "ansi" => parser
                    .parse_nested_block(|p| -> Result<AnsiColor, cssparser::ParseError<'_, ()>> {
                        let ident = p.expect_ident()?.to_ascii_lowercase();
                        parse_ansi_name(&ident).ok_or_else(|| p.new_custom_error(()))
                    })
                    .ok()
                    .map(Color::Ansi)?,
                "rgb" | "rgba" => parser
                    .parse_nested_block(|p| {
                        let r = component(p)?;
                        let _ = p.try_parse(|p| p.expect_comma());
                        let g = component(p)?;
                        let _ = p.try_parse(|p| p.expect_comma());
                        let b = component(p)?;
                        // Alpha (slash or comma form) is ignored: cells have
                        // no compositor.
                        while p.next().is_ok() {}
                        Ok::<_, cssparser::ParseError<'_, ()>>(Color::Rgb(r, g, b))
                    })
                    .ok()?,
                _ => return None,
            }
        }
        Token::IDHash(hash) | Token::Hash(hash) => parse_hex(&hash)?,
        _ => return None,
    };
    Some(color)
}

fn component<'i>(parser: &mut Parser<'i, '_>) -> Result<u8, cssparser::ParseError<'i, ()>> {
    let location = parser.current_source_location();
    match parser.next()?.clone() {
        Token::Number { value, .. } => Ok(value.clamp(0.0, 255.0) as u8),
        Token::Percentage { unit_value, .. } => Ok((unit_value * 255.0).clamp(0.0, 255.0) as u8),
        _ => Err(location.new_custom_error(())),
    }
}

fn parse_ansi_name(name: &str) -> Option<AnsiColor> {
    Some(match name {
        "black" => AnsiColor::Black,
        "red" => AnsiColor::Red,
        "green" => AnsiColor::Green,
        "yellow" => AnsiColor::Yellow,
        "blue" => AnsiColor::Blue,
        "magenta" => AnsiColor::Magenta,
        "cyan" => AnsiColor::Cyan,
        "gray" | "grey" => AnsiColor::Gray,
        "dark-gray" | "dark-grey" => AnsiColor::DarkGray,
        "light-red" => AnsiColor::LightRed,
        "light-green" => AnsiColor::LightGreen,
        "light-yellow" => AnsiColor::LightYellow,
        "light-blue" => AnsiColor::LightBlue,
        "light-magenta" => AnsiColor::LightMagenta,
        "light-cyan" => AnsiColor::LightCyan,
        "white" => AnsiColor::White,
        _ => return None,
    })
}

fn parse_hex(hash: &str) -> Option<Color> {
    let hex = hash.as_bytes();
    let nibble = |b: u8| -> Option<u8> {
        match b {
            b'0'..=b'9' => Some(b - b'0'),
            b'a'..=b'f' => Some(b - b'a' + 10),
            b'A'..=b'F' => Some(b - b'A' + 10),
            _ => None,
        }
    };
    match hex.len() {
        3 | 4 => {
            let r = nibble(hex[0])?;
            let g = nibble(hex[1])?;
            let b = nibble(hex[2])?;
            Some(Color::Rgb(r * 17, g * 17, b * 17))
        }
        6 | 8 => {
            let r = nibble(hex[0])? * 16 + nibble(hex[1])?;
            let g = nibble(hex[2])? * 16 + nibble(hex[3])?;
            let b = nibble(hex[4])? * 16 + nibble(hex[5])?;
            Some(Color::Rgb(r, g, b))
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_calibration_reproduces_the_hardcoded_map() {
        let rows = |rem: f32| Length::Rem(rem).to_cells(Axis::Vertical, false).unwrap();
        assert_eq!(rows(0.25), 0.0, "mt-1");
        assert_eq!(rows(0.5), 0.0, "mt-2");
        assert_eq!(rows(1.0), 1.0, "mt-3");
        assert_eq!(rows(1.5), 1.0, "mt-4");
        assert_eq!(rows(3.0), 2.0, "mt-5");

        let cols = |rem: f32| Length::Rem(rem).to_cells(Axis::Horizontal, false).unwrap();
        assert_eq!(cols(1.0), 1.0, "card padding");
        assert_eq!(cols(0.75), 1.0, "input-group-text");

        // gap-2 = 0.5rem: zero rows, one column — with the separator floor.
        assert_eq!(rows(0.5), 0.0);
        assert_eq!(
            Length::Rem(0.5).to_cells(Axis::Horizontal, true).unwrap(),
            1.0
        );
    }

    #[test]
    fn colors_parse_across_the_value_space() {
        assert_eq!(
            parse_value("color", "ansi(light-red)"),
            Some(Value::Color(Color::Ansi(AnsiColor::LightRed)))
        );
        assert_eq!(
            parse_value("color", "#a3eea0"),
            Some(Value::Color(Color::Rgb(0xa3, 0xee, 0xa0)))
        );
        assert_eq!(
            parse_value("color", "rgba(222, 175, 143, 0.9)"),
            Some(Value::Color(Color::Rgb(222, 175, 143)))
        );
        assert_eq!(
            parse_value("background-color", "Highlight"),
            Some(Value::Color(Color::Highlight))
        );
    }

    #[test]
    fn lengths_and_keywords_parse() {
        assert_eq!(
            parse_value("padding-left", "2ch"),
            Some(Value::Length(Length::Ch(2.0)))
        );
        assert_eq!(
            parse_value("display", "inline-flex"),
            Some(Value::Keyword("inline-flex".to_string()))
        );
        assert_eq!(
            parse_value("margin", "0 0 0 1.7rem"),
            Some(Value::Sides(vec![
                Length::Zero,
                Length::Zero,
                Length::Zero,
                Length::Rem(1.7)
            ]))
        );
        assert_eq!(
            parse_value("width", "100%"),
            Some(Value::Length(Length::Percent(1.0)))
        );
    }
}
