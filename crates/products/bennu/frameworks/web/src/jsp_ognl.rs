//! Where a page writes OGNL **without saying so**, and what scope it is written in.
//!
//! Two facts about Struts that a page never spells out, and without which half the expressions
//! in a legacy JSP are invisible to an editor.
//!
//! ## 1. Most Struts attributes are expressions already
//!
//! ```jsp
//! <s:iterator value="comunicazioni.dati" var="riga">
//! <s:if test="showRiferimento">
//! ```
//!
//! Neither value is wrapped in `%{…}`, and both are OGNL: the tag's attribute is declared as an
//! `Object` (or a `Boolean`), so Struts evaluates it against the value stack rather than passing
//! it through as text. `%{…}` is only needed on the attributes declared `String`, which is
//! exactly backwards from what a reader expects and is why so much legacy markup has none.
//!
//! An editor that only looks inside `%{…}` therefore cannot follow `comunicazioni.dati` at all —
//! which is what this module fixes, for **go-to only**. Never for the checks: see below.
//!
//! ## 2. An iterator pushes its element onto the value stack
//!
//! ```jsp
//! <s:iterator value="comunicazioni.dati" var="riga">
//!   <s:property value="%{codice}"/>
//! </s:iterator>
//! ```
//!
//! `codice` is not a property of the action. It is a property of whatever `comunicazioni.dati`
//! holds — the iterator pushed the current element on top of the stack, and an unqualified name
//! is resolved against the top first. Without knowing that, an editor reports every name inside
//! every loop as "no such property on the action", which is a page full of yellow and a warning
//! nobody will read again.
//!
//! ## Why go-to is permissive here and the check is not
//!
//! They fail differently. A go-to that resolves nothing does nothing, and the user tries
//! something else; a **check** that flags a name is a claim, and a wrong claim trains people to
//! ignore the right ones. So:
//!
//! * [`ognl_attr_path_at`] answers on a bare attribute value — a caret question, so a wrong
//!   guess costs a jump that does not happen;
//! * the checks stay on `%{…}` and additionally go **silent inside an iterator whose element
//!   type could not be resolved**, because "I cannot see that type" is not evidence that a
//!   property is missing.
//!
//! That second rule is [`iterator_scopes`]'s reason for existing, and it is deliberately
//! conservative in both directions: a resolvable element type lets the check keep working inside
//! the loop, an unresolvable one turns it off there and nowhere else.

use crate::jsp::{attr_value, find_from, masked_regions, region_covering};
use crate::jsp_vars::{path_in_range, OgnlPath};

/// The taglib URIs that mean Struts. WebWork is the name the same tags shipped under before
/// Struts 2 absorbed it, and a genuinely old codebase still declares it.
const STRUTS_URIS: &[&str] = &["/struts-tags", "/webwork", "struts-tags", "webwork"];

/// Which attributes of which Struts tags hold an expression when nothing wraps them.
///
/// A table rather than a rule, because the rule ("whatever the TLD declares non-`String`") needs
/// the TLD and would still be wrong for the tags whose `value` is declared `String` and evaluated
/// anyway. These are the ones a legacy page is actually made of; an attribute not listed is
/// treated as text, which is the safe direction — it costs a go-to that does not fire, not a
/// wrong one.
const OGNL_ATTRS: &[(&str, &[&str])] = &[
    ("iterator", &["value", "begin", "end", "step"]),
    ("property", &["value", "default"]),
    ("if", &["test"]),
    ("elseif", &["test"]),
    ("set", &["value"]),
    ("push", &["value"]),
    ("bean", &["value"]),
    ("select", &["list", "value", "listKey", "listValue"]),
    ("checkboxlist", &["list", "value"]),
    ("radio", &["list", "value"]),
    ("optiontransferselect", &["list", "value"]),
    ("hidden", &["value"]),
    ("textfield", &["value"]),
    ("textarea", &["value"]),
    ("checkbox", &["value", "fieldValue"]),
    ("password", &["value"]),
    ("submit", &["value"]),
    ("label", &["value"]),
    ("a", &["value"]),
    ("url", &["value"]),
    ("param", &["value"]),
    ("action", &["value"]),
    ("sort", &["source", "comparator"]),
    ("subset", &["source", "count", "start"]),
    ("append", &["value"]),
    ("merge", &["value"]),
];

