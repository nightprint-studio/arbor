//! Action-property navigation + linting for JSP forms / OGNL and `*-validation.xml`.
//!
//! Resolves the field / OGNL reference under the caret to the **action class** it binds to, and from
//! there to that class's bean accessors, walking the action's **project `extends` chain** so an
//! accessor inherited from a `BaseAction` is found (go-to) and never falsely flagged (lint):
//!   * `bennu_action_property_target` — go-to from the field/root to the `get`/`set`/`is` accessor;
//!   * `bennu_action_property_lint` — a **warning** per field/root whose name is no property of the
//!     resolved action (the "parameter doesn't exist on the action" lint);
//!   * `bennu_jsp_actions` / `bennu_set_jsp_action` — the action-picker for a form-less **view** JSP:
//!     the reverse view→action candidates, and pinning which one the page's OGNL is checked against.
//!
//! Which action a JSP is bound to: a form's own `action=` for its fields; for a standalone OGNL
//! reference (a `%{prop}` NOT scoped `#…` and NOT a page variable), the user's pinned action, else
//! whatever the reverse lookup settles on ([`sole_answer`] — one candidate, or several that share
//! one implementation class, which is one answer written twice). Genuinely ambiguous (no pin, and
//! candidates disagreeing about the class) → OGNL stays silent.
//!
//! Conservative by construction (never a false positive): a lint hit needs the action to resolve to a
//! project class whose accessor set (own + inherited project supers) is non-empty; only OGNL `%{…}`
//! value-stack roots are linted (EL `${…}` scoped attributes and `#…` context/iterator vars are not).
//!
//! ## The value stack is a stack, and inside a loop it is deeper
//!
//! `<s:iterator value="comunicazioni.dati">` pushes the current element on top, so a bare name
//! written underneath it is a property of **that element** before it is anything of the action's.
//! Everything here therefore resolves top down — innermost element, each enclosing one, then the
//! action — and stops at the level that actually declares the name.
//!
//! For the check the same fact cuts the other way and has to be handled deliberately: a name
//! inside a loop whose element type could **not** be resolved is a name about which nothing is
//! known, so the check goes silent there rather than reporting it against the action it does not
//! belong to. "I cannot see that type" is not evidence that a property is missing, and the page
//! full of yellow that came of pretending otherwise is what taught this rule.
//!
//! Where the scopes and the bare-attribute expressions come from — and why the second is
//! go-to-only — is [`bennu_web::jsp_ognl`].

use std::collections::{BTreeSet, HashMap, HashSet};
use std::path::{Path, PathBuf};

use bennu_core::prelude::BennuState;
use bennu_proto::prelude::{
    ClassEntry, DeclarationTarget, Diagnostic, HoverInfo, JspActionBinding, JspActionOption,
    PropertyLintHit,
};
use bennu_web::prelude::{
    iterator_scopes, line_col, ognl_attr_path_at, ognl_path_at, parse_jsp_fields, parse_jsp_forms,
    parse_jsp_vars, parse_validation_text, scopes_at,
};
use serde::Deserialize;

use crate::action_props::{bean_property_names, find_property_member};
use crate::index_service::IndexService;

// ── args ─────────────────────────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct PropertyLintArgs {
    pub file: String,
    pub source: String,
}

#[derive(Deserialize)]
pub struct PropertyTargetArgs {
    pub file: String,
    pub source: String,
    pub offset: usize,
}

#[derive(Deserialize)]
pub struct JspActionsArgs {
    pub file: String,
}

#[derive(Deserialize)]
pub struct SetJspActionArgs {
    pub file: String,
    /// The action to pin, or `None`/empty to clear the pin (revert to auto-resolution).
    pub action: Option<String>,
}

// ── small helpers ──────────────────────────────────────────────────────────────

fn is_jsp(file: &str) -> bool {
    let f = file.to_ascii_lowercase();
    [".jsp", ".jspf", ".tag", ".tagx"].iter().any(|e| f.ends_with(e))
}
fn is_validation(file: &str) -> bool {
    file.to_ascii_lowercase().ends_with("-validation.xml")
}

/// The ROOT property of a (possibly nested / indexed) OGNL name: `user.name` → `user`,
/// `items[0]` → `items`, `plain` → `plain`.
fn property_root(name: &str) -> &str {
    let end = name.find(['.', '[', '(', ':', ' ']).unwrap_or(name.len());
    &name[..end]
}

/// One segment of a dotted name, with its `[0]` / `()` suffix removed: `items[0]` → `items`.
fn segment_name(seg: &str) -> &str {
    let end = seg.find(['[', '(', ':', ' ']).unwrap_or(seg.len());
    &seg[..end]
}

/// Split a dotted OGNL / field name at the caret: the segments **before** the one the caret is
/// on, and that segment.
///
/// `("ordine.cliente.nome", caret inside `cliente`)` → `(["ordine"], "cliente")`. A caret on the
/// first segment gives `([], "ordine")`, which is the behaviour everything had before paths were
/// followed at all.
///
/// `rel` is the caret's byte offset **within the name**. Out of range → the last segment, since a
/// caret just past the end of a name is still on its tail.
fn path_at(name: &str, rel: usize) -> (Vec<&str>, &str) {
    // Clamped first: a caret past the end belongs to the last segment, and without this the walk
    // below would fall off the end and answer with the whole path.
    let rel = rel.min(name.len());
    let mut before: Vec<&str> = Vec::new();
    let mut at = 0usize;
    let mut last = name;
    for seg in name.split('.') {
        let end = at + seg.len();
        if rel <= end {
            return (before, segment_name(seg));
        }
        before.push(segment_name(seg));
        at = end + 1; // the '.'
        last = seg;
    }
    // Unreachable for a clamped `rel` (the last segment always ends at `name.len()`), and an
    // answer rather than a panic if that ever stops being true.
    (before, segment_name(last))
}

/// Follow the leading segments of a dotted path from one class chain to the next, so the caret's
/// own segment is looked up on the class that actually declares it.
///
/// `ordine.cliente.nome` on an action: `ordine` is a property of the action whose declared type is
/// `Ordine`, `cliente` is a property of `Ordine` whose type is `Cliente`, and `nome` — the one under
/// the caret — is a property of `Cliente`. Every step is the same two questions asked again, which
/// is why this is a loop and not three cases.
///
/// Stops (returning `None`) the moment a step cannot be taken: a property with no accessor, a type
/// the project has no source for (a JDK or library class — go-to has nowhere to land), a `Map`
/// whose value type is anyone's guess. Stopping is the correct answer there; guessing which class a
/// name belongs to is how a go-to lands in the wrong file.
fn descend_path(
    svc: &IndexService,
    chain: Vec<(String, String)>,
    simple: String,
    before: &[&str],
) -> Option<(String, Vec<(String, String)>)> {
    let mut chain = chain;
    let mut simple = simple;
    // Bounded: a path is written by hand and `a.b.c.d.e.f` is already pathological, while a
    // self-referential type (`node.parent.parent…`) could otherwise walk as long as it is typed.
    // Every step logs its own outcome. A walk that stops has exactly four ways to stop, they are
    // indistinguishable from the outside (the gesture just does nothing), and the difference
    // between "that class has no such property" and "that class is in a jar" is the difference
    // between a typo and a limitation.
    for seg in before.iter().take(12) {
        if !is_plain_identifier(seg) {
            goto_log(format_args!("descend_path: '{seg}' is not a plain name — stop"));
            return None;
        }
        // The FILE the accessor was found in, not just its text: the type it names is resolved in
        // that file's context (its nested classes, its imports, its package), and a chain spans
        // several files — the property may come from a superclass three modules away.
        let Some((decl_file, decl_src, found)) = chain
            .iter()
            .find_map(|(f, src)| crate::action_props::find_property_type(src, seg).map(|t| (f, src, t)))
        else {
            goto_log(format_args!(
                "descend_path: '{simple}' declares no accessor for '{seg}' (searched {} source(s) \
                 in its chain) — stop",
                chain.len()
            ));
            return None;
        };
        let type_text = found.type_text;
        // `List<Ordine>` → `Ordine`: the interesting type inside the envelope, the same rule the
        // rest of the backend uses to see through a wrapper.
        let next = crate::index_service::element_type_of(&type_text);
        if next.is_empty() {
            goto_log(format_args!(
                "descend_path: '{seg}' is declared '{type_text}', which reduced to nothing — stop"
            ));
            return None;
        }
        let Some(fqcn) = resolve_type_in_context(&svc.all_project_classes(), decl_file, decl_src, &next)
        else {
            goto_log(format_args!(
                "descend_path: '{seg}' is a '{type_text}' -> '{next}', which no PROJECT class \
                 declares (a jar/JDK type has nowhere to land) — stop"
            ));
            return None;
        };
        let next_chain = class_chain(svc, &fqcn);
        if next_chain.is_empty() {
            goto_log(format_args!(
                "descend_path: '{seg}' resolved to '{fqcn}' but its source could not be read — stop"
            ));
            return None;
        }
        simple = fqcn.rsplit(['.', '$']).next().unwrap_or(&fqcn).to_string();
        goto_log(format_args!(
            "descend_path: '{seg}' : {type_text} -> '{fqcn}' ({} source(s))",
            next_chain.len()
        ));
        chain = next_chain;
    }
    Some((simple, chain))
}

