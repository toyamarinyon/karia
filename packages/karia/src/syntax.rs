//! Per-file concrete syntax tree kept up to date by Tree-sitter incremental
//! parsing. Byte offsets in the tree always refer to `Document::text`.

use tree_sitter::{InputEdit, Node, Parser, Point, Tree};

pub struct Document {
    pub text: String,
    tree: Tree,
}

pub fn parser() -> Parser {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_css::LANGUAGE.into())
        .expect("bundled tree-sitter-css is compatible with tree-sitter");
    parser
}

impl Document {
    pub fn parse(parser: &mut Parser, text: &str) -> Document {
        let tree = parser
            .parse(sanitize(text).as_bytes(), None)
            .expect("tree-sitter always returns a tree");
        Document {
            text: text.to_owned(),
            tree,
        }
    }
    // The protocol delivers the full text on every change, so the changed
    // range is computed here and the existing tree is edited before reparse.
    pub fn update(&mut self, parser: &mut Parser, text: &str) {
        self.tree.edit(&compute_edit(&self.text, text));
        self.tree = parser
            .parse(sanitize(text).as_bytes(), Some(&self.tree))
            .expect("tree-sitter always returns a tree");
        self.text = text.to_owned();
    }
    pub fn root(&self) -> Node<'_> {
        self.tree.root_node()
    }
    #[cfg(test)]
    pub fn sexp(&self) -> String {
        self.tree.root_node().to_sexp()
    }
}

// The grammar only recognises ASCII identifier bytes, and braces inside
// string literals derail its error recovery into bogus nested rules.
// Sanitization keeps every byte offset identical while making the input
// parse like the original: non-ASCII characters become '_' runs of the same
// UTF-8 length (Unicode identifiers like `.日本` / `--色` still parse as
// names), `{`/`}` inside strings become '_', and a leading BOM maps to
// spaces so it stays whitespace.
fn sanitize(text: &str) -> String {
    enum State {
        Code,
        Comment,
        Str(u8),
    }
    let mut out = String::with_capacity(text.len());
    let mut chars = text.char_indices().peekable();
    let mut state = State::Code;
    let push_non_ascii = |out: &mut String, i: usize, ch: char| {
        let fill = if ch == '\u{feff}' && i == 0 { ' ' } else { '_' };
        for _ in 0..ch.len_utf8() {
            out.push(fill);
        }
    };
    while let Some((i, ch)) = chars.next() {
        if !ch.is_ascii() {
            push_non_ascii(&mut out, i, ch);
            continue;
        }
        match state {
            State::Code => {
                if ch == '/' && chars.peek().is_some_and(|&(_, n)| n == '*') {
                    out.push('/');
                    out.push('*');
                    chars.next();
                    state = State::Comment;
                } else if ch == '\'' || ch == '"' {
                    out.push(ch);
                    state = State::Str(ch as u8);
                } else {
                    out.push(ch);
                }
            }
            State::Comment => {
                out.push(ch);
                if ch == '*' && chars.peek().is_some_and(|&(_, n)| n == '/') {
                    out.push('/');
                    chars.next();
                    state = State::Code;
                }
            }
            State::Str(q) => {
                if ch == '\\' {
                    out.push('\\');
                    if let Some((j, next)) = chars.next() {
                        if next.is_ascii() {
                            out.push(next);
                        } else {
                            push_non_ascii(&mut out, j, next);
                        }
                    }
                } else if ch as u8 == q {
                    out.push(ch);
                    state = State::Code;
                } else if ch == '{' || ch == '}' {
                    out.push('_');
                } else {
                    out.push(ch);
                }
            }
        }
    }
    out
}

fn compute_edit(old: &str, new: &str) -> InputEdit {
    let old_b = old.as_bytes();
    let new_b = new.as_bytes();
    let mut prefix = 0;
    while prefix < old_b.len().min(new_b.len()) && old_b[prefix] == new_b[prefix] {
        prefix += 1;
    }
    while prefix > 0 && !old.is_char_boundary(prefix) {
        prefix -= 1;
    }
    let mut suffix = 0;
    while suffix < (old_b.len() - prefix).min(new_b.len() - prefix)
        && old_b[old_b.len() - 1 - suffix] == new_b[new_b.len() - 1 - suffix]
    {
        suffix += 1;
    }
    while suffix > 0 && !old.is_char_boundary(old_b.len() - suffix) {
        suffix -= 1;
    }
    let start = prefix;
    let old_end = old_b.len() - suffix;
    let new_end = new_b.len() - suffix;
    InputEdit {
        start_byte: start,
        old_end_byte: old_end,
        new_end_byte: new_end,
        start_position: point_at(old, start),
        old_end_position: point_at(old, old_end),
        new_end_position: point_at(new, new_end),
    }
}

fn point_at(text: &str, byte: usize) -> Point {
    let mut row = 0;
    let mut column = 0;
    for &b in text.as_bytes().iter().take(byte) {
        if b == b'\n' {
            row += 1;
            column = 0;
        } else {
            column += 1;
        }
    }
    Point { row, column }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_preserves_byte_offsets() {
        let text = "\u{feff}/* 🌿 */ .日本 { --色: '🇯🇵' }";
        let clean = sanitize(text);
        assert_eq!(clean.len(), text.len());
        assert!(clean.is_ascii());
        assert!(clean.ends_with("{ --___: '________' }"));
        let strings = ".a { content: '.a { b: c; }'; --x: '{' }";
        assert_eq!(
            sanitize(strings),
            ".a { content: '.a _ b: c; _'; --x: '_' }"
        );
    }

    #[test]
    fn compute_edit_spans_single_change() {
        let old = ".a { color: red; }";
        let new = ".a { color: blue; }";
        let edit = compute_edit(old, new);
        assert_eq!(edit.start_byte, 12);
        assert_eq!(edit.old_end_byte, 15);
        assert_eq!(edit.new_end_byte, 16);
        let multi = compute_edit(".a {}\n.b {}", ".a {}\n.b {\n}");
        assert_eq!(multi.start_byte, 10);
        assert_eq!(multi.old_end_byte, 10);
        assert_eq!(multi.new_end_byte, 11);
        assert_eq!(multi.new_end_position.row, 2);
    }

    #[test]
    fn incremental_parse_matches_fresh_parse_across_edits() {
        let mut parser = parser();
        let versions = [
            "/* 🌿 */ .日本 { --色: red }",
            "/* 🌿 */ .日本 { --色: blue; --extra: var(--色) }",
            "@media (width > 10px) { .日本 { --色: blue } }",
            ".other {}",
            ".other { content: 'unclosed",
        ];
        let mut doc = Document::parse(&mut parser, versions[0]);
        for text in &versions[1..] {
            doc.update(&mut parser, text);
            let fresh = Document::parse(&mut parser, text);
            assert_eq!(
                doc.sexp(),
                fresh.sexp(),
                "incremental tree diverged for {text}"
            );
        }
    }
}
