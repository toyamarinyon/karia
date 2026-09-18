//! Definition / reference / context index built on top of the per-file CSTs
//! in `syntax`. Each `update` keeps the file's syntax tree incrementally
//! current, then rebuilds the cheap extracted records from that tree.

use crate::syntax::{self, Document};
use crate::{Definition, Diagnostic};
use std::collections::btree_map::Entry;
use std::collections::{BTreeMap, HashSet};
use tree_sitter::{Node, Parser};

#[derive(Default)]
struct Extracted {
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

struct Indexed {
    syntax: Document,
    variables: Vec<Definition>,
    classes: Vec<Definition>,
    uses: Vec<VariableUse>,
}

pub struct Index {
    parser: Parser,
    docs: BTreeMap<String, Indexed>,
}

impl Default for Index {
    fn default() -> Self {
        Self::new()
    }
}

impl Index {
    pub fn new() -> Self {
        Index {
            parser: syntax::parser(),
            docs: BTreeMap::new(),
        }
    }
    pub fn update(&mut self, uri: &str, text: &str) {
        let indexed = match self.docs.entry(uri.to_owned()) {
            Entry::Occupied(mut e) => {
                e.get_mut().syntax.update(&mut self.parser, text);
                e.into_mut()
            }
            Entry::Vacant(e) => e.insert(Indexed {
                syntax: Document::parse(&mut self.parser, text),
                variables: Vec::new(),
                classes: Vec::new(),
                uses: Vec::new(),
            }),
        };
        let found = extract(&indexed.syntax, uri);
        indexed.variables = found.variables;
        indexed.classes = found.classes;
        indexed.uses = found.uses;
    }
    pub fn remove(&mut self, uri: &str) {
        self.docs.remove(uri);
    }
    pub fn variables(&self) -> Vec<Definition> {
        self.docs
            .values()
            .flat_map(|d| d.variables.iter().cloned())
            .collect()
    }
    pub fn classes(&self, uri: &str) -> Vec<Definition> {
        self.docs
            .get(uri)
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
            .flat_map(|d| d.variables.iter().map(|v| v.name.as_str()))
            .collect();
        self.docs
            .get(uri)
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
    pub fn check_text(&self, uri: &str) -> Option<&str> {
        self.docs.get(uri).map(|d| d.syntax.text.as_str())
    }
}

fn text_at<'a>(text: &'a str, node: Node) -> &'a str {
    &text[node.start_byte()..node.end_byte()]
}

fn trim_span(text: &str, start: usize, end: usize) -> (usize, usize) {
    let bytes = text.as_bytes();
    let mut s = start;
    let mut e = end;
    while s < e && bytes[s].is_ascii_whitespace() {
        s += 1;
    }
    while e > s && bytes[e - 1].is_ascii_whitespace() {
        e -= 1;
    }
    (s, e)
}

fn child_of<'a>(node: Node<'a>, kind: &str) -> Option<Node<'a>> {
    let mut cursor = node.walk();
    node.named_children(&mut cursor).find(|n| n.kind() == kind)
}

// CSS escape sequences: `\XY` hex (with optional trailing whitespace) maps to
// the code point, any other escaped character stands for itself.
fn unescape(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut chars = raw.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some(h) if h.is_ascii_hexdigit() => {
                let mut hex = String::from(h);
                for _ in 0..5 {
                    match chars.peek() {
                        Some(&d) if d.is_ascii_hexdigit() => {
                            hex.push(d);
                            chars.next();
                        }
                        _ => break,
                    }
                }
                if chars.peek().is_some_and(|w| w.is_whitespace()) {
                    chars.next();
                }
                match u32::from_str_radix(&hex, 16).ok().and_then(char::from_u32) {
                    Some(ch) => out.push(ch),
                    None => out.push('\u{fffd}'),
                }
            }
            Some('\n') => {}
            Some(other) => out.push(other),
            None => break,
        }
    }
    out
}

