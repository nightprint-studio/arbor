//! The resolver-backed checks, listed **once**.
//!
//! Two drivers run them: the whole-file pass ([`check_file_resolved`](crate::engine::check::check_file_resolved))
//! and the incremental one ([`check_file_resolved_incremental`](crate::engine::incremental::check_file_resolved_incremental)),
//! which the editor and the MCP `bennu_check_file` go through. They used to keep a list each, "in
//! lock-step" by comment, and a check added to one silently never ran in the other: fifteen of them,
//! the annotation-element ones among them, were missing from the editor for as long as nobody
//! compared the two lists. A check that is not registered and a check that finds nothing print the
//! same thing — an empty page — so the only defence is that there is nowhere else to register it.
//!
//! The split is by what a check reads, and it is the incremental pass's only reason to exist:
//!   * [`whole_file`] — declarations, imports, headers, annotations. Cheap, and answers about the
//!     file as a unit, so the incremental pass recomputes them on every run.
//!   * [`per_body`] — per-expression checks that infer types. They visit each node on its own, so
//!     running them over a bucket of nodes (one method body) and over the whole file give the same
//!     union; that is what lets the incremental pass cache them body by body.
//!
//! A check that takes an [`InferCache`] and visits expressions belongs in [`per_body`]; one that
//! reads a declaration, or needs the file whole, in [`whole_file`].

use std::time::{Duration, Instant};

use bennu_java::prelude::{FileSymbols, InferCache, TypeResolver};
use bennu_proto::prelude::Diagnostic;
use tree_sitter::Node;

use crate::engine::check::FileContext;

/// What every resolver-backed check is handed, bundled so a new one is one line below and not a
/// signature change in two drivers.
#[derive(Clone, Copy)]
pub(crate) struct Subject<'a, 'x> {
    pub root: Node<'a>,
    pub source: &'x str,
    pub symbols: &'x FileSymbols,
    pub resolver: &'x dyn TypeResolver,
    pub ctx: &'x FileContext,
}

/// The diagnostics collected so far, plus — when profiling is on — what each check cost.
pub(crate) struct Run {
    pub out: Vec<Diagnostic>,
    pub times: Vec<(&'static str, Duration)>,
    profile: bool,
}

impl Run {
    pub(crate) fn new(out: Vec<Diagnostic>, times: Vec<(&'static str, Duration)>, profile: bool) -> Self {
        Self { out, times, profile }
    }

    /// Run one check. The clock starts before the closure is called: passing the call as an argument
    /// would evaluate it first, and every check would measure as ~0.
    fn check(&mut self, label: &'static str, found: impl FnOnce() -> Vec<Diagnostic>) {
        let started = self.profile.then(Instant::now);
        let diagnostics = found();
        if let Some(started) = started {
            self.times.push((label, started.elapsed()));
        }
        self.out.extend(diagnostics);
    }
}

/// [`whole_file`], collected on its own.
pub(crate) fn whole_file_diagnostics<'a>(subject: &Subject<'a, '_>, nodes: &[Node<'a>], infer: &InferCache) -> Vec<Diagnostic> {
    let mut run = Run::new(Vec::new(), Vec::new(), false);
    whole_file(&mut run, subject, nodes, infer);
    run.out
}

/// [`per_body`], collected on its own.
pub(crate) fn per_body_diagnostics<'a>(subject: &Subject<'a, '_>, nodes: &[Node<'a>], infer: &InferCache) -> Vec<Diagnostic> {
    let mut run = Run::new(Vec::new(), Vec::new(), false);
    per_body(&mut run, subject, nodes, infer);
    run.out
}

