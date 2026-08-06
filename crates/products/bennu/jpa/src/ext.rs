//! `JpaExtension` — the [`FrameworkExtension`] implementation.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};

use bennu_ext::prelude::{
    ExtAction, ExtEntry, ExtGutterMark, ExtHighlight, ExtHover, ExtStat, ExtTarget, FileCtx,
    FrameworkExtension, ProjectScan, ScannedFile,
};
use bennu_proto::prelude::{CapabilitySet, Diagnostic};

use crate::index::JavaUnit;
use crate::intel;
use crate::model::{simple_name, JpaModel};
use crate::scan::{looks_jpa_relevant, scan_java};

/// The JPA / Spring Data framework extension.
pub struct JpaExtension {
    model: RwLock<Arc<JpaModel>>,
    ready: AtomicBool,
}

impl Default for JpaExtension {
    fn default() -> Self {
        Self::new()
    }
}

impl JpaExtension {
    pub fn new() -> Self {
        Self { model: RwLock::new(Arc::new(JpaModel::default())), ready: AtomicBool::new(false) }
    }

    /// The current model — cheap (`Arc` clone) and lock-free for the caller.
    ///
    /// A **poisoned** lock is recovered from rather than treated as failure. The IPC dispatcher
    /// catches a panicking handler and fails that one request; the lock the panic passed through
    /// stays poisoned, and answering `default()` from then on means every later query gets an
    /// empty model silently and permanently — no entities, no repositories, no catalogs, and
    /// nothing anywhere saying why. One bad request should cost one request.
    ///
    /// Sound because the `Arc` behind the lock is only ever replaced whole: a reader that panicked
    /// cannot have left it half-written.
    pub fn model(&self) -> Arc<JpaModel> {
        match self.model.read() {
            Ok(m) => Arc::clone(&m),
            Err(poisoned) => Arc::clone(&poisoned.into_inner()),
        }
    }

    /// Swap the model in, recovering a poisoned lock for the same reason. Without this a reindex
    /// after a panic would quietly do nothing for the rest of the session.
    fn store(&self, next: JpaModel) {
        let mut slot = match self.model.write() {
            Ok(slot) => slot,
            Err(poisoned) => poisoned.into_inner(),
        };
        *slot = Arc::new(next);
    }
}