/// The segments of an expression as written in a `value=` / `items=` attribute:
/// `%{elencoBandi}` → `["elencoBandi"]`, `${order.lines}` → `["order", "lines"]`.
///
/// Empty when the expression is not a plain path — a call, a literal, a comparison. Those are
/// answers the resolver has no way to type, and an empty list stops the walk rather than
/// starting it somewhere invented.
fn expr_segments(expr: &str) -> Vec<&str> {
    let inner = expr
        .trim()
        .trim_start_matches(['$', '%', '#'])
        .trim_start_matches('{')
        .trim_end_matches('}')
        .trim();
    // A path and nothing else. Checked on the WHOLE expression before it is split, because a
    // split hides what disqualifies it: `a == b` has one segment whose first word is a perfectly
    // good identifier, and reading `a` out of a comparison is how a resolver ends up confidently
    // typing a variable from an expression that says something else entirely.
    let is_path = !inner.is_empty()
        && inner
            .chars()
            .all(|c| c.is_alphanumeric() || matches!(c, '_' | '$' | '.' | '[' | ']'));
    if !is_path {
        return Vec::new();
    }
    let segments: Vec<&str> = inner.split('.').map(segment_name).collect();
    match segments.iter().all(|s| is_plain_identifier(s)) {
        true => segments,
        false => Vec::new(),
    }
}

/// The class chain for a **page variable** — the type of whatever declared it.
///
/// `<s:iterator value="%{elencoBandi}" var="bando">` is the only place a page says what `bando`
/// is, and everything written on `bando` underneath depends on reading it. The variable's
/// expression is walked from the action exactly like any other path ([`descend_path`]), which
/// also means the container is seen through: `elencoBandi` being a `List<Bando>` makes the
/// variable a `Bando`, which is what an iterator variable is.
///
/// `None` when the declaration names no expression, when the expression is not a plain path, or
/// when the walk stops — a variable whose type nothing states stays untyped rather than guessed.
fn chain_for_page_var(
    svc: &IndexService,
    file: &str,
    source: &str,
    decl: &bennu_web::prelude::JspVarDecl,
    action: &str,
) -> Option<(String, Vec<(String, String)>)> {
    let segments = expr_segments(&decl.source_expr);
    if segments.is_empty() {
        goto_log(format_args!(
            "chain_for_page_var: '{}' is declared by <{}> with no path expression ({:?}) — untyped",
            decl.name, decl.tag, decl.source_expr
        ));
        return None;
    }
    // From whatever is on top of the stack **where the declaration is written**, not from the
    // action: `<s:iterator value="celle" var="cella">` nested inside `<s:iterator value="righe">`
    // walks `righe`'s element's `celle`, and resolving `celle` on the action would type `cella`
    // as something the page never said. The declaration's own tag has not pushed yet — its
    // `var=` sits in the opening tag, before the body its scope covers — so this is the
    // enclosing scopes and only them.
    let all = iterator_scopes(source);
    let scopes = scopes_at(&all, decl.start);
    let (simple, chain) = chain_for_scopes(svc, file, action, &scopes)?;
    descend_path(svc, chain, simple, &segments)
}

/// The class chain of the element an iterator's body sees, walking the scopes outermost first.
///
/// `<s:iterator value="comunicazioni.dati">` pushes an element of `comunicazioni.dati` onto the
/// value stack, so inside it a bare name is a property of **that** class. And a nested loop's own
/// expression is relative to its parent's element — `<s:iterator value="celle">` inside
/// `<s:iterator value="righe">` walks `righe`'s element's `celle` — which is why the scopes are
/// folded in order rather than each resolved against the action.
///
/// `None` the moment one link cannot be typed: a scope whose expression is a call, or whose class
/// lives in a jar. That answer is load-bearing for the check — see [`stack_property_sets`].
fn chain_for_scopes(
    svc: &IndexService,
    file: &str,
    action: &str,
    scopes: &[&bennu_web::prelude::IteratorScope],
) -> Option<(String, Vec<(String, String)>)> {
    let (mut simple, mut chain) = resolve_bound_action(svc, file, action)?;
    for scope in scopes {
        let segments = expr_segments(&scope.source_expr);
        if segments.is_empty() {
            goto_log(format_args!(
                "chain_for_scopes: <s:{}> value={:?} is not a path — the scope stays untyped",
                scope.tag, scope.source_expr
            ));
            return None;
        }
        let (next_simple, next_chain) = descend_path(svc, chain, simple, &segments)?;
        simple = next_simple;
        chain = next_chain;
    }
    Some((simple, chain))
}

/// The property sets the value stack offers at `offset`, **top first**: the innermost element,
/// then each enclosing one, then the action itself.
///
/// `None` when any level could not be typed, and that is the whole point of the return type. A
/// name that is not on any *known* level is a name the check can flag; a name on a stack whose
/// top nobody could read is a name the check knows nothing about, and flagging it would be
/// guessing. The two cases have to be distinguishable, so they are different values rather than
/// an empty list.
fn stack_property_sets(
    svc: &IndexService,
    file: &str,
    action: &str,
    scopes: &[&bennu_web::prelude::IteratorScope],
) -> Option<Vec<(String, BTreeSet<String>)>> {
    let mut out = Vec::new();
    // Innermost first: `for depth in (1..=n).rev()` is the stack read from the top down.
    for depth in (1..=scopes.len()).rev() {
        let (simple, chain) = chain_for_scopes(svc, file, action, &scopes[..depth])?;
        out.push((simple, chain_property_set(&chain)));
    }
    Some(out)
}

/// Whether `root` is a plain Java identifier we can look up (a computed `%{…}`/`${…}` name is not).
fn is_plain_identifier(root: &str) -> bool {
    !root.is_empty() && root.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '$')
}

/// The forward-slashed key the JSP→action binding map is stored under.
fn binding_key(file: &str) -> String {
    file.replace('\\', "/")
}

/// Whether the JSP→action go-to diagnostic log is enabled (env `BENNU_GOTO_LOG` set at BE launch).
/// Gated so the per-keystroke lint/hover paths (which share the resolution helpers) stay silent —
/// only an explicit repro with the flag on emits the trace. Read once, then cached.
fn goto_log_enabled() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("BENNU_GOTO_LOG").is_some())
}

/// Emit one JSP→action go-to diagnostic line to **stderr** (stdout is the RPC protocol channel).
/// A no-op unless [`goto_log_enabled`]. Grep the BE console for `[bennu-goto]`.
fn goto_log(args: std::fmt::Arguments) {
    if goto_log_enabled() {
        eprintln!("[bennu-goto] {args}");
    }
}

/// The one action a reverse-lookup settles on with no pin, or `None` when the view is genuinely
/// ambiguous.
///
/// Not "exactly one candidate": several actions routinely share an implementation class — a legacy
/// config declares the same class under three namespaces, and a page reached three ways lists three
/// candidates — and the OGNL is checked against the **class**, so those are one answer written three
/// times. Requiring a single candidate left exactly the well-travelled pages unchecked.
///
/// Classes unknown → ambiguous: `None == None` must not read as agreement.
///
/// One function because the answer is consumed twice, by the lint and by the picker's `effective`.
/// Two copies of this rule means a toolbar that says which action it is checking against while the
/// checker stays silent.
fn sole_answer(cands: &[(String, Option<String>)]) -> Option<String> {
    let (first_qname, first_class) = cands.first()?;
    let one_class = first_class.is_some() && cands.iter().all(|(_, c)| c == first_class);
    (cands.len() == 1 || one_class).then(|| first_qname.clone())
}

/// The action a JSP's OGNL is bound to: the persisted pin, else what the candidates settle on
/// ([`sole_answer`]). `None` for a genuinely ambiguous view → OGNL stays silent (no false hits).
fn jsp_bound_action(svc: &IndexService, file: &str, source: &str) -> Option<String> {
    let cfg = bennu_core::config::load();
    if let Some(a) = cfg.jsp_action_bindings.get(&binding_key(file)) {
        goto_log(format_args!("jsp_bound_action: PINNED '{a}' (key '{}')", binding_key(file)));
        return Some(a.clone());
    }
    let cands = svc.jsp_action_candidates(file);
    goto_log(format_args!(
        "jsp_bound_action: no pin (key '{}'), {} reverse candidate(s): {:?}",
        binding_key(file),
        cands.len(),
        cands.iter().map(|(q, _)| q.as_str()).collect::<Vec<_>>(),
    ));
    if let Some(qname) = sole_answer(&cands) {
        return Some(qname);
    }
    // Fallback: the page's OWN form action. A self-posting `<form action="X">` (the norm in legacy
    // Struts) both renders FROM and submits TO action X, so the page's standalone OGNL `%{prop}` refs
    // (e.g. `<s:if test="%{elencoRiservato}">`) resolve against X's value stack — the same action its
    // form fields already bind to. Used only when the reverse view→action lookup didn't decide (0 or
    // ambiguous) and there's no pin, and only when the page names a SINGLE distinct form action.
    let actions: BTreeSet<String> = parse_jsp_forms(source)
        .into_iter()
        .filter_map(|f| f.action)
        .filter(|a| !a.trim().is_empty())
        .collect();
    goto_log(format_args!(
        "jsp_bound_action: form-action fallback, {} distinct form action(s): {:?}",
        actions.len(),
        actions
    ));
    if actions.len() == 1 {
        return actions.into_iter().next();
    }
    None
}

