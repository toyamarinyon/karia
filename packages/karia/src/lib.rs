use cssparser::{Parser, ParserInput, Token};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};

pub mod tsx;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Definition {
    pub name: String,
    pub value: String,
    pub uri: String,
    pub start: usize,
    pub end: usize,
    pub context: String,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Diagnostic {
    pub uri: String,
    pub message: String,
    pub start: usize,
    pub end: usize,
    pub code: String,
}
#[derive(Default)]
pub struct Index {
    docs: BTreeMap<String, Document>,
}
enum Document {
    Css(CssDocument),
    Script(String),
}
impl Document {
    fn css(&self) -> Option<&CssDocument> {
        match self {
            Self::Css(doc) => Some(doc),
            Self::Script(_) => None,
        }
    }
}
#[derive(Default)]
struct CssDocument {
    text: String,
    variables: Vec<Definition>,
    classes: Vec<Definition>,
    uses: Vec<VariableUse>,
}
struct VariableUse {
    name: String,
    start: usize,
    end: usize,
    fallback: bool,
}
impl Index {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn update(&mut self, uri: &str, text: &str) {
        if matches!(
            std::path::Path::new(uri)
                .extension()
                .and_then(|ext| ext.to_str()),
            Some("ts" | "tsx" | "js" | "jsx")
        ) {
            self.docs
                .insert(uri.to_owned(), Document::Script(text.to_owned()));
            return;
        }
        let mut input = ParserInput::new(text);
        let mut parser = Parser::new(&mut input);
        let nodes = tokenize(&mut parser);
        let mut doc = CssDocument {
            text: text.to_owned(),
            ..CssDocument::default()
        };
        analyze(&nodes, text.len(), uri, text, &[], &mut doc);
        self.docs.insert(uri.to_owned(), Document::Css(doc));
    }
    pub fn remove(&mut self, uri: &str) {
        self.docs.remove(uri);
    }
    pub fn variables(&self) -> Vec<Definition> {
        self.docs
            .values()
            .filter_map(Document::css)
            .flat_map(|d| d.variables.iter().cloned())
            .collect()
    }
    pub fn classes(&self, uri: &str) -> Vec<Definition> {
        self.docs
            .get(uri)
            .and_then(Document::css)
            .map(|d| d.classes.clone())
            .unwrap_or_default()
    }
    pub fn inspect(&self, name: &str) -> Vec<Definition> {
        self.variables()
            .into_iter()
            .filter(|d| d.name == name)
            .collect()
    }
    pub fn diagnostics(&self, uri: &str) -> Vec<Diagnostic> {
        let known: HashSet<_> = self
            .docs
            .values()
            .filter_map(Document::css)
            .flat_map(|d| d.variables.iter().map(|v| v.name.as_str()))
            .collect();
        self.docs
            .get(uri)
            .and_then(Document::css)
            .map(|d| {
                d.uses
                    .iter()
                    .filter(|v| !v.fallback && !known.contains(v.name.as_str()))
                    .map(|v| Diagnostic {
                        uri: uri.to_owned(),
                        message: format!("No declaration for {} in the indexed CSS files", v.name),
                        start: v.start,
                        end: v.end,
                        code: "unknown-custom-property".into(),
                    })
                    .collect()
            })
            .unwrap_or_default()
    }
    /// Resolve a stored script at a UTF-16 cursor offset. Returned ranges use UTF-8 bytes.
    pub fn module_access(&self, uri: &str, offset: usize) -> Option<tsx::ModuleAccess> {
        match self.docs.get(uri)? {
            Document::Script(text) => tsx::module_access(text, offset),
            Document::Css(_) => None,
        }
    }
    pub fn check_text(&self, uri: &str) -> Option<&str> {
        self.docs.get(uri).map(|d| match d {
            Document::Css(doc) => doc.text.as_str(),
            Document::Script(text) => text.as_str(),
        })
    }
}

#[derive(Debug)]
struct Node {
    kind: Kind,
    start: usize,
    end: usize,
}
#[derive(Debug)]
enum Kind {
    Ident(String),
    Delim(char),
    Colon,
    Semicolon,
    Comma,
    Function(String, Vec<Node>),
    Block(Vec<Node>),
    Group(Vec<Node>),
    Other,
}
// Nested blocks are consumed by cssparser, so strings, escapes, comments and
// punctuation inside functions never masquerade as selectors/declarations.
fn tokenize(parser: &mut Parser<'_, '_>) -> Vec<Node> {
    let mut out = Vec::new();
    loop {
        let start = parser.position().byte_index();
        let Ok(token) = parser.next_including_whitespace_and_comments().cloned() else {
            break;
        };
        let kind = match token {
            Token::WhiteSpace(_) | Token::Comment(_) => continue,
            Token::Ident(x) => Kind::Ident(x.to_string()),
            Token::Delim(c) => Kind::Delim(c),
            Token::Colon => Kind::Colon,
            Token::Semicolon => Kind::Semicolon,
            Token::Comma => Kind::Comma,
            Token::Function(name) => {
                let name = name.to_string();
                let children = parser
                    .parse_nested_block(|p| Ok::<_, cssparser::ParseError<'_, ()>>(tokenize(p)))
                    .unwrap_or_default();
                Kind::Function(name, children)
            }
            Token::CurlyBracketBlock => {
                let children = parser
                    .parse_nested_block(|p| Ok::<_, cssparser::ParseError<'_, ()>>(tokenize(p)))
                    .unwrap_or_default();
                Kind::Block(children)
            }
            Token::ParenthesisBlock | Token::SquareBracketBlock => {
                let children = parser
                    .parse_nested_block(|p| Ok::<_, cssparser::ParseError<'_, ()>>(tokenize(p)))
                    .unwrap_or_default();
                Kind::Group(children)
            }
            _ => Kind::Other,
        };
        out.push(Node {
            kind,
            start,
            end: parser.position().byte_index(),
        });
    }
    out
}
fn declaration(nodes: &[Node]) -> bool {
    matches!(nodes.first().map(|n| &n.kind), Some(Kind::Ident(_)))
        && matches!(nodes.get(1).map(|n| &n.kind), Some(Kind::Colon))
}
fn analyze(
    nodes: &[Node],
    end: usize,
    uri: &str,
    text: &str,
    contexts: &[String],
    doc: &mut CssDocument,
) {
    let mut from = 0;
    for (i, node) in nodes.iter().enumerate() {
        match &node.kind {
            Kind::Semicolon => {
                add_declaration(&nodes[from..i], node.start, uri, text, contexts, doc);
                from = i + 1;
            }
            Kind::Block(body) if !declaration(&nodes[from..i]) => {
                let head = &nodes[from..i];
                let Some(first) = head.first() else {
                    from = i + 1;
                    continue;
                };
                let label = text[first.start..node.start].trim().to_owned();
                let mut nested = contexts.to_vec();
                nested.push(label.clone());
                if !label.starts_with('@') {
                    let mut found = Vec::new();
                    selector_classes(head, false, &mut found);
                    for (name, start, finish) in found {
                        doc.classes.push(Definition {
                            name,
                            uri: uri.to_owned(),
                            start,
                            end: finish,
                            value: text
                                [node.start + 1..node.end.saturating_sub(1).max(node.start + 1)]
                                .trim()
                                .to_owned(),
                            context: nested.join(" → "),
                        });
                    }
                }
                analyze(body, node.end.saturating_sub(1), uri, text, &nested, doc);
                from = i + 1;
            }
            _ => {}
        }
    }
    add_declaration(&nodes[from..], end, uri, text, contexts, doc);
}
fn add_declaration(
    nodes: &[Node],
    end: usize,
    uri: &str,
    text: &str,
    contexts: &[String],
    doc: &mut CssDocument,
) {
    if !declaration(nodes) {
        return;
    }
    let Kind::Ident(name) = &nodes[0].kind else {
        return;
    };
    if name.starts_with("--") {
        doc.variables.push(Definition {
            name: name.clone(),
            uri: uri.to_owned(),
            start: nodes[0].start,
            end: nodes[0].end,
            value: text[nodes[1].end..end].trim().to_owned(),
            context: contexts.join(" → "),
        });
    }
    variable_uses(&nodes[2..], &mut doc.uses);
}
fn variable_uses(nodes: &[Node], out: &mut Vec<VariableUse>) {
    for node in nodes {
        match &node.kind {
            Kind::Function(name, body) => {
                if name.eq_ignore_ascii_case("var")
                    && let Some(first) = body.first()
                    && let Kind::Ident(name) = &first.kind
                {
                    out.push(VariableUse {
                        name: name.clone(),
                        start: first.start,
                        end: first.end,
                        fallback: body.iter().any(|n| matches!(n.kind, Kind::Comma)),
                    });
                }
                variable_uses(body, out);
            }
            Kind::Group(body) | Kind::Block(body) => variable_uses(body, out),
            _ => {}
        }
    }
}
fn selector_classes(nodes: &[Node], initial_global: bool, out: &mut Vec<(String, usize, usize)>) {
    let mut global = initial_global;
    for (i, node) in nodes.iter().enumerate() {
        match &node.kind {
            Kind::Comma => global = initial_global,
            Kind::Ident(name) if i > 0 && matches!(nodes[i - 1].kind, Kind::Colon) => {
                if name == "global" {
                    global = true;
                } else if name == "local" {
                    global = false;
                }
            }
            Kind::Delim('.') if !global => {
                if let Some(next) = nodes.get(i + 1)
                    && let Kind::Ident(name) = &next.kind
                {
                    out.push((name.clone(), next.start, next.end));
                }
            }
            Kind::Function(name, body) => {
                if name == "global" {
                    continue;
                }
                selector_classes(body, if name == "local" { false } else { global }, out);
            }
            // Attribute selector strings such as [href="a.foo"] are not classes.
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn script_documents_are_not_css_and_resolve_only_stored_source() {
        let mut idx = Index::new();
        idx.update("theme.css", ".card { --theme: red; color: var(--missing) }");
        for extension in ["ts", "tsx", "js", "jsx"] {
            let uri = format!("file:///app.{extension}");
            idx.update(&uri, ".fake { --missing: blue; color: var(--other) }");
            assert!(idx.classes(&uri).is_empty());
            assert!(idx.diagnostics(&uri).is_empty());
            assert!(idx.inspect("--missing").is_empty());
            assert_eq!(idx.diagnostics("theme.css").len(), 1);

            let text = "/* 🌿 */ import styles from './a.module.css'; styles.card;";
            let offset = text[..text.find("card").unwrap() + 2]
                .encode_utf16()
                .count();
            idx.update(&uri, text);
            let access = idx.module_access(&uri, offset).unwrap();
            assert_eq!(access.name, "card");
            assert_eq!(&text[access.start..access.end], "card");
            idx.update(&uri, &text.replace("card", "hero"));
            assert_eq!(idx.module_access(&uri, offset).unwrap().name, "hero");
            idx.remove(&uri);
            assert!(idx.module_access(&uri, offset).is_none());
            assert!(idx.check_text(&uri).is_none());
        }
        assert_eq!(idx.classes("theme.css")[0].name, "card");
        assert_eq!(idx.variables().len(), 1);
        assert!(idx.module_access("theme.css", 0).is_none());
        idx.remove("theme.css");
        assert!(idx.variables().is_empty());
    }
    #[test]
    fn unicode_replacement_and_semicolonless_declarations() {
        let mut idx = Index::new();
        let text = "/* 🌿 */ .日本 { /* token */ --色: red }";
        idx.update("a.css", text);
        let v = idx.variables();
        assert_eq!(v[0].name, "--色");
        assert_eq!(v[0].value, "red");
        assert_eq!(&text[v[0].start..v[0].end], "--色");
        idx.update("a.css", ".new {}");
        assert!(idx.variables().is_empty());
        assert_eq!(idx.classes("a.css")[0].name, "new");
    }
    #[test]
    fn tokens_distinguish_comments_strings_globals_and_functions() {
        let mut idx = Index::new();
        idx.update("a.css", "/* .fake {} */ .card, .button:hover { content: '.fake { --bad: x; }'; --ok: url('data:a;b'); } :global(.nope .also) .local {} :global .hidden {} :local(.yes) {} [href='.fake'] .real {}");
        assert_eq!(idx.variables().len(), 1);
        assert_eq!(
            idx.classes("a.css")
                .iter()
                .map(|d| d.name.as_str())
                .collect::<Vec<_>>(),
            vec!["card", "button", "local", "yes", "real"]
        );
        assert!(idx.classes("a.css")[0].value.contains("content:"));
    }
    #[test]
    fn nested_context_restores_outer_rule() {
        let mut idx = Index::new();
        idx.update(
            "a.css",
            "@media (width > 10px) { .card { --a: red; &:hover { --a: blue } --b: white } }",
        );
        assert_eq!(idx.inspect("--a").len(), 2);
        assert_eq!(
            idx.inspect("--b")[0].context,
            "@media (width > 10px) → .card"
        );
    }
    #[test]
    fn diagnostics_use_workspace_definitions_and_fallbacks() {
        let mut idx = Index::new();
        idx.update("a.css", ".a { color: var(--missing); background: var(--optional, red); padding: var(--space); content: 'var(--fake)'; }");
        idx.update("b.css", ":root { --space: 12px }");
        let d = idx.diagnostics("a.css");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("--missing"));
        idx.remove("b.css");
        assert_eq!(idx.diagnostics("a.css").len(), 2);
    }
    #[test]
    fn escaped_identifiers_and_dash_class_are_classes() {
        let mut idx = Index::new();
        idx.update("a.css", ".foo\\:bar, .--dash { --x: 1 }");
        assert_eq!(idx.classes("a.css")[0].name, "foo:bar");
        assert_eq!(idx.classes("a.css")[1].name, "--dash");
        assert_eq!(idx.variables().len(), 1);
    }
}
