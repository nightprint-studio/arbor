//! JSP source scanner — Struts action references + taglib directives.
//!
//! Unlike the other parsers in this crate, a JSP is **not valid XML**: it carries
//! scriptlets (`<% … %>`), EL/OGNL expressions (`${…}` / `%{…}`), unclosed HTML and
//! custom taglib tags. So we do a lightweight *linear text scan* (no roxmltree, no
//! regex) rather than a DOM parse — mirroring the byte-oriented scan style of
//! `spring::bean_class_value_spans` / `scan_setter_properties`, and matching the
//! skip-and-continue tolerance of the sibling parsers.
//!
//! We extract two things, each carrying the **byte-offset span of its value** (inside the
//! quotes) so the diagnostic layer can draw a precise squiggle:
//!
//!   - **Action references** ([`JspActionRef`]) — the `action="…"` attribute on the Struts
//!     taglib tags (`<s:form>`, `<s:url>`, `<s:a>`, `<s:submit>`), the legacy Struts1
//!     `<html:form action="…">`, and plain HTML `<form action="X.action">` /
//!     `<a href="…/X.action">` whose URL ends in `.action` or `.do`. The `name` is the
//!     bare action key to look up (trailing `.action`/`.do` stripped, a leading
//!     `/namespace/` kept); `raw` is the exact attribute value.
//!   - **Taglib directives** ([`JspTaglib`]) — `<%@ taglib prefix="s" uri="/struts-tags" %>`.
//!
//! An action whose value is a runtime expression (`${…}`, `%{…}`, `<%= … %>`) is a
//! **computed** ref: it is emitted with `computed = true` so the action-existence
//! diagnostic never flags it as missing (the same *inconclusive → don't flag* rule the
//! wildcard/backref refs follow — docs §7, §8).

use std::path::Path;

/// A Struts action reference found in a JSP (`action="…"` on a taglib/HTML tag, or an
/// `X.action`/`X.do` URL). String-keyed like the other records; the byte span points at
/// the attribute **value** (inside the quotes) for precise diagnostics.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JspActionRef {
    /// The bare action key to look up: trailing `.action`/`.do` stripped, a leading
    /// `/namespace/` kept. For a computed ref this is the raw expression (never resolved).
    pub name: String,
    /// The exact attribute value as written (before normalization).
    pub raw: String,
    /// Start byte offset of the value inside the quotes.
    pub start: usize,
    /// End byte offset (exclusive).
    pub end: usize,
    /// The value is a runtime expression (`${…}`, `%{…}`, `<%= … %>`) — resolved at
    /// request time, so the diagnostic layer must treat it as *inconclusive*, never
    /// "action does not exist".
    pub computed: bool,
}

/// A JSP `<%@ taglib prefix="…" uri="…" %>` directive. The span points at the **prefix**
/// value (the token diagnostics/rename care about).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JspTaglib {
    /// The tag prefix declared (`s`, `html`, `c`, …).
    pub prefix: String,
    /// The taglib URI it binds to (`/struts-tags`, …).
    pub uri: String,
    /// Start byte offset of the prefix value inside the quotes.
    pub start: usize,
    /// End byte offset (exclusive).
    pub end: usize,
}

/// Everything extracted from one JSP source.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct JspParse {
    pub action_refs: Vec<JspActionRef>,
    pub taglibs: Vec<JspTaglib>,
}

/// The Struts/HTML tag local-names (after any `prefix:`) that carry an `action="…"` we
/// treat as an action reference. Matched case-insensitively.
const ACTION_TAGS: &[&str] = &["form", "url", "a", "submit"];

/// Scan a JSP `source` string for action references + taglib directives.
///
/// Robust by construction: JSP comments (`<%-- … --%>`) and scriptlets (`<% … %>`,
/// including `<%= … %>` and `<%! … %>`) are masked out first, so an `action=` inside them
/// is ignored. Malformed / unclosed tags are skipped, never fatal.
pub fn parse_jsp(source: &str) -> JspParse {
    let bytes = source.as_bytes();
    // Regions [start, end) that must be ignored (comments + scriptlets). Kept sorted by
    // construction (we scan left-to-right).
    let masked = masked_regions(source);

    let mut out = JspParse::default();

    // Taglib directives: `<%@ taglib prefix="…" uri="…" %>`. These live *inside* a `<%@ …%>`
    // directive block, which the scriptlet mask does NOT cover (only `<% … %>` without `@`),
    // so we scan the raw source for them.
    scan_taglibs(source, &mut out.taglibs);

    // Action references: walk tags, skipping masked regions.
    let mut i = 0usize;
    while i < bytes.len() {
        if let Some(reg) = region_covering(&masked, i) {
            i = reg.1; // jump past the masked region
            continue;
        }
        if bytes[i] == b'<' {
            if let Some((tag_end, refs)) = scan_tag(source, i) {
                out.action_refs.extend(refs);
                i = tag_end;
                continue;
            }
        }
        i += 1;
    }

    out
}

