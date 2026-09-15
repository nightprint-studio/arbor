//! Deciding whether an annotation is the one we think it is.
//!
//! `@Service` is not a reserved word. Neither is `@Entity`, `@Table` or `@Query`. Anyone can
//! declare `com.acme.Service` and put it on a class, and a tool that matches on the simple
//! name alone will register a bean that does not exist — then navigate to it, count it in a
//! panel, and offer it as an injection candidate. Every one of those is a confident lie, which
//! is the failure mode the extension crates exist to avoid.
//!
//! So the origin is resolved the way the Java compiler resolves it, in the same order:
//!
//! 1. **Written qualified** (`@org.springframework.stereotype.Service`) — the source says it
//!    outright, nothing else matters.
//! 2. **A single-type import of that simple name** — `import com.acme.Service;` makes every
//!    bare `@Service` in the file *that* one. This is the decisive case and the one that
//!    catches a project's own annotation.
//! 3. **An on-demand import** of one of the expected packages
//!    (`import org.springframework.stereotype.*;`).
//! 4. **Nothing at all** — then the name can only resolve to a type in the *same package*,
//!    which by definition is not the framework's. Rejected.
//!
//! Step 4 looks strict and is simply the language: `@Service` with no import does not compile
//! unless it is declared next door.
//!
//! ## The table is the caller's, the rule is not
//!
//! Which packages `@Entity` may legitimately come from is a JPA question; which ones `@Bean`
//! may come from is a Spring one. So each extension owns its [`AnnotationTable`] and this
//! module owns only the resolution — the part that would otherwise be copied, and drift, once
//! per framework.
//!
//! ## The one thing this does not see
//!
//! A **meta-annotation** — a project's `@MyService` that is itself annotated `@Service` — is a
//! real stereotype and is not recognised here, because recognising it means resolving the
//! annotation's own declaration. That is an under-report: the thing is missed, nothing false is
//! claimed. The right direction when in doubt.

use crate::scan::{AnnFacts, JavaFacts};

/// An annotation an extension reasons about, and the packages it may legitimately come from.
///
/// Several packages because the same annotation genuinely lives in more than one namespace:
/// `javax.persistence` and `jakarta.persistence` are the same `@Entity`, and a legacy project
/// and a current one differ only in which they import.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KnownAnnotation {
    pub simple: &'static str,
    pub packages: &'static [&'static str],
}

/// One framework's catalogue of annotations.
///
/// A name **absent** from the table is never verified — it falls back to a name match. That is
/// deliberate: callers only ask about names they act on, and asking about a name nobody has
/// pinned a package for should not silently answer "no". Adding an entry is how you tighten a
/// check, and forgetting one degrades to today's behaviour rather than to silence.
#[derive(Debug, Clone, Copy)]
pub struct AnnotationTable(&'static [KnownAnnotation]);

impl AnnotationTable {
    pub const fn new(entries: &'static [KnownAnnotation]) -> Self {
        Self(entries)
    }

    /// The packages pinned for `simple`, or `None` when it is not in the table.
    pub fn packages_for(&self, simple: &str) -> Option<&'static [&'static str]> {
        self.0.iter().find(|k| k.simple == simple).map(|k| k.packages)
    }

    /// Whether `ann` **is** the annotation called `simple` — same name *and* an origin that
    /// resolves to one of its packages.
    pub fn is(&self, ann: &AnnFacts, facts: &JavaFacts, simple: &str) -> bool {
        if ann.name != simple {
            return false;
        }
        match self.packages_for(simple) {
            Some(packages) => resolves_to(ann, facts, packages),
            None => true,
        }
    }

    /// The first of `names` that `ann` actually is, or `None`.
    pub fn is_any<'a>(
        &self,
        ann: &AnnFacts,
        facts: &JavaFacts,
        names: &[&'a str],
    ) -> Option<&'a str> {
        names.iter().copied().find(|n| self.is(ann, facts, n))
    }

    /// Whether any annotation in `anns` is `simple`.
    pub fn has(&self, anns: &[AnnFacts], facts: &JavaFacts, simple: &str) -> bool {
        anns.iter().any(|a| self.is(a, facts, simple))
    }

    /// The annotation `simple` among `anns`, if written.
    pub fn find<'a>(
        &self,
        anns: &'a [AnnFacts],
        facts: &JavaFacts,
        simple: &str,
    ) -> Option<&'a AnnFacts> {
        anns.iter().find(|a| self.is(a, facts, simple))
    }
}

