//! Recursive-descent parser for the lit-flavored template dialect.

use crate::{AttrPart, Attribute, Element, Expr, Node, ParseError, Template};

/// `[a-z_][a-z0-9_]*` — the loop-variable naming rule for `as=…` bindings.
fn is_ident(s: &str) -> bool {
    let mut chars = s.chars();
    matches!(chars.next(), Some(c) if c.is_ascii_lowercase() || c == '_')
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

/// HTML void elements: no children, no closing tag.
const VOID_ELEMENTS: &[&str] = &[
    "area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta", "param", "source",
    "track", "wbr",
];

/// Parses a template fragment into its IR.
pub fn parse(src: &str) -> Result<Template, ParseError> {
    let mut parser = Parser {
        src,
        pos: 0,
        scope: Vec::new(),
    };
    let roots = parser.parse_nodes(None)?;
    Ok(Template { roots })
}

struct Parser<'s> {
    src: &'s str,
    pos: usize,
    /// Loop variables in scope, innermost last (ADR 0018); a `${base.field}`
    /// hole is valid only when `base` is one of these.
    scope: Vec<String>,
}

impl<'s> Parser<'s> {
    fn rest(&self) -> &'s str {
        &self.src[self.pos..]
    }

    fn peek(&self) -> Option<char> {
        self.rest().chars().next()
    }

    fn starts_with(&self, prefix: &str) -> bool {
        self.rest().starts_with(prefix)
    }

    fn eat(&mut self, prefix: &str) -> bool {
        if self.starts_with(prefix) {
            self.pos += prefix.len();
            true
        } else {
            false
        }
    }

    fn bump(&mut self) -> Option<char> {
        let ch = self.peek()?;
        self.pos += ch.len_utf8();
        Some(ch)
    }

    fn skip_ws(&mut self) {
        while self.peek().is_some_and(|c| c.is_ascii_whitespace()) {
            self.pos += 1;
        }
    }

    fn error(&self, message: impl Into<String>) -> ParseError {
        self.error_at(self.pos, message)
    }

    fn error_at(&self, offset: usize, message: impl Into<String>) -> ParseError {
        let prefix = &self.src[..offset.min(self.src.len())];
        let line = prefix.matches('\n').count() + 1;
        let column = prefix.rsplit('\n').next().map_or(0, str::len) + 1;
        ParseError {
            message: message.into(),
            offset,
            line,
            column,
        }
    }

    /// Parses sibling nodes until EOF (`close = None`) or the matching
    /// closing tag (`close = Some(tag)`), which is consumed.
    fn parse_nodes(&mut self, close: Option<&str>) -> Result<Vec<Node>, ParseError> {
        let mut nodes = Vec::new();
        loop {
            if self.rest().is_empty() {
                return match close {
                    None => Ok(nodes),
                    Some(tag) => Err(self.error(format!("unclosed <{tag}>: expected </{tag}>"))),
                };
            }
            if self.starts_with("</") {
                let offset = self.pos;
                self.pos += 2;
                let found = self.parse_tag_name()?;
                self.skip_ws();
                if !self.eat(">") {
                    return Err(self.error(format!("expected '>' to end </{found}>")));
                }
                return match close {
                    Some(tag) if tag == found => Ok(nodes),
                    Some(tag) => Err(self.error_at(
                        offset,
                        format!("mismatched closing tag: expected </{tag}>, found </{found}>"),
                    )),
                    None => {
                        Err(self.error_at(offset, format!("unexpected closing tag </{found}>")))
                    }
                };
            }
            if self.starts_with("<!--") {
                let offset = self.pos;
                match self.rest().find("-->") {
                    Some(end) => self.pos += end + 3,
                    None => return Err(self.error_at(offset, "unterminated comment")),
                }
                continue;
            }
            if self.starts_with("<") {
                nodes.push(self.parse_element()?);
                continue;
            }
            if self.starts_with("${") {
                let expr = self.parse_hole()?;
                nodes.push(Node::TextHole(expr));
                continue;
            }
            nodes.push(Node::Text(self.parse_text()?));
        }
    }

    /// Accumulates text up to the next markup start, resolving `\${` escapes.
    fn parse_text(&mut self) -> Result<String, ParseError> {
        let mut text = String::new();
        loop {
            if self.rest().is_empty() || self.starts_with("<") || self.starts_with("${") {
                return Ok(text);
            }
            if self.eat("\\${") {
                text.push_str("${");
                continue;
            }
            text.push(self.bump().expect("rest is non-empty"));
        }
    }

    /// Parses `${expr}` at the current position.
    fn parse_hole(&mut self) -> Result<Expr, ParseError> {
        let offset = self.pos;
        assert!(self.eat("${"), "caller checked for '${{'");
        self.skip_ws();
        let negated = self.eat("!");
        self.skip_ws();
        let ident = self.parse_ident().map_err(|_| {
            self.error_at(
                offset,
                "unsupported expression: holes take a property or computed name, or !name",
            )
        })?;
        // `${base.field}` — a member of a loop variable in scope (ADR 0018).
        let member_field = if self.starts_with(".") {
            if negated {
                return Err(self.error_at(offset, "a loop variable member cannot be negated"));
            }
            if !self.scope.contains(&ident) {
                return Err(self.error_at(
                    offset,
                    format!(
                        "unknown loop variable '{ident}'; a member hole `${{base.field}}` \
                         needs an enclosing <template for=… as={ident}>"
                    ),
                ));
            }
            self.eat(".");
            Some(
                self.parse_ident()
                    .map_err(|_| self.error("a member hole takes a field name: ${base.field}"))?,
            )
        } else {
            None
        };
        self.skip_ws();
        if self.starts_with("|") {
            return Err(
                self.error("formatters are reserved; declare a computed property on the component")
            );
        }
        if !self.eat("}") {
            return Err(self.error_at(
                offset,
                "unsupported expression: holes take a property or computed name, or !name",
            ));
        }
        Ok(match (negated, member_field) {
            (_, Some(field)) => Expr::Member { base: ident, field },
            (true, None) => Expr::Not(ident),
            (false, None) => Expr::Ident(ident),
        })
    }

    /// `[a-z_][a-z0-9_]*` — property, computed, and handler names follow the
    /// Rust field naming of the component definition.
    fn parse_ident(&mut self) -> Result<String, ()> {
        let start = self.pos;
        match self.peek() {
            Some(c) if c.is_ascii_lowercase() || c == '_' => self.pos += 1,
            _ => return Err(()),
        }
        while self
            .peek()
            .is_some_and(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
        {
            self.pos += 1;
        }
        Ok(self.src[start..self.pos].to_string())
    }

    /// `[a-z][a-z0-9-]*` — tags are written lowercase; a dash marks a custom
    /// element.
    fn parse_tag_name(&mut self) -> Result<&'s str, ParseError> {
        let start = self.pos;
        match self.peek() {
            Some(c) if c.is_ascii_lowercase() => self.pos += 1,
            _ => return Err(self.error("expected a lowercase tag name")),
        }
        while self
            .peek()
            .is_some_and(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        {
            self.pos += 1;
        }
        Ok(&self.src[start..self.pos])
    }

    /// `[a-zA-Z_][a-zA-Z0-9_-]*` — attribute, property, and event names.
    fn parse_attr_name(&mut self) -> Result<&'s str, ParseError> {
        let start = self.pos;
        match self.peek() {
            Some(c) if c.is_ascii_alphabetic() || c == '_' => self.pos += 1,
            _ => return Err(self.error("expected an attribute name")),
        }
        while self
            .peek()
            .is_some_and(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
        {
            self.pos += 1;
        }
        Ok(&self.src[start..self.pos])
    }

    fn parse_element(&mut self) -> Result<Node, ParseError> {
        let offset = self.pos;
        assert!(self.eat("<"), "caller checked for '<'");
        let tag = self.parse_tag_name()?.to_string();

        let mut attrs = Vec::new();
        let self_closing = loop {
            self.skip_ws();
            if self.eat("/>") {
                break true;
            }
            if self.eat(">") {
                break false;
            }
            if self.rest().is_empty() {
                return Err(self.error_at(offset, format!("unclosed start tag <{tag}>")));
            }
            let attr = self.parse_attribute()?;
            if attrs
                .iter()
                .any(|existing: &Attribute| existing.name() == attr.name())
            {
                return Err(self.error(format!("duplicate attribute '{}'", attr.name())));
            }
            attrs.push(attr);
        };

        if tag == "template" {
            return if attrs.iter().any(|attr| attr.name() == "for") {
                self.finish_template_for(offset, attrs, self_closing)
            } else {
                self.finish_template_if(offset, attrs, self_closing)
            };
        }

        let children = if self_closing || VOID_ELEMENTS.contains(&tag.as_str()) {
            Vec::new()
        } else {
            self.parse_nodes(Some(&tag))?
        };

        Ok(Node::Element(Element {
            tag,
            attrs,
            children,
        }))
    }

    /// Converts a parsed `<template …>` into `Node::If`.
    fn finish_template_if(
        &mut self,
        offset: usize,
        mut attrs: Vec<Attribute>,
        self_closing: bool,
    ) -> Result<Node, ParseError> {
        let cond = match (attrs.pop(), attrs.is_empty()) {
            (Some(Attribute::Attr { name, mut parts }), true) if name == "if" => {
                match (parts.pop(), parts.is_empty()) {
                    (Some(AttrPart::Expr(expr)), true) => expr,
                    _ => {
                        return Err(self.error_at(
                            offset,
                            "<template> takes exactly if=${name} or if=${!name}",
                        ))
                    }
                }
            }
            _ => {
                return Err(
                    self.error_at(offset, "<template> takes exactly if=${name} or if=${!name}")
                )
            }
        };
        if self_closing {
            return Err(self.error_at(offset, "<template if=…> requires children"));
        }
        let then = self.parse_nodes(Some("template"))?;
        Ok(Node::If { cond, then })
    }

    /// Converts a parsed `<template for=${each} as=item>` into `Node::For`.
    /// The `as` binding enters scope for the body, so `${item.field}` holes
    /// inside resolve (ADR 0018).
    fn finish_template_for(
        &mut self,
        offset: usize,
        attrs: Vec<Attribute>,
        self_closing: bool,
    ) -> Result<Node, ParseError> {
        let syntax = "<template> takes exactly for=${each} as=item";
        let mut each = None;
        let mut item = None;
        for attr in attrs {
            match attr {
                Attribute::Attr { name, mut parts } if name == "for" => {
                    match (parts.pop(), parts.is_empty()) {
                        // The array reference is a property, computed, or an
                        // outer loop variable's member (a nested list).
                        (
                            Some(AttrPart::Expr(expr @ (Expr::Ident(_) | Expr::Member { .. }))),
                            true,
                        ) => each = Some(expr),
                        _ => return Err(self.error_at(offset, syntax)),
                    }
                }
                Attribute::Static { name, value } if name == "as" => {
                    if !is_ident(&value) {
                        return Err(self.error_at(
                            offset,
                            format!("`as={value}` must be a lowercase identifier"),
                        ));
                    }
                    item = Some(value);
                }
                _ => return Err(self.error_at(offset, syntax)),
            }
        }
        let (Some(each), Some(item)) = (each, item) else {
            return Err(self.error_at(offset, syntax));
        };
        if self_closing {
            return Err(self.error_at(offset, "<template for=…> requires children"));
        }
        self.scope.push(item.clone());
        let body = self.parse_nodes(Some("template"));
        self.scope.pop();
        Ok(Node::For {
            each,
            item,
            body: body?,
        })
    }

    fn parse_attribute(&mut self) -> Result<Attribute, ParseError> {
        let sigil = match self.peek() {
            Some(c @ ('?' | '.' | '@')) => {
                self.pos += 1;
                Some(c)
            }
            _ => None,
        };
        let name_offset = self.pos;
        let name = self.parse_attr_name()?.to_string();

        if !self.eat("=") {
            return match sigil {
                None => Ok(Attribute::Static {
                    name,
                    value: String::new(),
                }),
                Some(c) => {
                    Err(self.error_at(name_offset, format!("'{c}{name}' requires a ${{…}} value")))
                }
            };
        }

        match sigil {
            Some('@') => {
                let handler = self.parse_handler_value(&name)?;
                Ok(Attribute::Event { name, handler })
            }
            Some(c @ ('?' | '.')) => {
                if !self.starts_with("${") {
                    return Err(self.error(format!("'{c}{name}' requires a single ${{…}} value")));
                }
                let expr = self.parse_hole()?;
                Ok(match c {
                    '?' => Attribute::Bool { name, expr },
                    _ => Attribute::Prop { name, expr },
                })
            }
            Some(_) => unreachable!("sigil is one of ?, ., @"),
            None => self.parse_plain_attr_value(name),
        }
    }

    /// `@event=${handler}` — the value must be a bare handler name.
    fn parse_handler_value(&mut self, event: &str) -> Result<String, ParseError> {
        let offset = self.pos;
        if !self.eat("${") {
            return Err(self.error(format!("'@{event}' requires a ${{handler_name}} value")));
        }
        self.skip_ws();
        let handler = self.parse_ident().map_err(|_| {
            self.error_at(
                offset,
                format!(
                    "'@{event}' takes a handler name declared on the component, \
                     not an inline expression"
                ),
            )
        })?;
        self.skip_ws();
        if !self.eat("}") {
            return Err(self.error_at(
                offset,
                format!(
                    "'@{event}' takes a handler name declared on the component, \
                     not an inline expression"
                ),
            ));
        }
        Ok(handler)
    }

    /// `name=${x}`, `name="a ${x} b"`, or `name=bare`.
    fn parse_plain_attr_value(&mut self, name: String) -> Result<Attribute, ParseError> {
        if self.starts_with("${") {
            let expr = self.parse_hole()?;
            return Ok(Attribute::Attr {
                name,
                parts: vec![AttrPart::Expr(expr)],
            });
        }
        if self.eat("\"") {
            let mut parts: Vec<AttrPart> = Vec::new();
            let mut text = String::new();
            loop {
                if self.eat("\"") {
                    if !text.is_empty() {
                        parts.push(AttrPart::Static(text));
                    }
                    break;
                }
                if self.rest().is_empty() {
                    return Err(self.error(format!("unterminated value for attribute '{name}'")));
                }
                if self.eat("\\${") {
                    text.push_str("${");
                    continue;
                }
                if self.starts_with("${") {
                    if !text.is_empty() {
                        parts.push(AttrPart::Static(std::mem::take(&mut text)));
                    }
                    parts.push(AttrPart::Expr(self.parse_hole()?));
                    continue;
                }
                text.push(self.bump().expect("rest is non-empty"));
            }
            let has_holes = parts.iter().any(|p| matches!(p, AttrPart::Expr(_)));
            return Ok(if has_holes {
                Attribute::Attr { name, parts }
            } else {
                let value = match parts.pop() {
                    Some(AttrPart::Static(value)) => value,
                    _ => String::new(),
                };
                Attribute::Static { name, value }
            });
        }
        // Bare (unquoted) value: a single token.
        let start = self.pos;
        while self
            .peek()
            .is_some_and(|c| !c.is_ascii_whitespace() && c != '>' && c != '/' && c != '"')
        {
            self.pos += 1;
        }
        if start == self.pos {
            return Err(self.error(format!("missing value for attribute '{name}'")));
        }
        Ok(Attribute::Static {
            name,
            value: self.src[start..self.pos].to_string(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_ok(src: &str) -> Template {
        parse(src).expect("template parses")
    }

    fn parse_err(src: &str) -> ParseError {
        parse(src).expect_err("template must not parse")
    }

    #[test]
    fn element_with_static_attrs_and_text() {
        let t = parse_ok(r#"<div class="a b">hi</div>"#);
        assert_eq!(
            t.roots,
            vec![Node::Element(Element {
                tag: "div".into(),
                attrs: vec![Attribute::Static {
                    name: "class".into(),
                    value: "a b".into(),
                }],
                children: vec![Node::Text("hi".into())],
            })]
        );
    }

    #[test]
    fn text_hole() {
        let t = parse_ok("<span>${label}</span>");
        let Node::Element(el) = &t.roots[0] else {
            panic!("expected element");
        };
        assert_eq!(
            el.children,
            vec![Node::TextHole(Expr::Ident("label".into()))]
        );
    }

    #[test]
    fn mixed_attribute_parts() {
        let t = parse_ok(r#"<input class="form-control ${extra} text-center">"#);
        let Node::Element(el) = &t.roots[0] else {
            panic!("expected element");
        };
        assert_eq!(
            el.attrs,
            vec![Attribute::Attr {
                name: "class".into(),
                parts: vec![
                    AttrPart::Static("form-control ".into()),
                    AttrPart::Expr(Expr::Ident("extra".into())),
                    AttrPart::Static(" text-center".into()),
                ],
            }]
        );
    }

    #[test]
    fn unquoted_hole_attribute() {
        let t = parse_ok("<input placeholder=${placeholder_text}>");
        let Node::Element(el) = &t.roots[0] else {
            panic!("expected element");
        };
        assert_eq!(
            el.attrs,
            vec![Attribute::Attr {
                name: "placeholder".into(),
                parts: vec![AttrPart::Expr(Expr::Ident("placeholder_text".into()))],
            }]
        );
    }

    #[test]
    fn sigil_bindings() {
        let t = parse_ok(
            "<input ?disabled=${disabled} ?hidden=${!visible} .value=${value} @change=${on_change}>",
        );
        let Node::Element(el) = &t.roots[0] else {
            panic!("expected element");
        };
        assert_eq!(
            el.attrs,
            vec![
                Attribute::Bool {
                    name: "disabled".into(),
                    expr: Expr::Ident("disabled".into()),
                },
                Attribute::Bool {
                    name: "hidden".into(),
                    expr: Expr::Not("visible".into()),
                },
                Attribute::Prop {
                    name: "value".into(),
                    expr: Expr::Ident("value".into()),
                },
                Attribute::Event {
                    name: "change".into(),
                    handler: "on_change".into(),
                },
            ]
        );
    }

    #[test]
    fn event_with_dashed_name() {
        let t = parse_ok("<input-timezone @value-changed=${on_timezone_changed}></input-timezone>");
        let Node::Element(el) = &t.roots[0] else {
            panic!("expected element");
        };
        assert!(el.is_custom());
        assert_eq!(
            el.attrs,
            vec![Attribute::Event {
                name: "value-changed".into(),
                handler: "on_timezone_changed".into(),
            }]
        );
    }

    #[test]
    fn template_if_and_negation() {
        let t = parse_ok(
            "<template if=${label}><label>${label}</label></template>\
             <template if=${!error_message}>ok</template>",
        );
        assert_eq!(t.roots.len(), 2);
        let Node::If { cond, then } = &t.roots[0] else {
            panic!("expected if");
        };
        assert_eq!(cond, &Expr::Ident("label".into()));
        assert_eq!(then.len(), 1);
        let Node::If { cond, then } = &t.roots[1] else {
            panic!("expected if");
        };
        assert_eq!(cond, &Expr::Not("error_message".into()));
        assert_eq!(then, &vec![Node::Text("ok".into())]);
    }

    #[test]
    fn nested_template_if_expresses_and() {
        let t = parse_ok(
            "<template if=${hint}><template if=${!error_message}>${hint}</template></template>",
        );
        let Node::If { then, .. } = &t.roots[0] else {
            panic!("expected if");
        };
        assert!(matches!(&then[0], Node::If { .. }));
    }

    #[test]
    fn void_and_self_closing_elements() {
        let t = parse_ok("<input type=\"text\"><br><input-date /><div/>");
        assert_eq!(t.roots.len(), 4);
        let Node::Element(input_date) = &t.roots[2] else {
            panic!("expected element");
        };
        assert_eq!(input_date.tag, "input-date");
        assert!(input_date.children.is_empty());
    }

    #[test]
    fn comments_are_skipped() {
        let t = parse_ok("<!-- chrome --><div><!-- inner -->x</div>");
        assert_eq!(t.roots.len(), 1);
        let Node::Element(el) = &t.roots[0] else {
            panic!("expected element");
        };
        assert_eq!(el.children, vec![Node::Text("x".into())]);
    }

    #[test]
    fn escaped_hole_in_text_and_attribute() {
        let t = parse_ok(r#"<code data-example="\${x}">\${literal}</code>"#);
        let Node::Element(el) = &t.roots[0] else {
            panic!("expected element");
        };
        assert_eq!(
            el.attrs,
            vec![Attribute::Static {
                name: "data-example".into(),
                value: "${x}".into(),
            }]
        );
        assert_eq!(el.children, vec![Node::Text("${literal}".into())]);
    }

    #[test]
    fn bare_attribute_and_bare_value() {
        let t = parse_ok("<input disabled data-suffix=2>");
        let Node::Element(el) = &t.roots[0] else {
            panic!("expected element");
        };
        assert_eq!(
            el.attrs,
            vec![
                Attribute::Static {
                    name: "disabled".into(),
                    value: String::new(),
                },
                Attribute::Static {
                    name: "data-suffix".into(),
                    value: "2".into(),
                },
            ]
        );
    }

    #[test]
    fn whitespace_text_is_preserved() {
        let t = parse_ok("<div>\n  <b>x</b>\n</div>");
        let Node::Element(el) = &t.roots[0] else {
            panic!("expected element");
        };
        assert_eq!(el.children.len(), 3);
        assert_eq!(el.children[0], Node::Text("\n  ".into()));
        assert_eq!(el.children[2], Node::Text("\n".into()));
    }

    #[test]
    fn referenced_names_are_collected() {
        let t = parse_ok(
            "<template if=${label}><label class=\"x ${label_class}\">${label}</label></template>\
             <input .value=${value} ?disabled=${disabled} @change=${on_change} \
             placeholder=${placeholder_text}>",
        );
        let idents: Vec<_> = t.referenced_idents().into_iter().collect();
        assert_eq!(
            idents,
            vec![
                "disabled",
                "label",
                "label_class",
                "placeholder_text",
                "value"
            ]
        );
        let handlers: Vec<_> = t.referenced_handlers().into_iter().collect();
        assert_eq!(handlers, vec!["on_change"]);
    }

    #[test]
    fn custom_tags_are_collected() {
        let t = parse_ok("<div><input-timezone></input-timezone><ui-icon-material /></div>");
        let tags: Vec<_> = t.custom_tags().into_iter().collect();
        assert_eq!(tags, vec!["input-timezone", "ui-icon-material"]);
    }

    #[test]
    fn error_unclosed_tag() {
        let err = parse_err("<div><span>x</div>");
        assert!(err.message.contains("mismatched closing tag"), "{err}");
        assert!(err.message.contains("</span>"), "{err}");
    }

    #[test]
    fn error_unexpected_close() {
        let err = parse_err("x</div>");
        assert!(err.message.contains("unexpected closing tag"), "{err}");
    }

    #[test]
    fn error_missing_close_at_eof() {
        let err = parse_err("<div>");
        assert!(err.message.contains("unclosed <div>"), "{err}");
    }

    #[test]
    fn error_unsupported_expression() {
        let err = parse_err("<span>${a ? b : c}</span>");
        assert!(err.message.contains("unsupported expression"), "{err}");
    }

    #[test]
    fn for_loop_binds_a_scope_for_member_holes() {
        let template =
            parse("<template for=${rows} as=row><td>${row.name}</td></template>").expect("parses");
        let Node::For { each, item, body } = &template.roots[0] else {
            panic!("expected a for node, got {:?}", template.roots[0]);
        };
        assert_eq!(each, &Expr::Ident("rows".into()));
        assert_eq!(item, "row");
        let Node::Element(td) = &body[0] else {
            panic!("expected a cell");
        };
        assert_eq!(
            td.children[0],
            Node::TextHole(Expr::Member {
                base: "row".into(),
                field: "name".into(),
            })
        );
        // The array reference is a referenced ident; the member is not.
        assert!(template.referenced_idents().contains("rows"));
        assert!(!template.referenced_idents().contains("row"));
        assert!(!template.referenced_idents().contains("name"));
    }

    #[test]
    fn for_loops_nest_and_stack_scopes() {
        let template = parse(
            "<template for=${groups} as=g><template for=${g.items} as=i>${i.label}</template></template>",
        )
        .expect("nested for parses");
        let Node::For { body, .. } = &template.roots[0] else {
            panic!("outer for");
        };
        let Node::For { each, .. } = &body[0] else {
            panic!("inner for");
        };
        assert_eq!(
            each,
            &Expr::Member {
                base: "g".into(),
                field: "items".into()
            }
        );
    }

    #[test]
    fn error_member_hole_needs_a_loop_variable() {
        let err = parse_err("<span>${a.b}</span>");
        assert!(err.message.contains("unknown loop variable"), "{err}");
    }

    #[test]
    fn error_member_hole_out_of_scope() {
        let err = parse_err("<template for=${rows} as=row></template>${row.name}");
        assert!(err.message.contains("unknown loop variable"), "{err}");
    }

    #[test]
    fn error_for_requires_as() {
        let err = parse_err("<template for=${rows}><td>x</td></template>");
        assert!(err.message.contains("for=${each} as=item"), "{err}");
    }

    #[test]
    fn error_formatters_reserved() {
        let err = parse_err("<span>${value | upper}</span>");
        assert!(err.message.contains("formatters are reserved"), "{err}");
        assert!(err.message.contains("computed property"), "{err}");
    }

    #[test]
    fn error_sigil_requires_hole() {
        let err = parse_err(r#"<input ?disabled="yes">"#);
        assert!(
            err.message.contains("requires a single ${…} value"),
            "{err}"
        );
        let err = parse_err("<input .value=3>");
        assert!(
            err.message.contains("requires a single ${…} value"),
            "{err}"
        );
        let err = parse_err("<input ?disabled>");
        assert!(err.message.contains("requires a ${…} value"), "{err}");
    }

    #[test]
    fn error_handler_must_be_a_name() {
        let err = parse_err("<button @click=${() => increment()}>x</button>");
        assert!(err.message.contains("handler name"), "{err}");
        let err = parse_err("<button @click=${this.increment}>x</button>");
        assert!(err.message.contains("handler name"), "{err}");
    }

    #[test]
    fn error_duplicate_attribute() {
        let err = parse_err(r#"<div class="a" class="b"></div>"#);
        assert!(err.message.contains("duplicate attribute 'class'"), "{err}");
    }

    #[test]
    fn error_uppercase_tag() {
        let err = parse_err("<Div></Div>");
        assert!(err.message.contains("lowercase tag name"), "{err}");
    }

    #[test]
    fn error_stray_angle_bracket() {
        let err = parse_err("<div>a < b</div>");
        assert!(err.message.contains("lowercase tag name"), "{err}");
    }

    #[test]
    fn error_template_without_if() {
        let err = parse_err("<template>x</template>");
        assert!(err.message.contains("<template> takes exactly"), "{err}");
        let err = parse_err(r#"<template if="label">x</template>"#);
        assert!(err.message.contains("<template> takes exactly"), "{err}");
    }

    #[test]
    fn error_unterminated_comment() {
        let err = parse_err("<!-- oops");
        assert!(err.message.contains("unterminated comment"), "{err}");
    }

    #[test]
    fn error_positions_are_line_and_column() {
        let err = parse_err("<div>\n  ${a.b}\n</div>");
        assert_eq!(err.line, 2);
        assert_eq!(err.column, 3);
        assert_eq!(err.offset, 8);
    }
}