/// Convenience: read `path` and [`parse_jsp`] it. A read error yields an empty parse
/// (skip-and-continue — one unreadable JSP never aborts a project-wide scan).
pub fn parse_jsp_file(path: &Path) -> JspParse {
    match std::fs::read_to_string(path) {
        Ok(text) => parse_jsp(&text),
        Err(_) => JspParse::default(),
    }
}

/// Byte ranges `[start, end)` of JSP comments and scriptlets that must be ignored when
/// hunting for `action=`. Directives (`<%@ …%>`) are deliberately NOT masked (we scan
/// them for taglibs). Order is ascending and non-overlapping.
///
/// `pub(crate)` so the sibling form scanner ([`crate::forms`]) masks the same regions
/// instead of duplicating the comment/scriptlet skip logic.
pub(crate) fn masked_regions(source: &str) -> Vec<(usize, usize)> {
    let bytes = source.as_bytes();
    let mut regions = Vec::new();
    let mut i = 0usize;
    while i + 1 < bytes.len() {
        if bytes[i] == b'<' && bytes[i + 1] == b'%' {
            // Distinguish `<%--` comment, `<%@` directive (not masked), `<%= / <%! / <%`.
            if starts_with_at(bytes, i, b"<%--") {
                let end = find_from(source, i + 4, "--%>").map(|e| e + 4).unwrap_or(bytes.len());
                regions.push((i, end));
                i = end;
                continue;
            }
            if starts_with_at(bytes, i, b"<%@") {
                // Directive — leave visible for taglib scan. Skip its extent so we don't
                // re-enter it as a scriptlet on the next byte.
                let end = find_from(source, i + 3, "%>").map(|e| e + 2).unwrap_or(bytes.len());
                i = end;
                continue;
            }
            // Scriptlet / expression / declaration: `<% … %>`, `<%= … %>`, `<%! … %>`.
            let end = find_from(source, i + 2, "%>").map(|e| e + 2).unwrap_or(bytes.len());
            regions.push((i, end));
            i = end;
            continue;
        }
        i += 1;
    }
    regions
}

/// If byte offset `i` falls inside a masked region, return that region.
pub(crate) fn region_covering(regions: &[(usize, usize)], i: usize) -> Option<(usize, usize)> {
    regions.iter().copied().find(|&(s, e)| i >= s && i < e)
}

/// Scan a single `<…>` tag starting at `open` (`source[open] == '<'`). Returns the byte
/// offset just past the tag's `>` and any action refs found on it, or `None` if this isn't
/// a tag we care about (or it's unterminated). Skips `</…>` closers and `<%…` blocks.
fn scan_tag(source: &str, open: usize) -> Option<(usize, Vec<JspActionRef>)> {
    let bytes = source.as_bytes();
    let after = open + 1;
    if after >= bytes.len() {
        return None;
    }
    // Not a start tag we handle: closer `</`, directive/scriptlet `<%`, comment `<!--`.
    if matches!(bytes[after], b'/' | b'%' | b'!') {
        return None;
    }
    let close = find_from(source, after, ">")?;
    // The tag's local name (strip an optional `prefix:`), lowercased for matching.
    let name = tag_local_name(source, after, close)?;
    if !ACTION_TAGS.contains(&name.as_str()) {
        return Some((close + 1, Vec::new()));
    }

    let mut refs = Vec::new();
    // A `<form>`/`<a>` may carry `action=` (Struts / plain HTML) OR `href=` (plain HTML
    // anchor). We collect the relevant attribute, then normalize.
    if let Some((raw, vstart, vend)) = attr_value(source, after, close, "action") {
        push_action_ref(&mut refs, raw, vstart, vend, false);
    }
    if let Some((raw, vstart, vend)) = attr_value(source, after, close, "href") {
        // Only a plain-HTML anchor href that points at a `.action`/`.do` URL is an action
        // reference; ordinary links are ignored.
        push_action_ref(&mut refs, raw, vstart, vend, true);
    }

    Some((close + 1, refs))
}

