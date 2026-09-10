//! Which schema a document is written against, and where to find it.
//!
//! ## The rule that makes this work offline
//!
//! A document names its schema by **URL**: `http://struts.apache.org/dtds/struts-2.5.dtd`,
//! `http://maven.apache.org/xsd/maven-4.0.0.xsd`. Fetching it is out of the question — an editor
//! that reaches the network to answer a keystroke is an editor that hangs on a train — and it is
//! also unnecessary, because the file is usually already on the machine: **frameworks ship their
//! own schema inside their own jar.** `struts2-core.jar` contains `struts-2.5.dtd`. The servlet
//! API jar contains the `web-app` DTDs. Spring's `spring-beans.jar` contains every version of
//! `spring-beans.xsd` it has ever published.
//!
//! So a location is matched by its **file name**, against every schema the host could find — in
//! the project, and inside the dependency jars. `…/dtds/struts-2.5.dtd` and
//! `struts2-core-2.5.30.jar!/struts-2.5.dtd` are the same file, and treating them as such is
//! what turns "we cannot reach the internet" into "we already had it".
//!
//! It is a heuristic, and the failure mode is the acceptable one: two different schemas with the
//! same file name resolve to whichever the host listed first. The alternative — matching the full
//! URL — resolves nothing at all on a machine that has never been online.
//!
//! ## When nothing resolves
//!
//! Nothing is offered. No completion, no ghost text, no diagnostics — see the crate docs. A
//! grammar guessed from the tags already in the file would confidently propose whatever typo is
//! already there.

use crate::builtin;
use crate::grammar::Grammar;
use crate::scan::Scan;

/// One schema the host was able to read, as text.
///
/// `path` is an absolute path — for a project file its own, and for one shipped inside a jar the
/// copy the host materialised into its cache. It is what the file-name match runs against and
/// what a go-to opens, and those being **the same string** is why following the URL a document
/// names its schema by lands in an editor rather than doing nothing.
///
/// A host that cannot write a cache may pass the `<jar>!/<entry>` display form instead; matching
/// still works and the go-to simply opens nothing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SchemaFile {
    pub path: String,
    pub text: String,
}

impl SchemaFile {
    fn is_xsd(&self) -> bool {
        self.path.to_ascii_lowercase().ends_with(".xsd")
    }
}

/// Every schema available to this project.
#[derive(Debug, Clone, Default)]
pub struct Catalog {
    files: Vec<SchemaFile>,
}

/// How many `xs:include` / `xs:import` hops to follow. Deep enough for the schema families that
/// exist (Spring's go two or three), shallow enough that a cycle costs nothing.
const MAX_INCLUDE_DEPTH: usize = 4;

impl Catalog {
    pub fn new(files: Vec<SchemaFile>) -> Self {
        Self { files }
    }

    pub fn is_empty(&self) -> bool {
        self.files.is_empty()
    }

    pub fn len(&self) -> usize {
        self.files.len()
    }

    /// The grammar this document is written against, or `None`.
    ///
    /// The three ways a document says so, in the order they are authoritative: a `DOCTYPE`, an
    /// `xsi:*schemaLocation` on the root, and finally the built-in catalogue for a document whose
    /// schema nobody ships locally.
    pub fn grammar_for(&self, doc_path: &str, scan: &Scan) -> Option<Grammar> {
        if let Some(dt) = &scan.doctype {
            if let Some(g) = self.resolve(&dt.system_id, doc_path) {
                return Some(g);
            }
        }
        for location in schema_locations(scan) {
            if let Some(g) = self.resolve(&location, doc_path) {
                return Some(g);
            }
        }
        builtin::grammar_for(scan)
    }

