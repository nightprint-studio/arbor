//! JSP **include/view-reference** scanner — a path pointing at another JSP view or
//! fragment, so the editor can Ctrl+B / Ctrl+click navigate to it.
//!
//! Companion to [`crate::jsp`] (action refs / taglibs), [`crate::forms`] (form + fields)
//! and [`crate::jsp_vars`] (page-scoped variables). This module answers: *for this JSP,
//! which attribute values point at another on-disk JSP view*, and *where does a given
//! reference resolve to*. It powers go-to on:
//!
//!   - directive include:   `<%@ include file="/WEB-INF/jsp/header.jspf" %>` (lives in a
//!     `<%@ … %>` directive, which the masking deliberately leaves visible);
//!   - standard action:      `<jsp:include page="foot.jsp"/>`,
//!     `<jsp:directive.include file="x.jspf"/>`;
//!   - Struts include:       `<s:include value="/common/nav.jsp"/>`;
//!   - JSTL import (local):   `<c:import url="/WEB-INF/x.jsp"/>` — an `http(s)://` URL is
//!     external, resolved to `None`.
//!
//! Same engineering as the sibling scanners: a tolerant linear byte scan (a JSP is not
//! valid XML) reusing the shared masking + attribute helpers (`pub(crate)` in `jsp.rs`), so
//! a reference inside a `<%-- comment --%>` or `<% scriptlet %>` is ignored. A value that is
//! a runtime expression (`${…}` / `%{…}` / `<%= … %>`) is flagged `computed` (inconclusive →
//! never navigable). Malformed / unclosed tags are skipped, never fatal.

use std::path::{Path, PathBuf};

use crate::jsp::{attr_value, find_from, masked_regions, region_covering, tag_local_name};

/// A JSP include / view reference — the path attribute value verbatim plus its byte span
/// (inside the quotes) and whether the value is a runtime expression.
///
/// Module-local record (not an ingested config record → not in `model.rs`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JspInclude {
    /// The path attribute value exactly as written (before any resolution).
    pub raw: String,
    /// Start byte offset of the value inside the quotes.
    pub start: usize,
    /// End byte offset (exclusive).
    pub end: usize,
    /// The value is a runtime expression (`${…}` / `%{…}` / `<%= … %>`) — inconclusive, so
    /// the go-to layer must treat it as *not navigable*.
    pub computed: bool,
}

/// Tag local-names (after any `prefix:`) that carry a JSP include/view path, mapped to the
/// attribute that holds it. Matched case-insensitively against a lowercased local-name.
///
///   - `include` → `<jsp:include page=>`, `<jsp:directive.include file=>` (local-name of
///     `jsp:directive.include` is `directive.include` — handled separately), `<s:include value=>`;
///   - `import`  → `<c:import url=>`.
const INCLUDE_ATTRS: &[(&str, &[&str])] = &[
    // A `<jsp:include>` uses `page=`; a `<s:include>` uses `value=`; a
    // `<jsp:directive.include>` uses `file=` (its local-name is `directive.include`).
    ("include", &["page", "value", "file"]),
    ("directive.include", &["file"]),
    ("import", &["url"]),
];

/// Scan a JSP `source` string for include / view references.
///
/// Directive includes (`<%@ include file="…" %>`) are found by scanning the raw source for
/// `<%@` blocks (the masking leaves directives visible — mirror [`crate::jsp`]'s taglib
/// scan). Tag includes are found by walking tags while skipping masked comment/scriptlet
/// regions, so a reference inside them is ignored.
pub fn parse_jsp_includes(source: &str) -> Vec<JspInclude> {
    let bytes = source.as_bytes();
    let masked = masked_regions(source);

    let mut out = Vec::new();

    // Directive includes: `<%@ include file="…" %>`. These live inside a `<%@ … %>` block,
    // which the scriptlet mask does NOT cover, so scan the raw source for them.
    scan_directive_includes(source, &mut out);

    // Tag includes: walk tags, skipping masked regions.
    let mut i = 0usize;
    while i < bytes.len() {
        if let Some(reg) = region_covering(&masked, i) {
            i = reg.1; // jump past the masked region
            continue;
        }
        if bytes[i] == b'<' {
            if let Some(tag_end) = scan_tag(source, i, &mut out) {
                i = tag_end;
                continue;
            }
        }
        i += 1;
    }

    out
}

