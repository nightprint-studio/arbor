//! The extension — what a host registers.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};

use bennu_ext::prelude::{
    ExtEntry, ExtGutterMark, ExtHover, ExtMemory, ExtStat, ExtTarget, FileCtx, FrameworkExtension, ProjectScan,
};
use bennu_facts::prelude::mentions_any;
use bennu_proto::prelude::{CapabilitySet, CompletionItem, Diagnostic};

use crate::model::{Buffer, Model, MARKERS};
use crate::web::is_web_xml;
use crate::{catalog, checks, intel};

pub struct JakartaEeExtension {
    model: RwLock<Arc<Model>>,
    beans: RwLock<Arc<Vec<ExtEntry>>>,
    endpoints: RwLock<Arc<Vec<ExtEntry>>>,
    scanned: AtomicBool,
    /// Spring's extension answers `@Inject` in this project — see [`JakartaEeExtension::applies`].
    spring: AtomicBool,
}

impl Default for JakartaEeExtension {
    fn default() -> Self {
        Self {
            model: RwLock::new(Arc::new(Model::empty())),
            beans: RwLock::default(),
            endpoints: RwLock::default(),
            scanned: AtomicBool::new(false),
            spring: AtomicBool::new(false),
        }
    }
}

impl JakartaEeExtension {
    pub fn new() -> Self {
        Self::default()
    }

    fn model(&self) -> Arc<Model> {
        self.model.read().map(|m| Arc::clone(&m)).unwrap_or_else(|_| Arc::new(Model::empty()))
    }

    fn spring(&self) -> bool {
        self.spring.load(Ordering::Relaxed)
    }

    /// Run `answer` on a Java buffer laid over the model; the default for anything else.
    ///
    /// A file that mentions none of the platform packages declares no bean and no injection point, so
    /// it is not parsed — most of a legacy tree, on every keystroke.
    fn with_buffer<T: Default>(&self, ctx: &FileCtx<'_>, answer: impl FnOnce(&Buffer<'_>) -> T) -> T {
        if ctx.extension() != "java" || !mentions_any(ctx.source, MARKERS) {
            return T::default();
        }
        let model = self.model();
        let path = ctx.path_str();
        match model.buffer(&path, ctx.source) {
            Some(buffer) => answer(&buffer),
            None => T::default(),
        }
    }

    fn rows(slot: &RwLock<Arc<Vec<ExtEntry>>>) -> Vec<ExtEntry> {
        slot.read().map(|rows| (**rows).clone()).unwrap_or_default()
    }

    fn count(slot: &RwLock<Arc<Vec<ExtEntry>>>) -> usize {
        slot.read().map(|rows| rows.len()).unwrap_or(0)
    }
}

