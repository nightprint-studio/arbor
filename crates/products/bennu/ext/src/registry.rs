//! The extension trait and the capability-gated registry that dispatches to it.

use std::sync::Arc;

use bennu_proto::prelude::{CapabilitySet, CompletionItem, Diagnostic};

use crate::model::{
    ExtAction, ExtEntry, ExtGutterMark, ExtHighlight, ExtHover, ExtIntention, ExtProblem, ExtStat,
    ExtTarget, FileCtx, ProjectScan,
};

/// What a framework plugin implements.
///
/// **Every method has an empty default.** An extension implements the questions it can
/// answer and ignores the rest; "nothing to say" is always a valid answer and is what the
/// host expects for a file (or a project state) the extension has no business with.
///
/// Implementations are shared across threads and queried concurrently — hence
/// `Send + Sync` and `&self` everywhere. An extension that caches a model keeps it behind
/// its own lock.
pub trait FrameworkExtension: Send + Sync {
    /// Stable identifier (`"spring"`). Namespaces this extension's highlight kinds,
    /// catalog kinds and diagnostic codes.
    fn id(&self) -> &'static str;

    /// Human name for the UI (`"Spring"`).
    fn display_name(&self) -> &'static str;

    /// Whether this extension has anything to do with a project that has these
    /// capabilities. Checked once, when the registry is built: a project that isn't a
    /// Spring project never carries the Spring extension at all, so there is no
    /// per-query cost and no chance of a stray answer.
    fn applies(&self, caps: &CapabilitySet) -> bool;

    /// Build (or rebuild) whatever model this extension needs from the project. Called
    /// off the request path — it may be expensive. The extension stores the result
    /// itself; the host keeps none of it.
    fn reindex(&self, scan: &ProjectScan<'_>);

    /// Whether a model has been built yet. The host uses this to decide between "no
    /// results" and "not ready" in the UI, which are very different things to a user
    /// staring at an empty panel.
    fn is_ready(&self) -> bool {
        false
    }

    /// Framework-specific problems in a buffer. Held to the same standard as every other
    /// check in bennu: **under-report rather than risk a false positive** (docs §7).
    fn diagnostics(&self, _ctx: &FileCtx<'_>) -> Vec<Diagnostic> {
        Vec::new()
    }

    /// Spans to colour — framework syntax embedded in text the host's language modes
    /// treat as opaque (a placeholder inside a Java string literal, an expression inside
    /// an XML attribute).
    fn highlights(&self, _ctx: &FileCtx<'_>) -> Vec<ExtHighlight> {
        Vec::new()
    }

    /// Completion candidates at a caret, on top of whatever the language itself offers.
    fn completions(&self, _ctx: &FileCtx<'_>, _offset: usize) -> Vec<CompletionItem> {
        Vec::new()
    }

    /// Hover card at a caret, when the extension knows something the language doesn't.
    fn hover(&self, _ctx: &FileCtx<'_>, _offset: usize) -> Option<ExtHover> {
        None
    }

    /// The text that **certainly** follows the caret — drawn inline as ghost text, accepted
    /// with Tab.
    ///
    /// Separate from [`completions`] because the bar is different, not because the data is.
    /// A completion list may offer twenty plausible candidates and let the user choose; this
    /// is rendered ahead of the caret as if it were already typed, so it must be
    /// single-valued: a documented default, a prefix exactly one known key can continue.
    /// When in doubt the answer is `None` and the popup does the job honestly.
    ///
    /// [`completions`]: FrameworkExtension::completions
    fn inline_hint(&self, _ctx: &FileCtx<'_>, _offset: usize) -> Option<String> {
        None
    }

    /// Go-to targets at a caret. Several = the host shows a picker.
    fn navigate(&self, _ctx: &FileCtx<'_>, _offset: usize) -> Vec<ExtTarget> {
        Vec::new()
    }

    /// Gutter marks for a whole file.
    fn gutter(&self, _ctx: &FileCtx<'_>) -> Vec<ExtGutterMark> {
        Vec::new()
    }

    /// What this extension offers to **write** into the file in front of you.
    ///
    /// Returned only when it applies: a `.java` file that declares no entity gets no entity
    /// actions. That makes the toolbar's contents the answer to "what kind of file is this",
    /// which is the affordance — a disabled button teaches nothing.
    fn actions(&self, _ctx: &FileCtx<'_>) -> Vec<ExtAction> {
        Vec::new()
    }

    /// What Alt+Enter offers at a caret: the fixes for this extension's own diagnostics among
    /// `problems` (the ones the editor is showing under the caret), and the rewrites that apply at
    /// `offset` with no diagnostic behind them.
    ///
    /// Held to the bar every fix in bennu is: an offer is made only when its edit is certain to
    /// do what its label says. A fix whose analysis no longer agrees there is a problem — the
    /// buffer moved on a keystroke after the squiggle was drawn — offers nothing.
    fn intentions(
        &self,
        _ctx: &FileCtx<'_>,
        _offset: usize,
        _problems: &[ExtProblem],
    ) -> Vec<ExtIntention> {
        Vec::new()
    }

    /// The rows of one catalog (`"beans"`, `"endpoints"`, …). Unknown kind → empty.
    fn catalog(&self, _kind: &str) -> Vec<ExtEntry> {
        Vec::new()
    }

    /// Headline numbers for the overview / index inspector.
    fn stats(&self) -> Vec<ExtStat> {
        Vec::new()
    }

    /// What this extension's model keeps in memory, for the process monitor's breakdown.
    ///
    /// Asked when somebody opens the breakdown, never on a timer — but still cheap: sum the
    /// capacities of what the model owns, and **count** what is too deep to walk. Empty (the
    /// default) when the extension keeps nothing worth a line.
    fn memory(&self) -> Vec<ExtMemory> {
        Vec::new()
    }
}

/// One structure an extension keeps in memory — see [`FrameworkExtension::memory`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtMemory {
    /// What it is, in words a person reads — "Schemas held as text".
    pub label: &'static str,
    pub count: usize,
    /// `None` when the structure is only counted.
    pub bytes: Option<usize>,
    /// `bytes` is text held, summed — not an estimate of a structure.
    pub exact: bool,
}