/// Convenience: read `path` and [`parse_jsp_includes`] it. A read error yields an empty
/// vec (skip-and-continue — one unreadable JSP never aborts a scan).
pub fn parse_jsp_includes_file(path: &Path) -> Vec<JspInclude> {
    match crate::io::read_to_string_lf(path) {
        Ok(text) => parse_jsp_includes(&text),
        Err(_) => Vec::new(),
    }
}

/// Resolve a raw include path (as extracted verbatim from the quotes) against the JSP at
/// `jsp_path`, returning the target file if it exists on disk.
///
///   - a query/fragment (`?…` / `#…`) is dropped;
///   - an empty / computed (`${…}` / `%{…}` / `<%= … %>`) / `http(s)://` value → `None`;
///   - an **absolute** path (`/WEB-INF/…`) resolves against the webapp root (the ancestor
///     dir of the JSP that contains a `WEB-INF` child — see [`webapp_root`]); with no
///     webapp root found, it falls back to the JSP's own dir with the leading `/` stripped;
///   - a **relative** path resolves against the JSP's own directory, normalizing `..`/`.`;
///   - the result is returned only if it **exists as a file**, else `None`.
pub fn resolve_include_target(jsp_path: &Path, raw: &str) -> Option<PathBuf> {
    let value = raw.trim();
    // Drop a query string / fragment.
    let path = value.split(['?', '#']).next().unwrap_or(value).trim();
    if path.is_empty() || is_computed(path) || is_external_url(path) {
        return None;
    }

    let joined = if let Some(rel) = path.strip_prefix('/') {
        // Absolute: resolve against the webapp root, else fall back to the JSP's own dir.
        match webapp_root(jsp_path) {
            Some(root) => root.join(rel),
            None => jsp_path.parent().unwrap_or(Path::new("")).join(rel),
        }
    } else {
        // Relative: resolve against the JSP's own directory.
        jsp_path.parent().unwrap_or(Path::new("")).join(path)
    };

    let resolved = normalize_path(&joined);
    resolved.is_file().then_some(resolved)
}

/// Every **static** include in `source` whose target doesn't resolve to a file on disk — the
/// input to the "included file not found" diagnostic. Computed (`${…}` / `%{…}` / `<%= … %>`)
/// and external (`http(s)://`) references (and empty values) are skipped, so a runtime or
/// remote include is never a false positive. `jsp_path` is the including JSP (the resolution
/// base). Each returned [`JspInclude`] carries the raw value + its byte span for the squiggle.
pub fn unresolved_includes(jsp_path: &Path, source: &str) -> Vec<JspInclude> {
    parse_jsp_includes(source)
        .into_iter()
        .filter(|inc| {
            let raw = inc.raw.trim();
            !inc.computed
                && !raw.is_empty()
                && !is_external_url(raw)
                && resolve_include_target(jsp_path, &inc.raw).is_none()
        })
        .collect()
}

/// [`unresolved_includes`] reading `jsp_path` from disk. Empty when the file can't be read.
pub fn unresolved_includes_file(jsp_path: &Path) -> Vec<JspInclude> {
    match crate::io::read_to_string_lf(jsp_path) {
        Ok(text) => unresolved_includes(jsp_path, &text),
        Err(_) => Vec::new(),
    }
}

