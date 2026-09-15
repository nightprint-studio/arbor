//! The extension — what a host registers.
//!
//! The model answers questions about OTHER files (what `UserDto` declares, which policy a config
//! sets, where the generated implementation is); the buffer is re-read for everything positional.
//! Positions from the model would point at where a mapping used to be.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};

use bennu_ext::prelude::{
    ExtEntry, ExtGutterMark, ExtHover, ExtIntention, ExtMemory, ExtProblem, ExtStat, ExtTarget, FileCtx,
    FrameworkExtension, ProjectScan,
};
use bennu_facts::prelude::{mentions_any, scan_java, JavaFacts};
use bennu_proto::prelude::{CapabilitySet, CompletionItem, Diagnostic};

use crate::catalog::line_of;
use crate::checks::findings;
use crate::mapper::{mappers_in, Mapper};
use crate::model::{build, Model};
use crate::{editor, fixes};

#[derive(Default)]
pub struct MapStructExtension {
    model: RwLock<Arc<Model>>,
    /// Kept apart from the model's contents: a project with no mappers is ready and empty.
    scanned: AtomicBool,
}

/// One buffer, analysed against the current model.
struct Buffer {
    model: Arc<Model>,
    facts: JavaFacts,
    mappers: Vec<Mapper>,
}

impl MapStructExtension {
    pub fn new() -> Self {
        Self::default()
    }

    fn model(&self) -> Arc<Model> {
        self.model.read().map(|m| Arc::clone(&m)).unwrap_or_default()
    }

    /// `None` for anything that is not a Java file declaring a mapper — which is nearly every file.
    fn buffer(&self, ctx: &FileCtx<'_>) -> Option<Buffer> {
        if ctx.extension() != "java" || !mentions_any(ctx.source, &["org.mapstruct"]) {
            return None;
        }
        let model = self.model();
        let facts = scan_java(&ctx.path_str(), ctx.source)?;
        let mappers = mappers_in(&facts, ctx.source, &model.knowledge);
        if mappers.is_empty() {
            return None;
        }
        Some(Buffer { model, facts, mappers })
    }
}

impl FrameworkExtension for MapStructExtension {
    fn id(&self) -> &'static str {
        "mapstruct"
    }

    fn display_name(&self) -> &'static str {
        "MapStruct"
    }

    fn applies(&self, caps: &CapabilitySet) -> bool {
        caps.mapstruct
    }

    fn reindex(&self, scan: &ProjectScan<'_>) {
        let model = Arc::new(build(scan));
        if let Ok(mut slot) = self.model.write() {
            *slot = model;
        }
        self.scanned.store(true, Ordering::Release);
    }

    fn is_ready(&self) -> bool {
        self.scanned.load(Ordering::Acquire)
    }

    fn diagnostics(&self, ctx: &FileCtx<'_>) -> Vec<Diagnostic> {
        let Some(b) = self.buffer(ctx) else { return Vec::new() };
        findings(&b.mappers, &b.model.knowledge.table).into_iter().map(|f| f.diag).collect()
    }

    fn completions(&self, ctx: &FileCtx<'_>, offset: usize) -> Vec<CompletionItem> {
        let Some(b) = self.buffer(ctx) else { return Vec::new() };
        editor::completions(&b.mappers, &b.model.knowledge.table, offset)
    }

    fn hover(&self, ctx: &FileCtx<'_>, offset: usize) -> Option<ExtHover> {
        let b = self.buffer(ctx)?;
        editor::hover(&b.mappers, &b.model.knowledge.table, offset)
    }

    fn navigate(&self, ctx: &FileCtx<'_>, offset: usize) -> Vec<ExtTarget> {
        let Some(b) = self.buffer(ctx) else { return Vec::new() };
        editor::navigate(&b.mappers, &b.model.knowledge.table, offset)
    }

    /// A mark on every mapping method whose generated implementation exists — the jump from "what I
    /// declared" to "what MapStruct actually wrote", which is where a mapping bug is finally visible.
    fn gutter(&self, ctx: &FileCtx<'_>) -> Vec<ExtGutterMark> {
        let Some(b) = self.buffer(ctx) else { return Vec::new() };
        let mut out = Vec::new();
        for mapper in &b.mappers {
            let Some(generated) = b.model.impls.get(&mapper.fqcn) else { continue };
            for method in &mapper.methods {
                let Some(implemented) = generated
                    .methods
                    .iter()
                    .find(|g| g.name == method.name && g.arity == method.param_types.len())
                else {
                    continue;
                };
                out.push(ExtGutterMark {
                    line: line_of(ctx.source, method.name_offset),
                    kind: "impl".to_string(),
                    tooltip: format!("Implemented in {}", generated.class),
                    targets: vec![ExtTarget {
                        file: generated.file.clone(),
                        offset: implemented.offset,
                        label: format!("{}.{}", generated.class, method.name),
                        detail: String::new(),
                    }],
                });
            }
        }
        out
    }

    fn intentions(&self, ctx: &FileCtx<'_>, _offset: usize, problems: &[ExtProblem]) -> Vec<ExtIntention> {
        if !problems.iter().any(|p| p.code.starts_with("mapstruct.")) {
            return Vec::new();
        }
        let Some(b) = self.buffer(ctx) else { return Vec::new() };
        fixes::intentions(ctx.source, &b.facts, &b.mappers, &b.model.knowledge.table, problems)
    }

    fn catalog(&self, kind: &str) -> Vec<ExtEntry> {
        if kind != "mappers" {
            return Vec::new();
        }
        self.model().catalog.clone()
    }

    fn stats(&self) -> Vec<ExtStat> {
        vec![ExtStat {
            label: "Mappers".to_string(),
            value: self.model().catalog.len(),
            catalog: Some("mappers".to_string()),
        }]
    }

    fn memory(&self) -> Vec<ExtMemory> {
        let model = self.model();
        vec![
            ExtMemory::counted("Project types read for mappers", model.knowledge.table.len()),
            ExtMemory::counted("Mappers", model.catalog.len()),
            ExtMemory::counted("Generated mapper implementations", model.impls.len()),
        ]
    }
}