/// The Struts tags whose `name=` is a **value-stack property** — the form controls, and nothing
/// else.
///
/// A **positive** list, and that is the point of it. Struts spells several unrelated ideas
/// `name=`, and reading them all as properties is how a page fills with warnings about names that
/// were never meant to be any:
///
/// | Tag | What `name=` is |
/// |---|---|
/// | `<s:textfield>`, `<s:select>`, `<s:hidden>`, … | a **property** of the action |
/// | `<s:text name="label.user"/>` | a key in a **resource bundle** |
/// | `<s:i18n name="…">` | the **bundle** itself |
/// | `<s:action name="…">` | an **action** to invoke |
/// | `<s:bean name="com.acme.X">` | a **class** |
/// | `<s:param name="…">` | the **parameter's own name** (its `value=` is the expression) |
/// | `<s:form name="…">` | the HTML element's name |
///
/// `text` is the one that actually collides: Struts 1 writes `<html:text property="user"/>`, a
/// text input, and Struts 2 writes `<s:text name="label.user"/>`, an i18n lookup. Same local
/// name, opposite meanings — which is why the prefix has to be resolved rather than assumed, and
/// why an unknown `<s:…>` tag is left alone instead of guessed at.
const STRUTS_CONTROL_TAGS: &[&str] = &[
    "textfield", "password", "hidden", "textarea", "select", "checkbox", "checkboxlist", "radio",
    "file", "combobox", "doubleselect", "optiontransferselect", "inputtransferselect",
    "updownselect", "datetimepicker", "token", "label", "submit", "reset",
];

/// Whether `name=` on this Struts tag names a property of the action. See [`STRUTS_CONTROL_TAGS`].
pub fn struts_name_is_property(local: &str) -> bool {
    STRUTS_CONTROL_TAGS.contains(&local)
}

/// The prefixes this page bound to a Struts tag library, in a form a per-tag test can use.
///
/// Public because the form scan needs the same answer this module already computes, and two
/// readings of "which prefix is Struts here" would disagree on the page that uses `ww:`.
pub fn struts_tag_prefixes(source: &str) -> Vec<String> {
    struts_prefixes(source).unwrap_or_default()
}

/// The tag names that push their element onto the value stack for the length of their body.
///
/// `s:iterator` is the one that matters. `s:push` is the same mechanism written explicitly, and
/// `s:bean` is the 2.0-era spelling of it — all three change what an unqualified name means
/// underneath them, so all three have to be seen.
const PUSHING_TAGS: &[&str] = &["iterator", "push", "bean"];

/// One `<s:iterator>` (or `push`/`bean`) and the region its element is on top of the stack for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IteratorScope {
    /// The tag's local name — `iterator`, `push`, `bean`.
    pub tag: String,
    /// The expression whose elements are being walked, as written and with whatever delimiters
    /// it had (`%{elencoBandi}`, `comunicazioni.dati`). Empty when the tag names none, which for
    /// `s:iterator` means it is iterating whatever is already on top of the stack.
    pub source_expr: String,
    /// The `var=` name, when it declared one. Not needed to type the scope — a `var` is a
    /// convenience, and the push happens with or without it.
    pub var: Option<String>,
    /// First byte after the opening tag.
    pub body_start: usize,
    /// The matching close, or the end of the file when the page never closes it.
    pub body_end: usize,
}

/// Every pushing scope in the page, outermost first.
///
/// Nested loops are ordinary in a legacy table, so the answer is a list and not a map: `#status`
/// is per-loop and so is the element, and the caller wants the **innermost** one that contains
/// its caret ([`innermost_scope_at`]) with the rest available as its ancestors.
pub fn iterator_scopes(source: &str) -> Vec<IteratorScope> {
    let Some(prefixes) = struts_prefixes(source) else { return Vec::new() };
    let masked = masked_regions(source);
    let bytes = source.as_bytes();

    let mut out: Vec<IteratorScope> = Vec::new();
    // (index into `out`, the qualified name it was opened with) — a stack, so nesting closes in
    // the right order and an unmatched close cannot detach a scope that is still open.
    let mut open: Vec<(usize, String)> = Vec::new();
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
        let after = i + 1;
        if after >= bytes.len() {
            break;
        }
        let closing = bytes[after] == b'/';
        if matches!(bytes[after], b'%' | b'!') {
            i += 1;
            continue;
        }
        let name_from = if closing { after + 1 } else { after };
        let Some(close) = find_from(source, name_from, ">") else { break };

        let Some(qname) = qualified_name(source, name_from, close) else {
            i = close + 1;
            continue;
        };
        let Some((prefix, local)) = qname.split_once(':') else {
            i = close + 1;
            continue;
        };
        if !prefixes.iter().any(|p| p.as_str() == prefix) || !PUSHING_TAGS.contains(&local) {
            i = close + 1;
            continue;
        }

        if closing {
            // The innermost still-open scope with this name. Not simply the innermost: a page
            // that closes `</s:iterator>` while an `<s:push>` is open is closing the iterator,
            // and ending the push instead would put the rest of the loop in the wrong scope.
            if let Some(at) = open.iter().rposition(|(_, n)| n == &qname) {
                while open.len() > at {
                    let (idx, _) = open.pop().expect("the position exists");
                    out[idx].body_end = i;
                }
            }
            i = close + 1;
            continue;
        }

        // `<s:iterator … />` pushes nothing anybody can write inside — it has no body.
        let self_closing = bytes.get(close.wrapping_sub(1)) == Some(&b'/');
        if !self_closing {
            let source_expr = attr_value(source, name_from, close, "value")
                .map(|(v, _, _)| v)
                .unwrap_or_default();
            let var = attr_value(source, name_from, close, "var")
                .or_else(|| attr_value(source, name_from, close, "id"))
                .map(|(v, _, _)| v)
                .filter(|v| !v.trim().is_empty());
            out.push(IteratorScope {
                tag: local.to_string(),
                source_expr,
                var,
                body_start: close + 1,
                // Until the close is found. A page that never closes it means the scope runs to
                // the end of the file, which is also what the browser would have done.
                body_end: source.len(),
                });
            open.push((out.len() - 1, qname));
        }
        i = close + 1;
    }

    out
}