/// The web application root for a JSP: the nearest ancestor directory that contains a
/// `WEB-INF` child dir. If the JSP itself sits under a `.../WEB-INF/...` path (fragments
/// commonly do), the webapp root is the parent of the top-most `WEB-INF` on its path.
///
/// `None` if no `WEB-INF` is found on the ancestry (caller falls back to the JSP's dir).
fn webapp_root(jsp_path: &Path) -> Option<PathBuf> {
    // Case 1: the JSP is itself under a WEB-INF — the webapp root is the parent of the
    // top-most WEB-INF component on its path (that dir's child IS WEB-INF).
    let mut top_web_inf: Option<PathBuf> = None;
    let mut acc = PathBuf::new();
    for comp in jsp_path.components() {
        acc.push(comp.as_os_str());
        if comp.as_os_str().eq_ignore_ascii_case("WEB-INF") && top_web_inf.is_none() {
            top_web_inf = acc.parent().map(Path::to_path_buf);
        }
    }
    if let Some(root) = top_web_inf {
        return Some(root);
    }

    // Case 2: the JSP is not under WEB-INF — walk ancestors, the webapp root is the first
    // ancestor dir that has a `WEB-INF` child.
    let mut dir = jsp_path.parent();
    while let Some(d) = dir {
        if d.join("WEB-INF").is_dir() {
            return Some(d.to_path_buf());
        }
        dir = d.parent();
    }
    None
}