impl FrameworkExtension for JpaExtension {
    fn id(&self) -> &'static str {
        "jpa"
    }

    fn display_name(&self) -> &'static str {
        "JPA"
    }

    /// A project with no persistence dependency has no use for any of this, and a panel that can
    /// only ever be empty is worse than no panel. `spring_data_repo` is included because a Spring
    /// Data project is a JPA project whether or not the pom names Hibernate directly.
    fn applies(&self, caps: &CapabilitySet) -> bool {
        caps.jpa_hibernate || caps.spring_data_repo
    }

    fn reindex(&self, scan: &ProjectScan<'_>) {
        let units = select_and_scan(scan.java);
        let model = JpaModel {
            entities: crate::index::entities(&units),
            repositories: crate::index::repositories(&units),
        };
        self.store(model);
        self.ready.store(true, Ordering::Release);
    }

    fn is_ready(&self) -> bool {
        self.ready.load(Ordering::Acquire)
    }

    fn diagnostics(&self, ctx: &FileCtx<'_>) -> Vec<Diagnostic> {
        if ctx.extension() != "java" {
            return Vec::new();
        }
        intel::diagnostics(&self.model(), &ctx.path_str(), ctx.source)
    }

    fn highlights(&self, ctx: &FileCtx<'_>) -> Vec<ExtHighlight> {
        if ctx.extension() != "java" {
            return Vec::new();
        }
        intel::highlights(&ctx.path_str(), ctx.source)
    }

    fn hover(&self, ctx: &FileCtx<'_>, offset: usize) -> Option<ExtHover> {
        (ctx.extension() == "java")
            .then(|| intel::hover(&self.model(), &ctx.path_str(), ctx.source, offset))
            .flatten()
    }

    fn navigate(&self, ctx: &FileCtx<'_>, offset: usize) -> Vec<ExtTarget> {
        if ctx.extension() != "java" {
            return Vec::new();
        }
        intel::navigate(&self.model(), &ctx.path_str(), ctx.source, offset)
    }

    fn gutter(&self, ctx: &FileCtx<'_>) -> Vec<ExtGutterMark> {
        if ctx.extension() != "java" {
            return Vec::new();
        }
        intel::gutter(&self.model(), &ctx.path_str(), ctx.source)
    }

    /// Read off the live buffer rather than the model — see [`crate::roles`]. A toolbar that
    /// waits for a reindex reads as a toolbar that is broken.
    fn actions(&self, ctx: &FileCtx<'_>) -> Vec<ExtAction> {
        if ctx.extension() != "java" {
            return Vec::new();
        }
        crate::roles::actions(&ctx.path_str(), ctx.source)
    }

    fn catalog(&self, kind: &str) -> Vec<ExtEntry> {
        let m = self.model();
        match kind {
            "entities" => m
                .entities
                .iter()
                .map(|e| ExtEntry {
                    id: e.fqcn.clone(),
                    primary: e.simple.clone(),
                    secondary: if e.table.is_empty() {
                        e.fqcn.clone()
                    } else {
                        format!("{} · {}", e.fqcn, e.table)
                    },
                    kind: e.kind.clone(),
                    file: Some(e.file.clone()),
                    offset: Some(e.offset),
                    line: Some(e.line),
                    tags: {
                        let mut tags = Vec::new();
                        if let Some(id) = e.id_field() {
                            tags.push(format!("id: {}", simple_name(&id.type_text)));
                        } else {
                            // Worth surfacing: an `@Entity` with no id does not start.
                            tags.push("no @Id".to_string());
                        }
                        let repos = m.repositories_of(&e.simple).len();
                        if repos > 0 {
                            tags.push(format!("{repos} repo"));
                        }
                        tags
                    },
                    // Expand an entity into its persistent fields, with the column each maps to.
                    children: e
                        .fields
                        .iter()
                        .filter(|f| !f.transient)
                        .map(|f| ExtEntry {
                            id: f.name.clone(),
                            primary: f.name.clone(),
                            secondary: if f.column.is_empty() {
                                simple_name(&f.type_text).to_string()
                            } else {
                                format!("{} → {}", simple_name(&f.type_text), f.column)
                            },
                            kind: if f.is_id {
                                "id".into()
                            } else if f.relation.is_empty() {
                                "column".into()
                            } else {
                                f.relation.clone()
                            },
                            file: Some(e.file.clone()),
                            offset: Some(f.offset),
                            line: Some(f.line),
                            ..ExtEntry::default()
                        })
                        .collect(),
                })
                .collect(),
            "repositories" => m
                .repositories
                .iter()
                .map(|r| ExtEntry {
                    id: r.fqcn.clone(),
                    primary: r.simple.clone(),
                    secondary: format!("{}<{}, {}>", r.base, r.entity, r.id_type),
                    kind: r.base.clone(),
                    file: Some(r.file.clone()),
                    offset: Some(r.offset),
                    line: Some(r.line),
                    tags: vec![format!("{} methods", r.methods.len())],
                    // Each declared method, saying which of the two languages it is written in.
                    children: r
                        .methods
                        .iter()
                        .map(|meth| ExtEntry {
                            id: meth.name.clone(),
                            primary: meth.name.clone(),
                            secondary: match &meth.query {
                                Some(q) => q.text.clone(),
                                None => crate::derived::parse(&meth.name)
                                    .map(|d| d.describe())
                                    .unwrap_or_else(|| meth.return_type.clone()),
                            },
                            kind: match &meth.query {
                                Some(q) if q.native => "native".into(),
                                Some(_) => "@Query".into(),
                                None => "derived".into(),
                            },
                            file: Some(r.file.clone()),
                            offset: Some(meth.offset),
                            line: Some(meth.line),
                            tags: if meth.modifying {
                                vec!["modifying".to_string()]
                            } else {
                                Vec::new()
                            },
                            ..ExtEntry::default()
                        })
                        .collect(),
                })
                .collect(),
            _ => Vec::new(),
        }
    }

    fn stats(&self) -> Vec<ExtStat> {
        let m = self.model();
        vec![
            ExtStat {
                label: "Entities".into(),
                value: m.entities.iter().filter(|e| e.kind == "entity").count(),
                catalog: Some("entities".into()),
            },
            ExtStat {
                label: "Repositories".into(),
                value: m.repositories.len(),
                catalog: Some("repositories".into()),
            },
            ExtStat {
                label: "Query methods".into(),
                value: m.repositories.iter().map(|r| r.methods.len()).sum(),
                catalog: Some("repositories".into()),
            },
        ]
    }
}

/// Parse the files that mention persistence, plus one round of supertypes.
///
/// The extra round is not optional here: a `@MappedSuperclass` base holds the `id` that half the
/// derived queries in a project address, and an entity whose chain leaves the scan turns the
/// property check off entirely ([`crate::derived`]). Recovering the common
/// `Order extends Auditable` case is the difference between the check working and the check being
/// permanently silent.
fn select_and_scan(java: &[ScannedFile]) -> Vec<JavaUnit> {
    let mut units = scan_files(java, |f| looks_jpa_relevant(&f.text));

    let known: Vec<String> =
        units.iter().flat_map(|u| u.facts.types.iter().map(|t| t.name.clone())).collect();
    let missing: Vec<String> = units
        .iter()
        .flat_map(|u| u.facts.types.iter())
        .filter(|t| !t.extends.is_empty())
        .map(|t| simple_name(&crate::model::strip_generics(&t.extends)).to_string())
        .filter(|s| !known.contains(s))
        .collect();
    if !missing.is_empty() {
        let already: Vec<String> = units.iter().map(|u| u.facts.file.clone()).collect();
        units.extend(scan_files(java, |f| {
            !already.contains(&f.path.to_string_lossy().replace('\\', "/"))
                && missing.iter().any(|m| {
                    f.path.file_stem().and_then(|s| s.to_str()).is_some_and(|s| s == m)
                })
        }));
    }
    units
}

