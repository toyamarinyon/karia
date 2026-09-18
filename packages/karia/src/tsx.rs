use oxc_allocator::Allocator;
use oxc_ast::ast::{
    Expression, IdentifierReference, ImportDeclarationSpecifier, MemberExpression, Program,
    Statement,
};
use oxc_ast_visit::{Visit, walk};
use oxc_parser::Parser;
use oxc_semantic::{Scoping, SemanticBuilder};
use oxc_span::{SourceType, Span};
use oxc_syntax::symbol::SymbolId;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

const SENTINEL: &str = "__css_lsp_cursor__";

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ModuleAccess {
    pub specifier: String,
    pub name: String,
    /// UTF-8 byte offsets, consistent with the rest of the index.
    pub start: usize,
    pub end: usize,
}

/// LSP positions are UTF-16 code units; oxc spans and returned offsets are
/// UTF-8 bytes, matching the index convention.
fn utf16_to_byte(text: &str, utf16_offset: usize) -> usize {
    let mut units = 0;
    for (byte, ch) in text.char_indices() {
        if units >= utf16_offset {
            return byte;
        }
        units += ch.len_utf16();
    }
    text.len()
}

fn is_module_css(source: &str) -> bool {
    (source.starts_with("./") || source.starts_with("../")) && source.ends_with(".module.css")
}

/// Resolve a CSS-module property access at a UTF-16 cursor offset.
pub fn module_access(text: &str, utf16_offset: usize) -> Option<ModuleAccess> {
    let cursor = utf16_to_byte(text, utf16_offset);
    find(text, cursor).or_else(|| {
        // A sentinel keeps the parser in the expression grammar when the file
        // is mid-edit; the returned offsets are remapped to the cursor.
        let mut injected = String::with_capacity(text.len() + SENTINEL.len());
        injected.push_str(&text[..cursor]);
        injected.push_str(SENTINEL);
        injected.push_str(&text[cursor..]);
        find(&injected, cursor)
    })
}

fn find(text: &str, cursor: usize) -> Option<ModuleAccess> {
    let allocator = Allocator::default();
    let parsed = Parser::new(&allocator, text, SourceType::tsx()).parse();
    let program: &Program<'_> = &parsed.program;
    let semantic = SemanticBuilder::new().build(program).semantic;
    let scoping = semantic.scoping();

    let mut imported: HashMap<SymbolId, String> = HashMap::new();
    for stmt in &program.body {
        let Statement::ImportDeclaration(decl) = stmt else {
            continue;
        };
        if !is_module_css(decl.source.value.as_str()) {
            continue;
        }
        if let Some(specifiers) = &decl.specifiers {
            for spec in specifiers {
                if let ImportDeclarationSpecifier::ImportDefaultSpecifier(def) = spec
                    && let Some(id) = def.local.symbol_id.get()
                {
                    imported.insert(id, decl.source.value.to_string());
                }
            }
        }
    }

    let mut finder = Finder {
        text,
        cursor,
        scoping,
        imported: &imported,
        result: None,
    };
    finder.visit_program(program);
    finder.result
}

