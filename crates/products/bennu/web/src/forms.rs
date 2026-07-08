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
    // Stack of enclosing conditional expressions (`<c:if test>` / `<s:if test>` / `else`),
    // innermost last — a field collected while this is non-empty is submitted CONDITIONALLY.
    let mut cond_stack: Vec<String> = Vec::new();

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
            Some(Tag::CondOpen { test, tag_end }) => {
                cond_stack.push(test);
                i = tag_end;
            }
            Some(Tag::CondClose { tag_end }) => {
                cond_stack.pop(); // tolerant: an unbalanced close just no-ops
                i = tag_end;
            }
            Some(Tag::Field { mut field, tag_end }) => {
                if let Some(idx) = open {
                    if let Some(cond) = cond_stack.last() {
                        field.conditional = true;
                        field.condition = Some(cond.clone());
                    }
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
    match crate::io::read_to_string_lf(path) {
        Ok(text) => parse_jsp_forms(&text),
        Err(_) => Vec::new(),
    }
}

/// Scan a JSP `source` for **every** input field, regardless of an enclosing `<form>`.
///
/// A `<jsp:include>`d fragment carries `<input>`/`<select>`/hidden controls that belong to the
/// *parent's* `<form>` (the include splices them in) — but the fragment itself has no `<form>`,
/// so [`parse_jsp_forms`] would miss them. This collects them all (with the same masking +
/// conditional-scope tracking), so the include-aware form aggregation can splice a fragment's
/// fields into the form that pulls it in.
pub fn parse_jsp_fields(source: &str) -> Vec<JspFormField> {
    let bytes = source.as_bytes();
    let masked = masked_regions(source);

    let mut fields: Vec<JspFormField> = Vec::new();
    let mut cond_stack: Vec<String> = Vec::new();

    let mut i = 0usize;
    while i < bytes.len() {
        if let Some(reg) = region_covering(&masked, i) {
            i = reg.1;
            continue;
        }
        if bytes[i] != b'<' {
            i += 1;
            continue;
        }
        match classify_tag(source, i) {
            Some(Tag::CondOpen { test, tag_end }) => {
                cond_stack.push(test);
                i = tag_end;
            }
            Some(Tag::CondClose { tag_end }) => {
                cond_stack.pop();
                i = tag_end;
            }
            Some(Tag::Field { mut field, tag_end }) => {
                if let Some(cond) = cond_stack.last() {
                    field.conditional = true;
                    field.condition = Some(cond.clone());
                }
                fields.push(field);
                i = tag_end;
            }
            Some(Tag::FormOpen { tag_end, .. })
            | Some(Tag::FormClose { tag_end })
            | Some(Tag::Other { tag_end }) => i = tag_end,
            None => i += 1,
        }
    }

    fields
}

/// Convenience: read `path` and [`parse_jsp_fields`] it. A read error yields no fields.
pub fn parse_jsp_fields_file(path: &Path) -> Vec<JspFormField> {
    match crate::io::read_to_string_lf(path) {
        Ok(text) => parse_jsp_fields(&text),
        Err(_) => Vec::new(),
    }
}

/// What a `<…>` at `open` turned out to be. Each variant carries the byte offset just past
/// the tag's `>` so the caller advances without re-scanning.
enum Tag {
    FormOpen { form: JspForm, tag_end: usize },
    FormClose { tag_end: usize },
    /// A conditional opener (`<c:if test>` / `<s:if test>` / `<c:when>` / `<c:otherwise>` …)
    /// with a non-empty body — fields between it and its close are submitted conditionally.
    CondOpen { test: String, tag_end: usize },
    /// A conditional closer (`</c:if>` / `</s:if>` / …).
    CondClose { tag_end: usize },
    Field { field: JspFormField, tag_end: usize },
    Other { tag_end: usize },
}

/// Classify the tag starting at `open` (`source[open] == '<'`). `None` for a `<%…`/`<!…`
/// block or an unterminated tag.
fn classify_tag(source: &str, open: usize) -> Option<Tag> {
    let bytes = source.as_bytes();
    let after = open + 1;
    if after >= bytes.len() {
        return None;
    }

    // Closing tag `</…>`: a `</…form>` ends the open form; a `</c:if>` etc. pops a condition.
    if bytes[after] == b'/' {
        let close = find_from(source, after, ">")?;
        let name = tag_local_name(source, after + 1, close)?; // skip the `/`
        if name == "form" {
            return Some(Tag::FormClose { tag_end: close + 1 });
        }
        if is_conditional_tag(&name) {
            return Some(Tag::CondClose { tag_end: close + 1 });
        }
        return Some(Tag::Other { tag_end: close + 1 });
    }
    // Directive/scriptlet `<%…` or comment `<!…` — not a tag we open/scan.
    if matches!(bytes[after], b'%' | b'!') {
        return None;
    }

    let close = find_from(source, after, ">")?;
    let name = tag_local_name(source, after, close)?;
    // Self-closing (`<x/>`) has no body, so it opens no conditional / form scope.
    let self_closing = close > after && bytes[close - 1] == b'/';

    if name == "form" {
        // A `<form>` start tag can carry a nested taglib in its `action=` (Entando
        // `action="<wp:action path='/x.action'/>"`), whose inner quote fools the naive
        // first-`>` scan. Find the tag's REAL end (quote-aware) and resolve the action key
        // through the nested tag when the outer value leaked one.
        let tag_end = find_tag_end(source, after).unwrap_or(close);
        let action = form_action(source, after, tag_end);
        let method = attr_value(source, after, tag_end, "method")
            .map(|(raw, _, _)| raw.trim().to_ascii_lowercase());
        let form = JspForm { action, method, start: open, end: tag_end + 1, fields: Vec::new() };
        return Some(Tag::FormOpen { form, tag_end: tag_end + 1 });
    }

    // A conditional container with a body → open a condition scope.
    if !self_closing && is_conditional_tag(&name) {
        return Some(Tag::CondOpen {
            test: conditional_expr(source, after, close, &name),
            tag_end: close + 1,
        });
    }

    if FIELD_TAGS.contains(&name.as_str()) {
        if let Some(field) = field_from_tag(source, after, close, &name) {
            return Some(Tag::Field { field, tag_end: close + 1 });
        }
        return Some(Tag::Other { tag_end: close + 1 });
    }

    Some(Tag::Other { tag_end: close + 1 })
}

/// The offset of the `>` that ends a start tag whose body begins at `after` (just past `<`),
/// skipping quoted attribute values so a `>` inside quotes — or a nested taglib emitted into
/// an attribute value (`action="<wp:action …/>"`) — does not terminate the tag early.
/// `None` if the tag is never closed. Quotes don't nest in HTML/XML attributes, so an
/// unbalanced inner quote (the Entando case) still lands the scan on the real closing `>`.
fn find_tag_end(source: &str, after: usize) -> Option<usize> {
    let bytes = source.as_bytes();
    let mut i = after;
    while i < bytes.len() {
        match bytes[i] {
            b'"' | b'\'' => {
                let quote = bytes[i];
                i += 1;
                while i < bytes.len() && bytes[i] != quote {
                    i += 1;
                }
                if i < bytes.len() {
                    i += 1; // step past the closing quote
                }
            }
            b'>' => return Some(i),
            _ => i += 1,
        }
    }
    None
}

/// Resolve a `<form>`'s action key from its start-tag span `[start, tag_end)`. Normally the
/// normalized `action=` value. For the Entando nested case
/// `action="<wp:action path="/x.action"/>"` the inner quote truncates the outer value to
/// `<wp:action path=`, so we read the nested `<…:action>`'s own `path=`/`name=` instead — the
/// meaningful action the form actually posts to.
fn form_action(source: &str, start: usize, tag_end: usize) -> Option<String> {
    let (raw, vstart, _) = attr_value(source, start, tag_end, "action")?;
    if raw.contains('<') {
        return nested_action_ref(source, vstart, tag_end);
    }
    normalize_action_ref(&raw)
}

/// Read the action key from a nested action tag that leaked into an attribute value: the `<`
/// at `nested_open` begins a `<…:action path=/name=>` (Entando `<wp:action>` URL generator or
/// Struts `<s:action>`). `limit` caps the search to the enclosing form start tag. `None` if it
/// isn't an `action` tag or carries no resolvable `path`/`name`.
fn nested_action_ref(source: &str, nested_open: usize, limit: usize) -> Option<String> {
    let bytes = source.as_bytes();
    if nested_open >= bytes.len() || bytes[nested_open] != b'<' {
        return None;
    }
    let nclose = find_from(source, nested_open, ">")?;
    if nclose >= limit {
        return None;
    }
    if tag_local_name(source, nested_open + 1, nclose)? != "action" {
        return None;
    }
    let (val, _, _) = attr_value(source, nested_open + 1, nclose, "path")
        .or_else(|| attr_value(source, nested_open + 1, nclose, "name"))?;
    normalize_action_ref(&val)
}

/// JSTL / Struts conditional container local-names (after any `prefix:`) whose body is
/// submitted only when a condition holds: `<c:if>`/`<s:if>`, `<c:when>`, `<c:otherwise>`,
/// `<s:elseif>`, `<s:else>`.
fn is_conditional_tag(local: &str) -> bool {
    matches!(local, "if" | "when" | "elseif" | "otherwise" | "else")
}

/// The condition expression for a conditional opener: the `test=` attribute of an
/// `if`/`when`/`elseif`, or `"else"` for the `otherwise`/`else` fallthrough branch. Falls back
/// to the tag name when a `test=` is expected but absent/empty.
fn conditional_expr(source: &str, start: usize, close: usize, local: &str) -> String {
    match local {
        "otherwise" | "else" => "else".to_string(),
        _ => attr_value(source, start, close, "test")
            .map(|(raw, _, _)| raw.trim().to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| local.to_string()),
    }
}

/// Build a [`JspFormField`] from a recognized field tag whose inner span is `[start, close)`
/// and lowercased local-name is `local`. A field with no name is skipped (`None`) — a
/// nameless submit's action is already the form's action. `conditional`/`condition` are left
/// default here and stamped by the caller from the enclosing conditional stack.
fn field_from_tag(source: &str, start: usize, close: usize, local: &str) -> Option<JspFormField> {
    // HTML uses `name=`; legacy struts-html (`<html:text property="x">`) uses `property=`.
    let (name, vstart, vend) = attr_value(source, start, close, "name")
        .or_else(|| attr_value(source, start, close, "property"))?;
    if name.trim().is_empty() {
        return None;
    }
    let control = classify_control(source, start, close, local);
    // The submitted `value=` (a fixed value or an `${…}`/`%{…}` expression), if present.
    let value = attr_value(source, start, close, "value")
        .map(|(raw, _, _)| raw.trim().to_string())
        .filter(|s| !s.is_empty());
    Some(JspFormField { name, control, value, conditional: false, condition: None, start: vstart, end: vend })
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

    // ── hidden fields + submitted values ────────────────────────────────────────

    #[test]
    fn hidden_field_captures_its_value() {
        let src = r#"<form action="a.action"><input type="hidden" name="id" value="42"></form>"#;
        let forms = parse_jsp_forms(src);
        let f = &forms[0].fields[0];
        assert_eq!(f.name, "id");
        assert_eq!(f.control, FormControl::Hidden);
        assert_eq!(f.value.as_deref(), Some("42"));
        assert!(!f.conditional);
        assert_eq!(f.condition, None);
    }

    #[test]
    fn value_expression_is_captured_verbatim() {
        // EL / OGNL values are the "hypothetical value" the form posts — kept as written.
        let src = r#"<s:form action="a"><s:hidden name="tok" value="%{token}"/><input name="u" value="${user.id}"></s:form>"#;
        let forms = parse_jsp_forms(src);
        let fields = &forms[0].fields;
        assert_eq!(fields[0].value.as_deref(), Some("%{token}"));
        assert_eq!(fields[1].value.as_deref(), Some("${user.id}"));
    }

    #[test]
    fn value_absent_is_none() {
        let src = r#"<form action="a"><input name="q"></form>"#;
        let forms = parse_jsp_forms(src);
        assert_eq!(forms[0].fields[0].value, None);
    }

    #[test]
    fn all_hidden_fields_are_collected() {
        let src = r#"<form action="a">
            <input type="hidden" name="a" value="1">
            <input type="hidden" name="b" value="2">
            <s:hidden name="c" value="3"/>
        </form>"#;
        let forms = parse_jsp_forms(src);
        let names: Vec<&str> = forms[0].fields.iter().map(|f| f.name.as_str()).collect();
        assert_eq!(names, vec!["a", "b", "c"]);
        assert!(forms[0].fields.iter().all(|f| f.control == FormControl::Hidden));
    }

    // ── conditional fields (`<c:if>` / `<s:if>` / `<c:choose>`) ──────────────────

    #[test]
    fn field_inside_c_if_is_conditional_with_its_test() {
        let src = r#"<form action="a">
            <c:if test="${admin}"><input type="hidden" name="role" value="ADMIN"></c:if>
        </form>"#;
        let forms = parse_jsp_forms(src);
        let f = &forms[0].fields[0];
        assert!(f.conditional, "field inside <c:if> must be conditional");
        assert_eq!(f.condition.as_deref(), Some("${admin}"));
    }

    #[test]
    fn field_inside_s_if_is_conditional() {
        let src = r#"<s:form action="a"><s:if test="%{loggedIn}"><s:hidden name="uid" value="%{id}"/></s:if></s:form>"#;
        let forms = parse_jsp_forms(src);
        let f = &forms[0].fields[0];
        assert!(f.conditional);
        assert_eq!(f.condition.as_deref(), Some("%{loggedIn}"));
    }

    #[test]
    fn field_outside_conditional_is_unconditional() {
        let src = r#"<form action="a">
            <c:if test="${x}"><input name="cond"></c:if>
            <input name="always">
        </form>"#;
        let forms = parse_jsp_forms(src);
        let fields = &forms[0].fields;
        let cond = fields.iter().find(|f| f.name == "cond").unwrap();
        let always = fields.iter().find(|f| f.name == "always").unwrap();
        assert!(cond.conditional);
        assert!(!always.conditional, "field AFTER </c:if> must not be conditional");
        assert_eq!(always.condition, None);
    }

    #[test]
    fn nested_conditionals_use_innermost_test() {
        let src = r#"<form action="a">
            <c:if test="${a}"><s:if test="%{b}"><input name="deep" value="v"></s:if></c:if>
        </form>"#;
        let forms = parse_jsp_forms(src);
        let f = &forms[0].fields[0];
        assert!(f.conditional);
        assert_eq!(f.condition.as_deref(), Some("%{b}"), "innermost condition wins");
    }

    #[test]
    fn choose_when_otherwise_branches_are_conditional() {
        let src = r#"<form action="a">
            <c:choose>
              <c:when test="${vip}"><input type="hidden" name="tier" value="VIP"></c:when>
              <c:otherwise><input type="hidden" name="tier" value="STD"></c:otherwise>
            </c:choose>
        </form>"#;
        let forms = parse_jsp_forms(src);
        let fields = &forms[0].fields;
        assert_eq!(fields.len(), 2);
        assert!(fields.iter().all(|f| f.conditional));
        assert_eq!(fields[0].condition.as_deref(), Some("${vip}"));
        assert_eq!(fields[1].condition.as_deref(), Some("else"));
    }

    #[test]
    fn struts_else_branch_condition_is_else() {
        let src = r#"<s:form action="a">
            <s:if test="%{ok}"><s:hidden name="a" value="1"/></s:if>
            <s:else><s:hidden name="a" value="0"/></s:else>
        </s:form>"#;
        let forms = parse_jsp_forms(src);
        let fields = &forms[0].fields;
        assert_eq!(fields[0].condition.as_deref(), Some("%{ok}"));
        assert_eq!(fields[1].condition.as_deref(), Some("else"));
    }

    #[test]
    fn self_closing_conditional_opens_no_scope() {
        // A self-closing `<c:if test="x"/>` has no body — a later field is NOT conditional.
        let src = r#"<form action="a"><c:if test="${x}"/><input name="q"></form>"#;
        let forms = parse_jsp_forms(src);
        let f = &forms[0].fields[0];
        assert!(!f.conditional, "self-closing conditional must open no scope");
    }

    #[test]
    fn unbalanced_conditional_close_is_tolerant() {
        // A stray `</c:if>` with no matching open must not panic / underflow.
        let src = r#"<form action="a"></c:if><input name="q" value="v"></form>"#;
        let forms = parse_jsp_forms(src);
        let f = &forms[0].fields[0];
        assert_eq!(f.name, "q");
        assert!(!f.conditional);
    }

    #[test]
    fn conditional_test_absent_falls_back_to_tag_name() {
        // A `<c:if>` with no `test=` (malformed) still opens a scope, labelled by the tag.
        let src = r#"<form action="a"><c:if><input name="q"></c:if></form>"#;
        let forms = parse_jsp_forms(src);
        let f = &forms[0].fields[0];
        assert!(f.conditional);
        assert_eq!(f.condition.as_deref(), Some("if"));
    }

    #[test]
    fn conditional_wrapping_the_whole_form_marks_all_fields() {
        let src = r#"<c:if test="${show}"><form action="a">
            <input type="hidden" name="a" value="1">
            <input name="b">
        </form></c:if>"#;
        let forms = parse_jsp_forms(src);
        let fields = &forms[0].fields;
        assert_eq!(fields.len(), 2);
        assert!(fields.iter().all(|f| f.conditional && f.condition.as_deref() == Some("${show}")));
    }

    #[test]
    fn conditional_field_inside_comment_is_ignored() {
        // Masking still wins: a conditional + field inside a comment contribute nothing.
        let src = r#"<form action="a">
            <%-- <c:if test="${x}"><input name="ghost"></c:if> --%>
            <input name="real">
        </form>"#;
        let forms = parse_jsp_forms(src);
        let fields = &forms[0].fields;
        assert_eq!(fields.len(), 1);
        assert_eq!(fields[0].name, "real");
        assert!(!fields[0].conditional);
    }

    // ── nested `<wp:action>` in the form action + quote-aware tag end ─────────────

    #[test]
    fn form_action_reads_nested_wp_action_path() {
        // Entando: `action="<wp:action path="/x.action"/>"`. The inner quote truncates the
        // outer value; the real action is the nested tag's `path=` (with `.action` stripped).
        let src = r#"<form action="<wp:action path="/ExtStr2/do/FrontEnd/DatiImpr/processPage.action" />" method="post">
            <input type="hidden" name="ext" value="${param.ext}">
        </form>"#;
        let forms = parse_jsp_forms(src);
        assert_eq!(forms.len(), 1);
        let f = &forms[0];
        assert_eq!(f.action.as_deref(), Some("/ExtStr2/do/FrontEnd/DatiImpr/processPage"));
        // The quote-aware tag end still finds `method="post"` past the nested tag.
        assert_eq!(f.method.as_deref(), Some("post"));
        // And the field after the (multi-quote) open tag is still collected.
        assert_eq!(f.fields.len(), 1);
        assert_eq!(f.fields[0].name, "ext");
        assert_eq!(f.fields[0].value.as_deref(), Some("${param.ext}"));
    }

    #[test]
    fn form_action_reads_nested_s_action_name() {
        let src = r#"<form action="<s:action name="listCategories"/>"><input name="q"></form>"#;
        let forms = parse_jsp_forms(src);
        assert_eq!(forms[0].action.as_deref(), Some("listCategories"));
    }

    #[test]
    fn form_action_with_gt_in_a_value_is_not_cut_short() {
        // A `>` inside a quoted attribute value must not end the tag early: `method` (declared
        // after such a value) is still read.
        let src = r#"<form action="save.action" title="a > b" method="get"><input name="q"></form>"#;
        let forms = parse_jsp_forms(src);
        assert_eq!(forms[0].action.as_deref(), Some("save"));
        assert_eq!(forms[0].method.as_deref(), Some("get"));
    }

    // ── parse_jsp_fields (all fields, no enclosing <form> needed) ─────────────────

    #[test]
    fn parse_jsp_fields_collects_fragment_inputs_without_a_form() {
        // A fragment (no `<form>`) still yields its inputs — they belong to the parent's form.
        let src = r#"<input type="hidden" name="_tk" value="${token}">
            <s:textfield name="q"/>
            <s:select name="cat"/>"#;
        let fields = parse_jsp_fields(src);
        let names: Vec<&str> = fields.iter().map(|f| f.name.as_str()).collect();
        assert_eq!(names, vec!["_tk", "q", "cat"]);
        assert_eq!(fields[0].control, FormControl::Hidden);
        assert_eq!(fields[0].value.as_deref(), Some("${token}"));
    }

    #[test]
    fn parse_jsp_fields_tracks_conditionals_and_masks_comments() {
        let src = r#"<c:if test="${admin}"><input type="hidden" name="role" value="ADMIN"></c:if>
            <input name="always">
            <%-- <input name="ghost"> --%>"#;
        let fields = parse_jsp_fields(src);
        let names: Vec<&str> = fields.iter().map(|f| f.name.as_str()).collect();
        assert_eq!(names, vec!["role", "always"], "commented field must not leak");
        let role = fields.iter().find(|f| f.name == "role").unwrap();
        assert!(role.conditional);
        assert_eq!(role.condition.as_deref(), Some("${admin}"));
        assert!(!fields.iter().find(|f| f.name == "always").unwrap().conditional);
    }

    #[test]
    fn parse_jsp_fields_empty_and_unreadable_are_graceful() {
        assert!(parse_jsp_fields("").is_empty());
        assert!(parse_jsp_fields_file(Path::new("/no/such/frag.jspf")).is_empty());
    }
}