// ── action class chain (own + project superclasses) ─────────────────────────────

/// Resolve a superclass reference (`BaseAction` or `com.acme.BaseAction`) to a PROJECT class FQCN via
/// the class index — dotted names match by FQCN, bare names by simple. `None` for a library super.
fn resolve_super_fqcn(extends: &str, classes: &[ClassEntry]) -> Option<String> {
    let simple = extends.rsplit('.').next().unwrap_or(extends);
    classes
        .iter()
        .find(|c| c.fqcn == extends)
        .or_else(|| classes.iter().find(|c| c.simple == simple))
        .map(|c| c.fqcn.clone())
}

/// Resolve a type NAME **as written**, in the context of the file that wrote it.
///
/// A getter says `JspParam`, and what that means is a question about the file it is written in —
/// not about the project. In a legacy tree the same simple name is declared a dozen times: every
/// action carries its own nested `JspParam`, `Row`, `Item`, `Params`. Taking the first class in
/// the index with that simple name is how a property chain walks into a *different action's*
/// inner class and then reports, correctly and uselessly, that it has no such property.
///
/// So the ladder is Java's own, in Java's own order:
///
/// 1. the **same file** — a nested class, or a second top-level one. This is the case above, and
///    the one a simple-name index can never get right on its own;
/// 2. an explicit **import**;
/// 3. the file's own **package**;
/// 4. a **star import**;
/// 5. finally the project-wide simple name — but only when it is **unique**. A name that means
///    one thing everywhere is that thing; a name that means five is not guessable from here, and
///    guessing is what this function exists to stop.
fn resolve_type_in_context(
    classes: &[ClassEntry],
    decl_file: &str,
    decl_src: &str,
    name: &str,
) -> Option<String> {
    let simple = name.rsplit(['.', '$']).next().unwrap_or(name);
    // Written qualified, and the index knows it: there is nothing to resolve.
    if name.contains('.') {
        if let Some(c) = classes.iter().find(|c| c.fqcn == name) {
            return Some(c.fqcn.clone());
        }
    }
    if let Some(c) = classes.iter().find(|c| c.simple == simple && same_file(&c.file, decl_file)) {
        return Some(c.fqcn.clone());
    }
    let syms = bennu_java::prelude::extract_symbols(decl_src);
    if let Some(imp) = syms.imports.iter().find(|i| !i.star && i.simple_name() == Some(simple)) {
        if let Some(c) = classes.iter().find(|c| c.fqcn == imp.path) {
            return Some(c.fqcn.clone());
        }
    }
    if let Some(pkg) = &syms.package {
        let qualified = format!("{pkg}.{simple}");
        if let Some(c) = classes.iter().find(|c| c.fqcn == qualified) {
            return Some(c.fqcn.clone());
        }
    }
    for imp in syms.imports.iter().filter(|i| i.star && !i.static_) {
        let qualified = format!("{}.{simple}", imp.path);
        if let Some(c) = classes.iter().find(|c| c.fqcn == qualified) {
            return Some(c.fqcn.clone());
        }
    }
    let mut same_name = classes.iter().filter(|c| c.simple == simple);
    let first = same_name.next()?;
    if same_name.next().is_some() {
        goto_log(format_args!(
            "resolve_type_in_context: '{simple}' is declared by several project classes and \
             nothing in {decl_file} says which — stop rather than pick one"
        ));
        return None;
    }
    Some(first.fqcn.clone())
}

/// Same file, whatever the OS wrote the separators as.
fn same_file(a: &str, b: &str) -> bool {
    a.replace('\\', "/").eq_ignore_ascii_case(&b.replace('\\', "/"))
}

/// The declared superclass FQCN of the type named `simple` in `src`, resolved to a project class.
fn superclass_of(src: &str, simple: &str, classes: &[ClassEntry]) -> Option<String> {
    let syms = bennu_java::prelude::extract_symbols(src);
    let td = syms.types.iter().find(|t| t.name == simple).or_else(|| syms.types.first())?;
    resolve_super_fqcn(td.extends.as_deref()?, classes)
}

/// The action class `fqcn` plus every PROJECT superclass up its `extends` chain, each as
/// `(file, source)`. So an accessor inherited from a project `BaseAction` is included (go-to finds
/// its real declaration; the lint never flags it). Stops at the first library/unknown super (its
/// accessors — e.g. `ActionSupport`'s — are rarely form-bound, so missing them is a false-negative,
/// never a false positive). Empty when `fqcn` isn't a readable project class.
fn class_chain(svc: &IndexService, fqcn: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    // Search EVERY open project's class index (not just the JSP's own root): the JSP (webapp) and the
    // action class (a Java module) can live under separate roots, and the go-to must still find it.
    let classes = svc.all_project_classes();
    goto_log(format_args!(
        "class_chain: seek '{fqcn}' across {} indexed project class(es)",
        classes.len()
    ));
    if classes.is_empty() {
        return out;
    }
    let mut cur = fqcn.to_string();
    let mut seen = HashSet::new();
    let mut depth = 0;
    while depth < 20 && seen.insert(cur.clone()) {
        // The action's class FQCN from the config graph may not match the class-index FQCN
        // byte-for-byte (a Spring bean/proxy, a package alias, a nested `Outer$Inner` vs `Outer.Inner`
        // form). Fall back to the simple name so the chain still finds the class — mirroring
        // `resolve_super_fqcn`'s fqcn-then-simple lookup. Without this the whole chain is empty →
        // `resolve_action` returns None → JSP→action go-to silently fails (and the lint goes quiet).
        let entry = classes.iter().find(|c| c.fqcn == cur).or_else(|| {
            let simple = cur.rsplit(['.', '/', '$']).next().unwrap_or(cur.as_str());
            classes.iter().find(|c| c.simple == simple)
        });
        let Some(entry) = entry else {
            goto_log(format_args!(
                "class_chain: '{cur}' NOT found (by fqcn or simple name) — chain stops here"
            ));
            break;
        };
        goto_log(format_args!("class_chain: matched '{cur}' -> {} ({})", entry.fqcn, entry.file));
        // Read + decode in the PROJECT's source encoding — legacy trees are frequently Cp1252, and
        // `read_to_string` is UTF-8-only: a non-UTF-8 action class would fail the read here, break out
        // with an EMPTY chain, and silently kill BOTH the JSP→action go-to and the property lint (which
        // shares this chain). Decode via `decode_for_index` (declared encoding, recover on mismatch)
        // then normalize to LF so the accessor offset lands correctly in the editor's LF document.
        let Ok(bytes) = std::fs::read(&entry.file) else {
            goto_log(format_args!("class_chain: read failed for {}", entry.file));
            break;
        };
        let enc = svc
            .root_for_file(&entry.file)
            .map(|r| crate::index_service::resolve_index_encoding(&r))
            .unwrap_or_default();
        let src = bennu_project::prelude::normalize_newlines(
            &bennu_project::prelude::decode_for_index(&bytes, &enc).text,
        );
        let sup = superclass_of(&src, &entry.simple, &classes);
        out.push((entry.file.clone(), src));
        match sup {
            Some(s) => {
                cur = s;
                depth += 1;
            }
            None => break,
        }
    }
    out
}

/// The union of bean-property names across the action class chain (own + inherited project supers).
fn chain_property_set(chain: &[(String, String)]) -> BTreeSet<String> {
    let mut props = BTreeSet::new();
    for (_file, src) in chain {
        props.extend(bean_property_names(src));
    }
    props
}

/// A [`DeclarationTarget`] for `prop`'s accessor in the FIRST class of the chain that declares it —
/// so go-to lands on the actual (possibly inherited) declaration. `None` when no class has it.
fn target_in_chain(
    chain: &[(String, String)],
    action_simple: &str,
    prop: &str,
) -> Option<DeclarationTarget> {
    for (file, src) in chain {
        if let Some((start, end)) = find_property_member(src, prop) {
            let (line, col) = line_col(src, start);
            return Some(DeclarationTarget {
                file: file.clone(),
                start,
                end,
                line: line as u32,
                col: col as u32,
                label: format!("property `{prop}` on `{action_simple}`"),
            });
        }
    }
    None
}

/// Resolve an action reference (a form's `action=`, or a view's bound action) → (simple-name, class
/// chain). Reuses the form→class resolution. `None` when the action isn't a readable project class.
fn resolve_action(svc: &IndexService, file: &str, action: &str) -> Option<(String, Vec<(String, String)>)> {
    let (fqcn, _config, _writable, _validated) = svc.form_action_context(file, action);
    let fqcn = fqcn?;
    let chain = class_chain(svc, &fqcn);
    if chain.is_empty() {
        return None;
    }
    let simple = fqcn.rsplit('.').next().unwrap_or(&fqcn).to_string();
    Some((simple, chain))
}

