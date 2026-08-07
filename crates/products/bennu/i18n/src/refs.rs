//! Where a message key is read.
//!
//! Six frameworks spell this six ways and every legacy app uses at least three of them, so the
//! rule here is by SHAPE rather than by a list of tags:
//!
//! - an attribute called `key`, or one whose name ends in `Key` — `<fmt:message key>`,
//!   `<bean:message key>`, `<html:errors key>`, `<display:column titleKey>`, a validator's
//!   `<message key>`;
//! - the `name` of a `*:text` tag — Struts 2's `<s:text name="…">`, whose `name` means something
//!   completely different from `name` on every other tag;
//! - the first string argument of `getText` / `getMessage` / `getString` in Java.
//!
//! A value that is computed (`${…}`, `%{…}`, a scriptlet) is **not** a key reference. It usually
//! IS one at runtime, but nothing here can say which, and a check that guessed would report a
//! missing key on every dynamic label in the project.
//!
//! Neither is a key that is answered from somewhere other than a `.properties` — Entando's
//! `<wp:i18n key="…">` reads the platform's label table in the **database**. See
//! [`reads_from_elsewhere`]: the shape rule above would otherwise sweep in a whole framework's
//! labels and report every one of them as missing.

/// One place a key is named.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyRef {
    pub key: String,
    /// Byte offset of the key text (inside the quotes).
    pub start: usize,
    /// Byte offset one past it.
    pub end: usize,
}

/// The file kinds a key can be named in.
pub fn is_scannable(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    [".jsp", ".jspf", ".jspx", ".tag", ".tagx", ".xml", ".java"]
        .iter()
        .any(|e| lower.ends_with(e))
}

/// Every key reference in a file, in source order.
pub fn keys_in(path: &str, source: &str) -> Vec<KeyRef> {
    if path.to_ascii_lowercase().ends_with(".java") {
        java_keys(source)
    } else if is_scannable(path) {
        markup_keys(source)
    } else {
        Vec::new()
    }
}

/// The key reference the caret sits on, if any.
pub fn key_at(path: &str, source: &str, offset: usize) -> Option<KeyRef> {
    keys_in(path, source).into_iter().find(|r| offset >= r.start && offset <= r.end)
}

/// The part of a key already typed at `offset`, when the caret sits inside a key-bearing
/// attribute value. `None` anywhere else — which is most of a page, and the difference between
/// a completion popup that helps and one that appears constantly.
///
/// Scans BACKWARDS rather than reusing [`keys_in`], because the value being completed is
/// half-written: `key="login.` is not a literal anything else here would recognise, and the empty
/// `key=""` — the moment the popup is most wanted — is not one at all.
pub fn key_prefix_at(path: &str, source: &str, offset: usize) -> Option<String> {
    if path.to_ascii_lowercase().ends_with(".java") || !is_scannable(path) {
        return None;
    }
    let bytes = source.as_bytes();
    if offset > bytes.len() {
        return None;
    }
    // The opening quote, on this line.
    let head = &source[..offset];
    let quote_at = head.rfind(['"', '\''])?;
    if head[quote_at..].contains('\n') {
        return None;
    }
    // Back over `=` and the attribute name.
    let mut p = quote_at;
    while p > 0 && bytes[p - 1].is_ascii_whitespace() {
        p -= 1;
    }
    if p == 0 || bytes[p - 1] != b'=' {
        return None;
    }
    p -= 1;
    while p > 0 && bytes[p - 1].is_ascii_whitespace() {
        p -= 1;
    }
    let attr_end = p;
    while p > 0 && is_ident_byte(bytes[p - 1]) {
        p -= 1;
    }
    let attr = &source[p..attr_end];
    if attr.is_empty() {
        return None;
    }
    let qualifies = (attr == "key" || (attr.len() > 3 && attr.ends_with("Key")))
        && !enclosing_tag_reads_elsewhere(source, p)
        || (attr == "name" && enclosing_tag_is_text(source, p));
    qualifies.then(|| source[quote_at + 1..offset].to_string())
}

