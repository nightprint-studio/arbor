//! Pure **authoring** of Struts2 `*-validation.xml` — string-in/string-out, no filesystem, no
//! Tauri. The FE chain-builder hands us an ordered chain of validators for a field; we emit the
//! XML (a fresh DTD-headed skeleton, one `<field-validator>`, a whole `<field>` block, or an
//! *append* into an existing document that preserves its surrounding formatting).
//!
//! Keeping this pure is the whole point: correctness is proven by a large unit-test suite here
//! (round-trip author → [`parse_file`](crate::validation::parse_file) → re-author, append
//! idempotence, escaping) — there is no FE test runner, so the XML surgery must be nailed in Rust.

/// The XWork validator DTD the skeleton emits (declaration only — never fetched).
const DTD: &str = "<!DOCTYPE validators PUBLIC \"-//Apache Struts//XWork Validator 1.0.3//EN\" \"http://struts.apache.org/dtds/xwork-validator-1.0.3.dtd\">";

/// One validator to author — the FE `ValidatorChainItem` 1:1. `params` order is preserved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthoredValidator {
    pub type_name: String,
    pub params: Vec<(String, String)>,
    pub message: Option<AuthoredMessage>,
    pub short_circuit: bool,
}

/// A validator's message: an optional i18n `key` + an inline default `text` (either may be empty).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthoredMessage {
    pub key: Option<String>,
    pub text: String,
}

/// A fresh, empty, DTD-headed validation document (`<validators></validators>`).
pub fn author_validation_skeleton() -> String {
    format!("{DTD}\n<validators>\n</validators>\n")
}

/// Author a single `<field-validator>` element, indented by `indent` (children get `indent+4`).
/// Self-closing when it has no params and no message; otherwise a multi-line block.
pub fn author_field_validator(v: &AuthoredValidator, indent: &str) -> String {
    let sc = if v.short_circuit { " short-circuit=\"true\"" } else { "" };
    let ty = esc_attr(&v.type_name);
    if v.params.is_empty() && v.message.is_none() {
        return format!("{indent}<field-validator type=\"{ty}\"{sc}/>");
    }
    let child = format!("{indent}    ");
    let mut lines = vec![format!("{indent}<field-validator type=\"{ty}\"{sc}>")];
    for (name, value) in &v.params {
        lines.push(format!("{child}<param name=\"{}\">{}</param>", esc_attr(name), esc_text(value)));
    }
    if let Some(m) = &v.message {
        let keyattr = match m.key.as_deref().filter(|k| !k.is_empty()) {
            Some(k) => format!(" key=\"{}\"", esc_attr(k)),
            None => String::new(),
        };
        lines.push(format!("{child}<message{keyattr}>{}</message>", esc_text(&m.text)));
    }
    lines.push(format!("{indent}</field-validator>"));
    lines.join("\n")
}

/// Author a whole `<field name=…>` block wrapping the ordered validator chain, indented by
/// `indent` (validators get `indent+4`).
pub fn author_field_block(field: &str, validators: &[AuthoredValidator], indent: &str) -> String {
    let vindent = format!("{indent}    ");
    let mut lines = vec![format!("{indent}<field name=\"{}\">", esc_attr(field))];
    for v in validators {
        lines.push(author_field_validator(v, &vindent));
    }
    lines.push(format!("{indent}</field>"));
    lines.join("\n")
}

/// Add `validators` to `field` inside `existing_xml`, returning the new document:
///  - **field exists** → append the validators before its `</field>` (chain grows, order kept);
///  - **field absent but `<validators>` present** → insert a new `<field>` block before
///    `</validators>`;
///  - **no `<validators>` root** → author a fresh skeleton and insert the field block.
///
/// String-splices at line boundaries so the rest of the document's formatting is untouched. Best
/// effort on hand-formatted files: field lookup matches `<field name="x">` / `'x'` (the shape this
/// module — and Struts tooling — emits).
pub fn append_validator(existing_xml: &str, field: &str, validators: &[AuthoredValidator]) -> String {
    if validators.is_empty() {
        return existing_xml.to_string();
    }
    // Case A — the field already exists: append its validators before `</field>`.
    if let Some(line_start) = find_field_close_line_start(existing_xml, field) {
        let vindent = "        ";
        let mut ins = String::new();
        for v in validators {
            ins.push_str(&author_field_validator(v, vindent));
            ins.push('\n');
        }
        let mut out = String::with_capacity(existing_xml.len() + ins.len());
        out.push_str(&existing_xml[..line_start]);
        out.push_str(&ins);
        out.push_str(&existing_xml[line_start..]);
        return out;
    }
    // Case B — `<validators>` present but no such field: insert a new field block.
    let block = author_field_block(field, validators, "    ");
    if let Some(close_line_start) = closing_validators_line_start(existing_xml) {
        let mut out = String::with_capacity(existing_xml.len() + block.len() + 1);
        out.push_str(&existing_xml[..close_line_start]);
        out.push_str(&block);
        out.push('\n');
        out.push_str(&existing_xml[close_line_start..]);
        return out;
    }
    // Case C — no `<validators>` root: author fresh, then recurse (now hits Case B).
    append_validator(&author_validation_skeleton(), field, validators)
}