/// Resolve a **bound** action (a pinned / reverse-lookup qname) to its (simple, chain). Prefers the
/// FQCN the candidate list ([`IndexService::jsp_action_candidates`]) already resolved for that qname
/// — the exact one the picker showed the user — before the config-graph action-ref resolution, so a
/// pinned action that `form_action_context` wouldn't re-resolve from the qname still navigates.
fn resolve_bound_action(
    svc: &IndexService,
    file: &str,
    action: &str,
) -> Option<(String, Vec<(String, String)>)> {
    let from_cand = svc
        .jsp_action_candidates(file)
        .into_iter()
        .find(|(q, _)| q == action)
        .and_then(|(_, fqcn)| fqcn);
    let from_ctx = if from_cand.is_none() { svc.form_action_context(file, action).0 } else { None };
    goto_log(format_args!(
        "resolve_bound_action: action='{action}' from_candidate={from_cand:?} from_config_graph={from_ctx:?}"
    ));
    let fqcn = from_cand.or(from_ctx)?;
    let chain = class_chain(svc, &fqcn);
    if chain.is_empty() {
        goto_log(format_args!("resolve_bound_action: fqcn='{fqcn}' resolved but class chain is EMPTY"));
        return None;
    }
    let simple = fqcn.rsplit('.').next().unwrap_or(&fqcn).to_string();
    Some((simple, chain))
}

/// The (simple, chain) a `*-validation.xml` binds to (by the filename convention).
fn resolve_validation(svc: &IndexService, file: &str) -> Option<(String, Vec<(String, String)>)> {
    let ctx = svc.validation_context(file);
    let fqcn = ctx.action_fqcn?;
    let chain = class_chain(svc, &fqcn);
    if chain.is_empty() {
        return None;
    }
    let simple = fqcn.rsplit('.').next().unwrap_or(&fqcn).to_string();
    Some((simple, chain))
}

/// Classify the OGNL/EL delimiter just before a ref at byte `start`: whether it opened with OGNL
/// `%{` (value stack → action) and whether the name was `#`-scoped (a context / iterator var, never
/// an action property). Only a bare, un-scoped `%{root}` safely maps to an action property.
fn ognl_ref_kind(source: &str, start: usize) -> (bool, bool) {
    let pre = source[..start.min(source.len())].as_bytes();
    let scoped = pre.last() == Some(&b'#');
    let brace = if scoped { pre.len().checked_sub(2) } else { pre.len().checked_sub(1) };
    let opener = brace.and_then(|i| {
        if pre.get(i) == Some(&b'{') {
            i.checked_sub(1).and_then(|j| pre.get(j)).copied()
        } else {
            None
        }
    });
    (opener == Some(b'%'), scoped)
}

/// A per-action cache of `(simple-name, property set)` so a JSP with many fields/refs resolves each
/// distinct action's class chain ONCE. `None` is cached for an unresolvable action.
type ActionCache = std::collections::HashMap<String, Option<(String, BTreeSet<String>)>>;

/// The `(simple, props)` of `action`, resolved-and-cached. `None` when it isn't a project class.
fn props_for<'a>(
    svc: &IndexService,
    file: &str,
    action: &str,
    cache: &'a mut ActionCache,
) -> Option<&'a (String, BTreeSet<String>)> {
    cache
        .entry(action.to_string())
        .or_insert_with(|| {
            // `resolve_bound_action` is a superset of `resolve_action` (candidate-FQCN first, then the
            // config-graph resolution), so a pinned/reverse-lookup qname resolves here too — keeping
            // the lint's property set in lock-step with what go-to can reach.
            resolve_bound_action(svc, file, action).map(|(s, chain)| (s, chain_property_set(&chain)))
        })
        .as_ref()
}

/// Every JSP field paired with the action it binds to: a field inside a `<form action="Y">` → Y; a
/// field with NO enclosing form (a fragment spliced into a parent page's form) → the JSP's `bound`
/// action (which may be inherited from an including page). Deduped by span, so a form field is never
/// also reported as a standalone one.
fn jsp_fields_with_action(source: &str, bound: Option<&str>) -> Vec<(String, usize, usize, String)> {
    let mut out = Vec::new();
    let mut covered: HashSet<(usize, usize)> = HashSet::new();
    for form in parse_jsp_forms(source) {
        let action = form.action;
        for f in form.fields {
            covered.insert((f.start, f.end));
            if let Some(a) = action.clone().or_else(|| bound.map(str::to_string)) {
                out.push((f.name, f.start, f.end, a));
            }
        }
    }
    if let Some(b) = bound {
        for f in parse_jsp_fields(source) {
            if covered.insert((f.start, f.end)) {
                // not already a form field → a standalone field (a fragment of a parent's form)
                out.push((f.name, f.start, f.end, b.to_string()));
            }
        }
    }
    out
}

// ── go-to ───────────────────────────────────────────────────────────────────────

/// Go-to from a JSP form field / OGNL root / validation `<field>` under the caret to the action
/// property's accessor. `None` (never an error) when the caret isn't on a resolvable field.
#[arbor_rpc::handler]
fn bennu_action_property_target(
    _ctx: &BennuState,
    args: PropertyTargetArgs,
) -> Result<Option<DeclarationTarget>, String> {
    let svc = IndexService::global();
    let Some((prop, simple, chain)) = resolve_property_at(svc, &args.file, &args.source, args.offset)
    else {
        return Ok(None);
    };
    let target = target_in_chain(&chain, &simple, &prop);
    if target.is_none() {
        goto_log(format_args!(
            "bennu_action_property_target: '{prop}' resolved to class '{simple}' but NO accessor \
             (get/is/set) found for it in the class chain"
        ));
    }
    Ok(target)
}

/// Hover on a JSP form field / OGNL root / validation `<field>` → the action property's **type**
/// (`String customer`, `List<Item> items`, …) and its owning action class. `None` (never an error)
/// when the caret isn't on a resolvable field, or its type can't be read from the class chain.
#[arbor_rpc::handler]
fn bennu_action_property_hover(
    _ctx: &BennuState,
    args: PropertyTargetArgs,
) -> Result<Option<HoverInfo>, String> {
    let svc = IndexService::global();
    let Some((prop, simple, chain)) = resolve_property_at(svc, &args.file, &args.source, args.offset)
    else {
        return Ok(None);
    };
    Ok(hover_in_chain(&chain, &simple, &prop))
}

/// Resolve the field / OGNL reference / validation `<field>` under the caret to
/// `(property_name, owner_simple_name, owner_class_chain)`.
///
/// **The caret's own segment**, not the head of the path: on `ordine.cliente.nome` the owner is
/// whichever class the segments before it lead to ([`descend_path`]), so everything downstream —
/// go-to, hover — asks the right class without knowing a path was walked at all.
///
/// The shared front half of go-to ([`bennu_action_property_target`]) and hover
/// ([`bennu_action_property_hover`]) — they differ only in what they do with the resolved chain.
/// `None` when the caret isn't on a resolvable reference.
fn resolve_property_at(
    svc: &IndexService,
    file: &str,
    source: &str,
    offset: usize,
) -> Option<(String, String, Vec<(String, String)>)> {
    if is_validation(file) {
        let rec = parse_validation_text(Path::new(file), source)?;
        let field = rec
            .fields
            .iter()
            .find(|f| offset >= f.name_offset && offset <= f.name_offset + f.name.len())?;
        // `<field name="ordine.cliente">` is a path like any other, and the caret's segment is
        // the question — the same rule the JSP branches below follow.
        let (before, seg) = path_at(&field.name, offset.saturating_sub(field.name_offset));
        if !is_plain_identifier(seg) {
            return None;
        }
        let (simple, chain) = resolve_validation(svc, file)?;
        let (simple, chain) = descend_path(svc, chain, simple, &before)?;
        return Some((seg.to_string(), simple, chain));
    }

    if is_jsp(file) {
        goto_log(format_args!("resolve_property_at: JSP '{file}' offset={offset}"));
        let bound = jsp_bound_action(svc, file, source);
        // A FIELD under the caret — a form field (→ its form's action) or a standalone field spliced
        // into a parent's form (→ the inherited bound action).
        for (name, start, end, action) in jsp_fields_with_action(source, bound.as_deref()) {
            if offset >= start && offset <= end {
                // A field name is a PATH as often as it is a name — `ordine.cliente.nome` binds
                // three classes deep — so the caret's own segment is what resolves, on the class
                // the segments before it lead to.
                let (before, seg) = path_at(&name, offset.saturating_sub(start));
                goto_log(format_args!(
                    "resolve_property_at: caret on FORM FIELD '{name}' (segment '{seg}', {} before) -> action '{action}'",
                    before.len()
                ));
                if !is_plain_identifier(seg) {
                    return None;
                }
                let (simple, chain) = resolve_action(svc, file, &action)?;
                let (simple, chain) = descend_path(svc, chain, simple, &before)?;
                return Some((seg.to_string(), simple, chain));
            }
        }
        // A standalone OGNL reference (a `%{prop}` that isn't a page variable) → the bound action.
        //
        // The **path**, not the reference: a reference is only ever the root identifier (that is
        // what a page variable's find-usages must count), so on `%{a.b.c}` the only ref is `a` and
        // a caret on `c` matched nothing at all — which is why nested go-to did nothing rather
        // than something wrong.
        let vars = parse_jsp_vars(source);
        let declared: HashSet<&str> = vars.decls.iter().map(|d| d.name.as_str()).collect();
        // Delimited first, then the bare Struts attribute. In that order because a `%{…}` inside
        // an attribute value is both, and the delimited scanner is the one that owns it — see
        // `bennu_web::jsp_ognl` for why the bare form exists at all.
        let hit = ognl_path_at(source, offset).or_else(|| ognl_attr_path_at(source, offset));
        goto_log(format_args!(
            "resolve_property_at: OGNL branch, caret path={:?}",
            hit.as_ref().map(|p| (
                p.segments.iter().map(|s| s.name.as_str()).collect::<Vec<_>>(),
                p.at
            )),
        ));
        if let Some(path) = &hit {
            let before: Vec<&str> =
                path.segments[..path.at].iter().map(|s| s.name.as_str()).collect();
            let seg = path.segment().name.as_str();
            // The ROOT decides whether this is a page variable, whichever segment is under the
            // caret: `x` being a `<c:set>` makes `x.y` the page's business, not the action's.
            let is_declared = declared.contains(path.root().name.as_str());
            let plain = is_plain_identifier(seg);
            goto_log(format_args!(
                "resolve_property_at: segment='{seg}' ({} before) page_var={is_declared} plain_ident={plain} bound={bound:?}",
                before.len(),
            ));
            // The root is a page variable AND the caret is on a segment past it: the variable
            // stands in for a class, and the segment is a property of THAT class. `bando.titolo`
            // inside `<s:iterator value="%{elencoBandi}" var="bando">` is a question about
            // `Bando`, and refusing it because `bando` is not an action property answers a
            // question nobody asked. (The root itself stays the page's own business — go-to on
            // `bando` belongs to the declaration, and `bennu_jsp_nav` already owns that.)
            if is_declared && plain && path.at > 0 {
                if let (Some(action), Some(decl)) =
                    (&bound, vars.decls.iter().find(|d| d.name == path.root().name))
                {
                    if let Some((simple, chain)) = chain_for_page_var(svc, file, source, decl, action)
                        .and_then(|(simple, chain)| {
                            descend_path(svc, chain, simple, &before[1..])
                        })
                    {
                        goto_log(format_args!(
                            "resolve_property_at: OK -> property '{seg}' on '{simple}' via page var '{}'",
                            decl.name
                        ));
                        return Some((seg.to_string(), simple, chain));
                    }
                }
            }
            if !is_declared && plain {
                match &bound {
                    Some(action) => {
                        // The value stack, read top down: inside `<s:iterator value="rows">` a
                        // bare name is a property of a `rows` ELEMENT before it is anything of
                        // the action's, because that is the order Struts resolves it in. Outside
                        // a loop the list is empty and this is the plain action lookup it always
                        // was.
                        let all = iterator_scopes(source);
                        let scopes = scopes_at(&all, path.root().start);
                        for depth in (0..=scopes.len()).rev() {
                            let Some((simple, chain)) =
                                chain_for_scopes(svc, file, action, &scopes[..depth])
                                    .and_then(|(s, c)| descend_path(svc, c, s, &before))
                            else {
                                continue;
                            };
                            // Only the level that actually declares it — otherwise the innermost
                            // element would answer for every name, and go-to on an action
                            // property inside a loop would land nowhere.
                            if !chain_property_set(&chain).contains(seg) {
                                continue;
                            }
                            goto_log(format_args!(
                                "resolve_property_at: OK -> property '{seg}' on '{simple}' \
                                 ({depth} iterator scope(s) deep, chain of {})",
                                chain.len()
                            ));
                            return Some((seg.to_string(), simple, chain));
                        }
                        goto_log(format_args!(
                            "resolve_property_at: '{seg}' is on no level of the value stack for \
                             action '{action}' ({} iterator scope(s) here)",
                            scopes.len()
                        ));
                    }
                    None => goto_log(format_args!(
                        "resolve_property_at: no bound action for this JSP -> cannot resolve OGNL"
                    )),
                }
            }
        }
    }

    None
}