struct Finder<'a> {
    text: &'a str,
    cursor: usize,
    scoping: &'a Scoping,
    imported: &'a HashMap<SymbolId, String>,
    result: Option<ModuleAccess>,
}
impl Finder<'_> {
    /// The specifier bound to this reference when it resolves to a default
    /// import of a relative .module.css source; shadowed names return None.
    fn specifier_of(&self, reference: &IdentifierReference<'_>) -> Option<String> {
        let symbol = self
            .scoping
            .get_reference(reference.reference_id.get()?)
            .symbol_id()?;
        self.imported.get(&symbol).cloned()
    }
    fn complete(&mut self, specifier: String, name: &str, span: Span) {
        let (mut name, mut start, mut end) =
            (name.to_string(), span.start as usize, span.end as usize);
        if name == SENTINEL && start >= self.cursor {
            name = String::new();
            start = self.cursor;
            end = self.cursor;
        }
        if self.cursor < start || self.cursor > end {
            return;
        }
        self.result = Some(ModuleAccess {
            specifier,
            name,
            start,
            end,
        });
    }
}
impl<'a> Visit<'a> for Finder<'a> {
    fn visit_member_expression(&mut self, it: &MemberExpression<'a>) {
        if self.result.is_none() {
            match it {
                MemberExpression::StaticMemberExpression(m) => {
                    if let Expression::Identifier(object) = &m.object
                        && let Some(source) = self.specifier_of(object)
                    {
                        let name = m.property.name.as_str();
                        let span = m.property.span;
                        self.complete(source, name, span);
                    }
                }
                MemberExpression::ComputedMemberExpression(m) => {
                    if let (Expression::Identifier(object), Expression::StringLiteral(lit)) =
                        (&m.object, &m.expression)
                        && let Some(source) = self.specifier_of(object)
                    {
                        // Range inside the quotes, matching the identifier case.
                        let span = Span::new(lit.span.start + 1, lit.span.end - 1);
                        let name = lit.value.to_string();
                        if self.cursor >= span.start as usize && self.cursor <= span.end as usize {
                            self.result = Some(ModuleAccess {
                                specifier: source,
                                name,
                                start: span.start as usize,
                                end: span.end as usize,
                            });
                        }
                    }
                }
                _ => {}
            }
        }
        walk::walk_member_expression(self, it);
    }
    fn visit_identifier_reference(&mut self, it: &IdentifierReference<'a>) {
        if self.result.is_none()
            && let Some(source) = self.specifier_of(it)
        {
            // Incomplete `styles.` has no resolvable member expression. The
            // identifier must be immediately followed by a dot at the cursor.
            let end = it.span.end as usize;
            if self.cursor >= end && self.cursor <= self.text.len() {
                let after = &self.text[end..self.cursor];
                if after.trim_start_matches(char::is_whitespace) == "." {
                    self.result = Some(ModuleAccess {
                        specifier: source,
                        name: String::new(),
                        start: self.cursor,
                        end: self.cursor,
                    });
                }
            }
        }
        walk::walk_identifier_reference(self, it);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn utf16_offset(text: &str, byte_offset: usize) -> usize {
        text[..byte_offset].chars().map(|c| c.len_utf16()).sum()
    }
    fn at(text: &str, needle: &str, delta: usize) -> usize {
        utf16_offset(text, text.find(needle).unwrap() + delta)
    }

    #[test]
    fn default_access_and_multiple_imports() {
        let s = "import styles from './a.module.css';\nimport other from './b.module.css';\nstyles.page;";
        let r = module_access(s, at(s, "page", 2)).unwrap();
        assert_eq!(r.specifier, "./a.module.css");
        assert_eq!(r.name, "page");
        assert_eq!(&s[r.start..r.end], "page");
        assert!(module_access(s, at(s, "other", 0)).is_none());
    }
    #[test]
    fn bracket_access_incomplete_dot_and_shadowing() {
        let a = "import styles from './a.module.css';\nstyles[\"hero\"];";
        assert_eq!(module_access(a, at(a, "hero", 1)).unwrap().name, "hero");
        let b = "import styles from './a.module.css';\nstyles.";
        let r = module_access(b, b.chars().map(|c| c.len_utf16()).sum()).unwrap();
        assert_eq!(r.name, "");
        let c = "import styles from './a.module.css';\nfunction f(styles) { return styles.page }";
        assert!(module_access(c, at(c, "page", 1)).is_none());
    }
    #[test]
    fn ignores_comments_strings_and_supports_jsx() {
        let s = "import styles from './a.module.css';\n// styles.fake\nconst x = \"styles.nope\";\nreturn <div className={styles.page}/>;";
        assert!(module_access(s, at(s, "fake", 2)).is_none());
        assert!(module_access(s, at(s, "nope", 2)).is_none());
        assert_eq!(module_access(s, at(s, "page", 2)).unwrap().name, "page");
    }
    #[test]
    fn incomplete_jsx_member_access() {
        let s = "import styles from './a.module.css';\nconst x = <div className={styles.} />;";
        let r = module_access(s, at(s, "styles.", "styles.".len())).unwrap();
        assert_eq!(r.name, "");
        assert_eq!(r.start, r.end);
    }
    #[test]
    fn incomplete_dot_before_a_following_statement() {
        let s = "import styles from './a.module.css';\n\nstyles.\nexport function f() { return styles.page; }";
        let cursor = at(s, "styles.\n", "styles.".len());
        let r = module_access(s, cursor).unwrap();
        assert_eq!(r.specifier, "./a.module.css");
        assert_eq!(r.name, "");
        let partial = s.replacen("styles.\n", "styles.pa\n", 1);
        let r = module_access(&partial, at(&partial, "styles.pa", "styles.pa".len())).unwrap();
        assert_eq!(r.name, "pa");
    }
    #[test]
    fn utf16_offsets_before_multibyte_text() {
        let s = "// 🌿 comment\nimport styles from './a.module.css';\nconst v = styles.card;";
        let cursor = at(s, "card", 2);
        let r = module_access(s, cursor).unwrap();
        assert_eq!(r.name, "card");
        assert_eq!(&s[r.start..r.end], "card");
    }
}
