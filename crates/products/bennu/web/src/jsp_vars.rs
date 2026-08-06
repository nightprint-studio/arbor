//! JSP **page-scoped variable** scanner — declarations (`<c:set var>`, `<s:set var>`,
//! `<c:forEach var>`, `<s:iterator var>`, …) and their EL/OGNL references (`${var}`,
//! `%{var}`, `#var`).
//!
//! This is the missing piece behind go-to-declaration / find-usages **inside** a JSP: the
//! config graph only knows Struts *actions*, so a caret on `${myVar}` (where `myVar` is a
//! `<c:set var="myVar">`) had no resolver at all. JSP variables are **page-scoped**, so this
//! is a purely single-file analysis — no project index needed, and it always answers.
//!
//! Same engineering as [`crate::jsp`] / [`crate::forms`]: a tolerant linear byte scan
//! reusing the shared masking + attribute helpers (`pub(crate)` in `jsp.rs`), so a
//! declaration/reference inside a `<%-- comment --%>` or `<% scriptlet %>` is ignored.
//!
//! References are the **root** identifiers of every EL `${…}` / `#{…}` and OGNL `%{…}`
//! expression (in text OR attribute values): the leading name of a chain (`foo` in
//! `${foo.bar}`), an index base (`foo` in `${foo[i]}`), and an OGNL context ref (`foo` in
//! `%{#foo}`). Property accesses after a `.`/`:` and string-literal contents are **not**
//! references. Over-collection is harmless — a reference that matches no declared variable
//! is only ever a no-op at resolution time.

use std::path::Path;

use crate::jsp::{attr_value, find_from, masked_regions, region_covering, tag_local_name};

/// A page-scoped variable **declaration** — a `var=` (or legacy `name=`/`id=`) attribute on
/// a JSTL/Struts var-producing tag. The span points at the variable NAME value inside the
/// quotes (the go-to-declaration target).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JspVarDecl {
    /// The declared variable name.
    pub name: String,
    /// The declaring tag, qualified (`c:set`, `s:iterator`, …) — for the target label.
    pub tag: String,
    /// Start byte offset of the name value inside the quotes.
    pub start: usize,
    /// End byte offset (exclusive).
    pub end: usize,
    /// The expression the variable takes its value FROM, as written and with its delimiters
    /// still on (`%{elencoBandi}`, `${order.lines}`). Empty when the tag names none.
    ///
    /// This is what makes a page variable *typed*. `<s:iterator value="%{elencoBandi}"
    /// var="bando">` says, in the only place the page says it, that `bando` is whatever
    /// `elencoBandi` holds — and without that, everything written on `bando` below is a name
    /// the editor can see and cannot follow.
    pub source_expr: String,
}

/// A page-scoped variable **reference** — a root identifier inside an EL/OGNL expression.
/// The span points at the identifier token (a find-usages hit / go-to source).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JspVarRef {
    /// The referenced name (no leading `#` for an OGNL context ref).
    pub name: String,
    /// Start byte offset of the identifier.
    pub start: usize,
    /// End byte offset (exclusive).
    pub end: usize,
}

/// Everything extracted from one JSP for variable navigation.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct JspVars {
    pub decls: Vec<JspVarDecl>,
    pub refs: Vec<JspVarRef>,
}

/// Scan a JSP `source` for page-scoped variable declarations + references.
pub fn parse_jsp_vars(source: &str) -> JspVars {
    let masked = masked_regions(source);
    let mut out = JspVars::default();
    collect_decls(source, &masked, &mut out.decls);
    collect_refs(source, &masked, &mut out.refs);
    out
}

/// Convenience: read `path` and [`parse_jsp_vars`] it (empty on read error).
pub fn parse_jsp_vars_file(path: &Path) -> JspVars {
    match crate::io::read_to_string_lf(path) {
        Ok(text) => parse_jsp_vars(&text),
        Err(_) => JspVars::default(),
    }
}

// ── resolution (pure, offset-based) ─────────────────────────────────────────────────

/// The variable name under `offset` (a UTF-8 byte offset) — whether the caret sits on a
/// declaration's name span or on a reference identifier. `None` when the caret isn't on a
/// JSP variable token. A declaration match wins over a reference at the same spot.
pub fn var_name_at(vars: &JspVars, offset: usize) -> Option<&str> {
    for d in &vars.decls {
        if offset >= d.start && offset <= d.end {
            return Some(&d.name);
        }
    }
    for r in &vars.refs {
        if offset >= r.start && offset <= r.end {
            return Some(&r.name);
        }
    }
    None
}