// ── internals ────────────────────────────────────────────────────────────────────

/// Byte index of the start of the line holding the `</field>` that closes `<field name="field">`.
fn find_field_close_line_start(hay: &str, field: &str) -> Option<usize> {
    let open = find_field_open(hay, field)?;
    let close_rel = hay[open..].find("</field>")?;
    let close_abs = open + close_rel;
    Some(hay[..close_abs].rfind('\n').map(|i| i + 1).unwrap_or(0))
}

/// Byte index of the `<field` that opens `<field name="field">` — skips `<field-validator>` (the
/// char after `<field` there is `-`, not whitespace) and `<param name=>` (not a `<field` tag).
fn find_field_open(hay: &str, field: &str) -> Option<usize> {
    let mut search = 0;
    while let Some(rel) = hay[search..].find("<field") {
        let start = search + rel;
        let after = &hay[start + "<field".len()..];
        let next = after.chars().next();
        let is_field_tag = matches!(next, Some(' ') | Some('\t') | Some('\n') | Some('\r'));
        if is_field_tag {
            if let Some(gt) = hay[start..].find('>') {
                let tag = &hay[start..start + gt];
                if tag.contains(&format!("name=\"{field}\"")) || tag.contains(&format!("name='{field}'")) {
                    return Some(start);
                }
            }
        }
        search = start + "<field".len();
    }
    None
}

/// Byte index of the start of the line holding the last `</validators>`.
fn closing_validators_line_start(hay: &str) -> Option<usize> {
    let c = hay.rfind("</validators>")?;
    Some(hay[..c].rfind('\n').map(|i| i + 1).unwrap_or(0))
}