fn extract(doc: &Document, uri: &str) -> Extracted {
    let mut out = Extracted::default();
    let mut contexts = Vec::new();
    visit_children(doc.root(), &doc.text, uri, &mut contexts, &mut out);
    out
}

fn visit_children(
    node: Node,
    text: &str,
    uri: &str,
    contexts: &mut Vec<String>,
    out: &mut Extracted,
) {
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        visit_child(child, text, uri, contexts, out, false);
    }
}

fn visit_child(
    node: Node,
    text: &str,
    uri: &str,
    contexts: &mut Vec<String>,
    out: &mut Extracted,
    recovery: bool,
) {
    match node.kind() {
        "rule_set" => rule_set(node, text, uri, contexts, out),
        "declaration" => declaration(node, text, uri, contexts, out),
        "comment" | "string_value" => {}
        "ERROR" => recover(node, text, uri, contexts, out),
        _ => {
            if let Some(body) = body_node(node) {
                let label = text[node.start_byte()..body.start_byte()].trim().to_owned();
                contexts.push(label);
                visit_children(body, text, uri, contexts, out);
                contexts.pop();
            } else if recovery {
                selector_classes(
                    node,
                    text,
                    &mut Scope::default(),
                    &mut |name, start, end| {
                        out.classes.push(Definition {
                            name,
                            value: String::new(),
                            uri: uri.to_owned(),
                            start,
                            end,
                            context: contexts.join(" → "),
                        });
                    },
                );
                scan_uses(node, text, out);
            }
        }
    }
}

// `{ ... }` container bodies; `keyframe_block_list` plays the same role for
// `@keyframes` blocks.
fn body_node<'a>(node: Node<'a>) -> Option<Node<'a>> {
    let mut cursor = node.walk();
    node.named_children(&mut cursor)
        .find(|n| matches!(n.kind(), "block" | "keyframe_block_list"))
}

fn rule_set(node: Node, text: &str, uri: &str, contexts: &mut Vec<String>, out: &mut Extracted) {
    let selectors = child_of(node, "selectors");
    let body = body_node(node);
    let label_start = selectors
        .map(|s| s.start_byte())
        .unwrap_or(node.start_byte());
    let label_end = body.map(|b| b.start_byte()).unwrap_or(node.end_byte());
    let label = text[label_start..label_end].trim().to_owned();
    contexts.push(label.clone());
    if !label.starts_with('@')
        && let Some(sel) = selectors
    {
        let value = body.map(|b| inner_text(b, text)).unwrap_or_default();
        let context = contexts.join(" → ");
        selector_classes(sel, text, &mut Scope::default(), &mut |name, start, end| {
            out.classes.push(Definition {
                name,
                value: value.clone(),
                uri: uri.to_owned(),
                start,
                end,
                context: context.clone(),
            });
        });
    }
    if let Some(b) = body {
        visit_children(b, text, uri, contexts, out);
    }
    contexts.pop();
}

// Text between the braces of a `block` node, like the old token inner range.
fn inner_text(block: Node, text: &str) -> String {
    let mut cursor = block.walk();
    let open = block
        .children(&mut cursor)
        .find(|n| n.kind() == "{")
        .map(|n| n.end_byte())
        .unwrap_or(block.start_byte());
    let close = block
        .children(&mut cursor)
        .filter(|n| n.kind() == "}")
        .last()
        .map(|n| n.start_byte())
        .unwrap_or(block.end_byte());
    let close = close.max(open);
    text[open..close.min(text.len())].trim().to_owned()
}

