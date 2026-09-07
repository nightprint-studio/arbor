//! The capability gate: does an annotation in THIS file actually resolve to Lombok?
//!
//! An annotation only generates anything when it is Lombok's, and it is Lombok's only when the file
//! imports it — which at compile time requires `org.projectlombok:lombok` on the classpath. So the
//! import is both the "is this really `@Data`" test and the "is Lombok a dependency" test, and a
//! project's own `@Data` in another package correctly generates nothing.
//!
//! Consumers hold imports in their own shape — a symbol-model struct, or an `import_declaration`
//! node's text. Both map onto [`ImportPath`], and everything here is expressed over that.

use crate::annotations::PACKAGE;

/// One `import …;`, reduced to what the gate needs.
///
/// `path` is the dotted path **as written minus the star**: `java.util.List` for
/// `import java.util.List;`, and `java.util` for `import java.util.*;` — the same convention the
/// index's own import record uses, so mapping onto this is a field copy.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ImportPath<'a> {
    pub path: &'a str,
    pub star: bool,
    pub is_static: bool,
}

impl<'a> ImportPath<'a> {
    /// A non-static import — the common case at every call site.
    pub fn new(path: &'a str, star: bool) -> Self {
        Self { path, star, is_static: false }
    }

    /// Whether this import comes from Lombok's package or one of its subpackages
    /// (`lombok.experimental`, `lombok.extern.slf4j`), star or not.
    pub fn is_lombok(&self) -> bool {
        self.subpath().is_some()
    }

    /// What follows `lombok` in the path — `""` for the package itself, `"Data"` for `lombok.Data`,
    /// `"experimental.Accessors"` for the subpackage. `None` when the path is not Lombok's, which is
    /// what keeps `lombokx.Data` and `com.acme.lombok.Data` out.
    fn subpath(&self) -> Option<&str> {
        match self.path.strip_prefix(PACKAGE)? {
            "" => Some(""),
            rest => rest.strip_prefix('.'),
        }
    }

    /// Whether this import is what makes the annotation simple-named `ann` resolve to Lombok: a
    /// specific import ending in `.<ann>` under `lombok`, or a `lombok`/`lombok.<sub>` wildcard.
    pub fn binds_annotation(&self, ann: &str) -> bool {
        match self.subpath() {
            None => false,
            Some(_) => self.star || self.path.rsplit('.').next() == Some(ann),
        }
    }
}

/// One `import …;` parsed out of source text, owning its path.
///
/// Owned rather than borrowed because the whitespace a path is *allowed* to carry (`import lombok
/// . val;` is legal Java) has to come out before the path can be compared — rare enough that nobody
/// writes it, common enough in a parser's contract that silently not recognising it would be a bug.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParsedImport {
    pub path: String,
    pub star: bool,
    pub is_static: bool,
}

impl ParsedImport {
    /// The borrowed view the gate functions take.
    pub fn as_path(&self) -> ImportPath<'_> {
        ImportPath { path: &self.path, star: self.star, is_static: self.is_static }
    }
}

/// Parse the source text of an `import_declaration` — `import static lombok.AccessLevel.PUBLIC;`,
/// `import lombok.*;` — into its path, star flag and static flag. `None` when `text` is not an
/// import at all.
pub fn parse_import_declaration(text: &str) -> Option<ParsedImport> {
    let compact: String = text.chars().filter(|c| !c.is_whitespace()).collect();
    let rest = compact.strip_prefix("import")?;
    let (rest, is_static) = match rest.strip_prefix("static") {
        Some(r) => (r, true),
        None => (rest, false),
    };
    let rest = rest.strip_suffix(';').unwrap_or(rest);
    let (path, star) = match rest.strip_suffix(".*") {
        Some(p) => (p, true),
        None => (rest, false),
    };
    if path.is_empty() {
        return None;
    }
    Some(ParsedImport { path: path.to_string(), star, is_static })
}

/// Whether the file imports Lombok at all. The coarse gate: no Lombok import means the file cannot
/// be using Lombok, so nothing it is annotated with generates anything.
pub fn file_uses_lombok<'a>(imports: impl IntoIterator<Item = ImportPath<'a>>) -> bool {
    imports.into_iter().any(|i| i.is_lombok())
}