/// Build the hover card for property `prop` from the action class chain: the first source in the
/// chain that declares an accessor for it supplies the type. `None` when no accessor backs it.
fn hover_in_chain(chain: &[(String, String)], simple: &str, prop: &str) -> Option<HoverInfo> {
    for (_file, src) in chain {
        if let Some(pt) = crate::action_props::find_property_type(src, prop) {
            // "property", not "action property": on a path (`ordine.cliente.nome`) the owner is
            // whatever class the walk landed on, and `container` below already names it.
            let kind = if pt.read { "property" } else { "property (write-only)" };
            return Some(HoverInfo {
                signature: format!("{} {}", pt.type_text, prop),
                kind: kind.to_string(),
                container: Some(simple.to_string()),
                doc: None,
            });
        }
    }
    None
}

// ── lint ─────────────────────────────────────────────────────────────────────────

fn push_if_unknown(
    out: &mut Vec<PropertyLintHit>,
    props: &BTreeSet<String>,
    simple: &str,
    name: &str,
    start: usize,
    end: usize,
) {
    let root = property_root(name);
    if is_plain_identifier(root) && !props.contains(root) {
        out.push(PropertyLintHit { start, end, name: name.to_string(), action: simple.to_string() });
    }
}

/// A warning for every field / OGNL root whose ROOT property name is no bean property of the resolved
/// action class (own + inherited project supers). Empty (never an error) when the action / its
/// property set can't be resolved — the conservative "unknown = silent" rule.
#[arbor_rpc::handler]
fn bennu_action_property_lint(
    _ctx: &BennuState,
    args: PropertyLintArgs,
) -> Result<Vec<PropertyLintHit>, String> {
    let svc = IndexService::global();
    let mut out = Vec::new();

    if is_validation(&args.file) {
        let Some(rec) = parse_validation_text(Path::new(&args.file), &args.source) else {
            return Ok(out);
        };
        let Some((simple, chain)) = resolve_validation(svc, &args.file) else {
            return Ok(out);
        };
        let props = chain_property_set(&chain);
        if props.is_empty() {
            return Ok(out);
        }
        for f in &rec.fields {
            push_if_unknown(&mut out, &props, &simple, &f.name, f.name_offset, f.name_offset + f.name.len());
        }
        return Ok(out);
    }

    if is_jsp(&args.file) {
        let bound = jsp_bound_action(svc, &args.file, &args.source);
        let mut cache = ActionCache::new();
        // Every field (form-bound or standalone-in-a-fragment) → the action it binds to.
        for (name, start, end, action) in jsp_fields_with_action(&args.source, bound.as_deref()) {
            if let Some((simple, props)) = props_for(svc, &args.file, &action, &mut cache) {
                if !props.is_empty() {
                    push_if_unknown(&mut out, props, simple, &name, start, end);
                }
            }
        }
        // Standalone OGNL value-stack roots (`%{root}`, un-scoped, not a page var) → the bound action.
        // EL `${…}` scoped attributes and `#…` context/iterator vars are NOT linted (false-positive
        // prone), so only the safe subset is checked.
        if let Some(action) = &bound {
            if let Some((simple, props)) = props_for(svc, &args.file, action, &mut cache).cloned() {
                if !props.is_empty() {
                    let vars = parse_jsp_vars(&args.source);
                    let declared: HashSet<&str> =
                        vars.decls.iter().map(|d| d.name.as_str()).collect();
                    // Where an `<s:iterator>` (or `push`/`bean`) has an element on top of the
                    // stack. Inside one, a bare name is that element's property before it is the
                    // action's, and a check that did not know it reported every name in every
                    // loop — a page of yellow, which is a warning nobody reads twice.
                    let scopes = iterator_scopes(&args.source);
                    // Keyed by the innermost scope's body start, which names one loop exactly:
                    // two sibling iterators are both "depth 1" and walk different classes, so a
                    // depth-keyed cache would answer one of them with the other's properties.
                    let mut stack_cache: HashMap<usize, Option<Vec<(String, BTreeSet<String>)>>> =
                        HashMap::new();
                    for r in &vars.refs {
                        if declared.contains(r.name.as_str()) {
                            continue;
                        }
                        let (ognl, scoped) = ognl_ref_kind(&args.source, r.start);
                        if !ognl || scoped {
                            continue;
                        }
                        let here = scopes_at(&scopes, r.start);
                        if here.is_empty() {
                            push_if_unknown(&mut out, &props, &simple, &r.name, r.start, r.end);
                            continue;
                        }
                        // Every reference inside the same loop asks the same question, and a
                        // legacy table has hundreds of them.
                        let key = here.last().map(|s| s.body_start).unwrap_or(0);
                        let levels = stack_cache
                            .entry(key)
                            .or_insert_with(|| stack_property_sets(svc, &args.file, action, &here))
                            .clone();
                        // A level nobody could type is not evidence that a property is missing —
                        // so the check goes quiet here, and only here.
                        let Some(levels) = levels else { continue };
                        let root = property_root(&r.name);
                        if levels.iter().any(|(_, p)| p.contains(root)) {
                            continue;
                        }
                        // On no element and not on the action either: a real one.
                        push_if_unknown(&mut out, &props, &simple, &r.name, r.start, r.end);
                    }
                }
            }
        }
    }

    Ok(out)
}

// ── action picker (reverse view→action) ─────────────────────────────────────────

/// The action-binding state for a JSP view: reverse-lookup candidates + the pinned + effective action.
#[arbor_rpc::handler]
fn bennu_jsp_actions(_ctx: &BennuState, args: JspActionsArgs) -> Result<JspActionBinding, String> {
    let svc = IndexService::global();
    let raw = svc.jsp_action_candidates(&args.file);
    // Resolved off the raw list, before it becomes wire data, so the picker's "effective" and the
    // lint's binding are the same decision made once.
    let auto = sole_answer(&raw);
    let candidates: Vec<JspActionOption> = raw
        .into_iter()
        .map(|(qname, class_fqcn)| {
            let simple = class_fqcn
                .as_deref()
                .map(|f| f.rsplit('.').next().unwrap_or(f).to_string())
                .unwrap_or_else(|| qname.rsplit('/').next().unwrap_or(&qname).to_string());
            JspActionOption { qname, class_fqcn, simple }
        })
        .collect();
    let cfg = bennu_core::config::load();
    let bound = cfg.jsp_action_bindings.get(&binding_key(&args.file)).cloned();
    let effective = bound.clone().or(auto);
    Ok(JspActionBinding { candidates, bound, effective })
}