/// The declaring site of `name` — the FIRST `<… var="name">` in document order. `None` when
/// the name is referenced but never declared in this page (e.g. a request-scoped attribute).
pub fn var_declaration<'a>(vars: &'a JspVars, name: &str) -> Option<&'a JspVarDecl> {
    vars.decls.iter().find(|d| d.name == name)
}

/// Every reference to `name`, in document order (the find-usages set).
pub fn var_usages<'a>(vars: &'a JspVars, name: &str) -> Vec<&'a JspVarRef> {
    vars.refs.iter().filter(|r| r.name == name).collect()
}

/// 1-based line + column for a UTF-8 byte `offset` in `src` (go-to needs a line, the FE the
/// column). Clamped to a char boundary so a multi-byte source is safe.
pub fn line_col(src: &str, offset: usize) -> (usize, usize) {
    let mut off = offset.min(src.len());
    while off > 0 && !src.is_char_boundary(off) {
        off -= 1;
    }
    let before = &src[..off];
    let line = before.bytes().filter(|&b| b == b'\n').count() + 1;
    let line_start = before.rfind('\n').map(|i| i + 1).unwrap_or(0);
    (line, off - line_start + 1)
}

// ── declarations ────────────────────────────────────────────────────────────────────

/// Map a var-producing tag's lowercased local-name to the attribute(s) that name the
/// variable, in priority order. `None` for a tag that never declares a page variable.
fn decl_var_attrs(local: &str) -> Option<&'static [&'static str]> {
    Some(match local {
        "set" => &["var", "name"] as &[&str], // c:set (var) / s:set (var | legacy name)
        "foreach" | "fortokens" => &["var"],  // c:forEach / c:forTokens
        "iterator" => &["var", "id"],         // s:iterator (var | legacy id)
        "catch" => &["var"],                  // c:catch
        // Struts value-producing tags that stash a result under `var=`.
        "append" | "merge" | "generator" | "subset" | "sort" | "bean" | "action" | "url"
        | "date" | "number" => &["var"],
        _ => return None,
    })
}

/// Where a var-producing tag takes its value from, in priority order. One list for every tag:
/// `value` is JSTL's and Struts's spelling, `items` is `<c:forEach>`'s, and no tag uses both, so
/// a per-tag table would be three ways of saying the same thing.
const SOURCE_ATTRS: &[&str] = &["value", "items"];

/// Walk tags, emitting a [`JspVarDecl`] for each var-producing tag that carries its naming
/// attribute. Masked regions (comments/scriptlets) are skipped.
fn collect_decls(source: &str, masked: &[(usize, usize)], out: &mut Vec<JspVarDecl>) {
    let bytes = source.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() {
        if let Some(reg) = region_covering(masked, i) {
            i = reg.1;
            continue;
        }
        if bytes[i] != b'<' {
            i += 1;
            continue;
        }
        let after = i + 1;
        // Skip closers `</…`, scriptlet/directive `<%…`, comment `<!…`.
        if after >= bytes.len() || matches!(bytes[after], b'/' | b'%' | b'!') {
            i += 1;
            continue;
        }
        let Some(close) = find_from(source, after, ">") else { break };
        if let Some(local) = tag_local_name(source, after, close) {
            if let Some(attrs) = decl_var_attrs(&local) {
                if let Some((name, vstart, vend)) = first_attr(source, after, close, attrs) {
                    if !name.trim().is_empty() {
                        let source_expr = first_attr(source, after, close, SOURCE_ATTRS)
                            .map(|(v, _, _)| v)
                            .unwrap_or_default();
                        out.push(JspVarDecl {
                            name,
                            tag: tag_full_name(source, after, close).unwrap_or(local),
                            start: vstart,
                            end: vend,
                            source_expr,
                        });
                    }
                }
            }
        }
        i = close + 1;
    }
}

/// The first present attribute among `attrs` (in priority order) within a tag's inner span.
fn first_attr(
    source: &str,
    start: usize,
    close: usize,
    attrs: &[&str],
) -> Option<(String, usize, usize)> {
    attrs.iter().find_map(|a| attr_value(source, start, close, a))
}