/// Whether the tag open before `at` is one whose `key` is not a bundle key — see
/// [`reads_from_elsewhere`]. Completing bundle keys into a `<wp:i18n key>` would offer the wrong
/// vocabulary entirely.
fn enclosing_tag_reads_elsewhere(source: &str, at: usize) -> bool {
    let Some(open) = source[..at].rfind('<') else { return false };
    let bytes = source.as_bytes();
    let name_end = scan_while(bytes, open + 1, |b| !b" \t\r\n/>".contains(&b));
    let tag = &source[open + 1..name_end];
    reads_from_elsewhere(tag.rsplit(':').next().unwrap_or(tag))
}

/// Whether a tag's `key` names something that is **not** in a `.properties` bundle.
///
/// Entando's `<wp:i18n key="…">` is the case that matters, and it is not a detail: its labels live
/// in the platform's own table in the **database**, edited from the admin console, and no file on
/// disk declares them. Reading it as a bundle key put "no bundle declares…" under every label on
/// every page of an Entando application — a check that is wrong everywhere is worse than no check,
/// and this crate's whole stated rule is to under-report rather than risk that.
///
/// Matched on the LOCAL name rather than the `wp:` prefix, which a page is free to bind to
/// whatever it likes. `<s:i18n name="bundle">` is a different tag with a different attribute — it
/// pushes a bundle onto the stack — and is not affected.
///
/// **Exactly** `i18n`, not a family of names guessed at around it. Bennu has no Entando tag
/// support, and a list of tags nobody has confirmed exist is the same invention as the check this
/// removes — just pointing the other way. A second tag that turns out to read from the database
/// is one more line, added when it is seen rather than in advance.
fn reads_from_elsewhere(local: &str) -> bool {
    local.eq_ignore_ascii_case("i18n")
}

/// Whether the tag open before `at` is a `*:text` — the one place `name` names a key.
fn enclosing_tag_is_text(source: &str, at: usize) -> bool {
    let Some(open) = source[..at].rfind('<') else { return false };
    let bytes = source.as_bytes();
    let name_end = scan_while(bytes, open + 1, |b| !b" \t\r\n/>".contains(&b));
    let tag = &source[open + 1..name_end];
    tag.rsplit(':').next().unwrap_or(tag).eq_ignore_ascii_case("text")
}

/// The Java calls whose first string argument is a message key.
const LOOKUP_CALLS: &[&str] = &["getText", "getMessage", "getString"];

fn java_keys(source: &str) -> Vec<KeyRef> {
    let bytes = source.as_bytes();
    let mut out = Vec::new();
    for call in LOOKUP_CALLS {
        for (at, _) in source.match_indices(call) {
            // Must START an identifier — `getText` inside `formatGetText` is not this call.
            if at > 0 && is_java_ident_byte(bytes[at - 1]) {
                continue;
            }
            let mut p = at + call.len();
            p = skip_ws(bytes, p);
            if bytes.get(p) != Some(&b'(') {
                continue;
            }
            p = skip_ws(bytes, p + 1);
            let Some(lit) = string_literal(source, p) else { continue };
            out.push(lit);
        }
    }
    out.sort_by_key(|r| r.start);
    out
}