/// Pin (or, with an empty/absent `action`, clear) which Struts action a JSP view's OGNL is checked
/// and navigated against. Persisted in the profile's bennu config.
#[arbor_rpc::handler]
fn bennu_set_jsp_action(_ctx: &BennuState, args: SetJspActionArgs) -> Result<bool, String> {
    let mut cfg = bennu_core::config::load();
    let key = binding_key(&args.file);
    match args.action {
        Some(a) if !a.is_empty() => {
            cfg.jsp_action_bindings.insert(key, a);
        }
        _ => {
            cfg.jsp_action_bindings.remove(&key);
        }
    }
    bennu_core::config::save(&cfg)?;
    Ok(true)
}

// ── Struts config XML: <result> navigation + linting ────────────────────────────────
//
// A `<result>` body is either a JSP path (`/WEB-INF/x.jsp`) or an OGNL/EL reference (`${urlErrori}` /
// `%{root}`) evaluated against the OWNING action's value stack. So the same two affordances the JSP
// side has apply here: go-to (open the JSP / jump to the action property) and a conservative lint
// (the JSP file doesn't exist / the OGNL root isn't a property of the action).

/// One `<result>` scanned from a struts XML buffer: the owning action's qualified name (namespace +
/// name, when resolvable), the trimmed target body, and its byte span (for go-to / squiggle).
struct StrutsResult {
    action_qname: Option<String>,
    target: String,
    target_start: usize,
    target_end: usize,
}

/// Scan a struts config buffer for `<result …>BODY</result>` elements, tracking the enclosing
/// `<package namespace="…">` and `<action name="…">` so each result carries its owning action's
/// qualified name. Body-only results (the common shape); a `<param>`-based result (body contains
/// `<`) is emitted with that raw body and skipped by the callers. Text scan (no XML parser) over the
/// LIVE buffer, so offsets match the editor exactly.
fn scan_struts_results(source: &str) -> Vec<StrutsResult> {
    let mut out = Vec::new();
    let mut namespace: Vec<String> = Vec::new(); // one per open <package>
    let mut action: Option<String> = None;
    let mut i = 0usize;
    while let Some(rel) = source[i..].find('<') {
        let ts = i + rel;
        let Some(gt) = source[ts..].find('>') else { break };
        let te = ts + gt; // index of '>'
        let tag = &source[ts..=te];
        let lower = tag.to_ascii_lowercase();
        if lower.starts_with("<package") {
            namespace.push(tag_attr(tag, "namespace").unwrap_or_default());
        } else if lower.starts_with("</package") {
            namespace.pop();
        } else if lower.starts_with("<action") {
            action = tag_attr(tag, "name");
        } else if lower.starts_with("</action") {
            action = None;
        } else if lower.starts_with("<result") && !lower.ends_with("/>") {
            let body_start = te + 1;
            if let Some(cr) = source[body_start..].find("</result") {
                let body_end = body_start + cr;
                let raw = &source[body_start..body_end];
                let trimmed = raw.trim();
                if !trimmed.is_empty() {
                    let lead = raw.len() - raw.trim_start().len();
                    let start = body_start + lead;
                    let qname = action.as_ref().map(|n| qualify_action(n, namespace.last()));
                    out.push(StrutsResult {
                        action_qname: qname,
                        target: trimmed.to_string(),
                        target_start: start,
                        target_end: start + trimmed.len(),
                    });
                }
                i = body_end;
                continue;
            }
        }
        i = te + 1;
    }
    out
}

/// The action qualified-name from an `<action name>` + its enclosing `<package namespace>`: an
/// absolute name (leading `/`) or an empty namespace is left as-is; else `namespace/name`.
fn qualify_action(name: &str, namespace: Option<&String>) -> String {
    let ns = namespace.map(String::as_str).unwrap_or("");
    if name.starts_with('/') || ns.is_empty() {
        name.to_string()
    } else {
        format!("{}/{}", ns.trim_end_matches('/'), name)
    }
}

/// The value of attribute `attr` in a single tag's text (`<result name="input" …>` → `input`).
/// Quote-aware, case-insensitive on the attribute name. `None` when absent.
fn tag_attr(tag: &str, attr: &str) -> Option<String> {
    let lower = tag.to_ascii_lowercase();
    let mut from = 0;
    while let Some(rel) = lower[from..].find(attr) {
        let at = from + rel;
        let before_ok = at == 0 || {
            let c = tag.as_bytes()[at - 1];
            c.is_ascii_whitespace() || c == b'<'
        };
        let mut j = at + attr.len();
        let b = tag.as_bytes();
        while j < tag.len() && b[j].is_ascii_whitespace() {
            j += 1;
        }
        if before_ok && j < tag.len() && b[j] == b'=' {
            j += 1;
            while j < tag.len() && b[j].is_ascii_whitespace() {
                j += 1;
            }
            if j < tag.len() && (b[j] == b'"' || b[j] == b'\'') {
                let q = b[j];
                let vs = j + 1;
                let mut k = vs;
                while k < tag.len() && b[k] != q {
                    k += 1;
                }
                return Some(tag[vs..k].to_string());
            }
        }
        from = at + attr.len();
    }
    None
}

/// The plain-identifier OGNL/EL root of a result body (`${urlErrori}` / `%{user}` → `urlErrori` /
/// `user`), or `None` when the body isn't a simple `${…}` / `%{…}` wrapping a plain identifier root
/// (a JSP path, a Tiles def name, a computed/complex expression).
fn ognl_result_root(target: &str) -> Option<&str> {
    let t = target.trim();
    let inner = t
        .strip_prefix("${")
        .or_else(|| t.strip_prefix("%{"))?
        .strip_suffix('}')?;
    let root = property_root(inner.trim());
    is_plain_identifier(root).then_some(root)
}

/// Whether a result target looks like a JSP path (an absolute path ending `.jsp`/`.jspf`).
fn is_jsp_result_path(target: &str) -> bool {
    let t = target.trim();
    t.starts_with('/') && {
        let l = t.to_ascii_lowercase();
        l.ends_with(".jsp") || l.ends_with(".jspf")
    }
}

/// The existing webapp base directories under a project root (Maven `src/main/webapp` + the common
/// alternatives), so a `/WEB-INF/x.jsp` result path resolves to a real file. Empty when the project
/// has no recognizable webapp dir — in which case the "JSP not found" lint stays silent (we can't
/// know where JSPs live, so flagging would risk a false positive).
fn webapp_bases(root: &Path) -> Vec<PathBuf> {
    crate::web_discovery::webapp_dirs(root)
}

/// Resolve a struts result JSP path to a real file under one of the webapp `bases` (else the project
/// root). `None` when no base holds it.
fn resolve_jsp_result(root: &Path, bases: &[PathBuf], jsp_ref: &str) -> Option<PathBuf> {
    let rel = jsp_ref.trim_start_matches('/');
    for base in bases {
        let cand = base.join(rel);
        if cand.is_file() {
            return Some(cand);
        }
    }
    let at_root = root.join(rel);
    at_root.is_file().then_some(at_root)
}

/// Go-to on a struts `<result>` body under the caret: a JSP path opens the JSP; an OGNL/EL root
/// (`${prop}`) jumps to the owning action's property accessor. `None` when the caret isn't on a
/// resolvable result target.
#[arbor_rpc::handler]
fn bennu_struts_result_target(
    _ctx: &BennuState,
    args: PropertyTargetArgs,
) -> Result<Option<DeclarationTarget>, String> {
    if is_validation(&args.file) || !args.file.to_ascii_lowercase().ends_with(".xml") {
        return Ok(None);
    }
    let svc = IndexService::global();
    Ok(struts_result_target_at(svc, &args.file, &args.source, args.offset))
}

/// Resolve the struts `<result>` target under `offset` to its go-to location (JSP file top, or the
/// owning action's property accessor). `Option`-returning so it can use `?` freely; the handler
/// wraps it. `None` when the caret isn't on a resolvable result target.
fn struts_result_target_at(
    svc: &IndexService,
    file: &str,
    source: &str,
    offset: usize,
) -> Option<DeclarationTarget> {
    for r in scan_struts_results(source) {
        if offset < r.target_start || offset > r.target_end || r.target.contains('<') {
            continue;
        }
        // An OGNL/EL root → the owning action's property accessor; if that root isn't a bean property
        // (it may be a request attribute the action sets, not a getter/setter), fall back to the
        // owning action's DECLARATION so the gesture still navigates "to the action".
        if let Some(root) = ognl_result_root(&r.target) {
            let action = r.action_qname.as_deref()?;
            if let Some((simple, chain)) = resolve_action(svc, file, action) {
                if let Some(t) = target_in_chain(&chain, &simple, root) {
                    return Some(t);
                }
            }
            let def = svc.definition_action(file, action)?;
            let config = (!def.config_file.is_empty()).then(|| def.config_file.replace('\\', "/"))?;
            return Some(DeclarationTarget {
                file: config,
                start: def.config_offset,
                end: def.config_offset,
                line: 0,
                col: 0,
                label: format!("action `{action}`"),
            });
        }
        // A JSP path → the JSP file (top of file).
        if is_jsp_result_path(&r.target) {
            let root = svc.root_for_file(file)?;
            let rp = Path::new(&root);
            let target = resolve_jsp_result(rp, &webapp_bases(rp), &r.target)?;
            return Some(DeclarationTarget {
                file: target.to_string_lossy().replace('\\', "/"),
                start: 0,
                end: 0,
                line: 0,
                col: 0,
                label: format!("JSP result `{}`", r.target),
            });
        }
        return None;
    }
    None
}