/// The qualified tag name (`prefix:local`, original case) for a tag whose content spans
/// `source[start..close]` — used for the declaration label. `None` if empty.
fn tag_full_name(source: &str, start: usize, close: usize) -> Option<String> {
    let inner = source.get(start..close)?;
    let trimmed = inner.trim_start();
    let end = trimmed
        .find(|c: char| c.is_whitespace() || c == '/' || c == '>')
        .unwrap_or(trimmed.len());
    let name = &trimmed[..end];
    (!name.is_empty()).then(|| name.to_string())
}

// ── references (EL / OGNL root identifiers) ───────────────────────────────────────────

/// Walk `source`, entering each EL `${…}` / `#{…}` / OGNL `%{…}` region and collecting its
/// root identifiers. Masked regions (comments/scriptlets) are skipped.
fn collect_refs(source: &str, masked: &[(usize, usize)], out: &mut Vec<JspVarRef>) {
    let bytes = source.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() {
        if let Some(reg) = region_covering(masked, i) {
            i = reg.1;
            continue;
        }
        // An expression opener: `${`, `#{`, `%{`.
        if i + 1 < bytes.len() && matches!(bytes[i], b'$' | b'#' | b'%') && bytes[i + 1] == b'{' {
            i = scan_expr(source, i + 2, out);
            continue;
        }
        i += 1;
    }
}

/// Scan one EL/OGNL expression body starting at `start` (just after the opening `{`),
/// collecting root identifiers until the matching `}` (or EOF). Returns the index just past
/// the `}`. String literals are skipped whole; an identifier is a **root** reference unless
/// the previous significant char was a `.` or `:` (a property / namespace access).
fn scan_expr(source: &str, start: usize, out: &mut Vec<JspVarRef>) -> usize {
    let bytes = source.as_bytes();
    let mut i = start;
    let mut prev_access = false; // previous significant char was `.` or `:`
    while i < bytes.len() {
        let c = bytes[i];
        if c == b'}' {
            return i + 1;
        }
        if c == b'\'' || c == b'"' {
            i = skip_string(bytes, i);
            prev_access = false;
            continue;
        }
        if c.is_ascii_whitespace() {
            i += 1; // keep `prev_access` across whitespace (`foo . bar`)
            continue;
        }
        if is_ident_start(c) {
            let id_start = i;
            i += 1;
            while i < bytes.len() && is_ident_char(bytes[i]) {
                i += 1;
            }
            if !prev_access {
                let name = &source[id_start..i];
                if !is_el_keyword(name) {
                    out.push(JspVarRef { name: name.to_string(), start: id_start, end: i });
                }
            }
            prev_access = false;
            continue;
        }
        prev_access = c == b'.' || c == b':';
        i += 1;
    }
    i
}

/// Skip a quoted string starting at `open` (`bytes[open]` is the quote), honouring `\`
/// escapes. Returns the index just past the closing quote (or EOF).
// ── dotted paths (property chains) ─────────────────────────────────────────────────

/// A dotted OGNL / EL **path** as written: `ordine.cliente.nome`, `items[0].nome`.
///
/// The counterpart to [`JspVarRef`], which is deliberately only the *root*: a page variable's
/// find-usages must count `x` in `%{x.name}` once and must not count `name` at all. But a
/// go-to on `name` is a real question about a real declaration, and answering it needs the
/// segments the root does not carry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OgnlPath {
    /// Every segment in order, each with its own span. Never empty.
    pub segments: Vec<JspVarRef>,
    /// Index into [`Self::segments`] of the one the caret is on.
    pub at: usize,
}

impl OgnlPath {
    /// The segment the caret is on.
    pub fn segment(&self) -> &JspVarRef {
        &self.segments[self.at]
    }

    /// The root of the path — what a page-variable check has to look at, whichever segment the
    /// caret happens to be on.
    pub fn root(&self) -> &JspVarRef {
        &self.segments[0]
    }
}

/// The dotted path under `offset`, if the caret is inside an EL/OGNL expression at all.
///
/// Only inside `${…}` / `#{…}` / `%{…}`, and never inside a string literal within one — the same
/// two rules the reference scan follows, for the same reason: text that merely looks like a path
/// is not one.
pub fn ognl_path_at(source: &str, offset: usize) -> Option<OgnlPath> {
    let masked = masked_regions(source);
    let bytes = source.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() {
        if let Some(reg) = region_covering(&masked, i) {
            i = reg.1;
            continue;
        }
        if i + 1 < bytes.len() && matches!(bytes[i], b'$' | b'#' | b'%') && bytes[i + 1] == b'{' {
            let (end, found) = scan_expr_paths(source, i + 2, offset);
            if found.is_some() {
                return found;
            }
            i = end;
            continue;
        }
        i += 1;
    }
    None
}