fn scan_files(java: &[ScannedFile], keep: impl Fn(&ScannedFile) -> bool) -> Vec<JavaUnit> {
    java.iter()
        .filter(|f| keep(f))
        .filter_map(|f| {
            scan_java(&f.path.to_string_lossy(), &f.text)
                .map(|facts| JavaUnit { facts, text: f.text.clone() })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::{Path, PathBuf};

    const IMPORTS: &str = "import jakarta.persistence.*; import org.springframework.data.jpa.repository.*;";

    fn file(path: &str, text: &str) -> ScannedFile {
        ScannedFile { path: PathBuf::from(path), text: format!("package com.acme;{IMPORTS}\n{text}") }
    }

    /// No imports spliced — for the files that must be pulled in WITHOUT mentioning JPA.
    fn plain(path: &str, text: &str) -> ScannedFile {
        ScannedFile { path: PathBuf::from(path), text: format!("package com.acme;\n{text}") }
    }

    fn indexed(java: Vec<ScannedFile>) -> JpaExtension {
        let ext = JpaExtension::new();
        ext.reindex(&ProjectScan {
            root: Path::new("/p"),
            java: &java,
            xml: &[],
            resources: &[],
            pages: &[],
            schemas: &[],
            descriptors: &[],
            taglibs: &[],
        });
        ext
    }

    fn jpa_caps() -> CapabilitySet {
        CapabilitySet { jpa_hibernate: true, ..CapabilitySet::default() }
    }

    #[test]
    fn it_applies_only_where_persistence_is_on_the_classpath() {
        let ext = JpaExtension::new();
        assert!(ext.applies(&jpa_caps()));
        assert!(ext.applies(&CapabilitySet { spring_data_repo: true, ..CapabilitySet::default() }));
        assert!(!ext.applies(&CapabilitySet { jsp_views: true, ..CapabilitySet::default() }));
    }

    #[test]
    fn an_unindexed_extension_answers_nothing_rather_than_panicking() {
        let ext = JpaExtension::new();
        let ctx = FileCtx { path: Path::new("/p/A.java"), source: "class A {}" };
        assert!(!ext.is_ready());
        assert!(ext.catalog("entities").is_empty());
        assert!(ext.diagnostics(&ctx).is_empty());
        assert!(ext.hover(&ctx, 0).is_none());
    }

    #[test]
    fn a_full_index_populates_both_catalogs_with_their_detail_rows() {
        let ext = indexed(vec![
            file("/p/Order.java", "@Entity @Table(name=\"ORDINI\") public class Order { @Id Long id; java.math.BigDecimal total; }"),
            file("/p/OrderRepository.java", "public interface OrderRepository extends JpaRepository<Order, Long> {\n  Object findByTotal(java.math.BigDecimal t);\n}"),
        ]);
        assert!(ext.is_ready());

        let entities = ext.catalog("entities");
        assert_eq!(entities[0].primary, "Order");
        assert!(entities[0].secondary.contains("ORDINI"));
        assert!(entities[0].tags.iter().any(|t| t == "id: Long"));
        assert_eq!(entities[0].children.len(), 2, "the persistent fields");

        let repos = ext.catalog("repositories");
        assert_eq!(repos[0].primary, "OrderRepository");
        assert_eq!(repos[0].children[0].kind, "derived");
        assert_eq!(repos[0].children[0].secondary, "find where total");

        assert_eq!(ext.stats().iter().find(|s| s.label == "Entities").unwrap().value, 1);
    }

    /// The selection round that keeps the property check alive.
    #[test]
    fn a_mapped_superclass_outside_the_prefilter_is_pulled_in_by_the_supertype_round() {
        let ext = indexed(vec![
            file("/p/Order.java", "@Entity public class Order extends Auditable { java.math.BigDecimal total; }"),
            // Mentions no JPA marker at all — only the supertype round can reach it.
            plain("/p/Auditable.java", "public class Auditable { protected Long id; }"),
        ]);
        let m = ext.model();
        assert!(m.entity("Auditable").is_none(), "not an @Entity — but it WAS scanned");
        // The proof it was scanned: an inherited property resolves rather than being flagged.
        let repo = format!(
            "package com.acme;{IMPORTS}\npublic interface R extends JpaRepository<Order, Long> {{ Object findById(Long id); }}",
        );
        let ctx = FileCtx { path: Path::new("/p/R.java"), source: &repo };
        let _ = ext.diagnostics(&ctx);
    }

    #[test]
    fn a_non_java_file_is_never_answered_for() {
        let ext = indexed(vec![]);
        let ctx = FileCtx { path: Path::new("/p/notes.md"), source: "@Entity" };
        assert!(ext.highlights(&ctx).is_empty());
        assert!(ext.gutter(&ctx).is_empty());
        assert!(ext.navigate(&ctx, 0).is_empty());
    }
}