/// The lowercased local name of a tag whose content spans `source[start..close]`
/// (`start` just after `<`, `close` at the `>`). Strips a `prefix:` and any leading
/// whitespace. `None` if empty.
///
/// `pub(crate)` so [`crate::forms`] classifies form/field tags with the same
/// prefix-stripping + lowercasing rule.
pub(crate) fn tag_local_name(source: &str, start: usize, close: usize) -> Option<String> {
    let inner = &source[start..close];
    let trimmed = inner.trim_start();
    // Name runs up to the first whitespace or `/` (self-closing) or `>`.
    let end = trimmed
        .find(|c: char| c.is_whitespace() || c == '/' || c == '>')
        .unwrap_or(trimmed.len());
    let full = &trimmed[..end];
    let local = full.rsplit(':').next().unwrap_or(full);
    if local.is_empty() {
        None
    } else {
        Some(local.to_ascii_lowercase())
    }
}

/// Find attribute `attr` (case-insensitive) within a tag's inner span `[start, close)` and
/// return `(raw_value, value_start_offset, value_end_offset)` — byte offsets into the whole
/// `source`, pointing inside the quotes. `None` if the attribute is absent or unquoted.
///
/// `pub(crate)` so [`crate::forms`] reads `action=`/`name=`/`property=`/`type=` attributes
/// with the same boundary-guarded, quote-aware scan (no copy-paste).
pub(crate) fn attr_value(
    source: &str,
    start: usize,
    close: usize,
    attr: &str,
) -> Option<(String, usize, usize)> {
    let bytes = source.as_bytes();
    let inner = source.get(start..close)?;
    let attr_lower = attr.to_ascii_lowercase();
    let mut search = 0usize;
    while let Some(rel) = find_ident(inner, &attr_lower, search) {
        let name_abs = start + rel;
        // Guard against matching a substring of a longer attribute (e.g. `data-action`):
        // the char before must be a tag/attr boundary (whitespace, `<`, or the tag start).
        let ok_before = name_abs == start || {
            let pb = bytes[name_abs - 1];
            pb.is_ascii_whitespace() || pb == b'<'
        };
        // After the name, allow whitespace, then require `=`.
        let mut j = name_abs + attr_lower.len();
        while j < close && bytes[j].is_ascii_whitespace() {
            j += 1;
        }
        if ok_before && j < close && bytes[j] == b'=' {
            j += 1;
            while j < close && bytes[j].is_ascii_whitespace() {
                j += 1;
            }
            if j < close && (bytes[j] == b'"' || bytes[j] == b'\'') {
                let quote = bytes[j];
                let vstart = j + 1;
                let mut k = vstart;
                while k < close && bytes[k] != quote {
                    k += 1;
                }
                if k <= close {
                    let raw = source.get(vstart..k)?.to_string();
                    return Some((raw, vstart, k));
                }
            }
        }
        search = rel + attr_lower.len();
    }
    None
}

/// Normalize a raw attribute value into a [`JspActionRef`] and push it. `href_mode` = the
/// value came from an `href=` (plain-HTML anchor) — only kept if it points at a
/// `.action`/`.do` URL. `action=` values are always kept.
fn push_action_ref(out: &mut Vec<JspActionRef>, raw: String, start: usize, end: usize, href_mode: bool) {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return;
    }
    let computed = is_computed(trimmed);
    if computed {
        // A computed value is inconclusive — keep it verbatim, flagged, never normalized.
        out.push(JspActionRef { name: raw.clone(), raw, start, end, computed: true });
        return;
    }
    let name = match normalize_action(trimmed, href_mode) {
        Some(n) => n,
        None => return, // href not pointing at an action/do URL → not an action reference
    };
    out.push(JspActionRef { name, raw, start, end, computed: false });
}