    /// The schema file a location refers to.
    ///
    /// Three ways, in order of how much they claim:
    ///
    /// 1. **a path relative to the document** — how a project's own schema is referenced, and it
    ///    wins even when a jar ships the same name;
    /// 2. **a file-name match anywhere in the catalog** — how every published URL resolves to the
    ///    copy inside a jar;
    /// 3. **another version of the same schema** — see [`Catalog::locate_family`].
    fn locate(&self, location: &str, doc_path: &str) -> Option<Located<'_>> {
        let location = location.trim();
        if location.is_empty() {
            return None;
        }
        let name = file_name(location);
        if name.is_empty() {
            return None;
        }
        // Relative to the document first: a project that keeps its own `config.xsd` beside the
        // file that uses it means *that* one, even if a jar happens to ship the same name.
        if !location.contains("://") {
            let resolved = join(parent_of(doc_path), location);
            if let Some(f) = self.files.iter().find(|f| same_path(&f.path, &resolved)) {
                return Some(Located { file: f, approximate: false });
            }
        }
        if let Some(f) = self.files.iter().find(|f| file_name(&f.path).eq_ignore_ascii_case(name)) {
            return Some(Located { file: f, approximate: false });
        }
        self.locate_family(name).map(|file| Located { file, approximate: true })
    }

    /// The nearest version of the same schema, when the exact one is nowhere.
    ///
    /// A legacy project routinely declares `struts-2.1.dtd` while the jar on its classpath ships
    /// `struts-2.5.dtd`, and a strict name match then resolves nothing at all — no completion, no
    /// hover, no go-to, on the file the whole application is configured in. The two differ in a
    /// handful of elements; treating them as unrelated because a digit differs is the worse
    /// error.
    ///
    /// **Which neighbour**, and the choice is not arbitrary: the newest version **at or below**
    /// the one asked for, because an older schema can only ever offer *less* than the project is
    /// allowed to write. Only when every candidate is newer does the oldest of those win — there
    /// is nothing else left, and something honest about its own provenance beats silence.
    ///
    /// What comes back is marked [`Grammar::approximate`], which is what keeps this from turning
    /// into a source of wrong accusations: the checks stay off.
    fn locate_family(&self, name: &str) -> Option<&SchemaFile> {
        let (stem, ext) = split_ext(name);
        let (family, wanted) = versioned(stem)?;
        let mut best: Option<(&SchemaFile, Vec<u32>)> = None;
        for file in &self.files {
            let (candidate_stem, candidate_ext) = split_ext(file_name(&file.path));
            if !candidate_ext.eq_ignore_ascii_case(ext) {
                continue;
            }
            let Some((candidate_family, version)) = versioned(candidate_stem) else { continue };
            if !candidate_family.eq_ignore_ascii_case(family) {
                continue;
            }
            let better = match &best {
                None => true,
                Some((_, held)) => closer(&version, held, &wanted),
            };
            if better {
                best = Some((file, version));
            }
        }
        best.map(|(file, _)| file)
    }

    /// One location, all the way to a usable grammar — or nothing.
    ///
    /// The `is_empty` check is not a formality: a schema that parsed to no elements has to behave
    /// exactly like no schema, or the *next* location never gets tried.
    fn resolve(&self, location: &str, doc_path: &str) -> Option<Grammar> {
        let found = self.locate(location, doc_path)?;
        let mut grammar = self.grammar_of(found.file, 0)?;
        if grammar.is_empty() {
            return None;
        }
        grammar.approximate |= found.approximate;
        Some(grammar)
    }

    /// Parse one schema file into a grammar, folding in what it includes.
    fn grammar_of(&self, file: &SchemaFile, depth: usize) -> Option<Grammar> {
        if file.is_xsd() {
            let schema = bennu_xsd::prelude::parse(&file.text)?;
            let mut grammar = crate::grammar::from_xsd(&schema, &file.path);
            if depth < MAX_INCLUDE_DEPTH {
                for location in &schema.includes {
                    // An include that cannot be resolved contributes nothing rather than making
                    // the whole schema unusable — the elements it would have brought simply stay
                    // unknown, and unknown means silent.
                    if let Some(next) = self.locate(location, &file.path) {
                        if next.file.path != file.path {
                            if let Some(g) = self.grammar_of(next.file, depth + 1) {
                                grammar.absorb(g);
                                // An include resolved by version family makes the WHOLE grammar
                                // approximate: the elements it brought are another version's.
                                grammar.approximate |= next.approximate;
                            }
                        }
                    }
                }
            }
            return Some(grammar);
        }
        let dtd = bennu_dtd::prelude::parse(&file.text);
        Some(crate::grammar::from_dtd(&dtd, &file.path))
    }
}