fn declaration(node: Node, text: &str, uri: &str, contexts: &[String], out: &mut Extracted) {
    let Some(prop) = child_of(node, "property_name") else {
        return;
    };
    let mut cursor = node.walk();
    let Some(colon) = node.children(&mut cursor).find(|n| n.kind() == ":") else {
        return;
    };
    let (name_start, name_end) = trim_span(text, prop.start_byte(), colon.start_byte());
    let name = unescape(&text[name_start..name_end]);
    let semi = node.children(&mut cursor).find(|n| n.kind() == ";");
    let value_end = semi.map(|s| s.start_byte()).unwrap_or(node.end_byte());
    if name.starts_with("--") {
        out.variables.push(Definition {
            name,
            value: text[colon.end_byte()..value_end].trim().to_owned(),
            uri: uri.to_owned(),
            start: name_start,
            end: name_end,
            context: contexts.join(" → "),
        });
    }
    scan_uses(node, text, out);
}

// `var(--name)` references anywhere inside a declaration value; nested calls
// (e.g. `calc`, `url`, fallback chains) are walked as well.
fn scan_uses(node: Node, text: &str, out: &mut Extracted) {
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        match child.kind() {
            "call_expression" => {
                let args = child_of(child, "arguments");
                if child_of(child, "function_name")
                    .map(|f| text_at(text, f).eq_ignore_ascii_case("var"))
                    .unwrap_or(false)
                    && let Some(args) = args
                    && let Some(first) = first_arg(args, text)
                {
                    let raw = text_at(text, first);
                    let fallback = args.children(&mut args.walk()).any(|n| n.kind() == ",");
                    out.uses.push(VariableUse {
                        name: unescape(raw.strip_suffix(':').unwrap_or(raw)),
                        start: first.start_byte(),
                        end: first.end_byte(),
                        fallback,
                    });
                }
                if let Some(args) = args {
                    scan_uses(args, text, out);
                }
            }
            "comment" | "string_value" => {}
            _ => scan_uses(child, text, out),
        }
    }
}

// Nodes that can carry an identifier — a `var(--x)` argument, a `.x` class
// name or a `--x` property name, in valid code or a broken region alike —
// are matched by their text so compound leaves like `class_name` work too.
fn name_token<'a>(text: &'a str, node: Node) -> Option<&'a str> {
    if !node.is_named() || node.kind() == "comment" {
        return None;
    }
    let t = &text[node.start_byte()..node.end_byte()];
    if t.is_empty()
        || t.contains(|c: char| {
            c.is_whitespace()
                || matches!(
                    c,
                    ':' | ';' | '(' | ')' | '{' | '}' | '[' | ']' | ',' | '.' | '"' | '\''
                )
        })
    {
        return None;
    }
    Some(t)
}

fn first_arg<'a>(args: Node<'a>, text: &str) -> Option<Node<'a>> {
    let mut cursor = args.walk();
    args.named_children(&mut cursor)
        .find(|n| name_token(text, *n).is_some())
}

// Whether `.name` classes in this selector region are in scope (local) or
// hidden behind `:global`. `reset` is the scope the region started in, used
// when a comma begins a new selector.
#[derive(Clone, Copy, Default)]
struct Scope {
    reset: bool,
    global: bool,
}

fn selector_classes(
    node: Node,
    text: &str,
    scope: &mut Scope,
    emit: &mut impl FnMut(String, usize, usize),
) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            "," => scope.global = scope.reset,
            "class_name" => {
                if !scope.global {
                    emit(
                        unescape(text_at(text, child)),
                        child.start_byte(),
                        child.end_byte(),
                    );
                }
            }
            "pseudo_class_selector" => pseudo(child, text, scope, emit),
            "attribute_selector" | "string_value" | "comment" => {}
            _ => selector_classes(child, text, scope, emit),
        }
    }
}

