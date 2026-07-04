//! Include-aware **form field aggregation** — the parameters a `<form>` actually posts, once
//! every `<jsp:include>`d fragment inside it is spliced in.
//!
//! A legacy JSP form is rarely self-contained: the `<form>` opens on the page, then a chain of
//! `<jsp:include>`s pulls in the hidden token, the CSRF field, the wizard-step inputs, the
//! button bar… each contributing `<input>`/`<select>`/hidden controls that are submitted with
//! that form. The per-file [`crate::forms`] scan sees only the fields lexically in one file; it
//! can't answer *"what is the complete parameter set this form sends"*. This module does, by
//! walking the include graph:
//!
//!   - **from the page** (the `<form>`'s host): collect the form's own fields, then splice in
//!     the fields of every fragment included *within the form's span*, recursively — so the
//!     page shows all the parameters, including the children's;
//!   - **from an included fragment**: walk the include graph in reverse to the page whose form
//!     the fragment feeds, and surface that whole form — so a fragment shows the parent form's
//!     parameters too (its siblings + the page's own fields), not just its own.
//!
//! Each aggregated field is tagged with the [`ExpandedField::source_file`] it actually lives
//! in, so the UI can show which parameters come from which include (and highlight the ones the
//! file you're looking at contributes).
//!
//! The walk is **cycle-safe** (a per-expansion `visited` set) and **node-capped** (the same
//! `max_nodes` backstop [`related_files`] uses), reporting truncation rather than looping or
//! silently dropping coverage. Pure over the filesystem + [`crate::forms`] / [`crate::jsp_includes`]
//! / [`crate::include_graph`] — no `bennu-be`, no live index — so it's unit-tested off temp
//! fixtures. The action/class correlation (bound/validated) is layered on in `bennu-be`.

use std::collections::HashSet;
use std::path::Path;

use crate::forms::{parse_jsp_fields_file, parse_jsp_forms_file};
use crate::include_graph::{key_of, related_files, IncludeGraph, IncludeRelation};
use crate::jsp_includes::{parse_jsp_includes_file, resolve_include_target};
use crate::model::{FormControl, JspFormField};

/// One aggregated form parameter — a field, plus the file it actually lives in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExpandedField {
    /// The form-field name (`name=` / legacy `property=`).
    pub name: String,
    /// The control kind.
    pub control: FormControl,
    /// The submitted `value=` as written (fixed value or `${…}`/`%{…}`), if any.
    pub value: Option<String>,
    /// True when the field sits inside a conditional block (submitted only when it holds).
    pub conditional: bool,
    /// The nearest enclosing condition, when [`Self::conditional`].
    pub condition: Option<String>,
    /// The forward-slashed path of the JSP this field's tag lives in — the host page for the
    /// form's own fields, or the included fragment for a spliced-in one.
    pub source_file: String,
    /// Start byte offset of the name value inside the quotes, **in `source_file`**.
    pub start: usize,
    /// End byte offset (exclusive), in `source_file`.
    pub end: usize,
}

/// One aggregated `<form>`: the declaration (in `host_file`) + its complete, include-expanded
/// field set. The action/class correlation is added by the `bennu-be` layer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExpandedForm {
    /// The normalized action key (`None` when absent / computed).
    pub action: Option<String>,
    /// The form's `method=` (lowercased), if present.
    pub method: Option<String>,
    /// The forward-slashed path of the JSP that declares the `<form>`.
    pub host_file: String,
    /// Start byte offset of the `<form>` open tag, in `host_file`.
    pub start: usize,
    /// End byte offset (exclusive), in `host_file`.
    pub end: usize,
    /// Every field the form posts, across the include expansion, each source-tagged.
    pub fields: Vec<ExpandedField>,
}

/// The forms relevant to a focus file, each with its include-expanded field set.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExpandedForms {
    /// The relevant forms (host page's own + fragment-participated), each fully expanded.
    pub forms: Vec<ExpandedForm>,
    /// True when the include walk hit its node cap and left related files unvisited.
    pub truncated: bool,
}