/// The locations named by `xsi:schemaLocation` / `xsi:noNamespaceSchemaLocation` on the root.
///
/// `schemaLocation` is a whitespace-separated list of **pairs** — namespace, then location — so
/// every second token is a location. `noNamespaceSchemaLocation` is a single location.
fn schema_locations(scan: &Scan) -> Vec<String> {
    let Some(root) = scan.tags.iter().find(|t| t.kind != crate::scan::TagKind::Close) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for attr in &root.attrs {
        match attr.local() {
            "noNamespaceSchemaLocation" => out.push(attr.value.clone()),
            "schemaLocation" => {
                out.extend(attr.value.split_whitespace().skip(1).step_by(2).map(str::to_string));
            }
            _ => {}
        }
    }
    out
}

/// A schema file the catalog matched, and how confident that match is.
struct Located<'a> {
    file: &'a SchemaFile,
    /// It is another version of what was asked for — see [`Catalog::locate_family`].
    approximate: bool,
}

/// A file name split at its last dot: `("struts-2.1", "dtd")`. A name with no dot keeps an empty
/// extension, and then only another extensionless name can match it.
fn split_ext(name: &str) -> (&str, &str) {
    match name.rfind('.') {
        Some(i) => (&name[..i], &name[i + 1..]),
        None => (name, ""),
    }
}

/// A stem split into the schema family it belongs to and the version it is of.
///
/// `struts-2.1` → (`struts`, `[2, 1]`) · `spring-beans-2.5` → (`spring-beans`, `[2, 5]`) ·
/// `maven-4.0.0` → (`maven`, `[4, 0, 0]`).
///
/// `None` when the name carries no version, which is most schemas — and for those there is
/// nothing to relax, so the strict match is already the whole answer. The tail has to be **only**
/// digits and dots: `web-app_2_3` is versioned too, in a spelling this deliberately does not
/// chase, because guessing at a second convention is how `spring-context` starts matching
/// `spring-context-support`.
fn versioned(stem: &str) -> Option<(&str, Vec<u32>)> {
    let cut = stem.rfind('-')?;
    let (family, tail) = (&stem[..cut], &stem[cut + 1..]);
    if family.is_empty() || tail.is_empty() {
        return None;
    }
    if !tail.starts_with(|c: char| c.is_ascii_digit()) {
        return None;
    }
    if !tail.chars().all(|c| c.is_ascii_digit() || c == '.') {
        return None;
    }
    let parts: Vec<u32> = tail.split('.').map(|p| p.parse::<u32>().unwrap_or(0)).collect();
    Some((family, parts))
}

/// Whether version `a` is a better stand-in for `wanted` than version `b`.
///
/// At-or-below beats above, because an older schema can only offer less than the project may
/// write. Among those below, the newest; among those above — when there is nothing below — the
/// oldest. Both tie-breaks move towards `wanted`.
fn closer(a: &[u32], b: &[u32], wanted: &[u32]) -> bool {
    use std::cmp::Ordering;
    let a_below = compare(a, wanted) != Ordering::Greater;
    let b_below = compare(b, wanted) != Ordering::Greater;
    match (a_below, b_below) {
        (true, false) => true,
        (false, true) => false,
        (true, true) => compare(a, b) == Ordering::Greater,
        (false, false) => compare(a, b) == Ordering::Less,
    }
}

/// Component-wise version comparison, a missing component reading as zero — so `2` and `2.0` are
/// the same version, which is how everyone writes them.
fn compare(a: &[u32], b: &[u32]) -> std::cmp::Ordering {
    let len = a.len().max(b.len());
    for i in 0..len {
        let (x, y) = (a.get(i).copied().unwrap_or(0), b.get(i).copied().unwrap_or(0));
        match x.cmp(&y) {
            std::cmp::Ordering::Equal => {}
            other => return other,
        }
    }
    std::cmp::Ordering::Equal
}

/// The last path segment, for either separator and for a jar entry.
fn file_name(path: &str) -> &str {
    path.rsplit(['/', '\\']).next().unwrap_or(path)
}

