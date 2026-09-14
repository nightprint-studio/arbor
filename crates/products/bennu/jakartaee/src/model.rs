//! The project model, and how it is built from a scan.
//!
//! ## Which files are read
//!
//! A legacy tree has thousands of sources, and most never mention the platform. Reading is done in
//! rounds, each one widening the set only by what the previous one showed to matter:
//!
//! 1. every file that mentions a platform package (a plain `contains`);
//! 2. the files named after a supertype, an injected type or a producer type already read — a public
//!    type lives in the file of its name;
//! 3. every file that **mentions** an injected project type, or a type already known to implement one
//!    — because in an `all` archive any class implementing it is a bean, and a class cannot implement
//!    a type without naming it;
//! 4. every file using a project stereotype, which can make a class a bean without a single platform
//!    import.
//!
//! The rounds stop when nothing new is wanted. If they are cut short, the model says so
//! ([`Model::complete`]) and no "unsatisfied" is ever reported from it.
//!
//! ## The model answers, the buffer positions
//!
//! Every editor query re-reads the live buffer and lays it over the model ([`Model::buffer`]): its own
//! types, beans and injection points replace the ones the model read from disk, so a class annotated a
//! second ago is already a bean.

use std::collections::HashSet;

use bennu_ext::prelude::{ProjectScan, ScannedFile};
use bennu_facts::prelude::mentions_any;

use crate::archive::{discovery_mode, ArchiveMode};
use crate::beans::{beans_of, Bean, BeanOrigin};
use crate::inject::{points_of, InjectionPoint};
use crate::matching::Gates;
use crate::text::{erase, mentions_word, module_root, simple_name, slash, stem};
use crate::types::{row_of, NameIndex, TypeRef, TypeTable, TypeView, Unit, WithExtra};
use crate::web::{annotated_components, is_web_xml, parse_web_xml, WebComponent};

/// What makes a Java file worth reading at all. Over-inclusive on purpose: a false hit costs a parse.
pub const MARKERS: &[&str] = &[
    "jakarta.enterprise",
    "javax.enterprise",
    "jakarta.inject",
    "javax.inject",
    "jakarta.ejb",
    "javax.ejb",
    "jakarta.servlet",
    "javax.servlet",
    "jakarta.decorator",
    "javax.decorator",
    "jakarta.interceptor",
    "javax.interceptor",
    "@WebServlet",
    "@WebFilter",
];

/// How many widening rounds a scan runs before it calls the model incomplete.
const MAX_ROUNDS: usize = 6;

pub struct Model {
    pub table: TypeTable,
    pub beans: Vec<Bean>,
    pub points: Vec<InjectionPoint>,
    pub web: Vec<WebComponent>,
    /// Modules whose `web.xml` is `metadata-complete`.
    pub metadata_complete: HashSet<String>,
    pub mode: ArchiveMode,
    pub vetoed_packages: HashSet<String>,
    /// The project implements a portable extension, which can add and veto beans at will.
    pub portable_extension: bool,
    /// A producer whose type could be a project type nobody read.
    pub dubious_producer: bool,
    /// Session or message-driven beans declared in an `ejb-jar.xml`.
    pub ejb_descriptor: bool,
    /// Quarkus: ArC's own discovery rules and relaxed proxying.
    pub quarkus: bool,
    /// The read rounds finished.
    pub complete: bool,
}

impl Model {
    pub fn empty() -> Self {
        Self {
            table: TypeTable::default(),
            beans: Vec::new(),
            points: Vec::new(),
            web: Vec::new(),
            metadata_complete: HashSet::new(),
            mode: ArchiveMode::Unknown,
            vetoed_packages: HashSet::new(),
            portable_extension: false,
            dubious_producer: false,
            ejb_descriptor: false,
            quarkus: false,
            complete: false,
        }
    }

    pub fn build(scan: &ProjectScan<'_>) -> Self {
        let (units, complete) = read_units(scan.java);
        let stems = scan.java.iter().map(|f| stem(&slash(&f.path)).to_string());
        let table = TypeTable::build(&units, stems);
        let vetoed_packages = vetoed_packages(scan.java);
        let (beans, points, portable_extension) = {
            let view = TypeView::of(&table);
            let beans: Vec<Bean> = units.iter().flat_map(|u| beans_of(u, &view, &vetoed_packages)).collect();
            let points: Vec<InjectionPoint> = units.iter().flat_map(|u| points_of(u, &view)).collect();
            (beans, points, implements_extension(&view))
        };
        let (mut web, metadata_complete) = web_descriptors(scan.xml);
        web.extend(units.iter().flat_map(annotated_components));
        let dubious_producer = beans.iter().any(|b| {
            b.origin != BeanOrigin::Class
                && (matches!(b.declared, TypeRef::Unresolved(_)) || (b.parameterized && !matches!(b.declared, TypeRef::Library(_))))
        });
        Self {
            table,
            beans,
            points,
            web,
            metadata_complete,
            mode: discovery_mode(scan.xml),
            vetoed_packages,
            portable_extension,
            dubious_producer,
            ejb_descriptor: scan.xml.iter().any(|f| declares_ejbs(f)),
            quarkus: scan.xml.iter().any(|f| file_name_is(f, "pom.xml") && f.text.contains("io.quarkus")),
            complete,
        }
    }