/// Escape a string for use inside an XML attribute value (double-quoted).
fn esc_attr(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

/// Escape a string for use as XML element text.
fn esc_text(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::validation::parse_file;

    fn v(type_name: &str, params: &[(&str, &str)], msg: Option<&str>, sc: bool) -> AuthoredValidator {
        AuthoredValidator {
            type_name: type_name.to_string(),
            params: params.iter().map(|(k, val)| (k.to_string(), val.to_string())).collect(),
            message: msg.map(|t| AuthoredMessage { key: None, text: t.to_string() }),
            short_circuit: sc,
        }
    }

    #[test]
    fn skeleton_has_dtd_and_empty_validators() {
        let s = author_validation_skeleton();
        assert!(s.contains("XWork Validator 1.0.3"));
        assert!(s.contains("<validators>"));
        assert!(s.contains("</validators>"));
    }

    #[test]
    fn self_closing_when_no_params_or_message() {
        let out = author_field_validator(&v("required", &[], None, false), "    ");
        assert_eq!(out, "    <field-validator type=\"required\"/>");
    }

    #[test]
    fn short_circuit_attribute_only_when_true() {
        let on = author_field_validator(&v("required", &[], Some("x"), true), "");
        assert!(on.starts_with("<field-validator type=\"required\" short-circuit=\"true\">"));
        let off = author_field_validator(&v("required", &[], Some("x"), false), "");
        assert!(off.starts_with("<field-validator type=\"required\">"));
    }

    #[test]
    fn params_and_message_in_order() {
        let out = author_field_validator(
            &v("stringlength", &[("minLength", "3"), ("maxLength", "20")], Some("too long"), false),
            "",
        );
        let expected = "<field-validator type=\"stringlength\">\n    \
             <param name=\"minLength\">3</param>\n    \
             <param name=\"maxLength\">20</param>\n    \
             <message>too long</message>\n\
             </field-validator>";
        assert_eq!(out, expected);
    }

    #[test]
    fn message_key_is_rendered() {
        let mut val = v("required", &[], None, false);
        val.message = Some(AuthoredMessage { key: Some("field.req".into()), text: "Required".into() });
        let out = author_field_validator(&val, "");
        assert!(out.contains("<message key=\"field.req\">Required</message>"));
    }

    #[test]
    fn field_block_wraps_a_chain() {
        let out = author_field_block(
            "username",
            &[v("required", &[], Some("req"), true), v("stringlength", &[("maxLength", "10")], Some("len"), false)],
            "    ",
        );
        assert!(out.starts_with("    <field name=\"username\">"));
        assert!(out.trim_end().ends_with("</field>"));
        assert_eq!(out.matches("<field-validator").count(), 2);
        // Chain order preserved.
        assert!(out.find("required").unwrap() < out.find("stringlength").unwrap());
    }

    #[test]
    fn append_into_fresh_skeleton_inserts_before_close() {
        let out = append_validator(&author_validation_skeleton(), "email", &[v("email", &[], Some("bad"), false)]);
        assert!(out.contains("<field name=\"email\">"));
        // The new field sits before the closing tag.
        assert!(out.find("<field name=\"email\">").unwrap() < out.find("</validators>").unwrap());
    }

    #[test]
    fn append_to_existing_field_grows_the_chain() {
        let existing = "<validators>\n    <field name=\"username\">\n        <field-validator type=\"required\"><message>req</message></field-validator>\n    </field>\n</validators>\n";
        let out = append_validator(existing, "username", &[v("stringlength", &[("maxLength", "10")], Some("len"), false)]);
        assert_eq!(out.matches("<field name=\"username\">").count(), 1, "no duplicate field");
        assert_eq!(out.matches("<field-validator").count(), 2, "chain grew to 2");
        assert!(out.find("required").unwrap() < out.find("stringlength").unwrap(), "appended after existing");
    }

    #[test]
    fn append_new_field_when_absent() {
        let existing = "<validators>\n    <field name=\"a\">\n        <field-validator type=\"required\"/>\n    </field>\n</validators>\n";
        let out = append_validator(existing, "b", &[v("email", &[], Some("bad"), false)]);
        assert_eq!(out.matches("<field name=").count(), 2);
        assert!(out.contains("<field name=\"b\">"));
    }

    #[test]
    fn append_with_no_validators_root_authors_fresh() {
        let out = append_validator("", "x", &[v("required", &[], Some("r"), false)]);
        assert!(out.contains("<validators>"));
        assert!(out.contains("<field name=\"x\">"));
    }

    #[test]
    fn appending_is_not_idempotent_it_stacks() {
        let mut doc = author_validation_skeleton();
        doc = append_validator(&doc, "f", &[v("required", &[], Some("r"), false)]);
        doc = append_validator(&doc, "f", &[v("email", &[], Some("e"), false)]);
        assert_eq!(doc.matches("<field name=\"f\">").count(), 1);
        assert_eq!(doc.matches("<field-validator").count(), 2);
    }

    #[test]
    fn escapes_text_content() {
        // `<param>` value + `<message>` body are element TEXT: `<`, `>`, `&` escape, `"` does not.
        let out = author_field_validator(
            &v("regex", &[("regexExpression", "a<b&c\"d")], Some("x < y & z"), false),
            "",
        );
        assert!(out.contains("a&lt;b&amp;c\"d"));
        assert!(out.contains("x &lt; y &amp; z"));
    }

    #[test]
    fn escapes_attribute_values() {
        // The `type` + field `name` land in double-quoted ATTRIBUTES → `"` and `<`/`&` escape.
        let block = author_field_block("na\"me", &[v("required", &[], Some("r"), false)], "");
        assert!(block.contains("name=\"na&quot;me\""));
    }

    #[test]
    fn round_trips_through_the_parser() {
        // Author a full document, parse it back, and assert the chain survived intact.
        let chain = [
            v("requiredstring", &[("trim", "true")], Some("required"), true),
            v("stringlength", &[("minLength", "3"), ("maxLength", "20")], Some("length"), false),
        ];
        let doc = append_validator(&author_validation_skeleton(), "username", &chain);
        let dir = crate::test_support::tmp_dir("author-rt");
        let file = dir.join("LoginAction-validation.xml");
        std::fs::write(&file, &doc).unwrap();

        let rec = parse_file(&file).unwrap();
        let f = rec.fields.iter().find(|f| f.name == "username").unwrap();
        assert_eq!(f.validators.len(), 2);
        assert_eq!(f.validators[0].type_name, "requiredstring");
        assert!(f.validators[0].short_circuit);
        assert_eq!(
            f.validators[0].params.iter().map(|p| (p.name.as_str(), p.value.as_str())).collect::<Vec<_>>(),
            vec![("trim", "true")]
        );
        assert_eq!(f.validators[1].type_name, "stringlength");
        assert_eq!(f.validators[1].params.len(), 2);
        assert_eq!(f.validators[1].message.as_ref().unwrap().text, "length");
    }

    #[test]
    fn round_trip_preserves_escaped_values() {
        let doc = append_validator(
            &author_validation_skeleton(),
            "expr",
            &[v("regex", &[("regexExpression", "^<a>&\"b$")], Some("x & y"), false)],
        );
        let dir = crate::test_support::tmp_dir("author-esc");
        let file = dir.join("FooAction-validation.xml");
        std::fs::write(&file, &doc).unwrap();
        let rec = parse_file(&file).unwrap();
        let v0 = &rec.fields[0].validators[0];
        assert_eq!(v0.params[0].value, "^<a>&\"b$");
        assert_eq!(v0.message.as_ref().unwrap().text, "x & y");
    }
}
