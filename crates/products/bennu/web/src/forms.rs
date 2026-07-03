//! JSP `<form>` scanner — each form's action reference + its input fields.
//!
//! Companion to [`crate::jsp`] (which extracts action *references* and taglibs). This
//! module answers a different question the FE inspector asks: *for this JSP, what forms
//! are here, what action does each post to, and what fields does each carry* — so the
//! integration can correlate a field name against the resolved action class's writable
//! properties (setters) and its validation rules ("form → action → fields, which bind,
//! which are validated").
//!
//! Same engineering as `jsp.rs`: a lightweight **linear byte scan** (a JSP is not valid
//! XML), reusing the very same masking + attribute-scan helpers from `jsp.rs` (they are
//! `pub(crate)` for exactly this reason — no copy-paste) so a `<form>`/`<input>` sitting
//! inside a `<%-- comment --%>` or `<% scriptlet %>` is ignored. Malformed / unclosed tags
//! are skipped, never fatal.

use std::path::Path;

use crate::jsp::{attr_value, find_from, masked_regions, normalize_action_ref, region_covering, tag_local_name};
use crate::model::{FormControl, JspForm, JspFormField};

/// Field tag local-names (after any `prefix:`) collected as form inputs: the HTML controls
/// plus the Struts UI-tag controls. Matched against a lowercased local-name.
const FIELD_TAGS: &[&str] = &[
    // HTML
    "input", "textarea", "select", //
    // Struts UI tags (`<s:*>` / legacy `<html:*>` share these local-names)
    "textfield", "password", "hidden", "checkbox", "checkboxlist", "radio", "file", "submit",
    "combobox", "datetimepicker",
    // Legacy Struts1 `<html:*>` controls whose local-name differs from the `<s:*>` set:
    // `<html:text property=>`, `<html:textarea>`, `<html:select>`, `<html:file>`, `<html:submit>`.
    "text",
];

/// Scan a JSP `source` string for `<form>`s and their input fields.
///
/// Comments (`<%-- … --%>`) and scriptlets (`<% … %>`) are masked first (shared with
/// [`crate::jsp`]), so a field/form inside them is ignored. Forms don't nest in practice; a
/// new `<form>` opener while one is open closes the previous conservatively.
pub fn parse_jsp_forms(source: &str) -> Vec<JspForm> {
    let bytes = source.as_bytes();
    let masked = masked_regions(source);

    let mut forms: Vec<JspForm> = Vec::new();
    // Index into `forms` of the form currently open (accepting fields), if any.
    let mut open: Option<usize> = None;

    let mut i = 0usize;
    while i < bytes.len() {
        if let Some(reg) = region_covering(&masked, i) {
            i = reg.1; // jump past a masked comment/scriptlet
            continue;
        }
        if bytes[i] != b'<' {
            i += 1;
            continue;
        }
        match classify_tag(source, i) {
            Some(Tag::FormOpen { form, tag_end }) => {
                // A new form conservatively closes any still-open one (forms don't nest).
                forms.push(form);
                open = Some(forms.len() - 1);
                i = tag_end;
            }
            Some(Tag::FormClose { tag_end }) => {
                if let Some(idx) = open.take() {
                    forms[idx].end = tag_end;
                }
                i = tag_end;
            }
            Some(Tag::Field { field, tag_end }) => {
                if let Some(idx) = open {
                    forms[idx].fields.push(field);
                }
                i = tag_end;
            }
            Some(Tag::Other { tag_end }) => i = tag_end,
            None => i += 1, // not a real tag (`</`, `<%`, `<!`, unterminated) — advance one
        }
    }

    forms
}

/// Convenience: read `path` and [`parse_jsp_forms`] it. A read error yields no forms
/// (skip-and-continue — mirrors [`crate::jsp::parse_jsp_file`]).
pub fn parse_jsp_forms_file(path: &Path) -> Vec<JspForm> {
    match std::fs::read_to_string(path) {
        Ok(text) => parse_jsp_forms(&text),
        Err(_) => Vec::new(),
    }
}

/// What a `<…>` at `open` turned out to be. Each variant carries the byte offset just past
/// the tag's `>` so the caller advances without re-scanning.
enum Tag {
    FormOpen { form: JspForm, tag_end: usize },
    FormClose { tag_end: usize },
    Field { field: JspFormField, tag_end: usize },
    Other { tag_end: usize },
}