/// Conservative lint over a struts config buffer's `<result>` targets: a JSP path that resolves to no
/// file under the project's webapp dir → "JSP not found"; an OGNL/EL root that isn't a property of the
/// owning action → "not a property of action". Never a false positive: JSP-not-found needs a known
/// webapp dir; OGNL-not-found needs the action to resolve to a project class with a known property set.
#[arbor_rpc::handler]
fn bennu_struts_result_lint(_ctx: &BennuState, args: PropertyLintArgs) -> Result<Vec<Diagnostic>, String> {
    if is_validation(&args.file) || !args.file.to_ascii_lowercase().ends_with(".xml") {
        return Ok(Vec::new());
    }
    // Cheap gate: nothing to do unless the buffer actually has results.
    if !args.source.contains("<result") {
        return Ok(Vec::new());
    }
    let svc = IndexService::global();
    let root = svc.root_for_file(&args.file);
    let bases = root.as_ref().map(|r| webapp_bases(Path::new(r))).unwrap_or_default();
    let mut cache: ActionCache = ActionCache::new();
    // Lazily-discovered project JSP paths (forward-slashed) — the safety net so a JSP that lives
    // OUTSIDE the primary webapp dir (a multi-module / unusual layout) is never mis-flagged missing.
    // Built at most once per lint, and only if some result path misses the webapp dirs.
    let mut project_jsps: Option<Vec<String>> = None;
    let mut out = Vec::new();

    for r in scan_struts_results(&args.source) {
        if r.target.contains('<') {
            continue;
        }
        if let Some(prop) = ognl_result_root(&r.target) {
            if let Some(action) = &r.action_qname {
                if let Some((simple, props)) = props_for(svc, &args.file, action, &mut cache) {
                    if !props.contains(prop) {
                        out.push(Diagnostic {
                            message: format!("`{prop}` is not a property of action `{simple}`"),
                            severity: "warning".to_string(),
                            code: "action-property-missing".to_string(),
                            start: r.target_start,
                            end: r.target_end,
                        });
                    }
                }
            }
        } else if is_jsp_result_path(&r.target) {
            // Only adjudicate when the project HAS a webapp dir (else we can't know where JSPs live).
            let (Some(root), false) = (&root, bases.is_empty()) else { continue };
            if resolve_jsp_result(Path::new(root), &bases, &r.target).is_some() {
                continue; // found under the web app → fine
            }
            // Missed the primary webapp dirs — before flagging, check the whole project for a JSP whose
            // path matches (a fragment in a sibling module). Only a genuine miss everywhere is flagged.
            let jsps = project_jsps.get_or_insert_with(|| {
                crate::web_discovery::discover_jsp_files(Path::new(root))
                    .iter()
                    .map(|p| p.to_string_lossy().replace('\\', "/").to_ascii_lowercase())
                    .collect()
            });
            let needle = r.target.trim_start_matches('/').to_ascii_lowercase();
            let matched = jsps.iter().any(|p| jsp_suffix_matches(p, &needle));
            if !matched {
                out.push(Diagnostic {
                    message: format!("JSP `{}` not found in the web app", r.target),
                    severity: "warning".to_string(),
                    code: "jsp-not-found".to_string(),
                    start: r.target_start,
                    end: r.target_end,
                });
            }
        }
    }
    Ok(out)
}

