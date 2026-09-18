mod index;
mod syntax;

use serde::{Deserialize, Serialize};

pub use index::Index;

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

#[cfg(test)]
mod tests {
    use super::*;
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
