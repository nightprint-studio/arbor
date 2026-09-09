//! A **class named as a string** inside an annotation.
//!
//! `@ConditionalOnClass(name = "com.zaxxer.hikari.HikariDataSource")` and
//! `@ConditionalOnMissingClass("…")` write a type the only way they can: as text. The class may
//! not be on the compile classpath at all — that is the whole point of the condition — so a class
//! literal would not compile, and Spring reads the string reflectively at runtime.
//!
//! Which means Java sees an opaque string, and so did every tool: no colour, no go-to, no
//! completion, and no complaint about a typo that silently turns the condition off for ever. This
//! module is the one place that says *where the type names are*, and the three features read it:
//!
//! * [`crate::java_intel::highlights`] colours them,
//! * [`crate::java_intel::caret_at`] resolves the one under the caret for go-to and hover,
//! * [`crate::java_intel::completions`] offers class names inside one.
//!
//! One rule, three readers — because the alternative is a colour that says "you can follow this"
//! over a site the go-to does not know about.

use crate::scan::{AnnFacts, JavaFacts};

/// Annotations whose **string** argument names a type, and which element carries it.
///
/// `""` means the bare positional value (`@ConditionalOnMissingClass("a.b.C")`). Deliberately
/// short: an annotation is on this list because Spring resolves its string as a class name, not
/// because the string happens to look like one. `@ConditionalOnBean(name = …)` is not here — that
/// names a **bean**, and a bean name is not a type name however much `orderService` looks like one.
const CLASS_NAME_ELEMENTS: &[(&str, &[&str])] = &[
    ("ConditionalOnClass", &["name"]),
    ("ConditionalOnMissingClass", &["value", ""]),
];

/// One place in the source where a type is named as text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClassRef {
    /// Byte span of the string's **contents** (quotes excluded).
    pub start: usize,
    pub end: usize,
    /// The fully-qualified name as written.
    pub fqcn: String,
}

/// Every class-naming string in the file.
pub fn class_refs(facts: &JavaFacts, anns: &[&AnnFacts]) -> Vec<ClassRef> {
    let mut out = Vec::new();
    for ann in anns {
        for site in refs_of(ann, facts) {
            out.push(site);
        }
    }
    out
}

/// The class-naming strings of ONE annotation, empty when it names none.
pub fn refs_of(ann: &AnnFacts, facts: &JavaFacts) -> Vec<ClassRef> {
    let Some((_, elements)) = CLASS_NAME_ELEMENTS
        .iter()
        .find(|(simple, _)| crate::known::is(ann, facts, simple))
    else {
        return Vec::new();
    };
    ann.strings
        .iter()
        .filter(|s| elements.contains(&s.element.as_str()))
        .filter(|s| !s.value.trim().is_empty() && s.end > s.start)
        .map(|s| ClassRef { start: s.start, end: s.end, fqcn: s.value.trim().to_string() })
        .collect()
}

/// The class named at `offset`, when the caret is inside one of these strings.
pub fn class_ref_at(facts: &JavaFacts, anns: &[&AnnFacts], offset: usize) -> Option<ClassRef> {
    class_refs(facts, anns)
        .into_iter()
        .find(|r| offset >= r.start && offset <= r.end)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scan::scan_java;

    const IMPORTS: &str = "import org.springframework.boot.autoconfigure.condition.*;";

    fn refs(body: &str) -> Vec<ClassRef> {
        let src = format!("{IMPORTS}\n{body}");
        let facts = scan_java("C.java", &src).expect("parses");
        let anns: Vec<&AnnFacts> = facts.types.iter().flat_map(|t| t.annotations.iter()).collect();
        class_refs(&facts, &anns)
    }

    #[test]
    fn a_conditional_on_class_name_is_a_class_reference() {
        let found = refs("@ConditionalOnClass(name = \"com.acme.Missing\")\nclass C {}");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].fqcn, "com.acme.Missing");
    }

    /// The bare positional form, which is how `@ConditionalOnMissingClass` is nearly always
    /// written.
    #[test]
    fn a_bare_positional_value_counts() {
        let found = refs("@ConditionalOnMissingClass(\"com.acme.Absent\")\nclass C {}");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].fqcn, "com.acme.Absent");
    }

    /// A `@ConditionalOnClass(Foo.class)` names a type the Java way — the compiler already
    /// resolves it, and there is no string here to claim anything about.
    #[test]
    fn a_class_literal_is_not_one_of_these() {
        assert!(refs("@ConditionalOnClass(HikariDataSource.class)\nclass C {}").is_empty());
    }

    /// A bean name is not a type name, however much it looks like one.
    #[test]
    fn a_bean_name_is_not_a_class_reference() {
        assert!(refs("@ConditionalOnBean(name = \"orderService\")\nclass C {}").is_empty());
    }

    #[test]
    fn the_span_is_the_contents_and_the_caret_finds_it() {
        let src = format!("{IMPORTS}\n@ConditionalOnClass(name = \"com.acme.X\")\nclass C {{}}");
        let facts = scan_java("C.java", &src).expect("parses");
        let anns: Vec<&AnnFacts> = facts.types.iter().flat_map(|t| t.annotations.iter()).collect();
        let at = src.find("com.acme.X").unwrap();
        let hit = class_ref_at(&facts, &anns, at + 3).expect("caret is inside the name");
        assert_eq!(&src[hit.start..hit.end], "com.acme.X");
        assert!(class_ref_at(&facts, &anns, 0).is_none(), "the import line names nothing");
    }
}