/// Classify the tag starting at `open` (`source[open] == '<'`). `None` for a closer we don't
/// handle here (a non-form `</…>`), a `<%…`/`<!…` block, or an unterminated tag.
fn classify_tag(source: &str, open: usize) -> Option<Tag> {
    let bytes = source.as_bytes();
    let after = open + 1;
    if after >= bytes.len() {
        return None;
    }

    // Closing tag `</…>`: only a `</…form>` matters (it ends the open form).
    if bytes[after] == b'/' {
        let close = find_from(source, after, ">")?;
        let name = tag_local_name(source, after + 1, close)?; // skip the `/`
        if name == "form" {
            return Some(Tag::FormClose { tag_end: close + 1 });
        }
        return Some(Tag::Other { tag_end: close + 1 });
    }
    // Directive/scriptlet `<%…` or comment `<!…` — not a tag we open/scan.
    if matches!(bytes[after], b'%' | b'!') {
        return None;
    }

    let close = find_from(source, after, ">")?;
    let name = tag_local_name(source, after, close)?;

    if name == "form" {
        let action = attr_value(source, after, close, "action")
            .and_then(|(raw, _, _)| normalize_action_ref(&raw));
        let method = attr_value(source, after, close, "method").map(|(raw, _, _)| raw.trim().to_ascii_lowercase());
        let form = JspForm { action, method, start: open, end: close + 1, fields: Vec::new() };
        return Some(Tag::FormOpen { form, tag_end: close + 1 });
    }

    if FIELD_TAGS.contains(&name.as_str()) {
        if let Some(field) = field_from_tag(source, after, close, &name) {
            return Some(Tag::Field { field, tag_end: close + 1 });
        }
        return Some(Tag::Other { tag_end: close + 1 });
    }

    Some(Tag::Other { tag_end: close + 1 })
}

/// Build a [`JspFormField`] from a recognized field tag whose inner span is `[start, close)`
/// and lowercased local-name is `local`. A field with no name is skipped (`None`) — a
/// nameless submit's action is already the form's action.
fn field_from_tag(source: &str, start: usize, close: usize, local: &str) -> Option<JspFormField> {
    // HTML uses `name=`; legacy struts-html (`<html:text property="x">`) uses `property=`.
    let (name, vstart, vend) = attr_value(source, start, close, "name")
        .or_else(|| attr_value(source, start, close, "property"))?;
    if name.trim().is_empty() {
        return None;
    }
    let control = classify_control(source, start, close, local);
    Some(JspFormField { name, control, start: vstart, end: vend })
}

/// Map a field tag's local-name (+ an `<input type=>` when present) to a [`FormControl`].
fn classify_control(source: &str, start: usize, close: usize, local: &str) -> FormControl {
    match local {
        // Plain HTML `<input>`: the `type=` decides.
        "input" => match input_type(source, start, close).as_deref() {
            Some("password") => FormControl::Password,
            Some("hidden") => FormControl::Hidden,
            Some("checkbox") => FormControl::Checkbox,
            Some("radio") => FormControl::Radio,
            Some("submit") => FormControl::Submit,
            Some("file") => FormControl::File,
            _ => FormControl::Text,
        },
        "textarea" => FormControl::TextArea,
        "select" | "combobox" | "datetimepicker" => FormControl::Select,
        // Struts UI tags name the control directly.
        "textfield" | "text" => FormControl::Text,
        "password" => FormControl::Password,
        "hidden" => FormControl::Hidden,
        "checkbox" | "checkboxlist" => FormControl::Checkbox,
        "radio" => FormControl::Radio,
        "submit" => FormControl::Submit,
        "file" => FormControl::File,
        _ => FormControl::Other,
    }
}

