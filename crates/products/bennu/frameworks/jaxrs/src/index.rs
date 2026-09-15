//! Building the model from a project scan, and re-reading one buffer against it.
//!
//! The model answers, the buffer positions: every editor query re-parses the text in front of the
//! user for spans and reads the project facts — the application path, the annotated interfaces, who
//! implements them — from the model. Spans from the model would point at where a method used to be.

use std::sync::Arc;

use bennu_ext::prelude::{ProjectScan, ScannedFile};
use bennu_facts::prelude::{mentions_any, scan_java, JavaFacts};

use crate::extract::{extract, implementors, is_resource_interface, Env};
use crate::known::{custom_verbs_in, CustomVerb, MARKERS};
use crate::model::{Context, JaxRsModel, Locators, ResourceMethod, Src, Unit};
use crate::prefix::applications;

/// A scanned file parsed for this pass, borrowing the scan's text rather than copying it.
struct Parsed<'a> {
    facts: JavaFacts,
    text: &'a str,
}

impl Parsed<'_> {
    fn src(&self) -> Src<'_> {
        Src { facts: &self.facts, text: self.text }
    }
}

fn parse(file: &ScannedFile) -> Option<Parsed<'_>> {
    let path = file.path.to_string_lossy().replace('\\', "/");
    scan_java(&path, &file.text).map(|facts| Parsed { facts, text: file.text.as_str() })
}

/// The whole model, from the project's Java sources and its `web.xml` files.
pub fn build(scan: &ProjectScan<'_>) -> JaxRsModel {
    let first: Vec<Parsed<'_>> =
        scan.java.iter().filter(|f| mentions_any(&f.text, MARKERS)).filter_map(parse).collect();
    let custom_verbs: Vec<CustomVerb> =
        first.iter().flat_map(|p| custom_verbs_in(&p.facts, p.text)).collect();
    let interfaces: Vec<Arc<Unit>> = first
        .iter()
        .filter(|p| p.facts.types.iter().any(|t| is_resource_interface(p.src(), t, &custom_verbs)))
        .map(|p| Arc::new(Unit { facts: p.facts.clone(), text: p.text.to_string() }))
        .collect();
    let mut ctx = Context { interfaces, custom_verbs, ..Context::default() };

    // A class serving an annotated interface needs no JAX-RS import of its own — that is the point
    // of the pattern — so the marker filter alone would never parse it. One more round reads the
    // files that mention an annotated interface by name.
    let names: Vec<String> = ctx.interface_names().into_iter().map(str::to_string).collect();
    let second: Vec<Parsed<'_>> = scan
        .java
        .iter()
        .filter(|f| {
            !names.is_empty()
                && !mentions_any(&f.text, MARKERS)
                && names.iter().any(|n| f.text.contains(n.as_str()))
        })
        .filter_map(parse)
        .collect();

    let firsts: Vec<Src<'_>> = first.iter().map(Parsed::src).collect();
    let units: Vec<Src<'_>> = firsts.iter().copied().chain(second.iter().map(Parsed::src)).collect();
    let apps = applications(&firsts, scan.xml);
    let interface_srcs: Vec<Src<'_>> = ctx.interfaces.iter().map(|u| u.src()).collect();
    let impls = implementors(&units, &interface_srcs, &ctx.custom_verbs);
    let env = Env { interfaces: interface_srcs, implementors: &impls, custom_verbs: &ctx.custom_verbs };
    let mut methods: Vec<ResourceMethod> = units.iter().flat_map(|u| extract(*u, &env)).collect();
    methods.sort_by(|a, b| (a.file.as_str(), a.offset).cmp(&(b.file.as_str(), b.offset)));

    ctx.locators = Locators::from_methods(&methods, &apps.prefix);
    ctx.implementors = impls;
    ctx.prefix = apps.prefix;
    ctx.applications = apps.count;
    ctx.registration = apps.registration;
    JaxRsModel { methods, ctx }
}

/// Whether a buffer is worth a parse: it mentions JAX-RS, or an annotated interface it may
/// implement.
pub fn relevant(model: &JaxRsModel, source: &str) -> bool {
    mentions_any(source, MARKERS) || model.ctx.interface_names().iter().any(|n| source.contains(n))
}

/// The resource methods a buffer declares or serves, with spans into the buffer as it is now.
///
/// The buffer's own interfaces are read from the buffer, never from the model's copy of the same
/// file — which is the version from before the keystroke.
pub fn read_buffer(model: &JaxRsModel, path: &str, source: &str) -> Option<Vec<ResourceMethod>> {
    let facts = scan_java(path, source)?;
    let unit = Src { facts: &facts, text: source };
    let mut interfaces = vec![unit];
    interfaces.extend(model.ctx.interfaces.iter().filter(|u| u.facts.file != facts.file).map(|u| u.src()));
    let env = Env {
        interfaces,
        implementors: &model.ctx.implementors,
        custom_verbs: &model.ctx.custom_verbs,
    };
    Some(extract(unit, &env))
}