impl ExtMemory {
    /// A structure sized by walking it — an estimate.
    pub fn estimate(label: &'static str, count: usize, bytes: usize) -> Self {
        Self { label, count, bytes: Some(bytes), exact: false }
    }

    /// Text held, summed.
    pub fn text(label: &'static str, count: usize, bytes: usize) -> Self {
        Self { label, count, bytes: Some(bytes), exact: true }
    }

    /// Only counted.
    pub fn counted(label: &'static str, count: usize) -> Self {
        Self { label, count, bytes: None, exact: false }
    }
}

/// The extensions active for one project.
///
/// Built once per project from the full set of known extensions plus that project's
/// capability bitset; only the ones that [`FrameworkExtension::applies`] to survive. Every
/// query fans out over the survivors and concatenates — order follows registration order,
/// so results are stable between runs.
pub struct ExtensionRegistry {
    active: Vec<Arc<dyn FrameworkExtension>>,
}

impl ExtensionRegistry {
    /// Keep the extensions that apply to a project with `caps`.
    pub fn new(all: Vec<Arc<dyn FrameworkExtension>>, caps: &CapabilitySet) -> Self {
        Self { active: all.into_iter().filter(|e| e.applies(caps)).collect() }
    }

    /// An empty registry — for a project whose capabilities aren't known yet.
    pub fn empty() -> Self {
        Self { active: Vec::new() }
    }

    /// Whether any extension is active.
    pub fn is_empty(&self) -> bool {
        self.active.is_empty()
    }