impl FrameworkExtension for JakartaEeExtension {
    fn id(&self) -> &'static str {
        "jakartaee"
    }

    fn display_name(&self) -> &'static str {
        "Jakarta EE"
    }

    /// A Jakarta EE project. Also remembers whether Spring wires beans here: Spring's extension already
    /// answers `@Inject`, and two extensions resolving the same point by different rules would draw two
    /// arrows and disagree about both.
    fn applies(&self, caps: &CapabilitySet) -> bool {
        self.spring.store(caps.spring_annotation_di || caps.spring_xml_di, Ordering::Relaxed);
        caps.jakarta_ee
    }

    fn reindex(&self, scan: &ProjectScan<'_>) {
        let model = Model::build(scan);
        let beans = catalog::bean_rows(&model, self.spring());
        let endpoints = catalog::endpoint_rows(&model);
        if let Ok(mut slot) = self.model.write() {
            *slot = Arc::new(model);
        }
        if let Ok(mut slot) = self.beans.write() {
            *slot = Arc::new(beans);
        }
        if let Ok(mut slot) = self.endpoints.write() {
            *slot = Arc::new(endpoints);
        }
        self.scanned.store(true, Ordering::Relaxed);
    }

    fn is_ready(&self) -> bool {
        self.scanned.load(Ordering::Relaxed)
    }

    /// Before a scan lands the model is empty and marked incomplete, so every injection check is
    /// silent by construction; the per-class checks need nothing but the buffer and still run.
    fn diagnostics(&self, ctx: &FileCtx<'_>) -> Vec<Diagnostic> {
        let path = ctx.path_str();
        if ctx.extension() == "xml" && is_web_xml(&path) {
            return checks::web_xml_diagnostics(&self.model(), &path, ctx.source);
        }
        let spring = self.spring();
        self.with_buffer(ctx, |buffer| checks::java_diagnostics(buffer, spring))
    }

    fn completions(&self, ctx: &FileCtx<'_>, offset: usize) -> Vec<CompletionItem> {
        let spring = self.spring();
        self.with_buffer(ctx, |buffer| intel::completions(buffer, offset, spring))
    }

    fn hover(&self, ctx: &FileCtx<'_>, offset: usize) -> Option<ExtHover> {
        let spring = self.spring();
        self.with_buffer(ctx, |buffer| intel::hover(buffer, offset, spring))
    }

    fn navigate(&self, ctx: &FileCtx<'_>, offset: usize) -> Vec<ExtTarget> {
        let spring = self.spring();
        self.with_buffer(ctx, |buffer| intel::navigate(buffer, offset, spring))
    }

    fn gutter(&self, ctx: &FileCtx<'_>) -> Vec<ExtGutterMark> {
        let spring = self.spring();
        self.with_buffer(ctx, |buffer| intel::gutter(buffer, spring))
    }

    fn catalog(&self, kind: &str) -> Vec<ExtEntry> {
        match kind {
            "beans" => Self::rows(&self.beans),
            "endpoints" => Self::rows(&self.endpoints),
            _ => Vec::new(),
        }
    }

    fn stats(&self) -> Vec<ExtStat> {
        vec![
            ExtStat { label: "CDI beans".to_string(), value: Self::count(&self.beans), catalog: Some("beans".to_string()) },
            ExtStat { label: "Servlets".to_string(), value: Self::count(&self.endpoints), catalog: Some("endpoints".to_string()) },
        ]
    }

    fn memory(&self) -> Vec<ExtMemory> {
        let model = self.model();
        vec![
            ExtMemory::counted("Types read", model.table.rows.len()),
            ExtMemory::counted("CDI beans", model.beans.len()),
            ExtMemory::counted("Injection points", model.points.len()),
            ExtMemory::counted("Servlets and filters", model.web.len()),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures::{scanned, with_imports};
    use std::path::Path;

    fn caps(spring: bool) -> CapabilitySet {
        CapabilitySet { jakarta_ee: true, spring_annotation_di: spring, ..CapabilitySet::default() }
    }

    fn indexed(spring: bool) -> (JakartaEeExtension, String) {
        let ext = JakartaEeExtension::new();
        assert!(ext.applies(&caps(spring)));
        let use_src = with_imports("package a;\n@RequestScoped public class Use {\n  @Inject Svc svc;\n}\n");
        let java = vec![
            scanned("/p/a/Svc.java", &with_imports("package a;\npublic interface Svc {}\n")),
            scanned("/p/a/Impl.java", &with_imports("package a;\n@ApplicationScoped public class Impl implements Svc {}\n")),
            scanned("/p/a/Use.java", &use_src),
        ];
        ext.reindex(&ProjectScan { java: &java, ..ProjectScan::empty(Path::new("/p")) });
        (ext, use_src)
    }

    #[test]
    fn identity_and_capability_gate() {
        let ext = JakartaEeExtension::new();
        assert_eq!((ext.id(), ext.display_name()), ("jakartaee", "Jakarta EE"));
        assert!(!ext.applies(&CapabilitySet::default()));
        assert!(!ext.is_ready());
    }

    #[test]
    fn catalogs_and_stats_after_a_scan() {
        let (ext, _) = indexed(false);
        assert!(ext.is_ready());
        assert_eq!(ext.catalog("beans").len(), 2);
        assert!(ext.catalog("endpoints").is_empty());
        assert!(ext.catalog("nope").is_empty());
        let stats = ext.stats();
        assert_eq!((stats[0].label.as_str(), stats[0].value, stats[0].catalog.as_deref()), ("CDI beans", 2, Some("beans")));
        assert_eq!(stats[1].catalog.as_deref(), Some("endpoints"));
    }

    #[test]
    fn spring_present_silences_injection_marks_but_keeps_bean_marks() {
        let ctx_path = Path::new("/p/a/Use.java");
        let (ext, src) = indexed(false);
        let ctx = FileCtx { path: ctx_path, source: &src };
        assert!(ext.gutter(&ctx).iter().any(|m| m.kind == "inject"));
        let (ext, src) = indexed(true);
        let ctx = FileCtx { path: ctx_path, source: &src };
        let marks = ext.gutter(&ctx);
        assert!(marks.iter().all(|m| m.kind != "inject"));
        assert!(marks.iter().any(|m| m.kind == "bean"));
    }

    #[test]
    fn a_file_outside_the_platform_is_not_even_parsed() {
        let (ext, _) = indexed(false);
        let ctx = FileCtx { path: Path::new("/p/a/Plain.java"), source: "package a;\nclass Plain { @Inject Svc svc; }\n" };
        assert!(ext.gutter(&ctx).is_empty());
    }
}