/// Resolve where `ann` comes from, in the compiler's own order. See the module docs.
pub fn resolves_to(ann: &AnnFacts, facts: &JavaFacts, packages: &[&str]) -> bool {
    // 1. Written qualified — the source settles it.
    if let Some((pkg, _)) = ann.qualified.rsplit_once('.') {
        return packages.contains(&pkg);
    }
    // 2. A single-type import of this simple name decides, whatever it names.
    let suffix = format!(".{}", ann.name);
    if let Some(import) = facts.imports.iter().find(|i| i.ends_with(&suffix)) {
        let pkg = &import[..import.len() - suffix.len()];
        return packages.contains(&pkg);
    }
    // 3. An on-demand import of one of the expected packages.
    if facts
        .imports
        .iter()
        .any(|i| i.strip_suffix(".*").is_some_and(|pkg| packages.contains(&pkg)))
    {
        return true;
    }
    // 4. No import: a bare name can only be a type in this file's own package, so it is the
    //    project's annotation and not the one we are looking for.
    false
}

/// Whether a call written `name(…)` — or `Qualifier.name(…)`, when `qualifier` is given — reaches a
/// static method of one of `owners` (dotted class names), resolved through the file's imports.
///
/// The method-call twin of [`resolves_to`], for the libraries that live in static methods rather
/// than in annotations: Mockito's `when` and `any`, AssertJ's `assertThat`. `when` is not a reserved
/// word any more than `@Service` is, and a test helper of that name must not be read as Mockito.
///
/// - **Qualified** (`Mockito.when`): a dotted qualifier must be one of `owners` outright; a simple one
///   must be imported as one of them, by name or through its package's on-demand import.
/// - **Bare** (`when`): a static import of `<owner>.when` or `<owner>.*`. A single static import of
///   the same name from anywhere else decides the other way, as it does for the compiler.
/// - **Shadowed**: a method of that name declared in the file hides every static import of it
///   (JLS §6.4.1), so the answer is `false`. An *inherited* one is not visible from here — the one
///   case this can mistake, and a rare one: a test base class with its own `when`.
pub fn static_call_resolves_to(
    name: &str,
    qualifier: Option<&str>,
    facts: &JavaFacts,
    owners: &[&str],
) -> bool {
    if let Some(q) = qualifier {
        if q.contains('.') {
            return owners.contains(&q);
        }
        let suffix = format!(".{q}");
        if let Some(import) = facts.imports.iter().find(|i| i.ends_with(&suffix)) {
            return owners.contains(&import.as_str());
        }
        return facts.imports.iter().any(|i| {
            i.strip_suffix(".*").is_some_and(|pkg| owners.contains(&format!("{pkg}.{q}").as_str()))
        });
    }
    if facts.types.iter().any(|t| t.methods.iter().any(|m| m.name == name && !m.is_constructor)) {
        return false;
    }
    let suffix = format!(".{name}");
    if let Some(import) = facts.imports.iter().find(|i| i.ends_with(&suffix)) {
        let owner = &import[..import.len() - suffix.len()];
        return owners.contains(&owner);
    }
    facts.imports.iter().any(|i| i.strip_suffix(".*").is_some_and(|owner| owners.contains(&owner)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scan::scan_java;

    const MOCKITO: &[&str] = &["org.mockito.Mockito", "org.mockito.ArgumentMatchers", "org.mockito.BDDMockito"];

    fn calls(src: &str, name: &str, qualifier: Option<&str>) -> bool {
        static_call_resolves_to(name, qualifier, &scan_java("/p/T.java", src).unwrap(), MOCKITO)
    }

    #[test]
    fn a_static_import_by_name_or_on_demand_reaches_the_method() {
        assert!(calls("import static org.mockito.Mockito.when;\nclass T {}", "when", None));
        assert!(calls("import static org.mockito.ArgumentMatchers.*;\nclass T {}", "any", None));
        assert!(!calls("class T {}", "when", None), "nothing imports it");
    }

    #[test]
    fn somebody_elses_method_of_the_same_name_is_not_the_librarys() {
        assert!(!calls("import static com.acme.Stubs.when;\nimport static org.mockito.Mockito.*;\nclass T {}", "when", None));
        // Declared in the file: it shadows every static import of that name.
        assert!(!calls("import static org.mockito.Mockito.when;\nclass T { void when() {} }", "when", None));
    }

    #[test]
    fn a_qualified_call_resolves_its_class_like_any_other_type() {
        assert!(calls("import org.mockito.Mockito;\nclass T {}", "when", Some("Mockito")));
        assert!(calls("import org.mockito.*;\nclass T {}", "mock", Some("Mockito")));
        assert!(calls("class T {}", "when", Some("org.mockito.Mockito")));
        assert!(!calls("import com.acme.Mockito;\nclass T {}", "when", Some("Mockito")));
        assert!(!calls("class T {}", "when", Some("Mockito")), "same package: not the library's");
    }

    /// A miniature two-framework table — enough to exercise every rule, and to show that two
    /// extensions with overlapping simple names do not interfere.
    const TABLE: AnnotationTable = AnnotationTable::new(&[
        KnownAnnotation { simple: "Service", packages: &["org.springframework.stereotype"] },
        KnownAnnotation {
            simple: "Entity",
            packages: &["jakarta.persistence", "javax.persistence"],
        },
    ]);

    fn facts(src: &str) -> JavaFacts {
        scan_java("/p/T.java", src).unwrap()
    }

    /// The class-level annotations of the first type.
    fn anns(src: &str) -> (JavaFacts, Vec<AnnFacts>) {
        let f = facts(src);
        let a = f.types[0].annotations.clone();
        (f, a)
    }

    #[test]
    fn the_real_annotation_is_recognised() {
        let (f, a) =
            anns("package p;\nimport org.springframework.stereotype.Service;\n@Service class C {}\n");
        assert!(TABLE.is(&a[0], &f, "Service"));
    }

    #[test]
    fn someone_elses_annotation_of_the_same_name_is_not() {
        // The whole point: a project may declare its own, and it must not be mistaken for this.
        let (f, a) = anns("package p;\nimport com.acme.annotations.Service;\n@Service class C {}\n");
        assert!(!TABLE.is(&a[0], &f, "Service"));
    }

    #[test]
    fn a_bare_annotation_with_no_import_is_same_package_and_therefore_not_the_frameworks() {
        let (f, a) = anns("package com.acme;\n@Service class C {}\n");
        assert!(!TABLE.is(&a[0], &f, "Service"), "no import means it is declared next door");
    }

    #[test]
    fn a_fully_qualified_use_needs_no_import() {
        let (f, a) = anns("package p;\n@org.springframework.stereotype.Service class C {}\n");
        assert!(TABLE.is(&a[0], &f, "Service"));
        let (f2, a2) = anns("package p;\n@com.acme.Service class C {}\n");
        assert!(!TABLE.is(&a2[0], &f2, "Service"));
    }

    #[test]
    fn an_on_demand_import_of_the_right_package_counts() {
        let (f, a) =
            anns("package p;\nimport org.springframework.stereotype.*;\n@Service class C {}\n");
        assert!(TABLE.is(&a[0], &f, "Service"));
        let (f2, a2) = anns("package p;\nimport com.acme.*;\n@Service class C {}\n");
        assert!(!TABLE.is(&a2[0], &f2, "Service"));
    }

    #[test]
    fn an_explicit_import_beats_an_on_demand_one() {
        // Java's own precedence: the single-type import wins, so this is com.acme's.
        let (f, a) = anns(
            "package p;\nimport org.springframework.stereotype.*;\nimport com.acme.Service;\n@Service class C {}\n",
        );
        assert!(!TABLE.is(&a[0], &f, "Service"));
    }

    /// The reason `packages` is a list: a legacy project and a current one write the same
    /// annotation in two namespaces, and both are right.
    #[test]
    fn javax_and_jakarta_are_both_accepted() {
        for pkg in ["javax.persistence", "jakarta.persistence"] {
            let (f, a) = anns(&format!("package p;\nimport {pkg}.Entity;\n@Entity class C {{}}\n"));
            assert!(TABLE.is(&a[0], &f, "Entity"), "{pkg}");
        }
        let (f, a) = anns("package p;\nimport com.acme.Entity;\n@Entity class C {}\n");
        assert!(!TABLE.is(&a[0], &f, "Entity"));
    }

    #[test]
    fn is_any_returns_which_one_matched() {
        let (f, a) =
            anns("package p;\nimport jakarta.persistence.Entity;\n@Entity class C {}\n");
        assert_eq!(TABLE.is_any(&a[0], &f, &["Service", "Entity"]), Some("Entity"));
        assert_eq!(TABLE.is_any(&a[0], &f, &["Service"]), None);
    }

    #[test]
    fn an_annotation_outside_the_table_falls_back_to_its_name() {
        let (f, a) = anns("package p;\nimport com.acme.Whatever;\n@Whatever class C {}\n");
        assert!(TABLE.is(&a[0], &f, "Whatever"), "nobody pinned a package for this one");
        assert!(TABLE.packages_for("Whatever").is_none());
    }

    #[test]
    fn has_and_find_agree_with_is() {
        let (f, a) = anns(
            "package p;\nimport org.springframework.stereotype.Service;\n@Service(\"x\") class C {}\n",
        );
        assert!(TABLE.has(&a, &f, "Service"));
        assert_eq!(TABLE.find(&a, &f, "Service").unwrap().value().unwrap().value, "x");
        assert!(!TABLE.has(&a, &f, "Entity"));
        assert!(TABLE.find(&a, &f, "Entity").is_none());
    }
}