fn markup_keys(source: &str) -> Vec<KeyRef> {
    let bytes = source.as_bytes();
    let mut out = Vec::new();
    let mut i = 0usize;
    while i < bytes.len() {
        let Some(rel) = source[i..].find('<') else { break };
        let open = i + rel;
        i = open + 1;
        // Not a tag: a scriptlet / directive, a comment or a doctype, a close tag.
        if matches!(bytes.get(i), Some(b'%') | Some(b'!') | Some(b'?') | Some(b'/')) {
            continue;
        }
        let name_end = scan_while(bytes, i, |b| !b" \t\r\n/>".contains(&b));
        let tag = &source[i..name_end];
        if tag.is_empty() {
            continue;
        }
        let local = tag.rsplit(':').next().unwrap_or(tag);
        if reads_from_elsewhere(local) {
            i = name_end;
            continue;
        }
        // Struts 2's `<s:text name>` — the one tag where `name` is a key.
        let name_is_key = local.eq_ignore_ascii_case("text");

        let mut p = name_end;
        while p < bytes.len() && bytes[p] != b'>' {
            // `<% … %>` inside a tag (a scriptlet-valued attribute): skip it whole, its `>` is
            // not the tag's.
            if bytes[p] == b'<' && bytes.get(p + 1) == Some(&b'%') {
                p = source[p..].find("%>").map(|r| p + r + 2).unwrap_or(bytes.len());
                continue;
            }
            if !is_ident_byte(bytes[p]) {
                p += 1;
                continue;
            }
            let attr_end = scan_while(bytes, p, is_ident_byte);
            let attr = &source[p..attr_end];
            let after = skip_ws(bytes, attr_end);
            if bytes.get(after) != Some(&b'=') {
                p = attr_end;
                continue;
            }
            let value_at = skip_ws(bytes, after + 1);
            let Some(lit) = string_literal(source, value_at) else {
                p = attr_end;
                continue;
            };
            let is_key = attr == "key" || (attr.len() > 3 && attr.ends_with("Key")) || (name_is_key && attr == "name");
            if is_key {
                out.push(lit.clone());
            }
            p = lit.end + 1;
        }
        i = p;
    }
    out
}

/// The quoted literal starting at `at`, as a [`KeyRef`] over its contents. `None` when `at` is
/// not a quote, when the literal never closes, or when the value is **computed** — an expression
/// is not a key anyone can look up.
fn string_literal(source: &str, at: usize) -> Option<KeyRef> {
    let bytes = source.as_bytes();
    let quote = *bytes.get(at)?;
    if quote != b'"' && quote != b'\'' {
        return None;
    }
    let start = at + 1;
    let end = start + source[start..].find(quote as char)?;
    let key = &source[start..end];
    if key.is_empty() || key.contains("${") || key.contains("%{") || key.contains("<%") {
        return None;
    }
    Some(KeyRef { key: key.to_string(), start, end })
}

/// Whether a byte can be part of an XML/JSP **attribute name** — where `.` and `-` are ordinary
/// (`data-key`, `bean.title`).
fn is_ident_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b == b'.' || b == b'-'
}

/// Whether a byte can be part of a **Java identifier**.
///
/// Deliberately not [`is_ident_byte`]: a `.` is an attribute name's business and a *separator* in
/// Java, so sharing the predicate made `this.getMessage(…)` and `bundle.getString(…)` look like
/// the tail of a longer name and skipped them — which is how those calls are normally written.
fn is_java_ident_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b == b'$'
}

fn skip_ws(bytes: &[u8], mut at: usize) -> usize {
    while at < bytes.len() && bytes[at].is_ascii_whitespace() {
        at += 1;
    }
    at
}

fn scan_while(bytes: &[u8], mut at: usize, f: impl Fn(u8) -> bool) -> usize {
    while at < bytes.len() && f(bytes[at]) {
        at += 1;
    }
    at
}

#[cfg(test)]
mod tests {
    use super::*;

    fn keys(path: &str, src: &str) -> Vec<String> {
        keys_in(path, src).into_iter().map(|r| r.key).collect()
    }

    #[test]
    fn every_spelling_of_a_key_attribute_is_one() {
        let src = r#"
            <fmt:message key="a.one"/>
            <bean:message key="a.two" />
            <s:text name="a.three"/>
            <display:column titleKey="a.four" property="x"/>
            <html:errors key='a.five'>
        "#;
        assert_eq!(keys("/p/page.jsp", src), ["a.one", "a.two", "a.three", "a.four", "a.five"]);
    }