    /// The project-wide reasons to claim nothing about injection.
    pub fn gates(&self) -> Gates {
        Gates {
            mode: if self.quarkus { ArchiveMode::Unknown } else { self.mode },
            open_world: self.portable_extension || self.dubious_producer || self.ejb_descriptor || !self.complete,
        }
    }

    /// The model with a live buffer laid over it. `None` when the buffer cannot be parsed.
    pub fn buffer<'m>(&'m self, path: &'m str, source: &str) -> Option<Buffer<'m>> {
        let unit = Unit::new(path, source)?;
        let mut names = NameIndex::default();
        for t in &unit.facts.types {
            names.add_type(&t.fqcn);
        }
        let rows = {
            let lookup = WithExtra { base: &self.table.names, extra: &names };
            unit.facts.types.iter().map(|t| row_of(t, &unit, &lookup)).collect()
        };
        let view = TypeView::overlay(&self.table, path, rows, names);
        let own_beans = beans_of(&unit, &view, &self.vetoed_packages);
        let beans = self.beans.iter().filter(|b| b.file != path).cloned().chain(own_beans).collect();
        let own_points = points_of(&unit, &view);
        let points = self.points.iter().filter(|p| p.file != path).cloned().chain(own_points.iter().cloned()).collect();
        Some(Buffer { model: self, path, unit, view, beans, points, own_points })
    }
}

/// A buffer read against the model. See the module docs.
pub struct Buffer<'m> {
    pub model: &'m Model,
    pub path: &'m str,
    pub unit: Unit,
    pub view: TypeView<'m>,
    /// Every bean in the project, the buffer's own as they are now.
    pub beans: Vec<Bean>,
    /// Every injection point in the project, likewise.
    pub points: Vec<InjectionPoint>,
    /// The buffer's own injection points.
    pub own_points: Vec<InjectionPoint>,
}

/// Read the Java sources in widening rounds. The flag is whether the rounds finished.
fn read_units(java: &[ScannedFile]) -> (Vec<Unit>, bool) {
    let paths: Vec<String> = java.iter().map(|f| slash(&f.path)).collect();
    let project_stems: HashSet<&str> = paths.iter().map(|p| stem(p)).collect();
    let mut read = vec![false; java.len()];
    let mut units: Vec<Unit> = Vec::new();
    let mut pending: Vec<usize> = (0..java.len()).filter(|&i| mentions_any(&java[i].text, MARKERS)).collect();
    for _ in 0..MAX_ROUNDS {
        if pending.is_empty() {
            return (units, true);
        }
        for i in pending.drain(..) {
            if !std::mem::replace(&mut read[i], true) {
                units.extend(Unit::new(&paths[i], &java[i].text));
            }
        }
        let wanted = Wanted::of(&units, &project_stems);
        pending = (0..java.len()).filter(|&i| !read[i] && wanted.wants(stem(&paths[i]), &java[i].text)).collect();
    }
    let finished = pending.is_empty();
    (units, finished)
}

/// What the next round should read. See the module docs.
struct Wanted {
    stems: HashSet<String>,
    words: HashSet<String>,
    stereotype_markers: Vec<String>,
}

impl Wanted {
    fn of(units: &[Unit], project_stems: &HashSet<&str>) -> Self {
        let mut stems = HashSet::new();
        let mut injected = HashSet::new();
        let mut stereotype_markers = Vec::new();
        for u in units {
            for t in &u.facts.types {
                stems.extend(std::iter::once(&t.extends).chain(t.implements.iter()).filter(|s| !s.is_empty()).map(|s| raw_simple(s)));
                if t.kind == "annotation" && t.annotations.iter().any(|a| a.name == "Stereotype") {
                    stereotype_markers.push(format!("@{}", t.name));
                }
                for f in t.fields.iter().filter(|f| f.annotations.iter().any(|a| matches!(a.name.as_str(), "Inject" | "EJB" | "Produces"))) {
                    injected.insert(raw_simple(&f.type_text));
                }
                for m in &t.methods {
                    if m.annotations.iter().any(|a| a.name == "Produces") {
                        stems.insert(raw_simple(&m.return_type));
                    }
                    if m.annotations.iter().any(|a| a.name == "Inject") {
                        injected.extend(m.params.iter().map(|p| raw_simple(&p.type_text)));
                    }
                }
            }
        }
        stems.extend(injected.iter().cloned());
        let read_names: HashSet<&str> = units.iter().flat_map(|u| u.facts.types.iter().map(|t| t.name.as_str())).collect();
        let mut words: HashSet<String> =
            injected.into_iter().filter(|n| project_stems.contains(n.as_str()) || read_names.contains(n.as_str())).collect();
        // Close over the implementations already read: a class extending one names it, not the interface.
        loop {
            let before = words.len();
            for t in units.iter().flat_map(|u| u.facts.types.iter()) {
                let supers = std::iter::once(&t.extends).chain(t.implements.iter());
                if !words.contains(&t.name) && supers.filter(|s| !s.is_empty()).any(|s| words.contains(&raw_simple(s))) {
                    words.insert(t.name.clone());
                }
            }
            if words.len() == before {
                break;
            }
        }
        Self { stems, words, stereotype_markers }
    }