/// Walk one expression body collecting `ident (. ident)*` chains, and return the one covering
/// `offset`. Mirrors [`scan_expr`]'s rules — strings skipped whole, whitespace kept transparent
/// so `foo . bar` is one chain — and adds two: an index (`items[0]`) does not break a chain, and
/// a property access with no root before it (`.foo` after a call) starts none.
fn scan_expr_paths(source: &str, start: usize, offset: usize) -> (usize, Option<OgnlPath>) {
    let bytes = source.as_bytes();
    let mut i = start;
    let mut chain: Vec<JspVarRef> = Vec::new();
    let mut prev_access = false;
    let mut found: Option<OgnlPath> = None;

    while i < bytes.len() {
        let c = bytes[i];
        if c == b'}' {
            take_chain(&mut chain, offset, &mut found);
            return (i + 1, found);
        }
        if c == b'\'' || c == b'"' {
            take_chain(&mut chain, offset, &mut found);
            i = skip_string(bytes, i);
            prev_access = false;
            continue;
        }
        if c.is_ascii_whitespace() {
            i += 1; // keeps `prev_access`, so `foo . bar` stays one chain
            continue;
        }
        if is_ident_start(c) {
            let id_start = i;
            i += 1;
            while i < bytes.len() && is_ident_char(bytes[i]) {
                i += 1;
            }
            let name = source[id_start..i].to_string();
            if prev_access {
                // A property access with nothing in front of it is not the start of a path.
                if !chain.is_empty() {
                    chain.push(JspVarRef { name, start: id_start, end: i });
                }
            } else {
                take_chain(&mut chain, offset, &mut found);
                if !is_el_keyword(&name) {
                    chain.push(JspVarRef { name, start: id_start, end: i });
                }
            }
            prev_access = false;
            continue;
        }
        if c == b'[' {
            // `items[0].nome` is one path: the index says which element, not which property.
            i = skip_index(bytes, i);
            continue;
        }
        if c == b'.' || c == b':' {
            // The one character that must NOT end the chain — it is what joins it.
            prev_access = true;
            i += 1;
            continue;
        }
        take_chain(&mut chain, offset, &mut found);
        prev_access = false;
        i += 1;
    }
    take_chain(&mut chain, offset, &mut found);
    (i, found)
}

/// End the chain being built: keep it as the answer when it covers `offset`, drop it otherwise.
/// The first covering chain wins — they cannot overlap.
fn take_chain(chain: &mut Vec<JspVarRef>, offset: usize, found: &mut Option<OgnlPath>) {
    if found.is_none() {
        if let Some(at) = chain.iter().position(|s| offset >= s.start && offset <= s.end) {
            *found = Some(OgnlPath { segments: std::mem::take(chain), at });
            return;
        }
    }
    chain.clear();
}

/// Past a `[ … ]` index, strings inside it skipped whole.
fn skip_index(bytes: &[u8], open: usize) -> usize {
    let mut i = open + 1;
    while i < bytes.len() {
        match bytes[i] {
            b']' => return i + 1,
            b'\'' | b'"' => i = skip_string(bytes, i),
            b'}' => return i, // an unterminated index: let the caller see the expression end
            _ => i += 1,
        }
    }
    i
}

fn skip_string(bytes: &[u8], open: usize) -> usize {
    let quote = bytes[open];
    let mut i = open + 1;
    while i < bytes.len() {
        if bytes[i] == b'\\' {
            i += 2;
            continue;
        }
        if bytes[i] == quote {
            return i + 1;
        }
        i += 1;
    }
    i
}

fn is_ident_start(c: u8) -> bool {
    c.is_ascii_alphabetic() || c == b'_' || c == b'$'
}

fn is_ident_char(c: u8) -> bool {
    c.is_ascii_alphanumeric() || c == b'_' || c == b'$'
}