// `:global` / `:local` scoping for CSS Modules. A pseudo class may embed its
// own compound selector (`.a:has(.b)`) and the pseudo name is a direct
// `class_name` child, which must not be emitted as a class itself.
fn pseudo(node: Node, text: &str, scope: &mut Scope, emit: &mut impl FnMut(String, usize, usize)) {
    let name = child_of(node, "class_name").map(|n| text_at(text, n).to_owned());
    let mut cursor = node.walk();
    let mut has_args = false;
    for child in node.children(&mut cursor) {
        match child.kind() {
            "class_name" => {}
            "arguments" => {
                has_args = true;
                match name.as_deref() {
                    Some("global") => {}
                    Some("local") => {
                        selector_classes(child, text, &mut Scope::default(), emit);
                    }
                    _ => {
                        let mut inner = *scope;
                        selector_classes(child, text, &mut inner, emit);
                    }
                }
            }
            _ => selector_classes(child, text, scope, emit),
        }
    }
    if !has_args {
        match name.as_deref() {
            Some("global") => scope.global = true,
            Some("local") => scope.global = false,
            _ => {}
        }
    }
}

// Best-effort extraction inside ERROR nodes for incomplete / invalid CSS,
// mirroring the old token stream behaviour: `--x: v` declarations, `.x`
// classes and `var(--x` references are recovered from the flat child atoms.
fn recover(err: Node, text: &str, uri: &str, contexts: &mut Vec<String>, out: &mut Extracted) {
    let mut cursor = err.walk();
    let children: Vec<Node> = err.children(&mut cursor).collect();
    let mut i = 0;
    while i < children.len() {
        let c = children[i];
        if c.is_named() && c.named_child_count() > 0 && name_token(text, c).is_none() {
            i += 1;
            continue;
        }
        let next = children.get(i + 1).copied();
        if c.kind() == "."
            && next.is_some_and(|n| name_token(text, n).is_some() && n.start_byte() == c.end_byte())
        {
            let n = next.unwrap();
            out.classes.push(Definition {
                name: unescape(text_at(text, n)),
                value: error_inner_text(err, text),
                uri: uri.to_owned(),
                start: n.start_byte(),
                end: n.end_byte(),
                context: contexts.join(" → "),
            });
            i += 2;
            continue;
        }
        if name_token(text, c).is_some_and(|t| t.eq_ignore_ascii_case("var"))
            && next.is_some_and(|n| n.kind() == "(")
        {
            i = recover_var_args(&children, i + 2, text, out);
            continue;
        }
        // `--name` may be one atom (`identifier --x`) or `-` `-` `atom`.
        let name_end = if c.kind() == "-"
            && next.is_some_and(|n| n.kind() == "-" && n.start_byte() == c.end_byte())
            && children.get(i + 2).is_some_and(|n| {
                name_token(text, *n).is_some() && n.start_byte() == children[i + 1].end_byte()
            }) {
            Some(children[i + 2].end_byte())
        } else if name_token(text, c).is_some_and(|t| t.starts_with("--")) {
            Some(c.end_byte())
        } else {
            None
        };
        if let Some(name_end) = name_end {
            let name_start = c.start_byte();
            let k = if text_at(text, c) == "-" {
                i + 3
            } else {
                i + 1
            };
            if children.get(k).is_some_and(|n| n.kind() == ":") {
                let colon = children[k];
                let value_end = children[k + 1..]
                    .iter()
                    .find(|n| n.kind() == ";")
                    .map(|n| n.start_byte())
                    .unwrap_or(err.end_byte());
                out.variables.push(Definition {
                    name: unescape(&text[name_start..name_end]),
                    value: text[colon.end_byte()..value_end].trim().to_owned(),
                    uri: uri.to_owned(),
                    start: name_start,
                    end: name_end,
                    context: contexts.join(" → "),
                });
                i = k + 1;
                continue;
            }
        }
        i += 1;
    }
    let mut cursor = err.walk();
    for child in err.named_children(&mut cursor) {
        visit_child(child, text, uri, contexts, out, true);
    }
}

