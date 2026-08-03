//! Character and text transformations for the terminal: whitespace
//! collapse, wrap measurement in cells, and marker-glyph rotation: the
//! pure string half of the layout, testable without a document.
//!
//! The rotation mapping is a curated table by necessity, not convenience:
//! Unicode defines no rotation property, many arrows have no rotated
//! counterpart among the ~611 arrow codepoints, and the crate ecosystem
//! accordingly offers nothing for semantic glyph rotation (only spinner
//! frame sequences and string-order reversal). Horizontal mirroring is the
//! one transformation with official data behind it (UAX #9's
//! Bidi_Mirroring_Glyph, served by the `unicode-brackets` crate), the
//! principled source if mirroring is ever needed here.

use unicode_width::UnicodeWidthStr;

/// Collapses runs of ASCII whitespace to single spaces and trims the ends,
/// like the browser flows prose. Non-breaking spaces are content, not
/// separators: the browser renders `&nbsp;`, so the terminal keeps it too
/// (indentation would otherwise collapse away).
pub(crate) fn collapse_whitespace(text: &str) -> String {
    words(text).collect::<Vec<_>>().join(" ")
}

/// The wrap words of a text: runs unbroken by ASCII whitespace, so a
/// non-breaking space glues its neighbors into one word.
fn words(text: &str) -> impl Iterator<Item = &str> {
    text.split(|c: char| c.is_ascii_whitespace())
        .filter(|word| !word.is_empty())
}

/// Greedy word wrap in cells, the reservation behind ratatui's `Wrap`:
/// words fill each line up to the width and an oversized word breaks
/// across lines.
pub(crate) fn wrapped_lines(text: &str, width: f32) -> u16 {
    let width = width.round().max(1.0) as usize;
    let mut lines: u16 = 1;
    let mut used = 0usize;
    for word in words(text) {
        let len = word.width();
        if used > 0 && used + 1 + len <= width {
            used += 1 + len;
            continue;
        }
        if used == 0 && len <= width {
            used = len;
            continue;
        }
        if used > 0 {
            lines += 1;
        }
        let mut rest = len;
        while rest > width {
            lines += 1;
            rest -= width;
        }
        used = rest;
    }
    lines
}

/// The widest single word: the narrowest a text can measure (MinContent).
pub(crate) fn longest_word(text: &str) -> f32 {
    words(text).map(|word| word.width()).max().unwrap_or(0) as f32
}

/// Leading ASCII whitespace, the collapse's separator class.
pub(crate) fn starts_spaced(raw: &str) -> bool {
    raw.starts_with(|c: char| c.is_ascii_whitespace())
}

pub(crate) fn ends_spaced(raw: &str) -> bool {
    raw.ends_with(|c: char| c.is_ascii_whitespace())
}

/// The terminal's take on rotating a marker glyph: single glyphs with a
/// right-angle mapping swap; everything else keeps its upright form.
pub(crate) fn rotated_glyph(text: String, degrees: u16) -> String {
    const ROTATED: &[(char, u16, char)] = &[
        ('▶', 90, '▼'),
        ('▶', 180, '◀'),
        ('▶', 270, '▲'),
        ('▸', 90, '▾'),
        ('▸', 180, '◂'),
        ('▸', 270, '▴'),
        ('▼', 90, '◀'),
        ('▼', 180, '▲'),
        ('▼', 270, '▶'),
    ];
    if degrees == 0 {
        return text;
    }
    let mut chars = text.chars();
    let (Some(glyph), None) = (chars.next(), chars.next()) else {
        return text;
    };
    ROTATED
        .iter()
        .find(|(from, angle, _)| *from == glyph && *angle == degrees)
        .map(|(_, _, to)| to.to_string())
        .unwrap_or(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ascii_whitespace_collapses_and_trims() {
        assert_eq!(collapse_whitespace("  a \n\t b  "), "a b");
    }

    #[test]
    fn non_breaking_spaces_survive_the_collapse() {
        assert_eq!(
            collapse_whitespace("\n  \u{a0}\u{a0}indented rest\n"),
            "\u{a0}\u{a0}indented rest"
        );
    }

    #[test]
    fn non_breaking_spaces_glue_wrap_words() {
        // One 8-cell word (two NBSPs plus "abcdef"): it wraps as a unit, so
        // a 10-cell line holds it and pushes the next word down.
        assert_eq!(wrapped_lines("\u{a0}\u{a0}abcdef xyz", 10.0), 2);
        assert_eq!(longest_word("\u{a0}\u{a0}abcdef xyz"), 8.0);
    }

    #[test]
    fn right_angle_rotations_swap_marker_glyphs() {
        assert_eq!(rotated_glyph("\u{25b6}".into(), 90), "\u{25bc}");
        assert_eq!(rotated_glyph("\u{25b6}".into(), 0), "\u{25b6}");
        assert_eq!(rotated_glyph("\u{25b6}".into(), 45), "\u{25b6}");
        assert_eq!(rotated_glyph("no glyph".into(), 90), "no glyph");
    }
}
