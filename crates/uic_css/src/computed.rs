//! The computed style: what the terminal's layout and paint consume.

use std::collections::HashMap;

use crate::value::{Axis, Color, Length, Value};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Display {
    #[default]
    Block,
    Flex,
    Grid,
    Inline,
    InlineFlex,
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FlexDirection {
    #[default]
    Row,
    Column,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum Dimension {
    #[default]
    Auto,
    Cells(f32),
    Percent(f32),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TextAlign {
    #[default]
    Start,
    Center,
    End,
}

/// Cell-resolved sides: top, right, bottom, left.
pub type Sides = [f32; 4];

/// The computed style of one element, cell-resolved.
#[derive(Debug, Clone, PartialEq)]
pub struct ComputedStyle {
    pub display: Display,
    pub margin: Sides,
    pub padding: Sides,
    /// Uniform border width in cells (terminal borders are one cell).
    pub border: f32,
    /// (row gap, column gap) in cells.
    pub gap: (f32, f32),
    pub flex_direction: FlexDirection,
    pub flex_wrap: bool,
    pub flex_grow: Option<f32>,
    pub flex_shrink: Option<f32>,
    pub align_items_center: bool,
    pub align_self_center: bool,
    pub width: Dimension,
    pub height: Dimension,
    pub min_width: Dimension,
    pub max_width: Dimension,

    // Inherited text styling (background inherits too: the terminal has no
    // transparency, so the enclosing paint shows through by inheritance —
    // a documented approximation).
    pub color: Option<Color>,
    pub background: Option<Color>,
    pub bold: bool,
    pub italic: bool,
    pub dim: bool,
    pub underlined: bool,
    pub crossed_out: bool,
    pub text_align: TextAlign,
    /// Inherited custom properties, raw token text.
    pub custom: HashMap<String, String>,

    // Pseudo-element payload: only meaningful on ::before/::after styles.
    pub content: Option<String>,
    /// Right-angle rotation for single-glyph content (the marker quirk).
    pub rotation: u16,
}

impl Default for ComputedStyle {
    fn default() -> Self {
        ComputedStyle {
            display: Display::Block,
            margin: [0.0; 4],
            padding: [0.0; 4],
            border: 0.0,
            gap: (0.0, 0.0),
            flex_direction: FlexDirection::Row,
            flex_wrap: false,
            flex_grow: None,
            flex_shrink: None,
            align_items_center: false,
            align_self_center: false,
            width: Dimension::Auto,
            height: Dimension::Auto,
            min_width: Dimension::Auto,
            max_width: Dimension::Auto,
            color: None,
            background: None,
            bold: false,
            italic: false,
            dim: false,
            underlined: false,
            crossed_out: false,
            text_align: TextAlign::Start,
            custom: HashMap::new(),
            content: None,
            rotation: 0,
        }
    }
}

impl ComputedStyle {
    /// The child's starting point: the inherited slice of this style.
    pub fn inherited(&self) -> ComputedStyle {
        ComputedStyle {
            color: self.color,
            background: self.background,
            bold: self.bold,
            italic: self.italic,
            dim: self.dim,
            underlined: self.underlined,
            crossed_out: self.crossed_out,
            text_align: self.text_align,
            custom: self.custom.clone(),
            ..ComputedStyle::default()
        }
    }

    /// Applies one typed declaration.
    pub fn apply(&mut self, name: &str, value: &Value) {
        match (name, value) {
            ("display", Value::Keyword(kw)) => {
                self.display = match kw.as_str() {
                    "block" => Display::Block,
                    "flex" => Display::Flex,
                    "grid" => Display::Grid,
                    "inline" => Display::Inline,
                    "inline-flex" => Display::InlineFlex,
                    "none" => Display::None,
                    _ => return,
                };
            }
            ("margin", Value::Sides(sides)) => self.margin = expand_sides(sides, false),
            ("padding", Value::Sides(sides)) => self.padding = expand_sides(sides, false),
            ("margin-top", Value::Length(l)) => set_side(&mut self.margin, 0, *l, false),
            ("margin-right", Value::Length(l)) => set_side(&mut self.margin, 1, *l, false),
            ("margin-bottom", Value::Length(l)) => set_side(&mut self.margin, 2, *l, false),
            ("margin-left", Value::Length(l)) => set_side(&mut self.margin, 3, *l, false),
            ("padding-top", Value::Length(l)) => set_side(&mut self.padding, 0, *l, false),
            ("padding-right", Value::Length(l)) => set_side(&mut self.padding, 1, *l, false),
            ("padding-bottom", Value::Length(l)) => set_side(&mut self.padding, 2, *l, false),
            ("padding-left", Value::Length(l)) => set_side(&mut self.padding, 3, *l, false),
            ("border-width", Value::Length(l)) => {
                if let Some(cells) = l.to_cells(Axis::Horizontal, true) {
                    self.border = cells;
                }
            }
            ("gap", Value::Sides(sides)) => {
                let row = sides[0].to_cells(Axis::Vertical, true).unwrap_or(0.0);
                let column = sides
                    .get(1)
                    .unwrap_or(&sides[0])
                    .to_cells(Axis::Horizontal, true)
                    .unwrap_or(0.0);
                self.gap = (row, column);
            }
            ("row-gap", Value::Length(l)) => {
                self.gap.0 = l.to_cells(Axis::Vertical, true).unwrap_or(self.gap.0);
            }
            ("column-gap", Value::Length(l)) => {
                self.gap.1 = l.to_cells(Axis::Horizontal, true).unwrap_or(self.gap.1);
            }
            ("flex-direction", Value::Keyword(kw)) => {
                self.flex_direction = match kw.as_str() {
                    "row" => FlexDirection::Row,
                    "column" => FlexDirection::Column,
                    _ => return,
                };
            }
            ("flex-wrap", Value::Keyword(kw)) => {
                self.flex_wrap = match kw.as_str() {
                    "wrap" => true,
                    "nowrap" => false,
                    _ => return,
                };
            }
            ("flex-grow", Value::Keyword(kw)) => {
                if let Ok(grow) = kw.parse::<f32>() {
                    self.flex_grow = Some(grow);
                }
            }
            ("flex-shrink", Value::Keyword(kw)) => {
                if let Ok(shrink) = kw.parse::<f32>() {
                    self.flex_shrink = Some(shrink);
                }
            }
            ("align-items", Value::Keyword(kw)) => {
                self.align_items_center = kw == "center";
            }
            ("align-self", Value::Keyword(kw)) => {
                self.align_self_center = kw == "center";
            }
            ("width", value) => self.width = dimension(value, Axis::Horizontal),
            ("height", value) => self.height = dimension(value, Axis::Vertical),
            ("min-width", value) => self.min_width = dimension(value, Axis::Horizontal),
            ("max-width", value) => self.max_width = dimension(value, Axis::Horizontal),
            ("color", Value::Color(color)) => self.color = Some(*color),
            ("background-color", Value::Color(color)) => self.background = Some(*color),
            ("font-weight", Value::Keyword(kw)) => {
                self.bold =
                    kw == "bold" || kw == "bolder" || kw.parse::<f32>().is_ok_and(|w| w >= 600.0);
            }
            ("font-style", Value::Keyword(kw)) => {
                self.italic = kw == "italic" || kw == "oblique";
            }
            ("font-size", value) => {
                // Sub-line sizes read as dim — `small { font-size: smaller }`.
                self.dim = match value {
                    Value::Keyword(kw) => kw == "smaller" || kw == "small" || kw == "x-small",
                    Value::Length(Length::Em(em)) => *em < 1.0,
                    Value::Length(Length::Rem(rem)) => *rem < 1.0,
                    Value::Length(Length::Px(px)) => *px < 16.0,
                    _ => self.dim,
                };
            }
            ("text-align", Value::Keyword(kw)) => {
                self.text_align = match kw.as_str() {
                    "center" => TextAlign::Center,
                    "right" | "end" => TextAlign::End,
                    "left" | "start" => TextAlign::Start,
                    _ => return,
                };
            }
            ("content", Value::Text(text)) => self.content = Some(text.clone()),
            ("transform", Value::Rotation(degrees)) => self.rotation = *degrees,
            ("text-decoration" | "text-decoration-line", Value::Keyword(kw)) => match kw.as_str() {
                "underline" => self.underlined = true,
                "line-through" => self.crossed_out = true,
                "none" => {
                    self.underlined = false;
                    self.crossed_out = false;
                }
                _ => {}
            },
            _ => {}
        }
    }
}

fn dimension(value: &Value, axis: Axis) -> Dimension {
    match value {
        Value::Length(length) => match length.percent() {
            Some(unit) => Dimension::Percent(unit),
            None => length
                .to_cells(axis, false)
                .map(Dimension::Cells)
                .unwrap_or(Dimension::Auto),
        },
        Value::Keyword(kw) if kw == "auto" => Dimension::Auto,
        _ => Dimension::Auto,
    }
}

fn set_side(sides: &mut Sides, index: usize, length: Length, separator: bool) {
    let axis = if index.is_multiple_of(2) {
        Axis::Vertical
    } else {
        Axis::Horizontal
    };
    if let Some(cells) = length.to_cells(axis, separator) {
        sides[index] = cells;
    }
}

/// CSS shorthand expansion: 1 → all, 2 → v/h, 3 → t/h/b, 4 → t/r/b/l.
fn expand_sides(lengths: &[Length], separator: bool) -> Sides {
    let (t, r, b, l) = match lengths {
        [all] => (*all, *all, *all, *all),
        [v, h] => (*v, *h, *v, *h),
        [t, h, b] => (*t, *h, *b, *h),
        [t, r, b, l] => (*t, *r, *b, *l),
        _ => return [0.0; 4],
    };
    let mut sides = [0.0; 4];
    set_side(&mut sides, 0, t, separator);
    set_side(&mut sides, 1, r, separator);
    set_side(&mut sides, 2, b, separator);
    set_side(&mut sides, 3, l, separator);
    sides
}