/// The lowercased `type=` value of an `<input>` (`[start, close)`), if present.
fn input_type(source: &str, start: usize, close: usize) -> Option<String> {
    attr_value(source, start, close, "type").map(|(raw, _, _)| raw.trim().to_ascii_lowercase())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn struts_form_collects_named_fields_and_skips_nameless_submit() {
        let src = r#"<s:form action="/do/Cat/save">
            <s:textfield name="q"/>
            <s:select name="cat"/>
            <s:submit/>
        </s:form>"#;
        let forms = parse_jsp_forms(src);
        assert_eq!(forms.len(), 1);
        let f = &forms[0];
        assert_eq!(f.action.as_deref(), Some("/do/Cat/save"));
        // Two NAMED fields; the nameless submit is skipped.
        let names: Vec<&str> = f.fields.iter().map(|x| x.name.as_str()).collect();
        assert_eq!(names, vec!["q", "cat"]);
        assert_eq!(f.fields[0].control, FormControl::Text);
        assert_eq!(f.fields[1].control, FormControl::Select);
    }

    #[test]
    fn plain_html_form_action_method_and_controls() {
        let src = r#"<form action="save.action" method="post">
            <input type="hidden" name="id">
            <input name="title">
            <textarea name="body"></textarea>
        </form>"#;
        let forms = parse_jsp_forms(src);
        assert_eq!(forms.len(), 1);
        let f = &forms[0];
        assert_eq!(f.action.as_deref(), Some("save"));
        assert_eq!(f.method.as_deref(), Some("post"));
        let got: Vec<(&str, &str)> =
            f.fields.iter().map(|x| (x.name.as_str(), x.control.as_str())).collect();
        assert_eq!(got, vec![("id", "hidden"), ("title", "text"), ("body", "textarea")]);
    }

    #[test]
    fn legacy_html_form_uses_property_as_field_name() {
        let src = r#"<html:form action="/x.do">
            <html:text property="user"/>
        </html:form>"#;
        let forms = parse_jsp_forms(src);
        assert_eq!(forms.len(), 1);
        assert_eq!(forms[0].action.as_deref(), Some("/x"));
        assert_eq!(forms[0].fields.len(), 1);
        assert_eq!(forms[0].fields[0].name, "user");
        // `<html:text>` has local-name `text` (in FIELD_TAGS) and uses `property=` for its
        // name → mapped to a Text control.
        assert_eq!(forms[0].fields[0].control, FormControl::Text);
    }

    #[test]
    fn field_inside_comment_or_scriptlet_is_ignored() {
        let src = r#"<form action="save.action">
            <input name="real">
            <%-- <input name="commented"> --%>
            <% String s = "<input name=\"scriptletish\">"; %>
        </form>"#;
        let forms = parse_jsp_forms(src);
        assert_eq!(forms.len(), 1);
        let names: Vec<&str> = forms[0].fields.iter().map(|x| x.name.as_str()).collect();
        assert!(names.contains(&"real"), "names = {names:?}");
        assert!(!names.contains(&"commented"), "commented field leaked: {names:?}");
        assert!(!names.contains(&"scriptletish"), "scriptlet field leaked: {names:?}");
    }

    #[test]
    fn computed_action_yields_none() {
        let src = r#"<s:form action="%{url}"><s:textfield name="q"/></s:form>"#;
        let forms = parse_jsp_forms(src);
        assert_eq!(forms.len(), 1);
        assert_eq!(forms[0].action, None);
        // The form + its field are still collected — only the action key is inconclusive.
        assert_eq!(forms[0].fields.len(), 1);
    }

    #[test]
    fn field_name_span_points_at_the_name_value() {
        let src = r#"<form action="save.action"><input type="hidden" name="id"></form>"#;
        let forms = parse_jsp_forms(src);
        let f = &forms[0].fields[0];
        assert_eq!(&src[f.start..f.end], "id");
    }

    #[test]
    fn absent_action_is_none() {
        let src = r#"<form><input name="q"></form>"#;
        let forms = parse_jsp_forms(src);
        assert_eq!(forms.len(), 1);
        assert_eq!(forms[0].action, None);
        assert_eq!(forms[0].method, None);
    }

    #[test]
    fn empty_and_unreadable_are_graceful() {
        assert!(parse_jsp_forms("").is_empty());
        assert!(parse_jsp_forms_file(Path::new("/no/such/file.jsp")).is_empty());
    }

    #[test]
    fn reads_and_parses_a_file() {
        let src = r#"<form action="a.action"><input name="q"></form>"#;
        let path = crate::test_support::tmp("form.jsp", src);
        let forms = parse_jsp_forms_file(&path);
        assert_eq!(forms.len(), 1);
        assert_eq!(forms[0].action.as_deref(), Some("a"));
    }
}