// `var(--x` / `var(--x, ...` mid-edit: the first argument atom is the use and
// any comma before `)` or the end marks fallback. Nested `var(` calls record
// their own uses; returns the index past `)` or the end of the atom list.
fn recover_var_args(children: &[Node], mut j: usize, text: &str, out: &mut Extracted) -> usize {
    let mut name_idx = None;
    let mut fallback = false;
    while j < children.len() {
        let kind = children[j].kind();
        if kind == ")" {
            j += 1;
            break;
        } else if kind == "," {
            fallback = true;
        } else if name_token(text, children[j]).is_some_and(|t| t.eq_ignore_ascii_case("var"))
            && children.get(j + 1).is_some_and(|n| n.kind() == "(")
        {
            j = recover_var_args(children, j + 2, text, out);
            continue;
        } else if name_idx.is_none() && name_token(text, children[j]).is_some() {
            name_idx = Some(j);
        }
        j += 1;
    }
    if let Some(ni) = name_idx {
        let n = children[ni];
        let raw = text_at(text, n);
        out.uses.push(VariableUse {
            name: unescape(raw.strip_suffix(':').unwrap_or(raw)),
            start: n.start_byte(),
            end: n.end_byte(),
            fallback,
        });
    }
    j
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Index;

    #[test]
    fn updates_reuse_the_cst_but_match_a_fresh_index() {
        let versions = [
            ":root { --accent: #285a43; --space: 24px; }",
            ":root { --accent: hotpink; --space: 24px; --fresh: 1px; }",
            "/* 🌿 */ .日本 { --色: red; &:hover { --色: blue } }",
            "@media (width > 10px) { .日本 { --色: red } }",
            ".card { color: var(--ac",
        ];
        let mut idx = Index::new();
        idx.update("a.css", versions[0]);
        for text in &versions[1..] {
            idx.update("a.css", text);
            let mut fresh = Index::new();
            fresh.update("a.css", text);
            assert_eq!(idx.variables(), fresh.variables(), "variables diverged");
            assert_eq!(idx.classes("a.css"), fresh.classes("a.css"));
            assert_eq!(idx.diagnostics("a.css"), fresh.diagnostics("a.css"));
        }
    }

    #[test]
    fn incomplete_and_broken_css_is_recovered() {
        let mut idx = Index::new();
        // Mid-typing: unclosed block and unclosed var() call.
        idx.update("a.css", ".card { color: var(--ac");
        assert_eq!(idx.classes("a.css")[0].name, "card");
        assert_eq!(
            idx.diagnostics("a.css")[0].message,
            "No declaration for --ac in the indexed CSS files"
        );
        // A broken declaration does not hide the following valid ones.
        idx.update("a.css", ".a { --x 1; --y: 2 }");
        assert_eq!(idx.inspect("--y").len(), 1);
        assert_eq!(idx.inspect("--y")[0].value, "2");
        assert_eq!(idx.inspect("--x").len(), 0);
    }

    #[test]
    fn nested_and_fallback_var_uses() {
        let mut idx = Index::new();
        idx.update(
            "a.css",
            ".a { color: var(--a, var(--b)); width: calc(var(--c) * 2); height: var(env(--d)); }",
        );
        idx.update("b.css", ":root { --b: 1; --c: 2; }");
        let d = idx.diagnostics("a.css");
        // --a has a fallback, --b/--c are declared; --d is inside env() which
        // is not var() so, as before, it is not reported.
        assert_eq!(d.len(), 0);
        idx.update("b.css", ":root { --b: 1; }");
        let d = idx.diagnostics("a.css");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("--c"));
    }
}

fn error_inner_text(err: Node, text: &str) -> String {
    let mut cursor = err.walk();
    let open = err
        .children(&mut cursor)
        .find(|n| n.kind() == "{")
        .map(|n| n.end_byte());
    let close = err
        .children(&mut cursor)
        .filter(|n| n.kind() == "}")
        .last()
        .map(|n| n.start_byte());
    match (open, close) {
        (Some(o), Some(c)) if c > o => text[o..c].trim().to_owned(),
        _ => String::new(),
    }
}
