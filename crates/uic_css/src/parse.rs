//! Stylesheet parsing into the closed dialect: qualified rules whose
//! selectors parse and whose declarations name supported properties survive;
//! everything else drops into the report — the degradation contract, made
//! measurable (ADR 0016's reserved slot).

use cssparser::{
    AtRuleParser, CowRcStr, DeclarationParser, ParseError, Parser, ParserInput,
    QualifiedRuleParser, RuleBodyItemParser, RuleBodyParser, SourceLocation, StyleSheetParser,
};
use selectors::parser::{ParseRelative, SelectorList, SelectorParseErrorKind};

use crate::select::{TuiSelectorParser, TuiSelectors};

/// The supported property names — the closed subset. Custom properties
/// (`--*`) are always kept.
const SUPPORTED: &[&str] = &[
    "display",
    "margin",
    "margin-top",
    "margin-right",
    "margin-bottom",
    "margin-left",
    "padding",
    "padding-top",
    "padding-right",
    "padding-bottom",
    "padding-left",
    "border-width",
    "gap",
    "row-gap",
    "column-gap",
    "flex-direction",
    "flex-wrap",
    "flex-grow",
    "flex-shrink",
    "align-items",
    "align-self",
    "justify-content",
    "width",
    "height",
    "min-width",
    "min-height",
    "max-width",
    "max-height",
    "color",
    "background-color",
    "font-weight",
    "font-style",
    "font-size",
    "text-align",
    "text-decoration",
    "text-decoration-line",
    "content",
    "transform",
];

/// One kept declaration: the property name, the raw value text (custom
/// properties and `var()` substitute at computed-value time), importance.
#[derive(Debug, Clone)]
pub struct Declaration {
    pub name: String,
    pub value: String,
    pub important: bool,
}

/// One kept rule, in source order.
pub struct Rule {
    pub selectors: SelectorList<TuiSelectors>,
    pub declarations: Vec<Declaration>,
    pub source_order: u32,
}

/// What the dialect dropped, for the lint and the generator report.
#[derive(Debug, Default)]
pub struct DropReport {
    pub selectors: Vec<String>,
    pub declarations: Vec<String>,
    pub at_rules: Vec<String>,
}

#[derive(Default)]
pub struct Stylesheet {
    pub rules: Vec<Rule>,
}

/// Parses a stylesheet, keeping the dialect and reporting the rest.
pub fn parse_stylesheet(source: &str) -> (Stylesheet, DropReport) {
    let mut input = ParserInput::new(source);
    let mut parser = Parser::new(&mut input);
    let mut sheet = Stylesheet::default();
    let mut report = DropReport::default();
    let mut top = TopParser {
        sheet: &mut sheet,
        report: &mut report,
        order: 0,
    };
    for result in StyleSheetParser::new(&mut parser, &mut top) {
        // Rule-level recovery: the iterator yields errors for skipped rules;
        // the parsers below already recorded the interesting drops.
        let _ = result;
    }
    (sheet, report)
}

struct TopParser<'a> {
    sheet: &'a mut Stylesheet,
    report: &'a mut DropReport,
    order: u32,
}

impl<'i> QualifiedRuleParser<'i> for TopParser<'_> {
    type Prelude = SelectorList<TuiSelectors>;
    type QualifiedRule = ();
    type Error = SelectorParseErrorKind<'i>;

    fn parse_prelude<'t>(
        &mut self,
        input: &mut Parser<'i, 't>,
    ) -> Result<Self::Prelude, ParseError<'i, Self::Error>> {
        let start = input.position();
        SelectorList::parse(&TuiSelectorParser, input, ParseRelative::No).inspect_err(|_| {
            self.report
                .selectors
                .push(input.slice_from(start).trim().to_string());
        })
    }

    fn parse_block<'t>(
        &mut self,
        prelude: Self::Prelude,
        _start: &cssparser::ParserState,
        input: &mut Parser<'i, 't>,
    ) -> Result<(), ParseError<'i, Self::Error>> {
        let mut declarations = Vec::new();
        let mut body = BodyParser {
            declarations: &mut declarations,
            report: self.report,
        };
        for result in RuleBodyParser::new(input, &mut body) {
            let _ = result;
        }
        if !declarations.is_empty() {
            self.sheet.rules.push(Rule {
                selectors: prelude,
                declarations,
                source_order: self.order,
            });
            self.order += 1;
        }
        Ok(())
    }
}

impl<'i> AtRuleParser<'i> for TopParser<'_> {
    type Prelude = ();
    type AtRule = ();
    type Error = SelectorParseErrorKind<'i>;

    fn parse_prelude<'t>(
        &mut self,
        name: CowRcStr<'i>,
        input: &mut Parser<'i, 't>,
    ) -> Result<Self::Prelude, ParseError<'i, Self::Error>> {
        // Media queries and friends drop wholesale into the report.
        self.report.at_rules.push(format!("@{name}"));
        Err(input.new_custom_error(SelectorParseErrorKind::UnexpectedIdent(name)))
    }
}

struct BodyParser<'a> {
    declarations: &'a mut Vec<Declaration>,
    report: &'a mut DropReport,
}

impl<'i> DeclarationParser<'i> for BodyParser<'_> {
    type Declaration = ();
    type Error = SelectorParseErrorKind<'i>;

    fn parse_value<'t>(
        &mut self,
        name: CowRcStr<'i>,
        input: &mut Parser<'i, 't>,
        _declaration_start: &cssparser::ParserState,
    ) -> Result<(), ParseError<'i, Self::Error>> {
        let supported = name.starts_with("--") || SUPPORTED.contains(&&*name.to_ascii_lowercase());
        let start = input.position();
        while input.next().is_ok() {}
        if !supported {
            self.report.declarations.push(name.to_string());
            return Ok(());
        }
        let raw = input.slice_from(start).trim();
        let (value, important) = match raw.to_ascii_lowercase().rfind("!important") {
            Some(pos) if raw[pos..].eq_ignore_ascii_case("!important") => {
                (raw[..pos].trim_end().to_string(), true)
            }
            _ => (raw.to_string(), false),
        };
        self.declarations.push(Declaration {
            name: name.to_string(),
            value,
            important,
        });
        Ok(())
    }
}

impl<'i> QualifiedRuleParser<'i> for BodyParser<'_> {
    type Prelude = ();
    type QualifiedRule = ();
    type Error = SelectorParseErrorKind<'i>;
}

impl<'i> AtRuleParser<'i> for BodyParser<'_> {
    type Prelude = ();
    type AtRule = ();
    type Error = SelectorParseErrorKind<'i>;
}

impl<'i> RuleBodyItemParser<'i, (), SelectorParseErrorKind<'i>> for BodyParser<'_> {
    fn parse_declarations(&self) -> bool {
        true
    }

    fn parse_qualified(&self) -> bool {
        false
    }
}

/// Where a `SourceLocation` diagnostic would go later; unused in the spike.
#[allow(dead_code)]
fn location_string(location: SourceLocation) -> String {
    format!("{}:{}", location.line, location.column)
}
