//! Cursor context over cssparser tokens. All positions are UTF-8 byte offsets.
use cssparser::{Parser, ParserInput, Token};
use serde::Serialize;

#[derive(Debug, Serialize, Default)]
pub struct CssContext {
    pub token: Option<ContextToken>,
    pub variable: Option<Variable>,
    pub completion: Option<Completion>,
}
#[derive(Debug, Serialize)]
pub struct ContextToken {
    pub kind: &'static str,
    pub start: usize,
    pub end: usize,
}
#[derive(Debug, Serialize)]
pub struct Variable {
    pub name: String,
    pub start: usize,
    pub end: usize,
}
#[derive(Debug, Serialize)]
pub struct Completion {
    pub prefix: String,
    pub start: usize,
    pub end: usize,
}
struct Node {
    kind: Kind,
    start: usize,
    end: usize,
    inner_start: usize,
    inner_end: usize,
}
enum Kind {
    Ident(String),
    AtKeyword,
    Function(String, Vec<Node>),
    Block(Vec<Node>),
    Group(Vec<Node>),
    Excluded(&'static str),
    Trivia,
    Colon,
    Separator,
    Other,
}
fn tokens(parser: &mut Parser<'_, '_>) -> Vec<Node> {
    let mut nodes = Vec::new();
    loop {
        let start = parser.position().byte_index();
        let Ok(token) = parser.next_including_whitespace_and_comments().cloned() else {
            break;
        };
        let inner_start = parser.position().byte_index();
        let mut inner_end = inner_start;
        let kind = match token {
            Token::Ident(name) => Kind::Ident(name.to_string()),
            Token::AtKeyword(_) => Kind::AtKeyword,
            Token::WhiteSpace(_) => Kind::Trivia,
            Token::Comment(_) => Kind::Excluded("comment"),
            Token::QuotedString(_) | Token::BadString(_) => Kind::Excluded("string"),
            Token::UnquotedUrl(_) | Token::BadUrl(_) => Kind::Excluded("url"),
            Token::Colon => Kind::Colon,
            Token::Comma | Token::Semicolon => Kind::Separator,
            Token::Function(_)
            | Token::CurlyBracketBlock
            | Token::ParenthesisBlock
            | Token::SquareBracketBlock => {
                let children = parser
                    .parse_nested_block(|p| {
                        let children = tokens(p);
                        inner_end = p.position().byte_index();
                        Ok::<_, cssparser::ParseError<'_, ()>>(children)
                    })
                    .unwrap_or_default();
                match token {
                    Token::Function(name) => Kind::Function(name.to_string(), children),
                    Token::CurlyBracketBlock => Kind::Block(children),
                    _ => Kind::Group(children),
                }
            }
            _ => Kind::Other,
        };
        nodes.push(Node {
            kind,
            start,
            end: parser.position().byte_index(),
            inner_start,
            inner_end,
        });
    }
    nodes
}
fn contains(node: &Node, offset: usize) -> bool {
    node.start <= offset && offset <= node.end
}
fn token_at(nodes: &[Node], offset: usize, text: &str) -> Option<ContextToken> {
    nodes.iter().find_map(|node| {
        // A completed comment is trivia at its right edge, including at EOF.
        // Unterminated comments continue to own the EOF cursor.
        if offset == node.end
            && matches!(node.kind, Kind::Excluded("comment"))
            && text[node.start..node.end].ends_with("*/")
        {
            return None;
        }
        if !(node.start <= offset
            && (offset < node.end
                || offset == node.end
                    && (node.end == text.len() || matches!(node.kind, Kind::Ident(_)))))
        {
            return None;
        }
        let kind = match &node.kind {
            Kind::Function(_, children) | Kind::Block(children) | Kind::Group(children) => {
                if let Some(token) = token_at(children, offset, text) {
                    return Some(token);
                }
                "block"
            }
            Kind::Ident(_) => "identifier",
            Kind::Excluded(kind) => kind,
            Kind::Trivia => "whitespace",
            _ => "punctuation",
        };
        Some(ContextToken {
            kind,
            start: node.start,
            end: node.end,
        })
    })
}
fn variable(node: &Node, offset: usize) -> Option<Variable> {
    if let Kind::Ident(name) = &node.kind
        && name.starts_with("--")
        && contains(node, offset)
    {
        Some(Variable {
            name: name.clone(),
            start: node.start,
            end: node.end,
        })
    } else {
        None
    }
}
fn prefix(text: &str) -> String {
    let mut input = ParserInput::new(text);
    let mut parser = Parser::new(&mut input);
    match parser.next() {
        Ok(Token::Ident(name)) => name.to_string(),
        _ => text.to_owned(),
    }
}
fn visit(nodes: &[Node], text: &str, offset: usize, declarations: bool, result: &mut CssContext) {
    let significant: Vec<_> = nodes
        .iter()
        .filter(|n| !matches!(n.kind, Kind::Trivia) && !matches!(n.kind, Kind::Excluded("comment")))
        .collect();
    let mut statement_start = true;
    let mut at_rule = false;
    for (index, node) in significant.iter().enumerate() {
        if statement_start {
            at_rule = matches!(node.kind, Kind::AtKeyword);
        }
        if declarations
            && statement_start
            && significant
                .get(index + 1)
                .is_some_and(|n| matches!(n.kind, Kind::Colon))
        {
            result.variable = variable(node, offset).or(result.variable.take());
        }
        statement_start = matches!(node.kind, Kind::Separator | Kind::Block(_));
        match &node.kind {
            Kind::Function(name, children) => {
                if name.eq_ignore_ascii_case("var")
                    && node.inner_start <= offset
                    && offset <= node.inner_end
                {
                    let first = children.iter().find(|n| {
                        !matches!(n.kind, Kind::Trivia)
                            && !matches!(n.kind, Kind::Excluded("comment"))
                    });
                    match first {
                        Some(first)
                            if matches!(first.kind, Kind::Ident(_)) && contains(first, offset) =>
                        {
                            result.variable = variable(first, offset).or(result.variable.take());
                            result.completion = Some(Completion {
                                prefix: prefix(&text[first.start..offset]),
                                start: first.start,
                                end: first.end,
                            });
                        }
                        Some(first) if offset <= first.start => {
                            result.completion = Some(Completion {
                                prefix: String::new(),
                                start: offset,
                                end: offset,
                            });
                        }
                        None => {
                            result.completion = Some(Completion {
                                prefix: String::new(),
                                start: offset,
                                end: offset,
                            })
                        }
                        _ => {}
                    }
                }
                visit(children, text, offset, false, result);
            }
            // Grouping at-rules preserve whether their containing block permits
            // declarations. Qualified rules introduce a declaration block.
            Kind::Block(children) => {
                visit(children, text, offset, !at_rule || declarations, result)
            }
            Kind::Group(children) => visit(children, text, offset, false, result),
            _ => {}
        }
    }
}
pub fn context(text: &str, offset: usize) -> Option<CssContext> {
    if offset > text.len() || !text.is_char_boundary(offset) {
        return None;
    }
    let mut input = ParserInput::new(text);
    let nodes = tokens(&mut Parser::new(&mut input));
    let mut result = CssContext {
        token: token_at(&nodes, offset, text),
        ..CssContext::default()
    };
    if !result
        .token
        .as_ref()
        .is_some_and(|token| matches!(token.kind, "comment" | "string" | "url"))
    {
        visit(&nodes, text, offset, false, &mut result);
    }
    Some(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn at(source: &str) -> CssContext {
        let offset = source.find('|').unwrap();
        context(&source.replace('|', ""), offset).unwrap()
    }
    #[test]
    fn variable_tokens_and_nested_functions() {
        let c = at("@media (width > 0) {.é { --th|ème: red; }}");
        assert_eq!(c.variable.unwrap().name, "--thème");
        assert!(
            at("@media screen { --ac|cent:hover {} }")
                .variable
                .is_none()
        );
        assert!(
            at("a { @media screen { --ac|cent: red; } }")
                .variable
                .is_some()
        );
        let c = at("a { color: calc(1 + var(--the|me, rgb(1,2,3))); }");
        assert_eq!(c.variable.unwrap().name, "--theme");
        assert_eq!(c.completion.unwrap().prefix, "--the");
        assert!(at("a{ color: var(--x, --fa|llback) }").completion.is_none());
        assert!(
            at("a{ color: var(--x, var(--ne|sted)) }")
                .completion
                .is_some()
        );
    }
    #[test]
    fn indexed_overlays_and_serialized_insertions() {
        let mut index = crate::Index::new();
        index.update("a.css", "a{--a\\.b:red;color:var(--a\\.b)}");
        assert_eq!(index.variables()[0].insertion_text, "--a\\.b");
        let offset = index.check_text("a.css").unwrap().find("var(").unwrap() + 4;
        assert!(
            index
                .css_context("a.css", offset)
                .unwrap()
                .completion
                .is_some()
        );
        index.update("a.css", "/* edited */");
        assert!(index.css_context("a.css", 3).unwrap().completion.is_none());
        index.remove("a.css");
        assert!(index.css_context("a.css", 3).is_none());
        let c = at("a{content:'abc\\'|");
        assert_eq!(c.token.unwrap().kind, "string");
        assert!(c.completion.is_none());
    }
    #[test]
    fn escapes_and_unicode_ranges() {
        let source = "é{color:var(--\\74 he|me)}";
        let c = at(source);
        let variable = c.variable.unwrap();
        assert_eq!(variable.name, "--theme");
        assert_eq!(variable.start, source.find("--").unwrap());
        assert_eq!(c.completion.unwrap().prefix, "--the");
        assert!(context("é", 1).is_none());
    }
    #[test]
    fn comments_strings_and_incomplete_input() {
        for source in [
            "a{color:var(/* --x| */)}",
            "a{content:'var(--x|)'}",
            "a{color:var(\"--x|\")}",
            "/* var(--x|",
            "a{content:'var(--x|",
        ] {
            let c = at(source);
            assert!(c.variable.is_none(), "{source}");
            assert!(c.completion.is_none(), "{source}");
        }
        for source in [
            "a{color:var(|",
            "a{color:var( /*hello*/ |",
            "a{color:var(/*hello*/|",
            "a{color:var(--x|",
        ] {
            assert!(at(source).completion.is_some(), "{source}");
        }
        assert!(at("a{color:var(--x) |}").completion.is_none());
        assert!(at("a{color:var(--x, |}").completion.is_none());
    }
}