/// Normalize a **raw** `action="…"` attribute value — exactly as the editor extracts it
/// verbatim from the quotes for a go-to / find-usages request — into the lookup key the
/// config graph is keyed by. This is the SAME normalization [`parse_jsp`] applies to the
/// refs it stores (an `action=`, not `href=`, so the whole path is kept, not tail-reduced),
/// so a caret token round-trips to a stored ref:
///
///   - a computed value (`${…}` / `%{…}` / `<%= … %>`) → `None` (no static key);
///   - a trailing `.action` / `.do` and any `?query` / `#frag` are stripped;
///   - an absolute `/ns/name` path is kept as-is (it already IS the qualified-name key);
///   - a bare `name` is kept as-is (the namespace is unknown from a JSP — the caller does an
///     unambiguous-suffix match against the known actions).
///
/// Without this the FE, which sends the attribute value verbatim, never matches: a
/// `action="/do/Cat/edit.action"` reference would look up `/do/Cat/edit.action` while the
/// graph is keyed `/do/Cat/edit`.
pub fn normalize_action_ref(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() || is_computed(trimmed) {
        return None;
    }
    normalize_action(trimmed, false)
}

/// Is this attribute value a runtime expression (EL / OGNL / JSP expression)?
fn is_computed(value: &str) -> bool {
    value.contains("${") || value.contains("%{") || value.contains("<%=") || value.contains("<%")
}

/// Turn a literal (non-computed) action URL/name into its lookup key.
///
///   - `href_mode == false` (`action=` on a taglib/form): keep as-is, only strip a trailing
///     `.action`/`.do` and drop a query string. A Struts `action="viewTree"` → `viewTree`;
///     `action="/do/Cat/edit"` → `/do/Cat/edit`.
///   - `href_mode == true` (plain-HTML `href=`): only an URL ending (before any `?`) in
///     `.action`/`.do` is an action reference; strip the scheme/host but keep the last path
///     segment (with a leading `/namespace/` if the path has one). Otherwise `None`.
fn normalize_action(value: &str, href_mode: bool) -> Option<String> {
    // Drop a query string / fragment.
    let no_query = value.split(['?', '#']).next().unwrap_or(value);
    let stripped = strip_action_suffix(no_query);

    if href_mode {
        // Only a `.action`/`.do` URL is an action ref.
        if !ends_with_action_suffix(no_query) {
            return None;
        }
        // Keep the meaningful path: last segment, or a `/ns/seg` tail if present.
        Some(action_path_tail(stripped))
    } else {
        Some(stripped.to_string())
    }
}

/// Does the (query-stripped) URL end in `.action` or `.do`?
fn ends_with_action_suffix(url: &str) -> bool {
    url.ends_with(".action") || url.ends_with(".do")
}

/// Strip a trailing `.action`/`.do` extension (if any).
fn strip_action_suffix(url: &str) -> &str {
    url.strip_suffix(".action").or_else(|| url.strip_suffix(".do")).unwrap_or(url)
}

/// For a plain-HTML href path, keep the action segment: if it carries a namespace path
/// (`/do/Cat/edit`) keep from the first `/` we consider meaningful; otherwise just the last
/// path segment. We conservatively keep a leading-slash absolute path as-is (it already is
/// the `namespace/name` key), and reduce a full URL to its last segment.
fn action_path_tail(path: &str) -> String {
    // Absolute path already in `namespace/name` shape → keep it.
    if path.starts_with('/') {
        return path.to_string();
    }
    // Full URL or relative multi-segment → last segment is the action name.
    match path.rsplit('/').next() {
        Some(seg) if !seg.is_empty() => seg.to_string(),
        _ => path.to_string(),
    }
}

/// Scan `source` for `<%@ taglib prefix="…" uri="…" %>` directives, pushing a [`JspTaglib`]
/// (span on the *prefix* value) for each. Directive order in the file is preserved.
fn scan_taglibs(source: &str, out: &mut Vec<JspTaglib>) {
    let bytes = source.as_bytes();
    let mut i = 0usize;
    while let Some(open) = find_from(source, i, "<%@") {
        let end = find_from(source, open + 3, "%>").map(|e| e).unwrap_or(bytes.len());
        let inner_start = open + 3;
        let inner = &source[inner_start..end.min(source.len())];
        // Only `taglib` directives (skip `page`, `include`).
        if inner.trim_start().to_ascii_lowercase().starts_with("taglib") {
            let prefix = attr_value(source, inner_start, end, "prefix");
            let uri = attr_value(source, inner_start, end, "uri");
            if let (Some((p_raw, p_start, p_end)), Some((u_raw, _, _))) = (prefix, uri) {
                out.push(JspTaglib {
                    prefix: p_raw,
                    uri: u_raw,
                    start: p_start,
                    end: p_end,
                });
            }
        }
        i = (end + 2).min(bytes.len());
    }
}