fn parent_of(path: &str) -> &str {
    let normalized = path.trim_end_matches(['/', '\\']);
    match normalized.rfind(['/', '\\']) {
        Some(i) => &normalized[..i],
        None => "",
    }
}

/// Join a relative location onto a directory, resolving `..` and `.` — so `../common/x.xsd`
/// beside a real file lands where the author meant rather than in a directory called `..`.
fn join(dir: &str, relative: &str) -> String {
    let rooted = dir.starts_with('/') || dir.starts_with('\\');
    let mut parts: Vec<&str> = dir.split(['/', '\\']).filter(|s| !s.is_empty()).collect();
    for segment in relative.split(['/', '\\']) {
        match segment {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            s => parts.push(s),
        }
    }
    let joined = parts.join("/");
    if rooted {
        format!("/{joined}")
    } else {
        joined
    }
}

/// Whether two paths name the same file, ignoring separator style and case.
///
/// Case-insensitively on every platform: the paths compared here come from a project scan and a
/// document's own text, and a `schemaLocation` written with different casing than the file on
/// disk is a Windows project that works — refusing to resolve it would be pedantry.
fn same_path(a: &str, b: &str) -> bool {
    a.replace('\\', "/").eq_ignore_ascii_case(&b.replace('\\', "/"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scan::scan;

    const STRUTS_DTD: &str = "<!ELEMENT struts (package*)>\n<!ELEMENT package EMPTY>\n\
                              <!ATTLIST package name CDATA #REQUIRED>";

    fn catalog(files: &[(&str, &str)]) -> Catalog {
        Catalog::new(
            files.iter().map(|(p, t)| SchemaFile { path: p.to_string(), text: t.to_string() }).collect(),
        )
    }

    /// The rule the whole module exists for: a published URL resolves to the copy inside the jar.
    #[test]
    fn a_url_in_a_doctype_resolves_to_the_copy_shipped_in_a_jar() {
        let c = catalog(&[("struts2-core-2.5.30.jar!/struts-2.5.dtd", STRUTS_DTD)]);
        let src = "<!DOCTYPE struts PUBLIC \"-//Apache//DTD Struts 2.5//EN\" \
                   \"http://struts.apache.org/dtds/struts-2.5.dtd\">\n<struts><package/></struts>";
        let g = c.grammar_for("/p/src/main/resources/struts.xml", &scan(src)).unwrap();
        assert_eq!(g.source, "struts2-core-2.5.30.jar!/struts-2.5.dtd");
        assert_eq!(g.element("struts").unwrap().child_names(), ["package"]);
        assert!(g.element("package").unwrap().attributes[0].required);
    }

    /// The case this fallback exists for: a legacy `struts.xml` declares 2.1 and the jar on the
    /// classpath ships 2.5. Strictly, nothing resolves — and nothing resolving means no
    /// completion at all in the file the whole application is configured in.
    #[test]
    fn a_neighbouring_version_answers_when_the_exact_one_is_nowhere() {
        let c = catalog(&[("struts2-core-2.5.30.jar!/struts-2.5.dtd", STRUTS_DTD)]);
        let src = "<!DOCTYPE struts PUBLIC \"-//Apache//DTD Struts 2.1//EN\" \
                   \"http://struts.apache.org/dtds/struts-2.1.dtd\">\n<struts><package/></struts>";
        let g = c.grammar_for("/p/struts.xml", &scan(src)).unwrap();
        assert_eq!(g.source, "struts2-core-2.5.30.jar!/struts-2.5.dtd");
        assert!(g.element("package").is_some(), "completion is the point of the fallback");
        assert!(g.approximate, "and it must say what it is");
        // The half that keeps this from being a liability: a schema the project is not written
        // against does not get to underline it.
        assert!(crate::intel::diagnostics(&g, &scan(src)).is_empty());
    }

    /// The exact match still wins, and is not approximate — otherwise the fallback would quietly
    /// switch the checks off for every document in the project.
    #[test]
    fn an_exact_name_is_never_treated_as_a_neighbour() {
        let c = catalog(&[
            ("struts2-core.jar!/struts-2.5.dtd", STRUTS_DTD),
            ("old.jar!/struts-2.1.dtd", STRUTS_DTD),
        ]);
        let src = "<!DOCTYPE struts SYSTEM \"http://struts.apache.org/dtds/struts-2.1.dtd\">\n\
                   <struts><package/></struts>";
        let g = c.grammar_for("/p/struts.xml", &scan(src)).unwrap();
        assert_eq!(g.source, "old.jar!/struts-2.1.dtd");
        assert!(!g.approximate);
        assert!(!crate::intel::diagnostics(&g, &scan(src)).is_empty());
    }

    /// Which neighbour: the newest at-or-below what was asked for, because an older schema can
    /// only ever offer less than the project is allowed to write.
    #[test]
    fn the_newest_version_not_above_the_one_asked_for_wins() {
        let c = catalog(&[
            ("a.jar!/struts-2.0.dtd", STRUTS_DTD),
            ("b.jar!/struts-2.3.dtd", STRUTS_DTD),
            ("c.jar!/struts-2.5.dtd", STRUTS_DTD),
        ]);
        let src = "<!DOCTYPE struts SYSTEM \"http://x/struts-2.4.dtd\">\n<struts/>";
        let g = c.grammar_for("/p/struts.xml", &scan(src)).unwrap();
        assert_eq!(g.source, "b.jar!/struts-2.3.dtd");
    }

    /// And when every candidate is newer, the oldest of those — there is nothing else left.
    #[test]
    fn with_nothing_below_the_oldest_above_is_taken() {
        let c = catalog(&[
            ("b.jar!/struts-2.3.dtd", STRUTS_DTD),
            ("c.jar!/struts-2.5.dtd", STRUTS_DTD),
        ]);
        let src = "<!DOCTYPE struts SYSTEM \"http://x/struts-2.1.dtd\">\n<struts/>";
        let g = c.grammar_for("/p/struts.xml", &scan(src)).unwrap();
        assert_eq!(g.source, "b.jar!/struts-2.3.dtd");
    }

    /// The family has to be the same family, and the extension the same extension. This is the
    /// test that stops `spring-context-2.5.xsd` from answering for `spring-context-support`.
    #[test]
    fn a_different_family_is_not_a_neighbour() {
        let c = catalog(&[
            ("a.jar!/spring-context-support-2.5.xsd", "<xs:schema xmlns:xs=\"http://www.w3.org/2001/XMLSchema\"><xs:element name=\"x\"/></xs:schema>"),
            ("b.jar!/struts-2.5.xsd", "<xs:schema xmlns:xs=\"http://www.w3.org/2001/XMLSchema\"><xs:element name=\"y\"/></xs:schema>"),
        ]);
        let src = r#"<beans xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance"
                            xsi:noNamespaceSchemaLocation="http://x/spring-context-2.5.xsd"/>"#;
        assert!(c.grammar_for("/p/beans.xml", &scan(src)).is_none());
    }

    #[test]
    fn a_version_tail_is_digits_and_dots_and_nothing_else() {
        assert_eq!(versioned("struts-2.1"), Some(("struts", vec![2, 1])));
        assert_eq!(versioned("spring-beans-2.5"), Some(("spring-beans", vec![2, 5])));
        assert_eq!(versioned("maven-4.0.0"), Some(("maven", vec![4, 0, 0])));
        // No version to relax — the strict match was already the whole answer.
        assert_eq!(versioned("spring-beans"), None);
        assert_eq!(versioned("struts"), None);
        // A second spelling this deliberately does not chase.
        assert_eq!(versioned("web-app_2_3"), None);
        assert_eq!(versioned("struts-next"), None);
    }

    /// `2` and `2.0` are the same version, which is how everyone writes them.
    #[test]
    fn a_missing_version_component_reads_as_zero() {
        use std::cmp::Ordering;
        assert_eq!(compare(&[2], &[2, 0]), Ordering::Equal);
        assert_eq!(compare(&[2, 0, 1], &[2, 1]), Ordering::Less);
    }

    #[test]
    fn a_relative_location_beside_the_document_beats_a_jar_with_the_same_name() {
        let c = catalog(&[
            ("some.jar!/config.xsd", "<xs:schema xmlns:xs=\"http://www.w3.org/2001/XMLSchema\"><xs:element name=\"wrong\"/></xs:schema>"),
            ("/p/src/main/resources/config.xsd", "<xs:schema xmlns:xs=\"http://www.w3.org/2001/XMLSchema\"><xs:element name=\"right\"/></xs:schema>"),
        ]);
        let src = r#"<app xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance"
                          xsi:noNamespaceSchemaLocation="config.xsd"/>"#;
        let g = c.grammar_for("/p/src/main/resources/app.xml", &scan(src)).unwrap();
        assert_eq!(g.source, "/p/src/main/resources/config.xsd");
        assert!(g.element("right").is_some());
    }

    #[test]
    fn a_schema_location_is_read_as_namespace_location_pairs() {
        let c = catalog(&[(
            "spring-beans.jar!/spring-beans.xsd",
            "<xs:schema xmlns:xs=\"http://www.w3.org/2001/XMLSchema\"><xs:element name=\"beans\"/></xs:schema>",
        )]);
        let src = r#"<beans xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance"
                            xsi:schemaLocation="http://www.springframework.org/schema/beans
                                                http://www.springframework.org/schema/beans/spring-beans.xsd"/>"#;
        let g = c.grammar_for("/p/beans.xml", &scan(src)).unwrap();
        assert!(g.element("beans").is_some());
    }

    #[test]
    fn an_include_is_folded_in_and_a_cycle_terminates() {
        let a = "<xs:schema xmlns:xs=\"http://www.w3.org/2001/XMLSchema\">\
                 <xs:include schemaLocation=\"b.xsd\"/><xs:element name=\"a\"/></xs:schema>";
        let b = "<xs:schema xmlns:xs=\"http://www.w3.org/2001/XMLSchema\">\
                 <xs:include schemaLocation=\"a.xsd\"/><xs:element name=\"b\"/></xs:schema>";
        let c = catalog(&[("/p/a.xsd", a), ("/p/b.xsd", b)]);
        let src = r#"<a xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance"
                        xsi:noNamespaceSchemaLocation="a.xsd"/>"#;
        let g = c.grammar_for("/p/doc.xml", &scan(src)).unwrap();
        assert!(g.element("a").is_some());
        assert!(g.element("b").is_some(), "the include contributed");
    }

    /// An unresolvable location contributes nothing rather than making the schema unusable.
    #[test]
    fn a_missing_include_leaves_the_rest_of_the_schema_working() {
        let a = "<xs:schema xmlns:xs=\"http://www.w3.org/2001/XMLSchema\">\
                 <xs:import schemaLocation=\"http://example.com/gone.xsd\"/>\
                 <xs:element name=\"a\"/></xs:schema>";
        let c = catalog(&[("/p/a.xsd", a)]);
        let src = r#"<a xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance"
                        xsi:noNamespaceSchemaLocation="a.xsd"/>"#;
        assert!(c.grammar_for("/p/doc.xml", &scan(src)).unwrap().element("a").is_some());
    }

    #[test]
    fn a_document_naming_nothing_we_have_gets_no_grammar_at_all() {
        let c = catalog(&[("/p/other.xsd", "<xs:schema xmlns:xs=\"http://www.w3.org/2001/XMLSchema\"/>")]);
        let src = "<!DOCTYPE web-app SYSTEM \"web-app_2_3.dtd\">\n<web-app/>";
        assert!(c.grammar_for("/p/web.xml", &scan(src)).is_none());
        // And a document that names nothing at all.
        assert!(catalog(&[]).grammar_for("/p/random.xml", &scan("<root><a/></root>")).is_none());
    }

    #[test]
    fn relative_segments_are_resolved_rather_than_taken_literally() {
        assert_eq!(join("/p/src/main", "../resources/x.xsd"), "/p/src/resources/x.xsd");
        assert_eq!(join("/p", "./x.xsd"), "/p/x.xsd");
        assert_eq!(join("p/src", "x.xsd"), "p/src/x.xsd", "a relative root stays relative");
        assert_eq!(parent_of("/p/src/app.xml"), "/p/src");
        assert_eq!(file_name("some.jar!/META-INF/x.xsd"), "x.xsd");
        assert!(same_path(r"C:\p\X.xsd", "c:/p/x.xsd"));
    }
}
