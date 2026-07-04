//! `form-analysis` domain — `bennu_form_analysis` (the form → parameters inspector).
//!
//! For a JSP, list every `<form>` **relevant** to it, each with its **complete parameter set**:
//! the form's own input fields plus every `<input>`/`<select>`/hidden a `<jsp:include>`d
//! fragment inside the form contributes, spliced in across the include graph (both directions —
//! so a page shows its children's parameters, and an included fragment shows the parent form it
//! feeds). Each field is correlated against the resolved action class: its name is checked
//! against the class's **writable properties** (its `setXxx` setters — what the form binds) and
//! its **validation rules**. The FE shows "form → action → parameters, which bind, which are
//! validated, and where each comes from".
//!
//! The structural aggregation lives in `bennu-web` ([`analyze_forms_expanded`]); the per-action
//! resolution lives on [`IndexService`] ([`form_action_context`]). This module holds the thin
//! handler that stitches them, plus the pure [`correlate_expanded`] core (unit-tested off a
//! fabricated [`ExpandedForms`] + a fake resolver, no live project).

use std::collections::HashSet;
use std::path::PathBuf;

use bennu_core::prelude::BennuState;
use bennu_proto::prelude::{FormAnalysis, FormFieldInfo, FormInfo};
use bennu_web::prelude::{analyze_forms_expanded, ExpandedForms, IncludeGraph};
use serde::Deserialize;

use crate::index_service::IndexService;

/// Hard backstop on the number of related JSPs the include-aware aggregation walks (cycle-safe
/// walk stops here and reports truncation). Generous — real form pages pull in a handful of
/// fragments, not hundreds — but bounds a pathological include graph.
const MAX_TREE_NODES: usize = 200;

/// Args for [`bennu_form_analysis`].
#[derive(Deserialize)]
pub struct FormAnalysisArgs {
    /// Absolute path (forward slashes) to the JSP being analysed.
    pub file: String,
    /// When true (the panel's Refresh button), force a full include-graph freshness pass —
    /// re-walk the project so a newly-added file or a newly-added parent include is picked up.
    /// Omitted / false on the reactive per-tab fetch (incremental, cheap).
    #[serde(default)]
    pub full: bool,
}

/// Analyse the forms relevant to a JSP, **include-aware**: every `<form>` on the file, on a
/// fragment it includes, or on a page that includes it, each with its complete parameter set
/// (own fields + every `<jsp:include>`d fragment's inputs, source-tagged) correlated against
/// the resolved action class. Empty (`forms = []`) for a non-JSP / project-less / form-less
/// file — never an error (the FE degrades gracefully).
#[arbor_rpc::handler]
fn bennu_form_analysis(_ctx: &BennuState, args: FormAnalysisArgs) -> Result<FormAnalysis, String> {
    Ok(build_form_analysis(&args.file, args.full))
}

/// Build the include-aware, correlated form analysis for `file`. When the owning project is
/// known (an open project, else the nearest `pom.xml`/`.arbor` ancestor), the include graph is
/// served from the incremental per-project cache (`force_full` = the Refresh button); otherwise
/// the graph is empty (the file's own forms still aggregate their on-disk includes). Every
/// form's action is then correlated against the owning project's config + index.
fn build_form_analysis(file: &str, force_full: bool) -> FormAnalysis {
    let svc = IndexService::global();

    let graph = match project_root_of(file) {
        Some(root) => svc.include_graph(&root, file, force_full),
        None => IncludeGraph::default(),
    };

    let expanded = analyze_forms_expanded(&graph, file, MAX_TREE_NODES);
    correlate_expanded(expanded, |host, action| svc.form_action_context(host, action))
}

/// The pure correlation core: join each aggregated [`ExpandedForm`] against its resolved action,
/// via a `resolve` closure that — given the form's host file + action key — yields
/// `(class_fqcn, config_file, writable_props, validated_fields)`. A field's `bound` flag is set
/// when its name is one of the writable props; `validated` when it is one of the validated
/// fields. The include-expanded field set + `source_file` tags flow through untouched.
///
/// A form with no resolvable action (`action == None`, or the closure returns empty sets) is
/// still emitted: every field is `bound = false` / `validated = false` and `action_class =
/// None`. Pure over its inputs (no `IndexService` / FS), so unit-testable off in-memory fixtures.
pub fn correlate_expanded(
    expanded: ExpandedForms,
    mut resolve: impl FnMut(&str, &str) -> (Option<String>, Option<String>, Vec<String>, Vec<String>),
) -> FormAnalysis {
    let ExpandedForms { forms, truncated } = expanded;
    let forms = forms
        .into_iter()
        .map(|form| {
            let (action_class, config_file, writable, validated) = match &form.action {
                Some(action) => resolve(&form.host_file, action),
                None => (None, None, Vec::new(), Vec::new()),
            };
            let writable: HashSet<&str> = writable.iter().map(String::as_str).collect();
            let validated: HashSet<&str> = validated.iter().map(String::as_str).collect();
            let fields = form
                .fields
                .into_iter()
                .map(|f| FormFieldInfo {
                    bound: writable.contains(f.name.as_str()),
                    validated: validated.contains(f.name.as_str()),
                    control: f.control.as_str().to_string(),
                    value: f.value,
                    conditional: f.conditional,
                    condition: f.condition,
                    source_file: f.source_file,
                    name: f.name,
                    start: f.start,
                    end: f.end,
                })
                .collect();
            FormInfo {
                action: form.action,
                action_class,
                config_file,
                method: form.method,
                host_file: form.host_file,
                start: form.start,
                end: form.end,
                fields,
            }
        })
        .collect();
    FormAnalysis { forms, truncated }
}

