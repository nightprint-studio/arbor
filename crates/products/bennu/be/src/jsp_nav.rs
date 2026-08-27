//! `jsp_nav` domain — single-file JSP navigation:
//!   - `bennu_jsp_nav`: go-to-declaration + find-usages for a JSP **page-scoped variable**
//!     (`<c:set var>` / `<s:set var>` / `<c:forEach var>` / `<s:iterator var>` …) and its
//!     EL/OGNL references (`${var}` / `%{var}` / `%{#var}`);
//!   - `bennu_jsp_include_target`: resolve a JSP **include / view reference** (`<%@ include
//!     file>` / `<jsp:include page>` / `<s:include value>` / `<c:import url>`) under the caret
//!     to the referenced file on disk, for cross-file Ctrl+B / Ctrl+click go-to.
//!
//! Unlike the Struts-action navigation (`bennu_definition` / `bennu_action_usages`, which
//! resolve across the config graph), a JSP variable is **page-scoped** — declaration and
//! uses live in the SAME file. So this handler needs no project index and always answers:
//! it parses the buffer the FE hands it (`source`) with `bennu-web`'s tested `parse_jsp_vars`
//! and resolves the caret entirely in-file. This is the resolver behind Ctrl+B / Alt+F7 on a
//! `${myVar}` — which previously had nothing behind it (the config graph only knows actions).

use std::path::Path;

use bennu_core::prelude::BennuState;
use bennu_proto::prelude::{DeclarationTarget, JspNav, UsageHit};
use bennu_web::prelude::{
    line_col, parse_jsp_vars, resolve_include_target, var_declaration, var_name_at, var_usages,
};
use serde::Deserialize;

/// Args for [`bennu_jsp_nav`].
#[derive(Deserialize)]
pub struct JspNavArgs {
    /// Absolute path of the JSP the caret is in (echoed back on the declaration + usages).
    pub file: String,
    /// The current (possibly-unsaved) buffer text — the caret is classified against it.
    pub source: String,
    /// UTF-8 byte offset of the caret.
    pub offset: usize,
}

/// Resolve the JSP variable under the caret to its in-page declaration + every reference.
/// Returns the default (empty) [`JspNav`] when the caret isn't on a JSP variable token — the
/// FE then falls back to the Struts-action resolution. Never errors (a JSP is scanned
/// tolerantly).
#[arbor_rpc::handler]
fn bennu_jsp_nav(_ctx: &BennuState, args: JspNavArgs) -> Result<JspNav, String> {
    let vars = parse_jsp_vars(&args.source);
    let Some(name) = var_name_at(&vars, args.offset) else {
        return Ok(JspNav::default());
    };
    let name = name.to_string();

    let declaration = var_declaration(&vars, &name).map(|d| {
        let (line, col) = line_col(&args.source, d.start);
        DeclarationTarget {
            file: args.file.clone(),
            start: d.start,
            end: d.end,
            line: line as u32,
            col: col as u32,
            label: format!("JSP variable `{}` (<{}>)", name, d.tag),
        }
    });

    let usages = var_usages(&vars, &name)
        .into_iter()
        .map(|r| {
            let (line, col, preview) = crate::index_service::line_col_preview(&args.source, r.start);
            UsageHit { file: args.file.clone(), start: r.start, end: r.end, line, col, preview, via: None }
        })
        .collect();

    Ok(JspNav { label: format!("JSP variable `{name}`"), declaration, usages })
}

/// Args for [`bennu_jsp_include_target`].
#[derive(Deserialize)]
pub struct JspIncludeTargetArgs {
    /// Absolute path of the JSP being edited (the resolution base).
    pub file: String,
    /// The include reference under the caret (the FE's `refAtCaret()` — the raw path attribute
    /// value, e.g. `/WEB-INF/inc/header.jspf` or `foot.jsp`).
    pub path: String,
}

/// Resolve a JSP **include / view reference** under the caret to the absolute path of the
/// referenced JSP file, for Ctrl+B / Ctrl+click go-to. Powers navigation on directive
/// includes (`<%@ include file>`), `<jsp:include page>`, `<s:include value>`,
/// `<c:import url>`, …
///
/// Returns `None` (never an error) when the reference isn't resolvable — a computed
/// expression (`${…}` / `%{…}` / `<%= … %>`), an external `http(s)://` URL, or a path that
/// doesn't point at an existing file. The path is forward-slashed so the FE opens it directly.
#[arbor_rpc::handler]
fn bennu_jsp_include_target(
    _ctx: &BennuState,
    args: JspIncludeTargetArgs,
) -> Result<Option<String>, String> {
    let target = resolve_include_target(Path::new(&args.file), &args.path)
        .map(|p| p.to_string_lossy().replace('\\', "/"));
    Ok(target)
}