/// Whether the annotation simple-named `ann` resolves to Lombok in this file. The precise gate: use
/// it where a single annotation's meaning is at stake, and [`file_uses_lombok`] where the question
/// is about the file.
pub fn annotation_is_lombok<'a>(
    ann: &str,
    imports: impl IntoIterator<Item = ImportPath<'a>>,
) -> bool {
    imports.into_iter().any(|i| i.binds_annotation(ann))
}

/// Whether the file imports Lombok's inference keyword `keyword` (`"val"` or `"var"`) — the specific
/// `import lombok.val;` or a `lombok` wildcard. Only then is a local typed `val` the keyword rather
/// than an unresolved class of that name.
pub fn imports_keyword<'a>(
    keyword: &str,
    imports: impl IntoIterator<Item = ImportPath<'a>>,
) -> bool {
    imports.into_iter().any(|i| {
        if i.star {
            // `import lombok.*;` brings in both keywords; a subpackage wildcard brings in neither.
            i.path == PACKAGE
        } else {
            i.subpath() == Some(keyword)
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(text: &str) -> ParsedImport {
        parse_import_declaration(text).expect("parses")
    }

    #[test]
    fn an_import_declaration_parses_to_its_path() {
        assert_eq!(parse("import lombok.val;"), ParsedImport {
            path: "lombok.val".into(),
            star: false,
            is_static: false
        });
        assert_eq!(parse("import lombok.*;"), ParsedImport {
            path: "lombok".into(),
            star: true,
            is_static: false
        });
        assert_eq!(parse("import   static   lombok.AccessLevel.PUBLIC ;"), ParsedImport {
            path: "lombok.AccessLevel.PUBLIC".into(),
            star: false,
            is_static: true
        });
        // Whitespace inside the path is legal Java, and comes out.
        assert_eq!(parse("import lombok . val ;").path, "lombok.val");
        assert!(parse_import_declaration("class C {}").is_none());
    }

    #[test]
    fn only_lombok_paths_pass_the_gate() {
        assert!(ImportPath::new("lombok.Data", false).is_lombok());
        assert!(ImportPath::new("lombok", true).is_lombok());
        assert!(ImportPath::new("lombok.experimental.Accessors", false).is_lombok());
        // A package that merely starts with the same letters, or has it in the middle.
        assert!(!ImportPath::new("lombokx.Data", false).is_lombok());
        assert!(!ImportPath::new("com.acme.lombok.Data", false).is_lombok());
    }

    #[test]
    fn an_annotation_is_lombok_only_when_the_import_binds_it() {
        let own = [ImportPath::new("com.acme.ann.Data", false)];
        assert!(!annotation_is_lombok("Data", own));
        let specific = [ImportPath::new("lombok.Data", false)];
        assert!(annotation_is_lombok("Data", specific));
        assert!(!annotation_is_lombok("Builder", specific));
        // A wildcard binds every annotation of the package it names.
        let wildcard = [ImportPath::new("lombok", true)];
        assert!(annotation_is_lombok("Builder", wildcard));
        // A subpackage import binds its own annotation.
        let experimental = [ImportPath::new("lombok.experimental.Accessors", false)];
        assert!(annotation_is_lombok("Accessors", experimental));
    }

    #[test]
    fn an_inference_keyword_needs_its_own_import_or_a_wildcard() {
        assert!(imports_keyword("val", [ImportPath::new("lombok.val", false)]));
        assert!(imports_keyword("val", [ImportPath::new("lombok", true)]));
        // `import lombok.val;` does not make `var` the keyword — `lombok.var` does.
        assert!(!imports_keyword("var", [ImportPath::new("lombok.val", false)]));
        assert!(imports_keyword("var", [ImportPath::new("lombok.var", false)]));
        assert!(!imports_keyword("val", [ImportPath::new("java.util", true)]));
    }

    #[test]
    fn the_file_gate_accepts_any_lombok_import() {
        assert!(file_uses_lombok([ImportPath::new("lombok.extern.slf4j.Slf4j", false)]));
        assert!(!file_uses_lombok([ImportPath::new("java.util.List", false)]));
        assert!(!file_uses_lombok([]));
    }
}
