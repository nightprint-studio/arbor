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
            if let Some(g) = self.locate(&dt.system_id, doc_path).and_then(|f| self.grammar_of(f, 0))
            {
                if !g.is_empty() {
                    return Some(g);
                }
            }
        }
        for location in schema_locations(scan) {
            if let Some(g) = self.locate(&location, doc_path).and_then(|f| self.grammar_of(f, 0)) {
                if !g.is_empty() {
                    return Some(g);
                }
            }
        }
        builtin::grammar_for(scan)
    }

    /// The schema file a location refers to.
    ///
    /// Two ways, and the second is the one that earns its place: a path relative to the document
    /// (how a project's own schema is referenced), then a file-name match anywhere in the catalog
    /// (how every published URL resolves to the copy inside a jar).
    fn locate(&self, location: &str, doc_path: &str) -> Option<&SchemaFile> {
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
                return Some(f);
            }
        }
        self.files.iter().find(|f| file_name(&f.path).eq_ignore_ascii_case(name))
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
                        if next.path != file.path {
                            if let Some(g) = self.grammar_of(next, depth + 1) {
                                grammar.absorb(g);
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
