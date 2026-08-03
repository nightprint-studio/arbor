//! The extension trait and the capability-gated registry that dispatches to it.

use std::sync::Arc;

use bennu_proto::prelude::{CapabilitySet, CompletionItem, Diagnostic};

use crate::model::{
    ExtEntry, ExtGutterMark, ExtHighlight, ExtHover, ExtStat, ExtTarget, FileCtx, ProjectScan,
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

    /// Go-to targets at a caret. Several = the host shows a picker.
    fn navigate(&self, _ctx: &FileCtx<'_>, _offset: usize) -> Vec<ExtTarget> {
        Vec::new()
    }

    /// Gutter marks for a whole file.
    fn gutter(&self, _ctx: &FileCtx<'_>) -> Vec<ExtGutterMark> {
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

    pub fn navigate(&self, ctx: &FileCtx<'_>, offset: usize) -> Vec<ExtTarget> {
        self.active.iter().flat_map(|e| e.navigate(ctx, offset)).collect()
    }

    pub fn gutter(&self, ctx: &FileCtx<'_>) -> Vec<ExtGutterMark> {
        self.active.iter().flat_map(|e| e.gutter(ctx)).collect()
    }

    /// Rows of `kind` from the extension that owns it. Catalog kinds are namespaced by
    /// the extension id (`"spring.beans"`); a bare kind is offered to every extension,
    /// first non-empty answer wins.
    pub fn catalog(&self, kind: &str) -> Vec<ExtEntry> {
        if let Some((id, rest)) = kind.split_once('.') {
            if let Some(e) = self.get(id) {
                return e.catalog(rest);
            }
        }
        self.active.iter().map(|e| e.catalog(kind)).find(|v| !v.is_empty()).unwrap_or_default()
    }

    pub fn stats(&self) -> Vec<ExtStat> {
        self.active.iter().flat_map(|e| e.stats()).collect()
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
        // A bare kind falls back to the first extension that answers it.
        assert_eq!(r.catalog("things")[0].id, "a");
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