    /// The ids of the active extensions, in registration order.
    pub fn ids(&self) -> Vec<&'static str> {
        self.active.iter().map(|e| e.id()).collect()
    }

    /// Every active extension's memory lines, each with the extension's display name.
    pub fn memory(&self) -> Vec<(&'static str, ExtMemory)> {
        self.active
            .iter()
            .flat_map(|e| {
                let name = e.display_name();
                e.memory().into_iter().map(move |m| (name, m))
            })
            .collect()
    }

    /// The active extension with this id, if any — for a query that names one
    /// explicitly (a panel asking "spring" for its beans).
    pub fn get(&self, id: &str) -> Option<&Arc<dyn FrameworkExtension>> {
        self.active.iter().find(|e| e.id() == id)
    }

    /// Rebuild every active extension's model from `scan`.
    pub fn reindex(&self, scan: &ProjectScan<'_>) {
        for e in &self.active {
            e.reindex(scan);
        }
    }

    /// Whether every active extension has a model (vacuously true when none are active).
    pub fn is_ready(&self) -> bool {
        self.active.iter().all(|e| e.is_ready())
    }

    pub fn diagnostics(&self, ctx: &FileCtx<'_>) -> Vec<Diagnostic> {
        self.active.iter().flat_map(|e| e.diagnostics(ctx)).collect()
    }

    pub fn highlights(&self, ctx: &FileCtx<'_>) -> Vec<ExtHighlight> {
        self.active.iter().flat_map(|e| e.highlights(ctx)).collect()
    }

    pub fn completions(&self, ctx: &FileCtx<'_>, offset: usize) -> Vec<CompletionItem> {
        self.active.iter().flat_map(|e| e.completions(ctx, offset)).collect()
    }

    /// The first extension that has something to say about the caret wins — a hover card
    /// shows one thing, and two extensions claiming the same span is a conflict to fix in
    /// their `applies`, not something to render twice.
    pub fn hover(&self, ctx: &FileCtx<'_>, offset: usize) -> Option<ExtHover> {
        self.active.iter().find_map(|e| e.hover(ctx, offset))
    }

    /// The first extension with a certain continuation wins — like `hover`, only one thing
    /// can be drawn at the caret, and two extensions claiming it is a conflict to fix in
    /// their `applies` rather than something to concatenate.
    pub fn inline_hint(&self, ctx: &FileCtx<'_>, offset: usize) -> Option<String> {
        self.active.iter().find_map(|e| e.inline_hint(ctx, offset))
    }

    pub fn navigate(&self, ctx: &FileCtx<'_>, offset: usize) -> Vec<ExtTarget> {
        self.active.iter().flat_map(|e| e.navigate(ctx, offset)).collect()
    }

    pub fn gutter(&self, ctx: &FileCtx<'_>) -> Vec<ExtGutterMark> {
        self.active.iter().flat_map(|e| e.gutter(ctx)).collect()
    }

    /// Every active extension's offers for this file, in registration order — so a file that is
    /// both a Spring bean and a JPA entity shows both groups, in a stable order.
    pub fn actions(&self, ctx: &FileCtx<'_>) -> Vec<ExtAction> {
        self.active.iter().flat_map(|e| e.actions(ctx)).collect()
    }

    /// Every active extension's Alt+Enter offers at the caret, in registration order. Concatenated
    /// rather than first-wins: a popup is a list, and a file that is both a mapper and a test has
    /// offers from both.
    ///
    /// An offer with no edits is dropped here rather than trusted to each extension — the editor
    /// would show it, and choosing it would do nothing.
    pub fn intentions(
        &self,
        ctx: &FileCtx<'_>,
        offset: usize,
        problems: &[ExtProblem],
    ) -> Vec<ExtIntention> {
        self.active
            .iter()
            .flat_map(|e| e.intentions(ctx, offset, problems))
            .filter(|i| !i.edits.is_empty())
            .collect()
    }

    /// Rows of `kind`. A **namespaced** kind (`"spring.beans"`) is answered by the extension
    /// that owns it; a **bare** kind (`"endpoints"`) is answered by every extension at once,
    /// concatenated in registration order.
    ///
    /// The bare form is what makes a panel a panel about the CONCEPT rather than about one
    /// framework. A Struts action and a `@GetMapping` are both the answer to "what URLs does
    /// this application answer", and an application that has both — which is what a half-migrated
    /// legacy codebase is — wants them in one list. Taking the first non-empty answer, as this
    /// used to, would have shown whichever framework registered earlier and silently hidden the
    /// other.
    pub fn catalog(&self, kind: &str) -> Vec<ExtEntry> {
        // The owner is the extension whose id the kind starts with — the LONGEST, because an id may
        // itself be dotted (`fulcrum.i18n`, `jakarta.validation`). Splitting at the first dot looked
        // for an extension called `fulcrum`, found none, and asked every extension for the whole
        // `fulcrum.i18n.labels` — which none of them answers, the owner included.
        let owner = self
            .active
            .iter()
            .filter(|e| kind.len() > e.id().len() + 1 && kind.starts_with(e.id()) && kind.as_bytes()[e.id().len()] == b'.')
            .max_by_key(|e| e.id().len());
        if let Some(e) = owner {
            return e.catalog(&kind[e.id().len() + 1..]);
        }
        self.active.iter().flat_map(|e| e.catalog(kind)).collect()
    }

    /// Every active extension's headline numbers, each stat's catalog id **namespaced by the
    /// extension that produced it** (`"endpoints"` → `"spring.endpoints"`).
    ///
    /// Namespaced here rather than in each extension, for the same reason [`catalog`] accepts a
    /// namespaced kind: an extension names its own catalogs, and the registry is what makes those
    /// names unique across extensions. It matters because these ids are load-bearing on the other
    /// side — the frontend decides whether a panel is worth offering at all from the count next to
    /// its kind, and two frameworks with an `entities` catalog would answer for each other.
    ///
    /// [`catalog`]: ExtensionRegistry::catalog
    pub fn stats(&self) -> Vec<ExtStat> {
        self.active
            .iter()
            .flat_map(|e| {
                let id = e.id();
                e.stats()
                    .into_iter()
                    .map(move |s| ExtStat { catalog: s.catalog.map(|c| format!("{id}.{c}")), ..s })
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    /// A stub extension that applies only when Lombok is on and answers one catalog.
    struct Stub(&'static str, bool);

    impl FrameworkExtension for Stub {
        fn id(&self) -> &'static str {
            self.0
        }
        fn display_name(&self) -> &'static str {
            "Stub"
        }
        fn applies(&self, caps: &CapabilitySet) -> bool {
            !self.1 || caps.lombok
        }
        fn reindex(&self, _scan: &ProjectScan<'_>) {}
        fn catalog(&self, kind: &str) -> Vec<ExtEntry> {
            if kind == "things" {
                vec![ExtEntry { id: self.0.to_string(), ..ExtEntry::default() }]
            } else {
                Vec::new()
            }
        }
        fn stats(&self) -> Vec<ExtStat> {
            vec![
                ExtStat { label: "Things".into(), value: 1, catalog: Some("things".into()) },
                ExtStat { label: "Files".into(), value: 3, catalog: None },
            ]
        }
        /// One real offer and one with nothing to write, so the registry's filter has both to see.
        fn intentions(
            &self,
            _ctx: &FileCtx<'_>,
            offset: usize,
            problems: &[ExtProblem],
        ) -> Vec<ExtIntention> {
            let fix = problems.iter().find(|p| p.code == "stub.problem").map(|p| ExtIntention {
                id: format!("{}.fix", self.0),
                label: "Fix it".into(),
                edits: vec![crate::model::ExtEdit::replace(p.start, p.end, "fixed")],
            });
            let empty = ExtIntention { id: format!("{}.empty", self.0), label: "Nothing".into(), edits: vec![] };
            let _ = offset;
            fix.into_iter().chain([empty]).collect()
        }
    }

    /// A popup is a list: every extension's offers, in registration order — and never one that
    /// would do nothing when chosen.
    #[test]
    fn intentions_union_every_extension_and_drop_the_empty_ones() {
        let r = reg(vec![Arc::new(Stub("a", false)), Arc::new(Stub("b", false))], false);
        let ctx = FileCtx { path: Path::new("/p/Foo.java"), source: "class Foo {}" };
        let problems = [ExtProblem { code: "stub.problem".into(), start: 0, end: 5 }];
        let ids: Vec<String> = r.intentions(&ctx, 2, &problems).into_iter().map(|i| i.id).collect();
        assert_eq!(ids, ["a.fix", "b.fix"]);
        assert!(r.intentions(&ctx, 2, &[]).is_empty(), "no problem, no fix — and the empty offer is gone");
    }

    fn reg(all: Vec<Arc<dyn FrameworkExtension>>, lombok: bool) -> ExtensionRegistry {
        ExtensionRegistry::new(all, &CapabilitySet { lombok, ..CapabilitySet::default() })
    }

    #[test]
    fn capability_gate_drops_the_extension_entirely() {
        let all: Vec<Arc<dyn FrameworkExtension>> = vec![Arc::new(Stub("a", true))];
        assert!(reg(all.clone(), false).is_empty());
        assert_eq!(reg(all, true).ids(), ["a"]);
    }

    #[test]
    fn a_namespaced_catalog_kind_goes_to_its_owner() {
        let r = reg(vec![Arc::new(Stub("a", false)), Arc::new(Stub("b", false))], false);
        let rows = r.catalog("b.things");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].id, "b", "the prefix selects the extension, not the first match");
        assert!(r.catalog("b.nope").is_empty());
    }

    /// An extension id may itself contain a dot (`fulcrum.i18n`, `jakarta.validation`). The kind's
    /// owner is the longest id it starts with — splitting at the first dot looked for an extension
    /// called `fulcrum`, and the panel came back empty.
    #[test]
    fn a_dotted_extension_id_still_owns_its_namespaced_catalog() {
        let r = reg(vec![Arc::new(Stub("x", false)), Arc::new(Stub("x.y", false))], false);
        let rows = r.catalog("x.y.things");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].id, "x.y", "the longer id owns it, not `x` asked for `y.things`");
        assert_eq!(r.catalog("x.things")[0].id, "x");
    }

    /// A bare kind is the CONCEPT, not one framework's version of it: every extension that has
    /// something to say is in the list, in registration order.
    #[test]
    fn a_bare_catalog_kind_unions_every_extension() {
        let r = reg(vec![Arc::new(Stub("a", false)), Arc::new(Stub("b", false))], false);
        let ids: Vec<String> = r.catalog("things").into_iter().map(|e| e.id).collect();
        assert_eq!(ids, ["a", "b"]);
    }

    /// The id a stat carries is what the frontend matches a panel by, so it has to say which
    /// extension counted — two frameworks with an `entities` catalog would otherwise answer for
    /// each other.
    #[test]
    fn a_stat_carries_the_namespaced_catalog_it_drills_into() {
        let r = reg(vec![Arc::new(Stub("a", false)), Arc::new(Stub("b", false))], false);
        let ids: Vec<_> = r.stats().into_iter().map(|s| s.catalog).collect();
        assert_eq!(
            ids,
            [Some("a.things".to_string()), None, Some("b.things".to_string()), None],
            "a stat with no catalog stays without one",
        );
    }

    #[test]
    fn defaults_make_an_unimplemented_query_empty_not_a_panic() {
        let r = reg(vec![Arc::new(Stub("a", false))], false);
        let ctx = FileCtx { path: Path::new("/p/Foo.java"), source: "class Foo {}" };
        assert!(r.diagnostics(&ctx).is_empty());
        assert!(r.highlights(&ctx).is_empty());
        assert!(r.navigate(&ctx, 0).is_empty());
        assert!(r.hover(&ctx, 0).is_none());
        assert!(!r.is_ready(), "a stub that never indexed is not ready");
    }

    #[test]
    fn file_ctx_normalizes_what_every_extension_branches_on() {
        let ctx = FileCtx { path: Path::new(r"C:\p\src\App.JAVA"), source: "" };
        assert_eq!(ctx.extension(), "java", "case-insensitive, like the rest of bennu");
        assert_eq!(ctx.file_name(), "App.JAVA");
        assert_eq!(ctx.path_str(), "C:/p/src/App.JAVA", "forward slashes on the wire");
    }
}