// ---- small byte-scan helpers (no regex dependency) --------------------------------------

/// Do `bytes[at..]` start with `needle`?
fn starts_with_at(bytes: &[u8], at: usize, needle: &[u8]) -> bool {
    bytes.len() >= at + needle.len() && &bytes[at..at + needle.len()] == needle
}

/// Byte offset of the next occurrence of `needle` in `source` at or after `from`.
pub(crate) fn find_from(source: &str, from: usize, needle: &str) -> Option<usize> {
    if from > source.len() {
        return None;
    }
    source[from..].find(needle).map(|rel| from + rel)
}

/// Case-insensitive search for `ident` (already lowercase) in `haystack` at or after
/// `from`, returning the byte offset of the match. `haystack` is ASCII-lowercased on the
/// fly only for the comparison window (JSP attribute names are ASCII).
fn find_ident(haystack: &str, ident: &str, from: usize) -> Option<usize> {
    let hb = haystack.as_bytes();
    let ib = ident.as_bytes();
    if ib.is_empty() || from >= hb.len() {
        return None;
    }
    let mut i = from;
    while i + ib.len() <= hb.len() {
        if hb[i..i + ib.len()].eq_ignore_ascii_case(ib) {
            return Some(i);
        }
        i += 1;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = r#"<%@ taglib prefix="s" uri="/struts-tags" %>
<html>
  <s:form action="viewTree">
    <s:url action="/do/Cat/edit"/>
    <form action="save.action">
    <s:a action="${someExpr}">go</s:a>
    <%-- <s:form action="ignoredInComment"> --%>
    <% String x = "also action=ignoredInScriptlet"; %>
  </s:form>
</html>"#;

    #[test]
    fn extracts_taglib_directive_with_prefix_offset() {
        let parse = parse_jsp(FIXTURE);
        assert_eq!(parse.taglibs.len(), 1);
        let t = &parse.taglibs[0];
        assert_eq!(t.prefix, "s");
        assert_eq!(t.uri, "/struts-tags");
        // Offset points at the prefix VALUE (inside the quotes).
        assert_eq!(&FIXTURE[t.start..t.end], "s");
    }

    #[test]
    fn extracts_struts_taglib_and_html_action_refs() {
        let parse = parse_jsp(FIXTURE);
        let names: Vec<&str> = parse.action_refs.iter().map(|r| r.name.as_str()).collect();

        // <s:form action="viewTree">  → viewTree
        assert!(names.contains(&"viewTree"), "names = {names:?}");
        // <s:url action="/do/Cat/edit"/> → kept as-is (namespace path)
        assert!(names.contains(&"/do/Cat/edit"), "names = {names:?}");
        // plain <form action="save.action"> → trailing .action stripped
        assert!(names.contains(&"save"), "names = {names:?}");
    }

    #[test]
    fn value_offsets_point_at_the_value_inside_quotes() {
        let parse = parse_jsp(FIXTURE);
        let vt = parse.action_refs.iter().find(|r| r.name == "viewTree").unwrap();
        assert_eq!(&FIXTURE[vt.start..vt.end], "viewTree");
        assert_eq!(vt.raw, "viewTree");

        let edit = parse.action_refs.iter().find(|r| r.name == "/do/Cat/edit").unwrap();
        assert_eq!(&FIXTURE[edit.start..edit.end], "/do/Cat/edit");

        // .action value: the span covers the raw value (with the extension), not the key.
        let save = parse.action_refs.iter().find(|r| r.name == "save").unwrap();
        assert_eq!(&FIXTURE[save.start..save.end], "save.action");
        assert_eq!(save.raw, "save.action");
    }

    #[test]
    fn computed_expression_is_flagged_not_normalized() {
        let parse = parse_jsp(FIXTURE);
        let computed = parse.action_refs.iter().find(|r| r.computed).unwrap();
        assert_eq!(computed.raw, "${someExpr}");
        // The name keeps the raw expression (never resolved) so downstream can't flag it.
        assert_eq!(computed.name, "${someExpr}");
        assert_eq!(&FIXTURE[computed.start..computed.end], "${someExpr}");
    }

    #[test]
    fn action_inside_comment_and_scriptlet_is_ignored() {
        let parse = parse_jsp(FIXTURE);
        assert!(
            !parse.action_refs.iter().any(|r| r.name.contains("ignored") || r.raw.contains("ignored")),
            "commented / scriptlet action leaked: {:?}",
            parse.action_refs
        );
        // Exactly the four real refs: viewTree, /do/Cat/edit, save, ${someExpr}.
        assert_eq!(parse.action_refs.len(), 4, "refs = {:?}", parse.action_refs);
    }

    #[test]
    fn legacy_html_form_action_and_submit_and_anchor_href() {
        let src = r#"<%@ taglib prefix="html" uri="/tags/struts-html" %>
            <html:form action="/saveUser.do">x</html:form>
            <s:submit action="reset"/>
            <a href="https://host/ctx/deleteItem.action?id=3">del</a>
            <a href="/plain/link.html">not an action</a>"#;
        let parse = parse_jsp(src);
        let names: Vec<&str> = parse.action_refs.iter().map(|r| r.name.as_str()).collect();

        // <html:form action="/saveUser.do"> → leading slash kept, .do stripped
        assert!(names.contains(&"/saveUser"), "names = {names:?}");
        // <s:submit action="reset"> → reset
        assert!(names.contains(&"reset"), "names = {names:?}");
        // anchor href .action URL → last segment, query dropped
        assert!(names.contains(&"deleteItem"), "names = {names:?}");
        // a plain .html link is NOT an action reference
        assert!(!names.contains(&"link"), "plain link leaked: {names:?}");
        assert!(!names.iter().any(|n| n.contains("link")), "plain link leaked: {names:?}");
    }

    #[test]
    fn case_insensitive_tags_and_attrs() {
        let src = r#"<S:FORM ACTION="Upper">y</S:FORM>"#;
        let parse = parse_jsp(src);
        assert_eq!(parse.action_refs.len(), 1);
        assert_eq!(parse.action_refs[0].name, "Upper");
    }

    #[test]
    fn empty_and_unreadable_are_graceful() {
        assert_eq!(parse_jsp(""), JspParse::default());
        assert_eq!(parse_jsp_file(Path::new("/no/such/file.jsp")), JspParse::default());
    }

    #[test]
    fn normalize_action_ref_matches_parse_jsp_keys() {
        // The raw attribute value (as the editor sends it) normalizes to the SAME key the
        // scanner stores — otherwise a go-to / find-usages needle never matches.
        assert_eq!(normalize_action_ref("/do/Cat/edit"), Some("/do/Cat/edit".into()));
        // trailing .action / .do stripped
        assert_eq!(normalize_action_ref("/do/Cat/edit.action"), Some("/do/Cat/edit".into()));
        assert_eq!(normalize_action_ref("/saveUser.do"), Some("/saveUser".into()));
        // query / fragment dropped
        assert_eq!(normalize_action_ref("/do/Cat/edit.action?id=3"), Some("/do/Cat/edit".into()));
        assert_eq!(normalize_action_ref("/do/Cat/edit#frag"), Some("/do/Cat/edit".into()));
        // bare name kept as-is (namespace unknown → caller does a suffix match)
        assert_eq!(normalize_action_ref("viewTree"), Some("viewTree".into()));
        // computed / empty → no static key
        assert_eq!(normalize_action_ref("${x}"), None);
        assert_eq!(normalize_action_ref("%{bean.url}"), None);
        assert_eq!(normalize_action_ref("   "), None);
    }

    #[test]
    fn normalize_action_ref_round_trips_scanner_output() {
        // For a real `action=` attribute, the scanner's stored name equals normalize_action_ref
        // of the raw value — the invariant the resolver relies on.
        for raw in ["viewTree", "/do/Cat/edit", "save.action", "/saveUser.do"] {
            let src = format!(r#"<s:url action="{raw}"/>"#);
            let stored = parse_jsp(&src).action_refs.into_iter().next().unwrap().name;
            assert_eq!(normalize_action_ref(raw), Some(stored), "raw = {raw}");
        }
    }

    #[test]
    fn does_not_match_substring_attribute() {
        // `data-action` must not be mistaken for `action`.
        let src = r#"<form data-action="notThis" action="realOne">z</form>"#;
        let parse = parse_jsp(src);
        let names: Vec<&str> = parse.action_refs.iter().map(|r| r.name.as_str()).collect();
        assert!(names.contains(&"realOne"), "names = {names:?}");
        assert!(!names.contains(&"notThis"), "matched a substring attr: {names:?}");
    }
}
