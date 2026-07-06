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
//! reference (a `%{prop}` NOT scoped `#…` and NOT a page variable), the user's pinned action, else the
//! SINGLE reverse-lookup candidate. Ambiguous (0 or >1 candidates, no pin) → OGNL stays silent.
//!
//! Conservative by construction (never a false positive): a lint hit needs the action to resolve to a
//! project class whose accessor set (own + inherited project supers) is non-empty; only OGNL `%{…}`
//! value-stack roots are linted (EL `${…}` scoped attributes and `#…` context/iterator vars are not).

use std::collections::{BTreeSet, HashSet};
use std::path::Path;

use bennu_core::prelude::BennuState;
use bennu_proto::prelude::{
    ClassEntry, DeclarationTarget, JspActionBinding, JspActionOption, PropertyLintHit,
};
use bennu_web::prelude::{
    line_col, parse_jsp_fields, parse_jsp_forms, parse_jsp_vars, parse_validation_text,
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

/// Whether `root` is a plain Java identifier we can look up (a computed `%{…}`/`${…}` name is not).
fn is_plain_identifier(root: &str) -> bool {
    !root.is_empty() && root.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '$')
}

/// The forward-slashed key the JSP→action binding map is stored under.
fn binding_key(file: &str) -> String {
    file.replace('\\', "/")
}

/// The action a JSP's OGNL is bound to: the persisted pin, else the SINGLE reverse-lookup candidate.
/// `None` for an ambiguous view (no pin and 0 or >1 candidates) → OGNL stays silent (no false hits).
fn jsp_bound_action(svc: &IndexService, file: &str) -> Option<String> {
    let cfg = bennu_core::config::load();
    if let Some(a) = cfg.jsp_action_bindings.get(&binding_key(file)) {
        return Some(a.clone());
    }
    let cands = svc.jsp_action_candidates(file);
    if cands.len() == 1 {
        Some(cands[0].0.clone())
    } else {
        None
    }
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
fn class_chain(svc: &IndexService, near_file: &str, fqcn: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let Some(root) = svc.root_for_file(near_file) else { return out };
    let Some(classes) = svc.class_index(&root) else { return out };
    let mut cur = fqcn.to_string();
    let mut seen = HashSet::new();
    let mut depth = 0;
    while depth < 20 && seen.insert(cur.clone()) {
        let Some(entry) = classes.iter().find(|c| c.fqcn == cur) else { break };
        let Ok(src) = std::fs::read_to_string(&entry.file) else { break };
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
    let chain = class_chain(svc, file, &fqcn);
    if chain.is_empty() {
        return None;
    }
    let simple = fqcn.rsplit('.').next().unwrap_or(&fqcn).to_string();
    Some((simple, chain))
}

/// The (simple, chain) a `*-validation.xml` binds to (by the filename convention).
fn resolve_validation(svc: &IndexService, file: &str) -> Option<(String, Vec<(String, String)>)> {
    let ctx = svc.validation_context(file);
    let fqcn = ctx.action_fqcn?;
    let chain = class_chain(svc, file, &fqcn);
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
            resolve_action(svc, file, action).map(|(s, chain)| (s, chain_property_set(&chain)))
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

    if is_validation(&args.file) {
        let Some(rec) = parse_validation_text(Path::new(&args.file), &args.source) else {
            return Ok(None);
        };
        let Some(field) = rec
            .fields
            .iter()
            .find(|f| args.offset >= f.name_offset && args.offset <= f.name_offset + f.name.len())
        else {
            return Ok(None);
        };
        let root = property_root(&field.name);
        if !is_plain_identifier(root) {
            return Ok(None);
        }
        let Some((simple, chain)) = resolve_validation(svc, &args.file) else {
            return Ok(None);
        };
        return Ok(target_in_chain(&chain, &simple, root));
    }

    if is_jsp(&args.file) {
        let bound = jsp_bound_action(svc, &args.file);
        // A FIELD under the caret — a form field (→ its form's action) or a standalone field spliced
        // into a parent's form (→ the inherited bound action).
        for (name, start, end, action) in jsp_fields_with_action(&args.source, bound.as_deref()) {
            if args.offset >= start && args.offset <= end {
                let root = property_root(&name);
                if !is_plain_identifier(root) {
                    return Ok(None);
                }
                let Some((simple, chain)) = resolve_action(svc, &args.file, &action) else {
                    return Ok(None);
                };
                return Ok(target_in_chain(&chain, &simple, root));
            }
        }
        // A standalone OGNL reference (a `%{prop}` that isn't a page variable) → the bound action.
        let vars = parse_jsp_vars(&args.source);
        let declared: HashSet<&str> = vars.decls.iter().map(|d| d.name.as_str()).collect();
        if let Some(r) = vars.refs.iter().find(|r| args.offset >= r.start && args.offset <= r.end) {
            let root = property_root(&r.name);
            if !declared.contains(r.name.as_str()) && is_plain_identifier(root) {
                if let Some(action) = &bound {
                    if let Some((simple, chain)) = resolve_action(svc, &args.file, action) {
                        return Ok(target_in_chain(&chain, &simple, root));
                    }
                }
            }
        }
    }

    Ok(None)
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
        let bound = jsp_bound_action(svc, &args.file);
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
                    for r in &vars.refs {
                        if declared.contains(r.name.as_str()) {
                            continue;
                        }
                        let (ognl, scoped) = ognl_ref_kind(&args.source, r.start);
                        if ognl && !scoped {
                            push_if_unknown(&mut out, &props, &simple, &r.name, r.start, r.end);
                        }
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
    let candidates: Vec<JspActionOption> = svc
        .jsp_action_candidates(&args.file)
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
    let effective = bound.clone().or_else(|| {
        (candidates.len() == 1).then(|| candidates[0].qname.clone())
    });
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

#[cfg(test)]
mod tests {
    use super::*;

    fn ce(fqcn: &str, simple: &str) -> ClassEntry {
        ClassEntry { fqcn: fqcn.into(), simple: simple.into(), file: "X.java".into(), line: 1 }
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
}