/// The innermost scope containing `offset`, and the ones enclosing it, outermost first.
///
/// Both, because a name resolves against the stack **top down**: the innermost element first,
/// then the one outside it, and only then the action. A caller that only had the innermost would
/// answer wrongly for the (common) `<s:iterator>` inside `<s:iterator>` table.
pub fn scopes_at(scopes: &[IteratorScope], offset: usize) -> Vec<&IteratorScope> {
    scopes.iter().filter(|s| offset >= s.body_start && offset < s.body_end).collect()
}

/// The dotted path under `offset` when the caret is inside a **bare** OGNL attribute value —
/// `<s:iterator value="comunicazioni.dati">` with no `%{…}` around it.
///
/// `None` unless every one of these holds, and each of them is a way of being wrong:
///
/// * the caret is inside a tag's attribute *value*, not its name and not the text around it;
/// * the tag's prefix is one this page bound to a Struts library — `<c:if test="…">` is JSTL EL
///   and a different language;
/// * that tag/attribute pair is one Struts evaluates ([`OGNL_ATTRS`]);
/// * there is a dotted path under the caret at all — a name inside a larger expression counts,
///   because `stato` in `test="stato == 'APERTO'"` is a real property and a real go-to.
pub fn ognl_attr_path_at(source: &str, offset: usize) -> Option<OgnlPath> {
    let prefixes = struts_prefixes(source)?;
    let masked = masked_regions(source);
    let bytes = source.as_bytes();
    let mut i = 0usize;

    while i < bytes.len() {
        if let Some(reg) = region_covering(&masked, i) {
            i = reg.1;
            continue;
        }
        if bytes[i] != b'<' || matches!(bytes.get(i + 1), Some(b'/') | Some(b'%') | Some(b'!')) {
            i += 1;
            continue;
        }
        let after = i + 1;
        let Some(close) = find_from(source, after, ">") else { break };
        if offset < after || offset > close {
            i = close + 1;
            continue;
        }
        // The caret is in this tag. Which attribute, and does Struts evaluate it?
        let qname = qualified_name(source, after, close)?;
        let (prefix, local) = qname.split_once(':')?;
        if !prefixes.iter().any(|p| p.as_str() == prefix) {
            return None;
        }
        let attrs = OGNL_ATTRS.iter().find(|(tag, _)| *tag == local).map(|(_, a)| *a)?;
        for attr in attrs {
            let Some((raw, vstart, vend)) = attr_value(source, after, close, attr) else {
                continue;
            };
            if offset < vstart || offset > vend {
                continue;
            }
            // An expression already delimited is not this function's business — `ognl_path_at`
            // owns those, and answering here too would be two resolvers for one caret.
            if raw.contains('{') {
                return None;
            }
            return path_in_range(source, vstart, vend, offset);
        }
        return None;
    }
    None
}