/// Aggregate the forms relevant to `focus_file`, each with its parameters expanded across the
/// include graph (see the module docs). `graph` is the project's include graph; `max_nodes`
/// caps both the related-file walk and each form's field-splice recursion.
///
/// A form is relevant to `focus_file` when:
///   - it is declared on `focus_file` itself, or on a fragment `focus_file` transitively
///     includes (the form renders on the page you're viewing); or
///   - `focus_file` is one of the fragments that (transitively) feed the form — i.e. the form
///     lives on a page that includes `focus_file` within its `<form>` span (so an included
///     fragment surfaces the parent form it belongs to).
pub fn analyze_forms_expanded(
    graph: &IncludeGraph,
    focus_file: &str,
    max_nodes: usize,
) -> ExpandedForms {
    let cap = max_nodes.max(1);
    let related = related_files(graph, focus_file, cap);
    let mut forms: Vec<ExpandedForm> = Vec::new();
    let mut truncated = related.truncated;

    for rf in &related.files {
        let host = rf.file.as_str();
        for form in parse_jsp_forms_file(Path::new(host)) {
            let (fields, tr) = expand_form_fields(host, &form, cap);
            truncated |= tr;

            // Self / a fragment the focus page pulls in → the form renders on the focus page,
            // always relevant. A page that INCLUDES the focus keeps only forms whose expansion
            // actually reaches the focus file (the focus feeds THIS form, not just this page).
            let relevant = match rf.relation {
                IncludeRelation::SelfPage | IncludeRelation::Includes => true,
                IncludeRelation::IncludedBy => {
                    fields.iter().any(|f| f.source_file.eq_ignore_ascii_case(focus_file))
                }
            };
            if !relevant {
                continue;
            }

            forms.push(ExpandedForm {
                action: form.action,
                method: form.method,
                host_file: host.to_string(),
                start: form.start,
                end: form.end,
                fields,
            });
        }
    }

    ExpandedForms { forms, truncated }
}

/// Expand ONE form's fields: its own (source = `host`) plus, for every include that sits inside
/// the form's `[start, end)` span, that fragment's fields spliced in recursively. Cycle-safe
/// (a `visited` set seeded with the host) and capped (`cap`), reporting truncation.
fn expand_form_fields(
    host: &str,
    form: &crate::model::JspForm,
    cap: usize,
) -> (Vec<ExpandedField>, bool) {
    let mut out: Vec<ExpandedField> =
        form.fields.iter().map(|f| to_expanded(f, host)).collect();
    let mut visited: HashSet<String> = HashSet::new();
    visited.insert(host.to_string());
    let mut truncated = false;

    for inc in parse_jsp_includes_file(Path::new(host)) {
        if inc.computed {
            continue; // a runtime `${…}` include isn't a static splice
        }
        // Only an include INSIDE the form span contributes its fields to this form.
        if inc.start < form.start || inc.start >= form.end {
            continue;
        }
        if let Some(target) = resolve_include_target(Path::new(host), &inc.raw) {
            splice_fragment(&key_of(&target), &mut out, &mut visited, cap, &mut truncated);
        }
    }

    (out, truncated)
}

/// Splice `file`'s fields (and, recursively, the fields of everything it includes) into `out`,
/// each tagged with its own source. The whole fragment is spliced, so ALL of its includes are
/// followed (a fragment has no `<form>` span to gate on). `visited` guards cycles; the `cap`
/// bounds a pathological graph (sets `truncated`).
fn splice_fragment(
    file: &str,
    out: &mut Vec<ExpandedField>,
    visited: &mut HashSet<String>,
    cap: usize,
    truncated: &mut bool,
) {
    if visited.contains(file) {
        return; // already spliced on this expansion — the cycle / diamond guard
    }
    if visited.len() >= cap {
        *truncated = true;
        return;
    }
    visited.insert(file.to_string());

    for f in parse_jsp_fields_file(Path::new(file)) {
        out.push(to_expanded(&f, file));
    }
    for inc in parse_jsp_includes_file(Path::new(file)) {
        if inc.computed {
            continue;
        }
        if let Some(target) = resolve_include_target(Path::new(file), &inc.raw) {
            splice_fragment(&key_of(&target), out, visited, cap, truncated);
        }
    }
}