    #[test]
    fn name_is_a_key_only_on_a_text_tag() {
        // The trap: `name` means a form field, a bean, a parameter — everywhere but `<s:text>`.
        let src = r#"<s:textfield name="username"/><s:text name="label.user"/>"#;
        assert_eq!(keys("/p/page.jsp", src), ["label.user"]);
    }

    /// Entando's labels live in the platform's database, not in a `.properties`. Reading its
    /// `key` as a bundle key put a warning under every label on every page.
    #[test]
    fn an_entando_label_is_not_a_bundle_key() {
        let src = "<th scope=\"col\"><wp:i18n key=\"LABEL_COMUNICAZIONI_RIFERIMENTO\" /></th>\n\
                   <fmt:message key=\"real.one\"/>";
        assert_eq!(keys("/p/page.jsp", src), ["real.one"]);
        // Nor is a key completed into one: the vocabulary there is a different vocabulary.
        let at = src.find("LABEL_").unwrap() + "LABEL_".len();
        assert_eq!(key_prefix_at("/p/page.jsp", src, at), None);
    }

    /// The tag is skipped, not the rest of the LINE — a real page puts several on one.
    #[test]
    fn a_skipped_tag_does_not_swallow_what_follows_it() {
        let src = "<wp:i18n key=\"IGNORED\"/><fmt:message key=\"after.it\"/>";
        assert_eq!(keys("/p/page.jsp", src), ["after.it"]);
    }

    #[test]
    fn a_computed_value_is_not_a_key() {
        let src = r#"
            <s:text name="%{keyName}"/>
            <fmt:message key="${row.label}"/>
            <fmt:message key="literal.one"/>
        "#;
        assert_eq!(keys("/p/page.jsp", src), ["literal.one"]);
    }

    #[test]
    fn the_span_covers_the_key_and_nothing_else() {
        let src = "<fmt:message key=\"login.title\"/>";
        let r = &keys_in("/p/page.jsp", src)[0];
        assert_eq!(&src[r.start..r.end], "login.title");
        assert_eq!(key_at("/p/page.jsp", src, r.start + 2).map(|k| k.key), Some("login.title".into()));
        assert!(key_at("/p/page.jsp", src, 2).is_none(), "the tag name is not a key");
    }

    #[test]
    fn a_validation_message_is_a_key() {
        let src = r#"<field-validator type="required"><message key="err.required">x</message></field-validator>"#;
        assert_eq!(keys("/p/Foo-validation.xml", src), ["err.required"]);
    }

    #[test]
    fn java_lookups_are_found_and_lookalikes_are_not() {
        let src = r#"
            String a = getText("j.one");
            String b = this.getMessage( "j.two" , x);
            String c = bundle.getString("j.three");
            String d = formatGetText("not.a.key");
            String e = getText(dynamicKey);
        "#;
        assert_eq!(keys("/p/A.java", src), ["j.one", "j.two", "j.three"]);
    }

    #[test]
    fn a_half_typed_key_offers_its_prefix_and_nothing_else_does() {
        let src = "<fmt:message key=\"login.\"/><s:textfield name=\"user\"/><s:text name=\"lab\"/>";
        let at = src.find("login.").unwrap() + "login.".len();
        assert_eq!(key_prefix_at("/p/page.jsp", src, at), Some("login.".into()));
        // The empty value is exactly when the popup is most wanted.
        assert_eq!(key_prefix_at("/p/page.jsp", src, at - "login.".len()), Some(String::new()));
        // `name` on a field is not a key…
        let field = src.find("user").unwrap() + 2;
        assert_eq!(key_prefix_at("/p/page.jsp", src, field), None);
        // …but on `<s:text>` it is.
        let text = src.find("lab").unwrap() + 3;
        assert_eq!(key_prefix_at("/p/page.jsp", src, text), Some("lab".into()));
    }

    #[test]
    fn a_scriptlet_inside_a_tag_does_not_swallow_the_page() {
        let src = "<s:text name=\"<%= x %>\"/><fmt:message key=\"after.it\"/>";
        assert_eq!(keys("/p/page.jsp", src), ["after.it"]);
    }
}