/// The prefixes this page bound to a Struts tag library, or `None` when it bound none.
///
/// Read from the page's own `<%@ taglib %>` lines rather than assumed to be `s`: a legacy tree
/// has pages using `ww:`, `struts:` and `s:` in the same module, and hardcoding one would leave
/// the other two unreadable. `None` rather than an empty list, so a caller short-circuits on
/// "this page has nothing to do with Struts" before scanning anything.
fn struts_prefixes(source: &str) -> Option<Vec<String>> {
    let found: Vec<String> = crate::jsp::parse_jsp(source)
        .taglibs
        .into_iter()
        .filter(|t| {
            let uri = t.uri.to_ascii_lowercase();
            STRUTS_URIS.iter().any(|s| uri.ends_with(s))
        })
        .map(|t| t.prefix)
        .collect();
    (!found.is_empty()).then_some(found)
}

/// The `prefix:local` of the tag whose inner span starts at `start`, lowercased.
///
/// `None` for a tag with no prefix: that is HTML, and every caller here is asking a question
/// about a tag library. Lowercased so the tables below can be, and so `<S:Iterator>` reads the
/// same as `<s:iterator>` — which is what the rest of this crate's tag scans already do.
fn qualified_name(source: &str, start: usize, close: usize) -> Option<String> {
    let inner = source.get(start..close)?;
    let trimmed = inner.trim_start();
    let end = trimmed
        .find(|c: char| c.is_whitespace() || c == '/' || c == '>')
        .unwrap_or(trimmed.len());
    let full = trimmed[..end].to_ascii_lowercase();
    full.contains(':').then_some(full)
}

#[cfg(test)]
mod tests {
    use super::*;

    const PAGE: &str = concat!(
        "<%@ taglib prefix=\"s\" uri=\"/struts-tags\"%>\n",
        "<%@ taglib prefix=\"c\" uri=\"http://java.sun.com/jsp/jstl/core\"%>\n",
        "<s:iterator var=\"riga\" value=\"comunicazioni.dati\" status=\"status\">\n",
        "  <c:if test=\"${showRiferimento}\">\n",
        "    <td><s:property value=\"%{codice}\"/></td>\n",
        "  </c:if>\n",
        "</s:iterator>\n",
    );

    fn at(needle: &str) -> usize {
        PAGE.find(needle).expect("the needle is in the page") + 1
    }

    // ── bare attribute values ───────────────────────────────────────────────────

    /// The user's case: a path in a `value=` with no `%{}` around it, which every previous
    /// version of this resolver could not see at all.
    #[test]
    fn a_bare_value_attribute_is_a_path() {
        let path = ognl_attr_path_at(PAGE, at("comunicazioni")).expect("a path");
        assert_eq!(
            path.segments.iter().map(|s| s.name.as_str()).collect::<Vec<_>>(),
            ["comunicazioni", "dati"]
        );
        assert_eq!(path.at, 0, "the caret is on the first segment");
    }

    /// And the caret's own segment, not the head — the same rule the delimited path follows.
    #[test]
    fn the_caret_picks_its_own_segment() {
        let path = ognl_attr_path_at(PAGE, at("dati")).expect("a path");
        assert_eq!(path.segment().name, "dati");
        assert_eq!(path.at, 1);
    }

    /// An already-delimited expression belongs to the other resolver. Two answers for one caret
    /// is how a go-to becomes unpredictable.
    #[test]
    fn a_delimited_expression_is_left_to_the_other_scanner() {
        assert!(ognl_attr_path_at(PAGE, at("codice")).is_none());
    }

    /// `<c:if test="…">` is JSTL. Same attribute name, different language, and reading it as
    /// OGNL would follow it to a class that has nothing to do with it.
    #[test]
    fn a_tag_from_another_library_is_not_struts() {
        assert!(ognl_attr_path_at(PAGE, at("showRiferimento")).is_none());
    }

    /// `var="riga"` declares a name; it is not an expression, and nothing should follow it.
    #[test]
    fn an_attribute_that_is_not_an_expression_answers_nothing() {
        assert!(ognl_attr_path_at(PAGE, at("riga")).is_none());
    }

    /// A page that never declared a Struts library has no Struts tags, whatever its prefixes
    /// look like — the reason the prefixes are read rather than assumed.
    #[test]
    fn a_page_that_declares_no_struts_library_answers_nothing() {
        let page = "<s:iterator value=\"rows.items\">\n</s:iterator>";
        assert!(ognl_attr_path_at(page, page.find("rows").unwrap() + 1).is_none());
    }

    /// The old spelling, still in the tree.
    #[test]
    fn the_webwork_prefix_is_read_too() {
        let page = concat!(
            "<%@ taglib prefix=\"ww\" uri=\"/webwork\"%>\n",
            "<ww:iterator value=\"rows.items\"></ww:iterator>",
        );
        let path = ognl_attr_path_at(page, page.find("items").unwrap() + 1).expect("a path");
        assert_eq!(path.segment().name, "items");
    }