    fn wants(&self, stem: &str, text: &str) -> bool {
        self.stems.contains(stem) || self.stereotype_markers.iter().any(|m| text.contains(m.as_str())) || mentions_word(text, &self.words)
    }
}

/// The simple raw name of a written type — `Repo` for `com.acme.Repo<Order>`.
fn raw_simple(written: &str) -> String {
    simple_name(&erase(written).raw).to_string()
}

/// The packages a `package-info.java` vetoes.
fn vetoed_packages(java: &[ScannedFile]) -> HashSet<String> {
    java.iter()
        .filter(|f| slash(&f.path).ends_with("/package-info.java"))
        .filter(|f| f.text.contains("@Vetoed") || f.text.contains("inject.Vetoed"))
        .filter_map(|f| package_of(&f.text))
        .collect()
}

/// The `package` a source declares, read off the text.
fn package_of(text: &str) -> Option<String> {
    let mut from = 0;
    while let Some(i) = text[from..].find("package ") {
        let start = from + i;
        let line_start = text[..start].rfind('\n').map(|n| n + 1).unwrap_or(0);
        let prefix = text[line_start..start].trim();
        if prefix.is_empty() || prefix.starts_with('@') || prefix.ends_with(')') {
            let rest = &text[start + "package ".len()..];
            let name = rest.split(';').next()?.trim();
            return (!name.is_empty()).then(|| name.to_string());
        }
        from = start + 1;
    }
    None
}

/// Whether some project type implements CDI's `Extension`.
fn implements_extension(view: &TypeView<'_>) -> bool {
    view.rows().any(|row| {
        row.implements.iter().any(|e| match &e.target {
            TypeRef::Library(n) => simple_name(n) == "Extension" && (n.contains("enterprise.inject.spi") || !n.contains('.')),
            TypeRef::Unresolved(n) => simple_name(n) == "Extension",
            TypeRef::Project(_) => false,
        })
    })
}

/// Every `web.xml` mapping, and the modules whose descriptor is metadata-complete.
fn web_descriptors(xml: &[ScannedFile]) -> (Vec<WebComponent>, HashSet<String>) {
    let mut components = Vec::new();
    let mut complete = HashSet::new();
    for f in xml {
        let path = slash(&f.path);
        if !is_web_xml(&path) {
            continue;
        }
        let parsed = parse_web_xml(&path, &f.text);
        if parsed.metadata_complete {
            complete.insert(module_root(&path).to_string());
        }
        components.extend(parsed.components);
    }
    (components, complete)
}

fn file_name_is(f: &ScannedFile, name: &str) -> bool {
    slash(&f.path).rsplit('/').next().is_some_and(|n| n.eq_ignore_ascii_case(name))
}

fn declares_ejbs(f: &ScannedFile) -> bool {
    file_name_is(f, "ejb-jar.xml") && (f.text.contains("<session") || f.text.contains("<message-driven"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures::{scanned, IMPORTS};

    #[test]
    fn the_rounds_read_an_interface_and_its_implementations_that_never_mention_the_platform() {
        let java = vec![
            scanned("/p/src/main/java/a/Use.java", &format!("package a;{IMPORTS}\n@RequestScoped public class Use {{ @Inject Svc svc; }}\n")),
            scanned("/p/src/main/java/a/Svc.java", "package a;\npublic interface Svc {}\n"),
            scanned("/p/src/main/java/a/Impl.java", "package a;\npublic class Impl implements Svc {}\n"),
            scanned("/p/src/main/java/a/Sub.java", "package a;\npublic class Sub extends Impl {}\n"),
            scanned("/p/src/main/java/a/Unrelated.java", "package a;\npublic class Unrelated {}\n"),
        ];
        let (units, complete) = read_units(&java);
        let mut names: Vec<&str> = units.iter().flat_map(|u| u.facts.types.iter().map(|t| t.name.as_str())).collect();
        names.sort();
        assert_eq!(names, ["Impl", "Sub", "Svc", "Use"]);
        assert!(complete);
    }

    #[test]
    fn a_package_info_veto_is_read() {
        let java = vec![scanned("/p/a/b/package-info.java", "@Vetoed\npackage a.b;\nimport jakarta.enterprise.inject.Vetoed;\n")];
        assert!(vetoed_packages(&java).contains("a.b"));
    }

    #[test]
    fn a_portable_extension_opens_the_world() {
        let java = vec![scanned("/p/a/Ext.java", "package a;\nimport jakarta.enterprise.inject.spi.Extension;\npublic class Ext implements Extension {}\n")];
        let root = std::path::Path::new("/p");
        let model = Model::build(&ProjectScan { java: &java, ..ProjectScan::empty(root) });
        assert!(model.portable_extension && model.gates().open_world);
    }
}
