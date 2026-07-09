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
use std::path::{Path, PathBuf};

use bennu_core::prelude::BennuState;
use bennu_proto::prelude::{
    ClassEntry, DeclarationTarget, Diagnostic, HoverInfo, JspActionBinding, JspActionOption,
    PropertyLintHit,
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
        // Normalize to LF so the property accessor go-to offset lands correctly in the editor's LF
        // document (a CRLF action class would otherwise drift the target).
        let Ok(raw) = std::fs::read_to_string(&entry.file) else { break };
        let src = bennu_project::prelude::normalize_newlines(&raw);
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
    let Some((root, simple, chain)) = resolve_property_at(svc, &args.file, &args.source, args.offset)
    else {
        return Ok(None);
    };
    Ok(target_in_chain(&chain, &simple, &root))
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
    let Some((root, simple, chain)) = resolve_property_at(svc, &args.file, &args.source, args.offset)
    else {
        return Ok(None);
    };
    Ok(hover_in_chain(&chain, &simple, &root))
}

/// Resolve the field / OGNL root / validation `<field>` reference under the caret to
/// `(property_root, action_simple_name, action_class_chain)`. The shared front half of go-to
/// ([`bennu_action_property_target`]) and hover ([`bennu_action_property_hover`]) — they differ only
/// in what they do with the resolved chain. `None` when the caret isn't on a resolvable reference.
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
        let root = property_root(&field.name);
        if !is_plain_identifier(root) {
            return None;
        }
        let (simple, chain) = resolve_validation(svc, file)?;
        return Some((root.to_string(), simple, chain));
    }

    if is_jsp(file) {
        let bound = jsp_bound_action(svc, file);
        // A FIELD under the caret — a form field (→ its form's action) or a standalone field spliced
        // into a parent's form (→ the inherited bound action).
        for (name, start, end, action) in jsp_fields_with_action(source, bound.as_deref()) {
            if offset >= start && offset <= end {
                let root = property_root(&name);
                if !is_plain_identifier(root) {
                    return None;
                }
                let (simple, chain) = resolve_action(svc, file, &action)?;
                return Some((root.to_string(), simple, chain));
            }
        }
        // A standalone OGNL reference (a `%{prop}` that isn't a page variable) → the bound action.
        let vars = parse_jsp_vars(source);
        let declared: HashSet<&str> = vars.decls.iter().map(|d| d.name.as_str()).collect();
        if let Some(r) = vars.refs.iter().find(|r| offset >= r.start && offset <= r.end) {
            let root = property_root(&r.name);
            if !declared.contains(r.name.as_str()) && is_plain_identifier(root) {
                if let Some(action) = &bound {
                    if let Some((simple, chain)) = resolve_action(svc, file, action) {
                        return Some((root.to_string(), simple, chain));
                    }
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
            let kind = if pt.read { "action property" } else { "action property (write-only)" };
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
    ["src/main/webapp", "web", "WebContent", "webapp", "src/webapp", "WebRoot"]
        .iter()
        .map(|b| root.join(b))
        .filter(|p| p.is_dir())
        .collect()
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