/// Clone a parsed field into an [`ExpandedField`] tagged with `source`.
fn to_expanded(f: &JspFormField, source: &str) -> ExpandedField {
    ExpandedField {
        name: f.name.clone(),
        control: f.control.clone(),
        value: f.value.clone(),
        conditional: f.conditional,
        condition: f.condition.clone(),
        source_file: source.to_string(),
        start: f.start,
        end: f.end,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::include_graph::build_include_graph;
    use crate::test_support::tmp_dir;
    use std::path::PathBuf;

    /// Write `name` with `body` under `dir`, returning its path.
    fn write(dir: &Path, name: &str, body: &str) -> PathBuf {
        let path = dir.join(name);
        std::fs::write(&path, body).unwrap();
        path
    }

    /// Analyse `focus` over a graph built from `files`, with a generous cap.
    fn analyze(files: &[PathBuf], focus: &Path) -> ExpandedForms {
        let graph = build_include_graph(files);
        analyze_forms_expanded(&graph, &key_of(focus), 200)
    }

    /// The (name, source basename) pairs of a form's fields — the shape most tests assert on.
    fn field_sources(form: &ExpandedForm) -> Vec<(String, String)> {
        form.fields
            .iter()
            .map(|f| {
                let base = f.source_file.rsplit('/').next().unwrap_or(&f.source_file).to_string();
                (f.name.clone(), base)
            })
            .collect()
    }

    #[test]
    fn self_page_splices_included_fragment_fields_tagged_by_source() {
        let dir = tmp_dir("expand");
        write(&dir, "token.jspf", r#"<input type="hidden" name="_tk" value="${token}">"#);
        let page = write(
            &dir,
            "page.jsp",
            r#"<form action="save.action" method="post">
                 <s:textfield name="title"/>
                 <jsp:include page="token.jspf"/>
               </form>"#,
        );

        let res = analyze(&[page.clone()], &page);
        assert_eq!(res.forms.len(), 1);
        let form = &res.forms[0];
        assert_eq!(form.action.as_deref(), Some("save"));
        assert_eq!(form.host_file, key_of(&page));
        // Own field (source = page) + the included hidden token (source = token.jspf).
        assert_eq!(
            field_sources(form),
            vec![
                ("title".to_string(), "page.jsp".to_string()),
                ("_tk".to_string(), "token.jspf".to_string()),
            ]
        );
        let tk = form.fields.iter().find(|f| f.name == "_tk").unwrap();
        assert_eq!(tk.control, FormControl::Hidden);
        assert_eq!(tk.value.as_deref(), Some("${token}"));
    }

    #[test]
    fn included_fragment_surfaces_parent_form_with_all_params() {
        // THE reverse case: sitting on the token fragment, we see the PARENT form and its whole
        // parameter set (the page's own field + our contributed hidden), not just our own.
        let dir = tmp_dir("expand");
        let token = write(&dir, "token.jspf", r#"<input type="hidden" name="_tk" value="${t}">"#);
        let page = write(
            &dir,
            "page.jsp",
            r#"<form action="save.action">
                 <s:textfield name="title"/>
                 <jsp:include page="token.jspf"/>
               </form>"#,
        );

        let res = analyze(&[page.clone(), token.clone()], &token);
        assert_eq!(res.forms.len(), 1, "the parent form is surfaced from the fragment");
        let form = &res.forms[0];
        assert_eq!(form.host_file, key_of(&page), "host is the page, not the fragment");
        assert_eq!(
            field_sources(form),
            vec![
                ("title".to_string(), "page.jsp".to_string()),
                ("_tk".to_string(), "token.jspf".to_string()),
            ]
        );
        // The focus (the fragment) is one of the sources — that's what makes it relevant.
        assert!(form.fields.iter().any(|f| f.source_file == key_of(&token)));
    }

    #[test]
    fn include_outside_the_form_span_is_not_spliced() {
        // A fragment included OUTSIDE the `<form>` (a page header) contributes nothing to it.
        let dir = tmp_dir("expand");
        write(&dir, "header.jspf", r#"<input name="notInForm">"#);
        write(&dir, "body.jspf", r#"<input name="inForm">"#);
        let page = write(
            &dir,
            "page.jsp",
            r#"<jsp:include page="header.jspf"/>
               <form action="a.action">
                 <jsp:include page="body.jspf"/>
               </form>"#,
        );

        let res = analyze(&[page.clone()], &page);
        let names: Vec<&str> = res.forms[0].fields.iter().map(|f| f.name.as_str()).collect();
        assert!(names.contains(&"inForm"), "names = {names:?}");
        assert!(!names.contains(&"notInForm"), "header include leaked: {names:?}");
    }

    #[test]
    fn transitive_include_is_spliced_recursively() {
        let dir = tmp_dir("expand");
        write(&dir, "leaf.jspf", r#"<input type="hidden" name="deep" value="1">"#);
        write(&dir, "mid.jspf", r#"<input name="mid"><jsp:include page="leaf.jspf"/>"#);
        let page = write(
            &dir,
            "page.jsp",
            r#"<form action="a.action"><jsp:include page="mid.jspf"/></form>"#,
        );

        let res = analyze(&[page.clone()], &page);
        let names: Vec<&str> = res.forms[0].fields.iter().map(|f| f.name.as_str()).collect();
        assert!(names.contains(&"mid"), "names = {names:?}");
        assert!(names.contains(&"deep"), "transitive include not spliced: {names:?}");
    }

    #[test]
    fn conditional_context_survives_into_a_fragment() {
        let dir = tmp_dir("expand");
        write(
            &dir,
            "admin.jspf",
            r#"<c:if test="${admin}"><input type="hidden" name="role" value="ADMIN"></c:if>"#,
        );
        let page = write(
            &dir,
            "page.jsp",
            r#"<form action="a.action"><jsp:include page="admin.jspf"/></form>"#,
        );

        let res = analyze(&[page.clone()], &page);
        let role = res.forms[0].fields.iter().find(|f| f.name == "role").unwrap();
        assert!(role.conditional);
        assert_eq!(role.condition.as_deref(), Some("${admin}"));
    }

    #[test]
    fn computed_include_is_skipped() {
        let dir = tmp_dir("expand");
        let page = write(
            &dir,
            "page.jsp",
            r#"<form action="a.action">
                 <input name="real">
                 <jsp:include page="${dynamic}"/>
               </form>"#,
        );
        let res = analyze(&[page.clone()], &page);
        let names: Vec<&str> = res.forms[0].fields.iter().map(|f| f.name.as_str()).collect();
        assert_eq!(names, vec!["real"], "a computed include must contribute nothing");
    }

    #[test]
    fn mutual_include_terminates() {
        // page's form includes frag; frag includes page back → the splice must not loop.
        let dir = tmp_dir("expand");
        let frag = write(
            &dir,
            "frag.jspf",
            r#"<input name="fromFrag"><jsp:include page="page.jsp"/>"#,
        );
        let page = write(
            &dir,
            "page.jsp",
            r#"<form action="a.action">
                 <input name="fromPage">
                 <jsp:include page="frag.jspf"/>
               </form>"#,
        );

        let res = analyze(&[page.clone(), frag.clone()], &page);
        assert_eq!(res.forms.len(), 1);
        let names: Vec<&str> = res.forms[0].fields.iter().map(|f| f.name.as_str()).collect();
        // Each contributed once; no infinite recursion, no duplicate re-splice of the page.
        assert!(names.contains(&"fromPage"));
        assert!(names.contains(&"fromFrag"));
        assert_eq!(names.iter().filter(|n| **n == "fromPage").count(), 1);
    }

    #[test]
    fn nested_wp_action_form_is_aggregated_with_resolved_action() {
        // The Entando nested-taglib action + a hidden field: the action resolves and the field
        // aggregates (regression for the `<wp:action path=` garbage header).
        let dir = tmp_dir("expand");
        let page = write(
            &dir,
            "page.jsp",
            r#"<form action="<wp:action path="/ExtStr2/do/FrontEnd/DatiImpr/processPage.action" />" method="post">
                 <input type="hidden" name="ext" value="${param.ext}">
               </form>"#,
        );
        let res = analyze(&[page.clone()], &page);
        assert_eq!(res.forms.len(), 1);
        let form = &res.forms[0];
        assert_eq!(form.action.as_deref(), Some("/ExtStr2/do/FrontEnd/DatiImpr/processPage"));
        assert_eq!(form.method.as_deref(), Some("post"));
        assert_eq!(form.fields.len(), 1);
        assert_eq!(form.fields[0].name, "ext");
    }

    #[test]
    fn a_fragment_feeding_two_pages_surfaces_both_forms() {
        // The token fragment is included by two different pages → sitting on it, BOTH parent
        // forms surface (each with the fragment's field), rendered as two forms.
        let dir = tmp_dir("expand");
        let token = write(&dir, "token.jspf", r#"<input type="hidden" name="_tk" value="1">"#);
        let a = write(
            &dir,
            "a.jsp",
            r#"<form action="a.action"><jsp:include page="token.jspf"/></form>"#,
        );
        let b = write(
            &dir,
            "b.jsp",
            r#"<form action="b.action"><jsp:include page="token.jspf"/></form>"#,
        );

        let res = analyze(&[a.clone(), b.clone(), token.clone()], &token);
        let actions: Vec<Option<&str>> =
            res.forms.iter().map(|f| f.action.as_deref()).collect();
        assert_eq!(res.forms.len(), 2, "both including forms surface: {actions:?}");
        assert!(actions.contains(&Some("a")));
        assert!(actions.contains(&Some("b")));
        assert!(res.forms.iter().all(|f| f.fields.iter().any(|x| x.name == "_tk")));
    }

    #[test]
    fn empty_graph_still_splices_self_page_includes_from_disk() {
        // With no include graph (project-less), the self page's own form + its filesystem
        // includes still aggregate — the graph only gates the reverse (parent) discovery.
        let dir = tmp_dir("expand");
        write(&dir, "frag.jspf", r#"<input name="fromFrag">"#);
        let page = write(
            &dir,
            "page.jsp",
            r#"<form action="a.action">
                 <input name="fromPage">
                 <jsp:include page="frag.jspf"/>
               </form>"#,
        );

        let empty = IncludeGraph::default();
        let res = analyze_forms_expanded(&empty, &key_of(&page), 200);
        assert_eq!(res.forms.len(), 1);
        let names: Vec<&str> = res.forms[0].fields.iter().map(|f| f.name.as_str()).collect();
        assert!(names.contains(&"fromPage"));
        assert!(names.contains(&"fromFrag"), "self-page include spliced regardless of graph");
    }

    #[test]
    fn multiple_forms_on_a_page_aggregate_independently() {
        let dir = tmp_dir("expand");
        write(&dir, "one.jspf", r#"<input name="oneField">"#);
        write(&dir, "two.jspf", r#"<input name="twoField">"#);
        let page = write(
            &dir,
            "page.jsp",
            r#"<form action="one.action"><jsp:include page="one.jspf"/></form>
               <form action="two.action"><jsp:include page="two.jspf"/></form>"#,
        );

        let res = analyze(&[page.clone()], &page);
        assert_eq!(res.forms.len(), 2);
        let f1 = res.forms.iter().find(|f| f.action.as_deref() == Some("one")).unwrap();
        let f2 = res.forms.iter().find(|f| f.action.as_deref() == Some("two")).unwrap();
        assert!(f1.fields.iter().any(|x| x.name == "oneField"));
        assert!(!f1.fields.iter().any(|x| x.name == "twoField"), "no cross-form leak");
        assert!(f2.fields.iter().any(|x| x.name == "twoField"));
    }

    #[test]
    fn page_with_no_forms_and_no_participation_is_empty() {
        // A fragment of loose inputs that no page includes within a form → no relevant form.
        let dir = tmp_dir("expand");
        let orphan = write(&dir, "orphan.jspf", r#"<input name="lonely">"#);
        let res = analyze(&[orphan.clone()], &orphan);
        assert!(res.forms.is_empty());
        assert!(!res.truncated);
    }

    #[test]
    fn included_fragment_reached_outside_a_form_span_does_not_surface_that_form() {
        // page includes header OUTSIDE its form; sitting on header we must NOT get the form
        // (header's fields don't feed it → not relevant).
        let dir = tmp_dir("expand");
        let header = write(&dir, "header.jspf", r#"<input name="h">"#);
        let page = write(
            &dir,
            "page.jsp",
            r#"<jsp:include page="header.jspf"/>
               <form action="a.action"><input name="inForm"></form>"#,
        );

        let res = analyze(&[page.clone(), header.clone()], &header);
        assert!(res.forms.is_empty(), "header doesn't feed the form → not surfaced");
    }
}