/// Segment-aligned suffix match: does the forward-slashed, lower-cased `file_path` END with the
/// (already `/`-trimmed, lower-cased) result `needle`, on a path-segment boundary? (`…/webapp/web-inf/
/// jsp/tree.jsp` matches `web-inf/jsp/tree.jsp`, but not `e-inf/jsp/tree.jsp`.)
fn jsp_suffix_matches(file_path: &str, needle: &str) -> bool {
    if !file_path.ends_with(needle) {
        return false;
    }
    let at = file_path.len() - needle.len();
    at == 0 || file_path.as_bytes()[at - 1] == b'/'
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ce(fqcn: &str, simple: &str) -> ClassEntry {
        ClassEntry { fqcn: fqcn.into(), simple: simple.into(), file: "X.java".into(), line: 1, kind: "class".into() }
    }

    /// `(qname, class)` the way the reverse lookup hands them over.
    fn cand(qname: &str, class: Option<&str>) -> (String, Option<String>) {
        (qname.to_string(), class.map(str::to_string))
    }

    #[test]
    fn several_routes_into_one_class_are_one_answer_not_an_ambiguity() {
        // The reported shape: one page reachable through three actions, all the same class. The
        // picker showed three identical rows and the checking stayed off until one was pinned.
        let cands = [
            cand("/do/a/dettaglio", Some("it.acme.DettaglioComunicazioniAction")),
            cand("/do/b/dettaglio", Some("it.acme.DettaglioComunicazioniAction")),
            cand("/do/c/dettaglio", Some("it.acme.DettaglioComunicazioniAction")),
        ];
        assert_eq!(sole_answer(&cands).as_deref(), Some("/do/a/dettaglio"));
    }

    #[test]
    fn candidates_disagreeing_about_the_class_stay_ambiguous() {
        let cands = [
            cand("/do/a/x", Some("it.acme.OneAction")),
            cand("/do/b/x", Some("it.acme.OtherAction")),
        ];
        assert!(sole_answer(&cands).is_none());
        // And unknown classes are not agreement: `None == None` must not decide anything.
        let unknown = [cand("/do/a/x", None), cand("/do/b/x", None)];
        assert!(sole_answer(&unknown).is_none());
        assert!(sole_answer(&[]).is_none());
    }

    #[test]
    fn a_single_candidate_decides_even_with_no_class_resolved() {
        assert_eq!(sole_answer(&[cand("/do/a/x", None)]).as_deref(), Some("/do/a/x"));
    }

    fn ce_in(fqcn: &str, simple: &str, file: &str) -> ClassEntry {
        ClassEntry { file: file.into(), ..ce(fqcn, simple) }
    }

    /// Every action in a legacy tree carries its own nested `JspParam`. The index knows five of
    /// them and the getter says `JspParam`; only the file it is written in says which.
    #[test]
    fn a_nested_class_resolves_to_the_one_in_its_own_file() {
        let classes = [
            ce_in("com.acme.a.VerificaAction.JspParam", "JspParam", "/p/a/VerificaAction.java"),
            ce_in("com.acme.b.DetailAction.JspParam", "JspParam", "/p/b/DetailAction.java"),
        ];
        let src = "package com.acme.b;\npublic class DetailAction { public static class JspParam {} }";
        assert_eq!(
            resolve_type_in_context(&classes, "/p/b/DetailAction.java", src, "JspParam").as_deref(),
            Some("com.acme.b.DetailAction.JspParam"),
        );
    }

    #[test]
    fn an_import_decides_when_the_file_itself_does_not() {
        let classes = [
            ce_in("com.acme.dto.Riga", "Riga", "/p/dto/Riga.java"),
            ce_in("com.other.Riga", "Riga", "/p/other/Riga.java"),
        ];
        let src = "package com.acme.web;\nimport com.acme.dto.Riga;\npublic class A {}";
        assert_eq!(
            resolve_type_in_context(&classes, "/p/web/A.java", src, "Riga").as_deref(),
            Some("com.acme.dto.Riga"),
        );
    }

    #[test]
    fn the_declaring_package_is_tried_before_a_project_wide_guess() {
        let classes = [
            ce_in("com.other.Riga", "Riga", "/p/other/Riga.java"),
            ce_in("com.acme.web.Riga", "Riga", "/p/web/Riga.java"),
        ];
        let src = "package com.acme.web;\npublic class A {}";
        assert_eq!(
            resolve_type_in_context(&classes, "/p/web/A.java", src, "Riga").as_deref(),
            Some("com.acme.web.Riga"),
        );
    }

    #[test]
    fn an_ambiguous_name_with_no_context_stops_instead_of_picking_one() {
        let classes = [
            ce_in("com.a.JspParam", "JspParam", "/p/a/JspParam.java"),
            ce_in("com.b.JspParam", "JspParam", "/p/b/JspParam.java"),
        ];
        let src = "package com.elsewhere;\npublic class A {}";
        assert!(resolve_type_in_context(&classes, "/p/A.java", src, "JspParam").is_none());

        // One declaration project-wide is not a guess — it is the answer.
        let one = [ce_in("com.a.JspParam", "JspParam", "/p/a/JspParam.java")];
        assert_eq!(
            resolve_type_in_context(&one, "/p/A.java", src, "JspParam").as_deref(),
            Some("com.a.JspParam"),
        );
    }

    #[test]
    fn a_declaring_expression_is_read_as_a_path_or_not_at_all() {
        assert_eq!(expr_segments("%{elencoBandi}"), vec!["elencoBandi"]);
        assert_eq!(expr_segments("${order.lines}"), vec!["order", "lines"]);
        assert_eq!(expr_segments("%{ bando.dati[0].righe }"), vec!["bando", "dati", "righe"]);
        // Not a path: nothing to type the variable from, and nothing invented.
        assert!(expr_segments("%{foo()}").is_empty());
        assert!(expr_segments("plain text").is_empty());
        assert!(expr_segments("%{a == b}").is_empty());
        assert!(expr_segments("").is_empty());
    }

    #[test]
    fn a_dotted_path_splits_at_the_caret() {
        let name = "ordine.cliente.nome";
        assert_eq!(path_at(name, 0), (vec![], "ordine"));
        assert_eq!(path_at(name, 3), (vec![], "ordine"));
        // On the '.' itself: still the segment it closes, which is where the caret looks to be.
        assert_eq!(path_at(name, 6), (vec![], "ordine"));
        assert_eq!(path_at(name, 9), (vec!["ordine"], "cliente"));
        assert_eq!(path_at(name, 16), (vec!["ordine", "cliente"], "nome"));
        // Past the end — a caret just after the last character is still on the last segment.
        assert_eq!(path_at(name, 999), (vec!["ordine", "cliente"], "nome"));
    }

    #[test]
    fn an_indexed_segment_keeps_only_its_name() {
        assert_eq!(path_at("items[0].nome", 10), (vec!["items"], "nome"));
        assert_eq!(segment_name("call()"), "call");
        assert_eq!(segment_name("plain"), "plain");
    }

    #[test]
    fn property_root_extracts_head() {
        assert_eq!(property_root("user"), "user");
        assert_eq!(property_root("user.name"), "user"); // nested → root only
        assert_eq!(property_root("items[0]"), "items"); // indexed
        assert_eq!(property_root("call()"), "call");
    }

    #[test]
    fn plain_identifier_rejects_computed() {
        assert!(is_plain_identifier("customer"));
        assert!(is_plain_identifier("user_id"));
        assert!(!is_plain_identifier("")); // empty
        assert!(!is_plain_identifier("%{x}")); // computed expression
        assert!(!is_plain_identifier("a.b")); // a path, not a root
    }

    #[test]
    fn ognl_kind_classifies_the_delimiter() {
        // OGNL value-stack `%{customer}` → (ognl, not scoped) → linted.
        let s = "<s:property value=\"%{customer}\"/>";
        assert_eq!(ognl_ref_kind(s, s.find("customer").unwrap()), (true, false));
        // EL `${customer}` → not OGNL → NOT linted.
        let el = "${customer}";
        assert_eq!(ognl_ref_kind(el, el.find("customer").unwrap()), (false, false));
        // `#`-scoped OGNL (`%{#row}`, an iterator/context var) → scoped → NOT linted.
        let ctx = "%{#row}";
        assert_eq!(ognl_ref_kind(ctx, ctx.find("row").unwrap()), (true, true));
    }

    #[test]
    fn binding_key_is_forward_slashed() {
        assert_eq!(binding_key("c:\\a\\b.jsp"), "c:/a/b.jsp");
        assert_eq!(binding_key("/a/b.jsp"), "/a/b.jsp");
    }

    #[test]
    fn super_fqcn_by_fqcn_or_simple() {
        let classes = vec![ce("com.x.BaseAction", "BaseAction"), ce("com.x.Foo", "Foo")];
        assert_eq!(resolve_super_fqcn("BaseAction", &classes).as_deref(), Some("com.x.BaseAction"));
        assert_eq!(resolve_super_fqcn("com.x.BaseAction", &classes).as_deref(), Some("com.x.BaseAction"));
        assert_eq!(resolve_super_fqcn("Absent", &classes), None); // a library super (not indexed)
    }

    #[test]
    fn file_kind_predicates() {
        assert!(is_jsp("/a/b.jsp"));
        assert!(is_jsp("/a/b.JSPF"));
        assert!(!is_jsp("/a/b.xml"));
        assert!(is_validation("/a/Foo-validation.xml"));
        assert!(!is_validation("/a/b.xml"));
    }

    #[test]
    fn fields_bind_to_own_form_or_inherited_action() {
        // `customer` is inside `<s:form action="save">` → binds to that form's action; `orphan` sits
        // OUTSIDE any form (a fragment spliced into a parent's form) → inherits the `bound` action.
        let src = r#"<s:form action="save"><s:textfield name="customer"/></s:form><s:textfield name="orphan"/>"#;
        let got = jsp_fields_with_action(src, Some("parentAction"));
        assert!(
            got.iter().any(|(n, _, _, a)| n == "orphan" && a == "parentAction"),
            "standalone field must inherit the bound action: {got:?}"
        );
        assert!(
            got.iter().any(|(n, _, _, a)| n == "customer" && a != "parentAction"),
            "in-form field must use its own form action: {got:?}"
        );
        // A form field is never ALSO reported as a standalone one (span-deduped).
        assert_eq!(
            got.iter().filter(|(n, ..)| n == "customer").count(),
            1,
            "form field double-reported: {got:?}"
        );
    }

    #[test]
    fn no_bound_action_leaves_standalone_fields_alone() {
        // With no bound action, a standalone field yields nothing (no false hits on an unbound view).
        let src = r#"<s:textfield name="orphan"/>"#;
        assert!(jsp_fields_with_action(src, None).is_empty());
    }

    // ── struts <result> navigation + lint ──────────────────────────────────────

    const STRUTS: &str = r#"<struts>
  <package name="p" namespace="/do/Cat">
    <action name="viewTree" class="cat">
      <result name="success">/WEB-INF/jsp/tree.jsp</result>
      <result name="input" type="redirect">${urlErrori}</result>
    </action>
  </package>
</struts>"#;

    #[test]
    fn scan_results_tracks_owning_action_and_target_spans() {
        let rs = scan_struts_results(STRUTS);
        assert_eq!(rs.len(), 2, "two results");
        // Both belong to the namespaced action.
        assert!(rs.iter().all(|r| r.action_qname.as_deref() == Some("/do/Cat/viewTree")), "qname");
        // The target span slices exactly the trimmed body.
        let jsp = rs.iter().find(|r| r.target.ends_with(".jsp")).unwrap();
        assert_eq!(&STRUTS[jsp.target_start..jsp.target_end], "/WEB-INF/jsp/tree.jsp");
        let ognl = rs.iter().find(|r| r.target.starts_with("${")).unwrap();
        assert_eq!(&STRUTS[ognl.target_start..ognl.target_end], "${urlErrori}");
    }

    #[test]
    fn ognl_result_root_extracts_plain_root_only() {
        assert_eq!(ognl_result_root("${urlErrori}"), Some("urlErrori"));
        assert_eq!(ognl_result_root("%{user.name}"), Some("user")); // nested → root
        assert_eq!(ognl_result_root("/WEB-INF/x.jsp"), None); // a JSP path, not OGNL
        assert_eq!(ognl_result_root("${foo.bar()}"), Some("foo"));
        assert_eq!(ognl_result_root("tilesDef"), None); // a bare Tiles name
    }

    #[test]
    fn jsp_result_path_predicate() {
        assert!(is_jsp_result_path("/WEB-INF/jsp/tree.jsp"));
        assert!(is_jsp_result_path("/pages/x.JSPF"));
        assert!(!is_jsp_result_path("${urlErrori}")); // OGNL
        assert!(!is_jsp_result_path("tilesDef")); // not absolute
        assert!(!is_jsp_result_path("/do/Other/chain")); // absolute but not a JSP
    }

    #[test]
    fn qualify_action_folds_namespace() {
        assert_eq!(qualify_action("viewTree", Some(&"/do/Cat".to_string())), "/do/Cat/viewTree");
        assert_eq!(qualify_action("/abs/name", Some(&"/do/Cat".to_string())), "/abs/name");
        assert_eq!(qualify_action("root", Some(&String::new())), "root");
        assert_eq!(qualify_action("root", None), "root");
    }

    #[test]
    fn jsp_suffix_matches_on_segment_boundary() {
        assert!(jsp_suffix_matches("c:/p/src/main/webapp/web-inf/jsp/tree.jsp", "web-inf/jsp/tree.jsp"));
        assert!(jsp_suffix_matches("web-inf/jsp/tree.jsp", "web-inf/jsp/tree.jsp")); // whole
        // Not on a segment boundary → no match (avoids `e-inf/...` matching `web-inf/...`).
        assert!(!jsp_suffix_matches("c:/p/webapp/web-inf/jsp/tree.jsp", "b-inf/jsp/tree.jsp"));
        assert!(!jsp_suffix_matches("c:/p/webapp/jsp/other.jsp", "jsp/tree.jsp"));
    }

    #[test]
    fn param_based_result_body_is_ignored_by_callers() {
        // A `<result><param name="location">/x.jsp</param></result>` body contains `<` → the scanner
        // still emits it, but go-to/lint skip any target with a `<` (not a simple path/OGNL).
        let src = r#"<action name="a"><result><param name="location">/x.jsp</param></result></action>"#;
        let rs = scan_struts_results(src);
        assert_eq!(rs.len(), 1);
        assert!(rs[0].target.contains('<'), "param body carries markup → skipped downstream");
    }
}