/// The project root owning `file`: the OPEN project's root when the index has one at a prefix
/// of `file` (the true reactor/module root the index was built at), else the nearest ancestor
/// directory holding an `.arbor` folder or a `pom.xml`. `None` when neither is found (a
/// scratch file with no project) → the aggregation runs against an empty graph.
fn project_root_of(file: &str) -> Option<String> {
    if let Some(root) = IndexService::global().root_for_file(file) {
        return Some(root);
    }
    let mut dir = PathBuf::from(file);
    dir.pop();
    loop {
        if dir.join(".arbor").is_dir() || dir.join("pom.xml").is_file() {
            return Some(dir.to_string_lossy().replace('\\', "/"));
        }
        if !dir.pop() {
            return None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bennu_web::prelude::{ExpandedField, ExpandedForm, FormControl};

    /// A minimal expanded field for fixtures.
    fn field(name: &str, control: FormControl, source: &str) -> ExpandedField {
        ExpandedField {
            name: name.to_string(),
            control,
            value: None,
            conditional: false,
            condition: None,
            source_file: source.to_string(),
            start: 0,
            end: 0,
        }
    }

    /// A field matching a writable prop is `bound`; matching a validation field is
    /// `validated`; an unmatched field is neither; and the resolved class/config flow through.
    /// The host file is passed to the resolver (correlation must use the form's host, not the
    /// analysed file).
    #[test]
    fn correlates_bound_and_validated_flags_using_host_file() {
        let expanded = ExpandedForms {
            forms: vec![ExpandedForm {
                action: Some("/do/Cat/save".to_string()),
                method: Some("post".to_string()),
                host_file: "/proj/page.jsp".to_string(),
                start: 0,
                end: 10,
                fields: vec![
                    field("title", FormControl::Text, "/proj/page.jsp"),
                    field("secret", FormControl::Password, "/proj/frag.jspf"),
                    field("ghost", FormControl::Text, "/proj/page.jsp"),
                ],
            }],
            truncated: false,
        };

        let analysis = correlate_expanded(expanded, |host, action| {
            assert_eq!(host, "/proj/page.jsp", "resolver must be keyed by the form host");
            assert_eq!(action, "/do/Cat/save");
            (
                Some("com.x.CatAction".to_string()),
                Some("s.xml".to_string()),
                vec!["title".to_string(), "secret".to_string()],
                vec!["title".to_string()],
            )
        });

        assert!(!analysis.truncated);
        assert_eq!(analysis.forms.len(), 1);
        let f = &analysis.forms[0];
        assert_eq!(f.action_class.as_deref(), Some("com.x.CatAction"));
        assert_eq!(f.config_file.as_deref(), Some("s.xml"));
        assert_eq!(f.host_file, "/proj/page.jsp");

        let by = |n: &str| f.fields.iter().find(|x| x.name == n).unwrap();
        let title = by("title");
        assert!(title.bound && title.validated);
        // The spliced-in field keeps its own source tag.
        let secret = by("secret");
        assert!(secret.bound && !secret.validated);
        assert_eq!(secret.source_file, "/proj/frag.jspf");
        let ghost = by("ghost");
        assert!(!ghost.bound && !ghost.validated);
    }

    /// A form with NO action never calls the resolver and lists its fields all-false; the
    /// `truncated` flag flows through.
    #[test]
    fn actionless_form_skips_resolver_and_truncated_flows_through() {
        let expanded = ExpandedForms {
            forms: vec![ExpandedForm {
                action: None,
                method: None,
                host_file: "/proj/p.jsp".to_string(),
                start: 0,
                end: 1,
                fields: vec![field("q", FormControl::Text, "/proj/p.jsp")],
            }],
            truncated: true,
        };

        let mut calls = 0;
        let analysis = correlate_expanded(expanded, |_, _| {
            calls += 1;
            (None, None, Vec::new(), Vec::new())
        });

        assert_eq!(calls, 0, "no action → resolver never called");
        assert!(analysis.truncated);
        assert_eq!(analysis.forms.len(), 1);
        assert!(!analysis.forms[0].fields[0].bound);
        assert_eq!(analysis.forms[0].action_class, None);
    }

    /// No forms → an empty analysis (never an error).
    #[test]
    fn no_forms_is_empty() {
        let analysis =
            correlate_expanded(ExpandedForms { forms: Vec::new(), truncated: false }, |_, _| {
                (None, None, Vec::new(), Vec::new())
            });
        assert!(analysis.forms.is_empty());
        assert!(!analysis.truncated);
    }
}
