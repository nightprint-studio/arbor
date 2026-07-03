//! `form-analysis` domain — `bennu_form_analysis` (the form → action → fields inspector).
//!
//! For a JSP, list each `<form>` with its input fields, correlated against the resolved
//! action class: each field name is checked against the class's **writable properties**
//! (its `setXxx` setters — what the form binds) and its **validation rules** (what the
//! action validates). The FE (a future sidebar) shows "form → action → fields, which bind,
//! which are validated".
//!
//! The scan itself lives in `bennu-web` ([`parse_jsp_forms_file`]); the per-action
//! resolution lives on [`IndexService`] ([`form_action_context`]). This module holds the
//! thin handler that stitches them, plus the pure [`correlate_forms`] core (unit-tested off
//! fabricated forms + a fake resolver, no live project).

use std::collections::HashSet;

use bennu_core::prelude::BennuState;
use bennu_proto::prelude::{FormAnalysis, FormFieldInfo, FormInfo};
use bennu_web::prelude::{parse_jsp_forms_file, JspForm};
use serde::Deserialize;

use crate::index_service::IndexService;

/// Args for [`bennu_form_analysis`].
#[derive(Deserialize)]
pub struct FormAnalysisArgs {
    /// Absolute path (forward slashes) to the JSP being analysed.
    pub file: String,
}

/// Analyse the forms of a JSP: every `<form>`, its action's resolved class + declaring
/// config fragment, and each field's bind/validate correlation. Empty (`forms = []`) for a
/// non-JSP / project-less file — never an error (the FE degrades gracefully).
#[arbor_rpc::handler]
fn bennu_form_analysis(_ctx: &BennuState, args: FormAnalysisArgs) -> Result<FormAnalysis, String> {
    let svc = IndexService::global();
    let file = args.file;
    let forms = parse_jsp_forms_file(std::path::Path::new(&file));
    // Resolve each distinct form action against the owning project's config + index. The
    // closure is keyed by the form's normalized action reference (the `JspForm::action`).
    let analysis =
        correlate_forms(forms, |action| svc.form_action_context(&file, action));
    Ok(analysis)
}

/// The pure correlation core: join each parsed [`JspForm`] against its resolved action, via
/// a `resolve` closure that — given the form's action key — yields
/// `(class_fqcn, config_file, writable_props, validated_fields)`. A field's `bound` flag is
/// set when its name is one of the writable props; `validated` when it is one of the
/// validated fields.
///
/// A form with no resolvable action (`action == None`, or the closure returns empty sets) is
/// still emitted: every field is `bound = false` / `validated = false` and `action_class =
/// None`, so the FE always shows the form. Pure over its inputs (no `IndexService` / FS), so
/// the field→flag logic is unit-testable off in-memory fixtures.
pub fn correlate_forms(
    forms: Vec<JspForm>,
    mut resolve: impl FnMut(&str) -> (Option<String>, Option<String>, Vec<String>, Vec<String>),
) -> FormAnalysis {
    let forms = forms
        .into_iter()
        .map(|form| {
            // Only resolve a form that actually names an action; a computed / absent action
            // stays uncorrelated (all fields unbound).
            let (action_class, config_file, writable, validated) = match &form.action {
                Some(action) => resolve(action),
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
                start: form.start,
                end: form.end,
                fields,
            }
        })
        .collect();
    FormAnalysis { forms }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bennu_web::prelude::parse_jsp_forms;

    /// A field matching a writable prop is `bound`; matching a validation field is
    /// `validated`; an unmatched field is neither; and the resolved class/config flow through.
    #[test]
    fn correlates_bound_and_validated_flags() {
        let src = r#"<s:form action="/do/Cat/save">
            <s:textfield name="title"/>
            <s:password name="secret"/>
            <s:textfield name="ghost"/>
        </s:form>"#;
        let forms = parse_jsp_forms(src);

        let analysis = correlate_forms(forms, |action| {
            assert_eq!(action, "/do/Cat/save");
            (
                Some("com.x.CatAction".to_string()),
                Some("s.xml".to_string()),
                // writable: title + secret bind; ghost does not.
                vec!["title".to_string(), "secret".to_string()],
                // validated: only title has a rule.
                vec!["title".to_string()],
            )
        });

        assert_eq!(analysis.forms.len(), 1);
        let f = &analysis.forms[0];
        assert_eq!(f.action.as_deref(), Some("/do/Cat/save"));
        assert_eq!(f.action_class.as_deref(), Some("com.x.CatAction"));
        assert_eq!(f.config_file.as_deref(), Some("s.xml"));

        let by_name = |n: &str| f.fields.iter().find(|x| x.name == n).unwrap();
        // `title`: writable AND validated.
        let title = by_name("title");
        assert!(title.bound && title.validated);
        assert_eq!(title.control, "text");
        // `secret`: writable, NOT validated.
        let secret = by_name("secret");
        assert!(secret.bound && !secret.validated);
        assert_eq!(secret.control, "password");
        // `ghost`: neither.
        let ghost = by_name("ghost");
        assert!(!ghost.bound && !ghost.validated);
    }

    /// A form whose action doesn't resolve (closure yields empty sets) is still listed, with
    /// every field unbound/unvalidated and no action class.
    #[test]
    fn unresolved_action_lists_form_with_all_flags_false() {
        let src = r#"<form action="mystery.action"><input name="a"><input name="b"></form>"#;
        let forms = parse_jsp_forms(src);
        let analysis = correlate_forms(forms, |_| (None, None, Vec::new(), Vec::new()));
        assert_eq!(analysis.forms.len(), 1);
        let f = &analysis.forms[0];
        assert_eq!(f.action_class, None);
        assert_eq!(f.config_file, None);
        assert_eq!(f.fields.len(), 2);
        assert!(f.fields.iter().all(|x| !x.bound && !x.validated));
    }

    /// A form with NO action (`action == None`) never calls the resolver and lists its
    /// fields all-false. Also proves multiple forms in one JSP are each emitted.
    #[test]
    fn actionless_form_skips_resolver_and_multiple_forms_are_listed() {
        let src = r#"<form><input name="q"></form>
            <s:form action="/do/x"><s:textfield name="p"/></s:form>"#;
        let forms = parse_jsp_forms(src);

        let mut resolver_calls = 0;
        let analysis = correlate_forms(forms, |action| {
            resolver_calls += 1;
            assert_eq!(action, "/do/x");
            (Some("com.x.X".to_string()), None, vec!["p".to_string()], Vec::new())
        });

        assert_eq!(analysis.forms.len(), 2, "both forms listed");
        assert_eq!(resolver_calls, 1, "resolver called only for the action-bearing form");
        // Form 1: no action → all-false.
        assert_eq!(analysis.forms[0].action, None);
        assert!(!analysis.forms[0].fields[0].bound);
        // Form 2: `p` binds.
        assert_eq!(analysis.forms[1].action.as_deref(), Some("/do/x"));
        assert!(analysis.forms[1].fields[0].bound);
    }

    /// No forms in the JSP → an empty analysis (never an error).
    #[test]
    fn no_forms_is_empty() {
        let analysis = correlate_forms(Vec::new(), |_| (None, None, Vec::new(), Vec::new()));
        assert!(analysis.forms.is_empty());
    }
}