/// Normalize a joined path lexically: resolve `.` (skip) and `..` (pop the previous normal
/// component). Does not touch the filesystem — the existence check is done by the caller.
fn normalize_path(path: &Path) -> PathBuf {
    use std::path::Component;
    let mut out = PathBuf::new();
    for comp in path.components() {
        match comp {
            Component::CurDir => {}
            Component::ParentDir => {
                // Only pop a real path segment; keep `..` if there's nothing to pop (e.g. a
                // root/prefix already at the front).
                if matches!(out.components().next_back(), Some(Component::Normal(_))) {
                    out.pop();
                } else {
                    out.push("..");
                }
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}

/// Is this value a runtime expression (EL / OGNL / JSP expression) → inconclusive?
fn is_computed(value: &str) -> bool {
    value.contains("${") || value.contains("%{") || value.contains("<%")
}

/// Is this value an external `http(s)://` URL (never a local file)?
fn is_external_url(value: &str) -> bool {
    let lower = value.trim_start().to_ascii_lowercase();
    lower.starts_with("http://") || lower.starts_with("https://")
}

/// Scan `source` for `<%@ include file="…" %>` directives, pushing a [`JspInclude`] (span on
/// the `file` value) for each. Mirrors [`crate::jsp`]'s taglib directive scan.
fn scan_directive_includes(source: &str, out: &mut Vec<JspInclude>) {
    let bytes = source.as_bytes();
    let mut i = 0usize;
    while let Some(open) = find_from(source, i, "<%@") {
        let end = find_from(source, open + 3, "%>").unwrap_or(bytes.len());
        let inner_start = open + 3;
        let inner = &source[inner_start..end.min(source.len())];
        // Only `include` directives (skip `taglib`, `page`).
        if inner.trim_start().to_ascii_lowercase().starts_with("include") {
            if let Some((raw, vstart, vend)) = attr_value(source, inner_start, end, "file") {
                push_include(out, raw, vstart, vend);
            }
        }
        i = (end + 2).min(bytes.len());
    }
}

/// Scan a single `<…>` tag starting at `open` (`source[open] == '<'`). On a tag we handle,
/// push its include reference and return the byte offset just past the tag's `>`. Returns
/// `None` if this isn't a tag we scan through (or it's unterminated) so the caller advances
/// one byte.
fn scan_tag(source: &str, open: usize, out: &mut Vec<JspInclude>) -> Option<usize> {
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
    let name = tag_local_name(source, after, close)?;

    let attrs = INCLUDE_ATTRS.iter().find(|(n, _)| *n == name).map(|(_, a)| *a);
    if let Some(attrs) = attrs {
        // Read the first attribute that is present (the tags carry exactly one path attr).
        for attr in attrs {
            if let Some((raw, vstart, vend)) = attr_value(source, after, close, attr) {
                push_include(out, raw, vstart, vend);
                break;
            }
        }
    }
    Some(close + 1)
}

/// Build a [`JspInclude`] from a raw attribute value + span and push it. An empty value is
/// skipped; a runtime expression is flagged `computed`.
fn push_include(out: &mut Vec<JspInclude>, raw: String, start: usize, end: usize) {
    if raw.trim().is_empty() {
        return;
    }
    let computed = is_computed(&raw);
    out.push(JspInclude { raw, start, end, computed });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::tmp_dir;

    #[test]
    fn unresolved_includes_flags_only_the_missing_static_one() {
        let dir = tmp_dir("inc-lint");
        std::fs::write(dir.join("header.jspf"), "<div/>").unwrap();
        let jsp = dir.join("page.jsp");
        let body = concat!(
            "<%@ include file=\"header.jspf\" %>\n",      // exists → OK
            "<jsp:include page=\"missing.jspf\"/>\n",       // missing → flagged
            "<c:import url=\"http://example.com/x\"/>\n",   // external → skipped
            "<jsp:include page=\"${dynamic}\"/>\n",         // computed → skipped
        );
        std::fs::write(&jsp, body).unwrap();

        let missing = unresolved_includes_file(&jsp);
        assert_eq!(missing.len(), 1, "only missing.jspf should be flagged");
        assert_eq!(missing[0].raw, "missing.jspf");
        // The span must cover exactly the raw value, so the FE underlines the right text.
        assert_eq!(&body[missing[0].start..missing[0].end], "missing.jspf");
    }

    const FIXTURE: &str = r#"<%@ taglib prefix="s" uri="/struts-tags" %>
<%@ include file="/WEB-INF/inc/h.jspf" %>
<html>
  <jsp:include page="foot.jsp"/>
  <jsp:directive.include file="parts/side.jspf"/>
  <s:include value="/common/nav.jsp"/>
  <c:import url="/WEB-INF/x.jsp"/>
  <c:import url="https://x/y"/>
  <jsp:include page="${p}"/>
  <%-- <jsp:include page="commented.jsp"/> --%>
  <% String s = "<jsp:include page=\"scriptlet.jsp\"/>"; %>
</html>"#;

    fn raws(source: &str) -> Vec<String> {
        parse_jsp_includes(source).into_iter().map(|i| i.raw).collect()
    }

    #[test]
    fn extracts_all_include_flavours_with_correct_spans() {
        let inc = parse_jsp_includes(FIXTURE);
        // Every value's span points at the raw text inside the quotes.
        for i in &inc {
            assert_eq!(&FIXTURE[i.start..i.end], i.raw, "span mismatch for {:?}", i);
        }
        let got = raws(FIXTURE);
        assert!(got.contains(&"/WEB-INF/inc/h.jspf".to_string()), "got = {got:?}");
        assert!(got.contains(&"foot.jsp".to_string()), "got = {got:?}");
        assert!(got.contains(&"parts/side.jspf".to_string()), "got = {got:?}");
        assert!(got.contains(&"/common/nav.jsp".to_string()), "got = {got:?}");
        assert!(got.contains(&"/WEB-INF/x.jsp".to_string()), "got = {got:?}");
    }

    #[test]
    fn http_url_is_found_but_resolves_to_none() {
        let inc = parse_jsp_includes(FIXTURE);
        let url = inc.iter().find(|i| i.raw == "https://x/y").expect("url extracted");
        assert!(!url.computed);
        // No file exists for an external URL — resolution is None regardless of jsp_path.
        assert_eq!(resolve_include_target(Path::new("/any/page.jsp"), &url.raw), None);
    }

    #[test]
    fn computed_expression_is_flagged() {
        let inc = parse_jsp_includes(FIXTURE);
        let expr = inc.iter().find(|i| i.raw == "${p}").expect("computed extracted");
        assert!(expr.computed, "expected computed flag: {:?}", expr);
        assert_eq!(resolve_include_target(Path::new("/any/page.jsp"), &expr.raw), None);
    }

    #[test]
    fn include_inside_comment_and_scriptlet_is_ignored() {
        let got = raws(FIXTURE);
        assert!(!got.iter().any(|r| r.contains("commented")), "comment leaked: {got:?}");
        assert!(!got.iter().any(|r| r.contains("scriptlet")), "scriptlet leaked: {got:?}");
        // Exactly the real refs: directive h.jspf + foot + side + nav + x + url + ${p}.
        assert_eq!(got.len(), 7, "got = {got:?}");
    }

    #[test]
    fn case_insensitive_tag_and_attr() {
        let src = r#"<JSP:INCLUDE PAGE="Foot.jsp"/>"#;
        let got = raws(src);
        assert_eq!(got, vec!["Foot.jsp".to_string()]);
    }

    #[test]
    fn empty_value_and_unreadable_file_are_graceful() {
        assert!(parse_jsp_includes(r#"<jsp:include page=""/>"#).is_empty());
        assert!(parse_jsp_includes("").is_empty());
        assert!(parse_jsp_includes_file(Path::new("/no/such/file.jsp")).is_empty());
    }

    /// Build `webroot/WEB-INF/jsp/page.jsp`, `webroot/WEB-INF/inc/header.jspf`,
    /// `webroot/common/nav.jsp` under a scratch dir. Returns the webroot.
    fn fixture_webapp() -> PathBuf {
        let root = tmp_dir("webapp").join("webroot");
        std::fs::create_dir_all(root.join("WEB-INF/jsp")).unwrap();
        std::fs::create_dir_all(root.join("WEB-INF/inc")).unwrap();
        std::fs::create_dir_all(root.join("common")).unwrap();
        std::fs::write(root.join("WEB-INF/jsp/page.jsp"), "<html/>").unwrap();
        std::fs::write(root.join("WEB-INF/inc/header.jspf"), "<div/>").unwrap();
        std::fs::write(root.join("common/nav.jsp"), "<nav/>").unwrap();
        root
    }

    #[test]
    fn absolute_path_resolves_against_webapp_root() {
        let root = fixture_webapp();
        let page = root.join("WEB-INF/jsp/page.jsp");
        // `/WEB-INF/inc/header.jspf` from a JSP under WEB-INF → the header fragment.
        let target = resolve_include_target(&page, "/WEB-INF/inc/header.jspf").expect("resolved");
        assert_eq!(target, normalize_path(&root.join("WEB-INF/inc/header.jspf")));
        // `/common/nav.jsp` → the nav file at the webapp root.
        let nav = resolve_include_target(&page, "/common/nav.jsp").expect("resolved");
        assert_eq!(nav, normalize_path(&root.join("common/nav.jsp")));
    }

    #[test]
    fn relative_path_resolves_against_jsp_dir_with_dotdot() {
        let root = fixture_webapp();
        let page = root.join("WEB-INF/jsp/page.jsp");
        // `../inc/header.jspf` from `.../WEB-INF/jsp/page.jsp` → `.../WEB-INF/inc/header.jspf`.
        let target = resolve_include_target(&page, "../inc/header.jspf").expect("resolved");
        assert_eq!(target, normalize_path(&root.join("WEB-INF/inc/header.jspf")));
    }

    #[test]
    fn nonexistent_and_computed_and_url_resolve_to_none() {
        let root = fixture_webapp();
        let page = root.join("WEB-INF/jsp/page.jsp");
        assert_eq!(resolve_include_target(&page, "/WEB-INF/nope.jsp"), None);
        assert_eq!(resolve_include_target(&page, "${x}.jsp"), None);
        assert_eq!(resolve_include_target(&page, "%{bean.view}"), None);
        assert_eq!(resolve_include_target(&page, "https://host/x.jsp"), None);
        assert_eq!(resolve_include_target(&page, ""), None);
    }

    #[test]
    fn query_and_fragment_are_dropped_before_resolving() {
        let root = fixture_webapp();
        let page = root.join("WEB-INF/jsp/page.jsp");
        let target = resolve_include_target(&page, "/common/nav.jsp?x=1#frag").expect("resolved");
        assert_eq!(target, normalize_path(&root.join("common/nav.jsp")));
    }
}