    /// A name inside a larger expression is still a name. The caret is on `stato`, `stato` is a
    /// real property, and go-to should land on it — the comparison around it is not the caller's
    /// question. (Typing a *variable* from such an expression is a different matter, and the one
    /// place that does it guards against exactly this.)
    #[test]
    fn a_comparison_still_resolves_the_name_the_caret_is_on() {
        let page = concat!(
            "<%@ taglib prefix=\"s\" uri=\"/struts-tags\"%>\n",
            "<s:if test=\"stato == 'APERTO'\"></s:if>",
        );
        let path = ognl_attr_path_at(page, page.find("stato").unwrap() + 1);
        assert_eq!(path.map(|p| p.segments.len()), Some(1), "the name alone, not the comparison");
    }

    // ── iterator scopes ─────────────────────────────────────────────────────────

    #[test]
    fn an_iterator_scope_covers_its_body_and_stops_at_its_close() {
        let scopes = iterator_scopes(PAGE);
        assert_eq!(scopes.len(), 1);
        let it = &scopes[0];
        assert_eq!(it.tag, "iterator");
        assert_eq!(it.source_expr, "comunicazioni.dati");
        assert_eq!(it.var.as_deref(), Some("riga"));
        assert!(scopes_at(&scopes, at("codice")).len() == 1, "inside");
        assert!(scopes_at(&scopes, PAGE.len() - 1).is_empty(), "past the close");
        assert!(scopes_at(&scopes, at("taglib")).is_empty(), "before the open");
    }

    /// Nested loops are ordinary in a table, and a name resolves against the innermost element
    /// first — so the answer has to be ordered, not just present.
    #[test]
    fn nested_scopes_come_back_outermost_first() {
        let page = concat!(
            "<%@ taglib prefix=\"s\" uri=\"/struts-tags\"%>\n",
            "<s:iterator value=\"righe\">\n",
            "  <s:iterator value=\"celle\">\n",
            "    <s:property value=\"%{testo}\"/>\n",
            "  </s:iterator>\n",
            "</s:iterator>",
        );
        let scopes = iterator_scopes(page);
        let here = scopes_at(&scopes, page.find("testo").unwrap());
        assert_eq!(here.len(), 2);
        assert_eq!(here[0].source_expr, "righe");
        assert_eq!(here[1].source_expr, "celle", "the innermost is last");
    }

    /// A self-closing iterator has no body, so it pushes nothing anybody can write inside — and
    /// a scope running to the end of the file from it would silence the check for the whole page.
    #[test]
    fn a_self_closing_iterator_opens_no_scope() {
        let page = concat!(
            "<%@ taglib prefix=\"s\" uri=\"/struts-tags\"%>\n",
            "<s:iterator value=\"righe\"/>\n",
            "<s:property value=\"%{nome}\"/>",
        );
        assert!(iterator_scopes(page).is_empty());
    }

    /// A page that never closes its loop is a page whose loop runs to the end — which is also
    /// what the browser did with it.
    #[test]
    fn an_unclosed_iterator_runs_to_the_end_of_the_page() {
        let page = concat!(
            "<%@ taglib prefix=\"s\" uri=\"/struts-tags\"%>\n",
            "<s:iterator value=\"righe\">\n",
            "  <s:property value=\"%{nome}\"/>",
        );
        let scopes = iterator_scopes(page);
        assert_eq!(scopes.len(), 1);
        assert_eq!(scopes[0].body_end, page.len());
    }

    /// A close with no open must not detach a scope that is still open above it.
    #[test]
    fn a_stray_close_is_ignored() {
        let page = concat!(
            "<%@ taglib prefix=\"s\" uri=\"/struts-tags\"%>\n",
            "</s:iterator>\n",
            "<s:iterator value=\"righe\">\n",
            "  <s:property value=\"%{nome}\"/>\n",
            "</s:iterator>",
        );
        let scopes = iterator_scopes(page);
        assert_eq!(scopes.len(), 1);
        assert!(!scopes_at(&scopes, page.find("nome").unwrap()).is_empty());
    }

    /// An iterator inside a comment is not an iterator — the mask is shared with every other
    /// scan in this crate for exactly this reason.
    #[test]
    fn a_commented_out_iterator_opens_no_scope() {
        let page = concat!(
            "<%@ taglib prefix=\"s\" uri=\"/struts-tags\"%>\n",
            "<%-- <s:iterator value=\"righe\"> --%>\n",
            "<s:property value=\"%{nome}\"/>",
        );
        assert!(iterator_scopes(page).is_empty());
    }
}