/// EL / OGNL reserved words that are never variable references.
fn is_el_keyword(word: &str) -> bool {
    matches!(
        word,
        "and" | "or" | "not" | "eq" | "ne" | "neq" | "lt" | "gt" | "le" | "ge" | "lte" | "gte"
            | "div" | "mod" | "instanceof" | "in" | "new" | "empty" | "true" | "false" | "null"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The path of the segment names under the caret, for the assertions below.
    fn path(src: &str, needle: &str) -> Option<(Vec<String>, usize)> {
        let at = src.find(needle).expect("needle") + 1;
        ognl_path_at(src, at).map(|p| (p.segments.iter().map(|s| s.name.clone()).collect(), p.at))
    }

    #[test]
    fn a_var_declaration_carries_the_expression_it_came_from() {
        let src = r#"<s:iterator value="%{elencoBandi}" var="bando">
              <c:set var="oggi" value="${today}"/>
              <c:forEach items="${order.lines}" var="line"/>
            </s:iterator>"#;
        let decls = parse_jsp_vars(src).decls;
        let of = |name: &str| {
            decls.iter().find(|d| d.name == name).map(|d| d.source_expr.clone()).expect(name)
        };
        assert_eq!(of("bando"), "%{elencoBandi}");
        assert_eq!(of("oggi"), "${today}");
        // `<c:forEach>` says `items`, everyone else says `value`.
        assert_eq!(of("line"), "${order.lines}");
    }

    #[test]
    fn a_dotted_path_is_read_whole_from_any_of_its_segments() {
        // The regression: the reference scan keeps only the ROOT, so a caret on `oraFine`
        // matched nothing and nested go-to had nowhere to start.
        let src = "<s:property value=\"%{jsp_param.element.datiGenerali.oraFine}\"/>";
        let all = vec![
            "jsp_param".to_string(),
            "element".to_string(),
            "datiGenerali".to_string(),
            "oraFine".to_string(),
        ];
        assert_eq!(path(src, "jsp_param"), Some((all.clone(), 0)));
        assert_eq!(path(src, "element"), Some((all.clone(), 1)));
        assert_eq!(path(src, "oraFine"), Some((all, 3)));
    }

    #[test]
    fn an_index_does_not_break_the_path() {
        let src = "${items[0].nome}";
        assert_eq!(path(src, "nome"), Some((vec!["items".into(), "nome".into()], 1)));
    }

    #[test]
    fn only_inside_an_expression_and_never_inside_a_string() {
        // Plain text that looks like a path is not one.
        assert!(ognl_path_at("<p>ordine.cliente</p>", 6).is_none());
        // A string literal inside an expression is not one either.
        let src = "%{foo('ordine.cliente')}";
        assert!(path(src, "ordine.cliente").is_none());
    }

    #[test]
    fn a_property_access_with_nothing_in_front_starts_no_path() {
        // `.trim()` after a call: there is no root to descend from, so nothing is offered.
        let src = "%{'x'.trim()}";
        assert!(path(src, "trim").is_none());
    }

    #[test]
    fn whitespace_around_the_dot_keeps_one_path() {
        let src = "%{ordine . cliente}";
        assert_eq!(path(src, "cliente"), Some((vec!["ordine".into(), "cliente".into()], 1)));
    }

    #[test]
    fn c_set_declaration_and_el_reference_resolve() {
        let src = r#"<c:set var="greeting" value="hi"/>
            <p>${greeting}</p>"#;
        let vars = parse_jsp_vars(src);
        assert_eq!(vars.decls.len(), 1);
        assert_eq!(vars.decls[0].name, "greeting");
        assert_eq!(vars.decls[0].tag, "c:set");
        assert_eq!(&src[vars.decls[0].start..vars.decls[0].end], "greeting");

        // one reference, pointing at the `${greeting}` identifier
        let refs = var_usages(&vars, "greeting");
        assert_eq!(refs.len(), 1);
        assert_eq!(&src[refs[0].start..refs[0].end], "greeting");

        // go-to from the reference → the declaration
        let off = refs[0].start + 1;
        assert_eq!(var_name_at(&vars, off), Some("greeting"));
        assert_eq!(var_declaration(&vars, "greeting").unwrap().name, "greeting");

        // caret ON the declaration name → still resolves the name (FE flips to usages)
        let decl_off = vars.decls[0].start + 1;
        assert_eq!(var_name_at(&vars, decl_off), Some("greeting"));
    }

    #[test]
    fn s_set_and_ognl_and_context_ref() {
        let src = r#"<s:set var="total" value="%{count}"/>
            <s:property value="%{total}"/>
            <span>${total}</span>
            <s:if test="%{#total > 0}">x</s:if>"#;
        let vars = parse_jsp_vars(src);
        assert_eq!(vars.decls.len(), 1);
        assert_eq!(vars.decls[0].name, "total");
        assert_eq!(vars.decls[0].tag, "s:set");

        // `count` (inside the value=), `total` ×3 (%{total}, ${total}, %{#total}) are refs.
        assert!(vars.refs.iter().any(|r| r.name == "count"));
        let totals = var_usages(&vars, "total");
        assert_eq!(totals.len(), 3, "refs = {:?}", vars.refs);
        // the OGNL context ref `#total` strips the `#` → name is `total`
        assert!(totals.iter().any(|r| &src[r.start..r.end] == "total"));
    }

    #[test]
    fn foreach_var_and_property_access_is_not_a_ref() {
        let src = r#"<c:forEach var="item" items="${list}">
              ${item.name} - ${item.price}
            </c:forEach>"#;
        let vars = parse_jsp_vars(src);
        // decl: item ; refs: list (items=), item, item — `name`/`price` are property accesses.
        assert_eq!(vars.decls.len(), 1);
        assert_eq!(vars.decls[0].name, "item");
        assert!(vars.refs.iter().any(|r| r.name == "list"));
        assert_eq!(var_usages(&vars, "item").len(), 2);
        assert!(!vars.refs.iter().any(|r| r.name == "name"), "property leaked: {:?}", vars.refs);
        assert!(!vars.refs.iter().any(|r| r.name == "price"), "property leaked: {:?}", vars.refs);
    }

    #[test]
    fn keywords_and_strings_and_indices() {
        let src = r#"<p>${a and b}</p>
            <p>${map['key']}</p>
            <p>${arr[idx]}</p>"#;
        let vars = parse_jsp_vars(src);
        // `and` is a keyword (not a ref); `'key'` is a string (not a ref).
        assert!(!vars.refs.iter().any(|r| r.name == "and"), "kw leaked: {:?}", vars.refs);
        assert!(!vars.refs.iter().any(|r| r.name == "key"), "string leaked: {:?}", vars.refs);
        assert!(vars.refs.iter().any(|r| r.name == "a"));
        assert!(vars.refs.iter().any(|r| r.name == "b"));
        assert!(vars.refs.iter().any(|r| r.name == "map"));
        // `arr` and `idx` are BOTH roots (idx is not after a dot).
        assert!(vars.refs.iter().any(|r| r.name == "arr"));
        assert!(vars.refs.iter().any(|r| r.name == "idx"));
    }

    #[test]
    fn declaration_and_reference_in_comment_or_scriptlet_are_ignored() {
        let src = r#"<%-- <c:set var="ghost"/> ${ghostRef} --%>
            <% String s = "${scriptletRef}"; %>
            <c:set var="real"/>
            ${real}"#;
        let vars = parse_jsp_vars(src);
        assert_eq!(vars.decls.len(), 1);
        assert_eq!(vars.decls[0].name, "real");
        assert!(!vars.refs.iter().any(|r| r.name == "ghostRef"), "comment ref leaked");
        assert!(!vars.refs.iter().any(|r| r.name == "scriptletRef"), "scriptlet ref leaked");
        assert_eq!(var_usages(&vars, "real").len(), 1);
    }

    #[test]
    fn el_in_attribute_values_counts() {
        let src = r#"<c:set var="flag" value="true"/>
            <div class="${flag ? 'on' : 'off'}">x</div>
            <c:if test="${flag}">y</c:if>"#;
        let vars = parse_jsp_vars(src);
        // `flag` referenced in the class= EL and the test= EL (the value="true" is a literal).
        assert_eq!(var_usages(&vars, "flag").len(), 2, "refs = {:?}", vars.refs);
    }

    #[test]
    fn var_name_at_prefers_declaration_over_reference() {
        // Same name declared then referenced; a caret in each span resolves the name.
        let src = r#"<c:set var="x" value="${x}"/>"#;
        let vars = parse_jsp_vars(src);
        let d = &vars.decls[0];
        assert_eq!(var_name_at(&vars, d.start), Some("x"));
        let r = vars.refs.iter().find(|r| r.name == "x").unwrap();
        assert_eq!(var_name_at(&vars, r.start), Some("x"));
        // an offset off any token → None
        assert_eq!(var_name_at(&vars, 0), None);
    }

    #[test]
    fn line_col_is_one_based() {
        let src = "a\n  ${foo}\n";
        let idx = src.find("foo").unwrap();
        let (line, col) = line_col(src, idx);
        assert_eq!((line, col), (2, 5)); // `foo` starts at column 5 on line 2
    }

    #[test]
    fn empty_and_unreadable_are_graceful() {
        assert_eq!(parse_jsp_vars(""), JspVars::default());
        assert_eq!(parse_jsp_vars_file(Path::new("/no/such/file.jsp")), JspVars::default());
    }
}
