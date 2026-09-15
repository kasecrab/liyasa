//! The stylesheet compiler (THM-30): nesting, custom media, minification, and
//! the split between the cached stylesheet and the per-page critical block.
//!
//! §6.2.1 names lightningcss for this step. It is MPL-2.0 and `deny.toml`'s
//! allow list, which WP-00 owns, does not carry that licence, so the gate
//! rejects the dependency today. The transforms the theme actually authors
//! against — `&` nesting, `@custom-media`, and minification — are implemented
//! here behind [`Compiler`], and swapping lightningcss in once the licence row
//! exists is one impl of that trait.
// TODO(rfc-0009): replace the built-in transforms with lightningcss.

use std::fmt::Write as _;

/// One parsed rule: a style rule, or an at-rule with or without a block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rule {
    /// The selector, or the at-rule's name and prelude (`@media (min-width: 0)`).
    pub prelude: String,
    pub block: Option<Block>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Block {
    pub declarations: Vec<Declaration>,
    pub rules: Vec<Rule>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Declaration {
    pub property: String,
    pub value: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Stylesheet {
    pub rules: Vec<Rule>,
}

impl Rule {
    pub fn at_name(&self) -> Option<&str> {
        let rest = self.prelude.strip_prefix('@')?;
        Some(rest.split_whitespace().next().unwrap_or(rest))
    }
}

impl Stylesheet {
    /// Parses a stylesheet. Anything the parser does not understand is carried
    /// through unchanged rather than dropped: a theme that silently loses a
    /// rule is worse than one that emits a rule the browser ignores.
    pub fn parse(css: &str) -> Self {
        let mut parser = Parser {
            bytes: css.as_bytes(),
            at: 0,
        };
        Self {
            rules: parser.rules(false),
        }
    }

    /// Every `--*` declaration, with the scope it was written in. Used to merge
    /// `theme/tokens.css` into the generated tokens (THM-10).
    pub fn custom_properties(&self) -> Vec<(Scope, Declaration)> {
        let mut out = Vec::new();
        collect_properties(&self.rules, Scope::Both, &mut out);
        out
    }

    /// Replaces `@custom-media --name <query>;` definitions and their uses.
    pub fn expand_custom_media(&mut self) {
        let mut queries = Vec::new();
        self.rules.retain(|rule| {
            if rule.at_name() != Some("custom-media") || rule.block.is_some() {
                return true;
            }
            let rest = rule.prelude.trim_start_matches("@custom-media").trim();
            if let Some((name, query)) = rest.split_once(char::is_whitespace) {
                queries.push((format!("({})", name.trim()), query.trim().to_owned()));
            }
            false
        });
        substitute(&mut self.rules, &queries);
    }

    /// Resolves `&` nesting into flat rules, in source order.
    pub fn flatten(&self) -> Vec<Rule> {
        let mut out = Vec::new();
        flatten_rules(&self.rules, "", &mut out);
        out
    }

    pub fn to_css(&self) -> String {
        let mut out = String::new();
        write_rules(&self.rules, 0, &mut out);
        out
    }

    /// Minified, deterministic output: one rule per line, no comments, no
    /// spacing the browser does not need.
    pub fn minify(&self) -> String {
        let mut out = String::new();
        for rule in self.flatten() {
            write_minified(&rule, &mut out);
        }
        out
    }
}

/// Which scheme a declaration in an override file applies to. A declaration
/// written once outside a scheme block applies to both (THM-12).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    Both,
    Light,
    Dark,
}

fn collect_properties(rules: &[Rule], scope: Scope, out: &mut Vec<(Scope, Declaration)>) {
    for rule in rules {
        let Some(block) = &rule.block else { continue };
        let scope = scope_of(&rule.prelude, scope);
        for declaration in &block.declarations {
            if declaration.property.starts_with("--") {
                out.push((scope, declaration.clone()));
            }
        }
        collect_properties(&block.rules, scope, out);
    }
}

/// A dark scope is either the attribute the toggle sets or the media query the
/// system preference matches; everything else keeps the scope it inherited.
fn scope_of(prelude: &str, inherited: Scope) -> Scope {
    let lower = prelude.to_ascii_lowercase();
    if lower.contains("prefers-color-scheme: dark") || lower.contains("data-theme=\"dark\"") {
        return Scope::Dark;
    }
    if lower.contains("prefers-color-scheme: light") || lower.contains("data-theme=\"light\"") {
        return Scope::Light;
    }
    inherited
}

fn substitute(rules: &mut [Rule], queries: &[(String, String)]) {
    for rule in rules {
        if rule.at_name() == Some("media") {
            for (name, query) in queries {
                if rule.prelude.contains(name.as_str()) {
                    rule.prelude = rule.prelude.replace(name.as_str(), query);
                }
            }
        }
        if let Some(block) = &mut rule.block {
            substitute(&mut block.rules, queries);
        }
    }
}

fn flatten_rules(rules: &[Rule], parent: &str, out: &mut Vec<Rule>) {
    for rule in rules {
        let Some(block) = &rule.block else {
            out.push(rule.clone());
            continue;
        };
        if rule.prelude.starts_with('@') {
            let mut inner = Vec::new();
            flatten_rules(&block.rules, parent, &mut inner);
            // Declarations directly inside an at-rule (`@font-face`) stay put.
            let mut nested = Block {
                declarations: block.declarations.clone(),
                rules: Vec::new(),
            };
            if !nested.declarations.is_empty() && !parent.is_empty() {
                out.push(Rule {
                    prelude: rule.prelude.clone(),
                    block: Some(Block {
                        declarations: Vec::new(),
                        rules: vec![Rule {
                            prelude: parent.to_owned(),
                            block: Some(std::mem::take(&mut nested)),
                        }],
                    }),
                });
            }
            let mut rules = inner;
            if !nested.declarations.is_empty() {
                rules.insert(
                    0,
                    Rule {
                        prelude: String::new(),
                        block: Some(nested),
                    },
                );
            }
            if !rules.is_empty() {
                out.push(Rule {
                    prelude: rule.prelude.clone(),
                    block: Some(Block {
                        declarations: Vec::new(),
                        rules,
                    }),
                });
            }
            continue;
        }

        let selector = resolve_selector(&rule.prelude, parent);
        if !block.declarations.is_empty() {
            out.push(Rule {
                prelude: selector.clone(),
                block: Some(Block {
                    declarations: block.declarations.clone(),
                    rules: Vec::new(),
                }),
            });
        }
        flatten_rules(&block.rules, &selector, out);
    }
}

/// `&` is replaced by the parent selector; a nested selector without one is a
/// descendant. Each comma-separated part is resolved on its own.
fn resolve_selector(selector: &str, parent: &str) -> String {
    if parent.is_empty() {
        return selector.trim().to_owned();
    }
    selector
        .split(',')
        .map(|part| {
            let part = part.trim();
            if part.contains('&') {
                part.replace('&', parent)
            } else {
                format!("{parent} {part}")
            }
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn write_rules(rules: &[Rule], depth: usize, out: &mut String) {
    let indent = "  ".repeat(depth);
    for rule in rules {
        let Some(block) = &rule.block else {
            let _ = writeln!(out, "{indent}{};", rule.prelude);
            continue;
        };
        let _ = writeln!(out, "{indent}{} {{", rule.prelude);
        for declaration in &block.declarations {
            let _ = writeln!(
                out,
                "{indent}  {}: {};",
                declaration.property, declaration.value
            );
        }
        write_rules(&block.rules, depth + 1, out);
        let _ = writeln!(out, "{indent}}}");
    }
}

fn write_minified(rule: &Rule, out: &mut String) {
    let Some(block) = &rule.block else {
        let _ = write!(out, "{};", compact(&rule.prelude));
        return;
    };
    if !block.rules.is_empty() {
        let _ = write!(out, "{}{{", compact(&rule.prelude));
        for inner in &block.rules {
            if inner.prelude.is_empty() {
                write_declarations(&inner.block.clone().unwrap_or_default(), out);
            } else {
                write_minified(inner, out);
            }
        }
        out.push('}');
        return;
    }
    if block.declarations.is_empty() {
        return;
    }
    let _ = write!(out, "{}{{", compact(&rule.prelude));
    write_declarations(block, out);
    out.push('}');
}

fn write_declarations(block: &Block, out: &mut String) {
    let mut first = true;
    for declaration in &block.declarations {
        if !first {
            out.push(';');
        }
        first = false;
        let _ = write!(
            out,
            "{}:{}",
            compact(&declaration.property),
            compact(&declaration.value)
        );
    }
}

/// Collapses runs of whitespace outside strings, and the space after a comma or
/// a combinator, which is all a stylesheet this theme authors needs.
fn compact(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut quote: Option<char> = None;
    let mut space = false;
    for c in text.chars() {
        if let Some(open) = quote {
            out.push(c);
            if c == open {
                quote = None;
            }
            continue;
        }
        match c {
            '"' | '\'' => {
                quote = Some(c);
                out.push(c);
                space = false;
            }
            c if c.is_whitespace() => space = true,
            _ => {
                if space && !out.is_empty() && !ends_with_separator(&out) && c != ',' && c != ')' {
                    out.push(' ');
                }
                space = false;
                out.push(c);
            }
        }
    }
    out
}

fn ends_with_separator(text: &str) -> bool {
    matches!(text.chars().last(), Some(',' | '(' | ':' | '>' | '~' | '+'))
}

struct Parser<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl Parser<'_> {
    fn rules(&mut self, nested: bool) -> Vec<Rule> {
        let mut out = Vec::new();
        loop {
            self.skip_trivia();
            match self.peek() {
                None => break,
                Some(b'}') if nested => break,
                Some(b'}') => {
                    self.at += 1;
                    continue;
                }
                _ => {}
            }
            let start = self.at;
            let prelude = self.until_top_level(b"{;}");
            match self.peek() {
                Some(b'{') => {
                    self.at += 1;
                    let block = self.block();
                    out.push(Rule {
                        prelude: prelude.trim().to_owned(),
                        block: Some(block),
                    });
                }
                Some(b';') => {
                    self.at += 1;
                    let text = prelude.trim();
                    if !text.is_empty() {
                        out.push(Rule {
                            prelude: text.to_owned(),
                            block: None,
                        });
                    }
                }
                _ => {
                    if self.at == start {
                        self.at += 1;
                    }
                    break;
                }
            }
        }
        out
    }

    fn block(&mut self) -> Block {
        let mut block = Block::default();
        loop {
            self.skip_trivia();
            match self.peek() {
                None => break,
                Some(b'}') => {
                    self.at += 1;
                    break;
                }
                _ => {}
            }
            let start = self.at;
            let text = self.until_top_level(b"{;}");
            match self.peek() {
                Some(b'{') => {
                    self.at += 1;
                    let inner = self.block();
                    block.rules.push(Rule {
                        prelude: text.trim().to_owned(),
                        block: Some(inner),
                    });
                }
                Some(b';') | Some(b'}') | None => {
                    let ended = self.peek() == Some(b'}');
                    if self.peek() == Some(b';') {
                        self.at += 1;
                    }
                    let text = text.trim();
                    if let Some(declaration) = declaration(text) {
                        block.declarations.push(declaration);
                    } else if text.starts_with('@') {
                        block.rules.push(Rule {
                            prelude: text.to_owned(),
                            block: None,
                        });
                    }
                    if ended {
                        self.at += 1;
                        break;
                    }
                }
                _ => {
                    if self.at == start {
                        self.at += 1;
                    }
                }
            }
        }
        block
    }

    /// Reads to the first delimiter that is not inside a string, a comment, or
    /// parentheses.
    fn until_top_level(&mut self, delimiters: &[u8]) -> String {
        let start = self.at;
        let mut depth = 0usize;
        while let Some(byte) = self.peek() {
            match byte {
                b'"' | b'\'' => self.skip_string(byte),
                b'/' if self.bytes.get(self.at + 1) == Some(&b'*') => self.skip_comment(),
                b'(' => {
                    depth += 1;
                    self.at += 1;
                }
                b')' => {
                    depth = depth.saturating_sub(1);
                    self.at += 1;
                }
                byte if depth == 0 && delimiters.contains(&byte) => break,
                _ => self.at += 1,
            }
        }
        strip_comments(&String::from_utf8_lossy(&self.bytes[start..self.at]))
    }

    fn skip_string(&mut self, quote: u8) {
        self.at += 1;
        while let Some(byte) = self.peek() {
            self.at += 1;
            match byte {
                b'\\' => self.at += 1,
                byte if byte == quote => break,
                _ => {}
            }
        }
    }

    fn skip_comment(&mut self) {
        self.at += 2;
        while self.at < self.bytes.len() {
            if self.bytes[self.at] == b'*' && self.bytes.get(self.at + 1) == Some(&b'/') {
                self.at += 2;
                return;
            }
            self.at += 1;
        }
    }

    fn skip_trivia(&mut self) {
        loop {
            match self.peek() {
                Some(byte) if byte.is_ascii_whitespace() => self.at += 1,
                Some(b'/') if self.bytes.get(self.at + 1) == Some(&b'*') => self.skip_comment(),
                _ => return,
            }
        }
    }

    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.at).copied()
    }
}

fn strip_comments(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find("/*") {
        out.push_str(&rest[..start]);
        match rest[start + 2..].find("*/") {
            Some(end) => rest = &rest[start + 2 + end + 2..],
            None => return out,
        }
    }
    out.push_str(rest);
    out
}

fn declaration(text: &str) -> Option<Declaration> {
    if text.is_empty() || text.starts_with('@') {
        return None;
    }
    let (property, value) = text.split_once(':')?;
    let property = property.trim();
    if property.is_empty() || property.contains(['{', '}']) {
        return None;
    }
    Some(Declaration {
        property: property.to_owned(),
        value: value.trim().to_owned(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_declarations_selectors_and_at_rules() {
        let sheet = Stylesheet::parse(
            r#"
            /* a comment */
            :root { --ly-color-primary: #0a7cff; }
            @media (min-width: 40em) { .a { color: red; } }
            @import "other.css";
            "#,
        );
        assert_eq!(sheet.rules.len(), 3);
        assert_eq!(sheet.rules[0].prelude, ":root");
        assert_eq!(
            sheet.rules[0]
                .block
                .as_ref()
                .map(|block| block.declarations.len()),
            Some(1)
        );
        assert_eq!(sheet.rules[1].at_name(), Some("media"));
        assert_eq!(sheet.rules[2].prelude, "@import \"other.css\"");
    }

    #[test]
    fn a_semicolon_inside_a_value_does_not_split_a_declaration() {
        let sheet = Stylesheet::parse(r#".a { background: url("i.png?a=1;b=2"); color: red; }"#);
        let block = sheet.rules[0].block.as_ref().expect("a block");
        assert_eq!(block.declarations.len(), 2);
        assert_eq!(block.declarations[0].value, "url(\"i.png?a=1;b=2\")");
    }

    #[test]
    fn nesting_resolves_against_the_parent_selector() {
        let sheet = Stylesheet::parse(
            ".card { color: red; &:hover { color: blue; } .title { font-weight: 600; } }",
        );
        let flat = sheet.flatten();
        let selectors: Vec<&str> = flat.iter().map(|rule| rule.prelude.as_str()).collect();
        assert_eq!(selectors, vec![".card", ".card:hover", ".card .title"]);
    }

    #[test]
    fn nesting_inside_a_media_query_keeps_the_query() {
        let sheet = Stylesheet::parse(".a { @media (min-width: 40em) { color: red; } }");
        let css = sheet.minify();
        assert_eq!(css, "@media (min-width:40em){.a{color:red}}");
    }

    #[test]
    fn custom_media_is_expanded_and_its_definition_dropped() {
        let mut sheet = Stylesheet::parse(
            "@custom-media --tablet (min-width: 48em);\n@media (--tablet) { .a { color: red; } }",
        );
        sheet.expand_custom_media();
        let css = sheet.minify();
        assert!(!css.contains("custom-media"), "{css}");
        assert_eq!(css, "@media (min-width:48em){.a{color:red}}");
    }

    #[test]
    fn minification_keeps_strings_intact() {
        let sheet = Stylesheet::parse(
            ".a::after { content: \"a  b\"; font-family: \"Fira Sans\", sans-serif; }",
        );
        assert_eq!(
            sheet.minify(),
            ".a::after{content:\"a  b\";font-family:\"Fira Sans\",sans-serif}"
        );
    }

    #[test]
    fn custom_properties_carry_the_scheme_they_were_written_in() {
        let sheet = Stylesheet::parse(
            r#"
            :root { --ly-color-primary: #111111; }
            [data-theme="dark"] { --ly-color-primary: #eeeeee; }
            @media (prefers-color-scheme: dark) { :root { --ly-color-bg: #000000; } }
            .card { color: red; }
            "#,
        );
        let properties = sheet.custom_properties();
        assert_eq!(properties.len(), 3);
        assert_eq!(properties[0].0, Scope::Both);
        assert_eq!(properties[1].0, Scope::Dark);
        assert_eq!(properties[2].0, Scope::Dark);
        assert_eq!(properties[2].1.property, "--ly-color-bg");
    }

    #[test]
    fn an_unparseable_rule_is_carried_through_rather_than_dropped() {
        let sheet = Stylesheet::parse("@supports (display: grid) { .a { display: grid; } }");
        assert!(sheet.minify().contains("@supports (display:grid)"));
    }
}