/// The checks that answer about the file as a unit. `nodes` must be every node of the file.
pub(crate) fn whole_file<'a>(run: &mut Run, subject: &Subject<'a, '_>, nodes: &[Node<'a>], infer: &InferCache) {
    let Subject { root, source, symbols, resolver, ctx } = *subject;
    run.check("unresolved_imports", || crate::source::imports::unresolved_imports(root, source, resolver, ctx.classpath_complete));
    run.check("import_ambiguity", || crate::source::import_ambiguity::import_ambiguity_errors_in(root, nodes, source, symbols, resolver, infer, ctx.classpath_complete));
    run.check("unresolved_static_imports", || crate::source::imports::unresolved_static_imports(root, source, resolver, ctx.classpath_complete));
    run.check("unresolved_types", || crate::typing::types::unresolved_types_in(nodes, source, symbols, resolver, ctx.classpath_complete));
    run.check("type_arg_arity", || crate::typing::type_arg_arity::type_arg_arity_errors_in(nodes, source, symbols, resolver));
    run.check("type_bounds", || crate::typing::bounds::bound_errors_in(nodes, source, symbols, resolver));
    run.check("undefined_var", || crate::calls::undefined_var::undefined_var_errors_in(root, nodes, source, symbols, resolver, ctx.classpath_complete));
    run.check("unresolved_call", || crate::calls::unresolved_call::unresolved_call_errors_in(root, nodes, source, symbols, resolver));
    run.check("inheritance", || crate::hierarchy::inheritance::inheritance_errors_in(nodes, source, symbols, resolver));
    run.check("missing_abstract", || crate::hierarchy::inheritance::missing_abstract_impls_in(nodes, source, symbols, resolver));
    run.check("super_constructor", || crate::decls::constructors::super_constructor_errors_in(nodes, source, symbols, resolver));
    run.check("final_override", || crate::flow::finals::final_override_errors_in(nodes, source, symbols, resolver));
    run.check("override_access", || crate::hierarchy::override_access::override_access_errors_in(nodes, source, symbols, resolver));
    run.check("override_return", || crate::hierarchy::override_return::override_return_errors_in(nodes, source, symbols, resolver));
    run.check("inherit_cycle", || crate::hierarchy::inherit_cycle::inherit_cycle_errors_in(nodes, source, symbols, resolver));
    run.check("pattern_dominance", || crate::switching::pattern_labels::pattern_dominance_in(nodes, source, symbols, resolver));
    run.check("type_access", || crate::typing::type_access::type_access_errors_in(nodes, source, symbols, resolver));
    run.check("implicit_super", || crate::throwing::implicit_super::implicit_super_errors_in(nodes, source, symbols, resolver));
    run.check("throws_widen", || crate::hierarchy::throws_widen::throws_widen_errors_in(nodes, source, symbols, resolver));
    run.check("static_access", || crate::hierarchy::static_access::static_access_errors_in(root, nodes, source, symbols, resolver));
    run.check("annotation_elements", || crate::annotation::annotation_elements::annotation_element_errors_in(nodes, source, symbols, resolver));
    run.check("annotation_target", || crate::annotation::annotation_target::annotation_target_errors_in(nodes, source, symbols, resolver));
    run.check("annotation_decl", || crate::annotation::annotation_decl::annotation_decl_errors_in(nodes, source, symbols, resolver));
}

/// The per-expression checks, over any bucket of the file's nodes — the whole file, or one body.
pub(crate) fn per_body<'a>(run: &mut Run, subject: &Subject<'a, '_>, nodes: &[Node<'a>], infer: &InferCache) {
    let Subject { root, source, symbols, resolver, ctx } = *subject;
    run.check("unknown_members", || crate::calls::members::unknown_members_in(root, nodes, source, symbols, resolver, infer));
    run.check("unknown_fields", || crate::calls::fields::unknown_fields_in(root, nodes, source, symbols, resolver, infer));
    run.check("arity", || crate::calls::arity::arity_errors_in(root, nodes, source, symbols, resolver, infer));
    run.check("argument_type", || crate::calls::arguments::argument_type_errors_in(root, nodes, source, symbols, resolver, infer));
    run.check("generic_args", || crate::calls::generic_args::generic_argument_errors_in(root, nodes, source, symbols, resolver, infer));
    run.check("type_compat", || crate::typing::casts::type_compat_errors_in(root, nodes, source, symbols, resolver, infer));
    run.check("functional", || crate::calls::functional::functional_errors_in(root, nodes, source, symbols, resolver, infer));
    run.check("qualified_finals", || crate::flow::qualified_finals::qualified_final_errors_in(root, nodes, source, symbols, resolver, infer));
    run.check("exceptions", || crate::throwing::exceptions::exception_errors_in(nodes, source, symbols, resolver));
    run.check("enum_switch", || crate::switching::enum_switch::enum_switch_errors_in(root, nodes, source, symbols, resolver, infer));
    run.check("switch_label_type", || crate::switching::switch_label_type::switch_label_type_errors_in(root, nodes, source, symbols, resolver, infer, ctx.java_major));
    run.check("super_method", || crate::hierarchy::super_method::super_method_errors_in(root, nodes, source, symbols, resolver, infer));
    run.check("condition_type", || crate::typing::condition_type::condition_type_errors_in(root, nodes, source, symbols, resolver, infer));
    run.check("operand_type", || crate::typing::operand_type::operand_type_errors_in(root, nodes, source, symbols, resolver, infer));
    run.check("instanceof_unconditional", || crate::typing::instanceof_unconditional::instanceof_unconditional_errors_in(root, nodes, source, symbols, resolver, infer, ctx.java_major));
    run.check("type_use", || crate::typing::type_use::type_use_errors_in(root, nodes, source, symbols, resolver, infer));
    run.check("narrowing", || crate::typing::narrowing::narrowing_errors_in(root, nodes, source, symbols, resolver, infer));
    run.check("checked_throw", || crate::throwing::checked_throw::checked_throw_errors_in(nodes, source, symbols, resolver));
    run.check("mref_throws", || crate::throwing::mref_throws::mref_throws_errors_in(root, nodes, source, symbols, resolver, infer));
    run.check("checked_call", || crate::throwing::checked_call::checked_call_errors_in(root, nodes, source, symbols, resolver, infer));
    run.check("dead_catch", || crate::throwing::dead_catch::dead_catch_errors_in(root, nodes, source, symbols, resolver, infer));
    run.check("static_via_instance", || crate::hierarchy::static_via_instance::static_via_instance_warnings_in(root, nodes, source, symbols, resolver, infer));
    run.check("visibility", || crate::hierarchy::visibility::visibility_errors_in(root, nodes, source, symbols, resolver, infer));
}
