//! The JSP taglib extension — what a host registers.
//!
//! Thin by design: it owns the catalog and nothing else. The host hands over every TLD it
//! could find (the project's own and the ones extracted from the dependency jars) plus the
//! project's `web.xml`s; [`crate::intel`] answers from them.
//!
//! Gated on the `jsp_taglib_tld` capability, which is on when the project has a `.tld`, a
//! `web.xml` `<taglib>`, or a `<%@ taglib %>` directive anywhere — so a project with no tag
//! libraries never pays for this and never hears from it.

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};

use bennu_ext::prelude::{
    ExtEntry, ExtHover, ExtStat, ExtTarget, FileCtx, FrameworkExtension, ProjectScan,
};
use bennu_proto::prelude::{CapabilitySet, CompletionItem, Diagnostic};

use crate::catalog::{web_xml_aliases, TaglibCatalog};
use crate::intel;

/// The extension. Its whole model is one catalog, rebuilt on every reindex.
#[derive(Default)]
pub struct JspExtension {
    catalog: RwLock<Arc<TaglibCatalog>>,
    /// Whether a scan has run. Kept apart from "the catalog has anything in it": a project that
    /// declares taglibs but resolves none is *ready and empty*, and reporting it as never-ready
    /// would leave the overview waiting forever.
    scanned: AtomicBool,
}

impl JspExtension {
    pub fn new() -> Self {
        Self::default()
    }

    /// The catalog, when there is one worth answering from. Named apart from the trait's
    /// `catalog(kind)` so the two are never confused at a call site.
    ///
    /// Public because the model tab reads it directly: it is not answering *about* a caret, it
    /// is rendering the whole page, so it goes around the `FrameworkExtension` seam rather than
    /// pushing a shape onto it that only one framework could ever fill.
    pub fn resolved(&self) -> Option<Arc<TaglibCatalog>> {
        let cat = self.catalog.read().ok()?;
        (!cat.is_empty()).then(|| Arc::clone(&cat))
    }

    /// The JSP-family files this extension answers about. A `.tag` file is a page written in
    /// the same language and declares its libraries the same way.
    fn applies_to(ctx: &FileCtx<'_>) -> bool {
        matches!(ctx.extension().as_str(), "jsp" | "jspf" | "jspx" | "tag" | "tagx")
    }
}

impl FrameworkExtension for JspExtension {
    fn id(&self) -> &'static str {
        "jsp"
    }

    fn display_name(&self) -> &'static str {
        "JSP tag libraries"
    }

    fn applies(&self, caps: &CapabilitySet) -> bool {
        caps.jsp_taglib_tld
    }

    fn reindex(&self, scan: &ProjectScan<'_>) {
        let files: Vec<(String, String)> = scan
            .taglibs
            .iter()
            .map(|f| (f.path.to_string_lossy().replace('\\', "/"), f.text.clone()))
            .collect();
        // The `<taglib-uri>` → `<taglib-location>` aliases, from every `web.xml` in the tree —
        // a multi-module project has one per web module and each binds its own.
        let aliases: Vec<(String, String)> = scan
            .xml
            .iter()
            .filter(|f| f.path.file_name().is_some_and(|n| n.to_string_lossy().eq_ignore_ascii_case("web.xml")))
            .flat_map(|f| web_xml_aliases(&f.text))
            .collect();
        let built = Arc::new(TaglibCatalog::build(&files, &aliases));
        if let Ok(mut slot) = self.catalog.write() {
            *slot = built;
        }
        self.scanned.store(true, Ordering::Relaxed);
    }

    fn is_ready(&self) -> bool {
        self.scanned.load(Ordering::Relaxed)
    }

    fn diagnostics(&self, ctx: &FileCtx<'_>) -> Vec<Diagnostic> {
        match (Self::applies_to(ctx), self.resolved()) {
            (true, Some(cat)) => intel::diagnostics(&cat, ctx.source),
            _ => Vec::new(),
        }
    }

    fn completions(&self, ctx: &FileCtx<'_>, offset: usize) -> Vec<CompletionItem> {
        match (Self::applies_to(ctx), self.resolved()) {
            (true, Some(cat)) => intel::completions(&cat, ctx.source, offset),
            _ => Vec::new(),
        }
    }

    fn hover(&self, ctx: &FileCtx<'_>, offset: usize) -> Option<ExtHover> {
        Self::applies_to(ctx).then(|| self.resolved()).flatten().and_then(|cat| intel::hover(&cat, ctx.source, offset))
    }

    fn navigate(&self, ctx: &FileCtx<'_>, offset: usize) -> Vec<ExtTarget> {
        match (Self::applies_to(ctx), self.resolved()) {
            (true, Some(cat)) => intel::navigate(&cat, ctx.source, offset),
            _ => Vec::new(),
        }
    }

    /// The libraries this project can resolve — the answer to "why is my tag not being
    /// completed", which is otherwise invisible.
    fn catalog(&self, kind: &str) -> Vec<ExtEntry> {
        let Some(cat) = (kind == "taglibs").then(|| self.resolved()).flatten() else {
            return Vec::new();
        };
        cat.all()
            .iter()
            .map(|lib| ExtEntry {
                id: lib.source.clone(),
                primary: if lib.uri.is_empty() { file_name(&lib.source) } else { lib.uri.clone() },
                secondary: file_name(&lib.source),
                kind: "taglib".to_string(),
                file: Some(lib.source.clone()),
                tags: vec![format!("{} tags", lib.tags.len())],
                ..ExtEntry::default()
            })
            .collect()
    }

    /// One number, and it is the one that explains everything else: a page whose tags are not
    /// being completed is a page whose libraries did not resolve, and there was no way to see
    /// that from the editor.
    fn stats(&self) -> Vec<ExtStat> {
        let count = self.resolved().map_or(0, |c| c.all().len());
        vec![ExtStat {
            label: "Tag libraries".to_string(),
            value: count,
            catalog: Some("taglibs".to_string()),
        }]
    }
}

fn file_name(path: &str) -> String {
    Path::new(path).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| path.to_string())
}
