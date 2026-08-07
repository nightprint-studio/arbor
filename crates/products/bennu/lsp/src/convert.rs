//! Protocol answers → [`crate::model`] values.
//!
//! Every function here does the same two things: turn a `file:` URI into a path, and turn
//! `{line, character}` into a byte offset. Both need context the protocol type does not
//! carry — the position encoding, and the *text* of the file the position is in — which is
//! why these are free functions taking that context rather than `From` impls.
//!
//! Conversions are grouped **by file** before they are done, so a find-usages answer
//! spanning forty files builds forty line indexes rather than one per hit. It matters: a
//! reference-heavy symbol in a large workspace comes back with hundreds of locations, and
//! re-indexing a file per hit turns a fast answer into a visible pause.

use std::collections::HashMap;

use crate::line_index::{LineIndex, PositionEncoding, Range};
use crate::model::{
    ActionEntry, CompletionEntry, DiagEntry, FileEdit, FileOp, HoverText, SignatureText,
    SpanTarget, SymbolNode,
};
use crate::types::{self, symbol_kind_name_for};
use crate::uri;

/// Supplies a file's text: the open buffer when there is one, else the file on disk.
/// `None` for a path we cannot read — a target inside a dependency the server has sources
/// for and Bennu does not.
pub type TextResolver<'a> = &'a dyn Fn(&str) -> Option<String>;

/// Build a [`SpanTarget`] from a name range inside an already-indexed file.
pub fn span_target(file: &str, name: Range, index: &LineIndex<'_>, enc: PositionEncoding) -> SpanTarget {
    let (start, end) = index.byte_range(name, enc);
    let (line, col) = index.line_col_utf16(start);
    SpanTarget {
        file: file.to_string(),
        start,
        end,
        line,
        col,
        preview: index.line_text(line.saturating_sub(1)).trim().to_string(),
    }
}

/// Convert goto / reference targets, grouping by file so each one is indexed once.
///
/// Targets in files that cannot be read are **kept**, with a zero span and an empty
/// preview: a reference the server found is a fact, and dropping it would silently
/// under-report the usage count. The caller decides whether an unopenable target is worth
/// showing.
pub fn targets(
    raw: Vec<(String, Range, Range)>,
    enc: PositionEncoding,
    resolve: TextResolver<'_>,
) -> Vec<SpanTarget> {
    // Group while remembering the original order, so the result reads in the order the
    // server reported (which for references is document order per file).
    let mut by_file: HashMap<String, Vec<(usize, Range)>> = HashMap::new();
    let mut skipped: Vec<(usize, String)> = Vec::new();
    for (i, (raw_uri, _whole, name)) in raw.into_iter().enumerate() {
        match uri::from_uri(&raw_uri) {
            Some(file) => by_file.entry(file).or_default().push((i, name)),
            // A non-`file:` URI (rust-analyzer uses its own scheme for macro expansions):
            // there is no path to open, so record the position and move on.
            None => skipped.push((i, raw_uri)),
        }
    }

    let mut out: Vec<(usize, SpanTarget)> = Vec::new();
    for (file, hits) in by_file {
        match resolve(&file) {
            Some(text) => {
                let index = LineIndex::new(&text);
                for (i, name) in hits {
                    out.push((i, span_target(&file, name, &index, enc)));
                }
            }
            None => {
                for (i, _) in hits {
                    out.push((
                        i,
                        SpanTarget {
                            file: file.clone(),
                            start: 0,
                            end: 0,
                            line: 1,
                            col: 1,
                            preview: String::new(),
                        },
                    ));
                }
            }
        }
    }
    for (i, raw_uri) in skipped {
        out.push((
            i,
            SpanTarget {
                file: raw_uri,
                start: 0,
                end: 0,
                line: 1,
                col: 1,
                preview: String::new(),
            },
        ));
    }
    out.sort_by_key(|(i, _)| *i);
    out.into_iter().map(|(_, t)| t).collect()
}

/// One completion item. `index` is over the buffer the request was made against, which is
/// the only file a completion's own edit can touch.
pub fn completion_item(
    id: usize,
    item: types::CompletionItem,
    file: &str,
    index: &LineIndex<'_>,
    enc: PositionEncoding,
) -> CompletionEntry {
    let (replace, edit_text) = match item.text_edit.as_ref() {
        Some(edit) => {
            let (range, text) = edit.resolve();
            (Some(index.byte_range(range, enc)), Some(text.to_string()))
        }
        None => (None, None),
    };
    // Precedence: the server's own edit, then `insertText`, then the label. The label is
    // the last resort because it is a *display* string — for Rust it can read
    // `push(…)` or carry a `use` path — and inserting it verbatim is how a completion
    // produces text that does not compile.
    let insert_text = edit_text
        .or_else(|| item.insert_text.clone())
        .unwrap_or_else(|| item.label.clone());

    // A snippet body is parsed HERE — once, on the way out — rather than by whoever renders the
    // completion. What crosses the wire is plain text plus the tab stops as byte ranges, which is a
    // shape any editor can use and none has to write a grammar for. See `crate::snippet`.
    let is_snippet = item.insert_text_format == Some(types::INSERT_TEXT_FORMAT_SNIPPET);
    let (insert_text, snippet_stops) = if is_snippet {
        let parsed = crate::snippet::parse(&insert_text);
        (parsed.text, parsed.stops)
    } else {
        (insert_text, Vec::new())
    };

    let detail = item
        .detail
        .clone()
        .or_else(|| label_detail(item.label_details.as_ref()));

    CompletionEntry {
        id,
        label: item.label.clone(),
        kind: types::completion_kind_name(item.kind).to_string(),
        detail,
        doc: item.documentation.as_ref().map(|d| d.text().to_string()),
        sort_text: item.sort_text.clone(),
        filter_text: item.filter_text.clone(),
        insert_text,
        replace,
        is_snippet,
        snippet_stops,
        additional_edits: item
            .additional_text_edits
            .iter()
            .map(|e| {
                let (start, end) = index.byte_range(e.range, enc);
                FileEdit { file: file.to_string(), start, end, new_text: e.new_text.clone() }
            })
            .collect(),
        deprecated: item.deprecated == Some(true)
            || item.tags.contains(&types::COMPLETION_TAG_DEPRECATED),
        preselect: item.preselect == Some(true),
    }
}

/// `labelDetails` flattened to one line, for servers that put the signature there instead
/// of in `detail` (rust-analyzer puts the type in `detail` and the import path in
/// `description`).
fn label_detail(d: Option<&types::CompletionItemLabelDetails>) -> Option<String> {
    let d = d?;
    let parts: Vec<&str> =
        [d.detail.as_deref(), d.description.as_deref()].into_iter().flatten().collect();
    (!parts.is_empty()).then(|| parts.join(" "))
}

/// A hover answer.
pub fn hover(h: types::Hover, index: &LineIndex<'_>, enc: PositionEncoding) -> Option<HoverText> {
    let markdown = h.contents.text();
    if markdown.trim().is_empty() {
        // A server may answer with an empty card rather than `null`. Showing an empty
        // tooltip is worse than showing none: it reads as broken.
        return None;
    }
    Some(HoverText { markdown, range: h.range.map(|r| index.byte_range(r, enc)) })
}

/// The hierarchical document-symbol tree.
///
/// `language` only decides how a kind is *named* — see [`symbol_kind_name_for`], which is why an
/// outline of a Rust file says `trait` rather than the protocol's `interface`.
pub fn symbol_tree(
    syms: Vec<types::DocumentSymbol>,
    file: &str,
    index: &LineIndex<'_>,
    enc: PositionEncoding,
    language: &str,
) -> Vec<SymbolNode> {
    syms.into_iter()
        .map(|s| {
            let (start, end) = index.byte_range(s.range, enc);
            let (name_start, name_end) = index.byte_range(s.selection_range, enc);
            let (line, col) = index.line_col_utf16(name_start);
            SymbolNode {
                name: s.name,
                kind: symbol_kind_name_for(s.kind, language).to_string(),
                detail: s.detail,
                start,
                end,
                name_start,
                name_end,
                line,
                col,
                file: file.to_string(),
                children: symbol_tree(s.children, file, index, enc, language),
                deprecated: s.deprecated == Some(true)
                    || s.tags.contains(&types::DIAGNOSTIC_TAG_DEPRECATED),
            }
        })
        .collect()
}

/// The flat `SymbolInformation` shape, as a one-level list.
///
/// `containerName` is folded into the name (`Foo::bar`) rather than dropped: in a flat list
/// forty methods called `new` are indistinguishable without it.
pub fn flat_symbols(
    syms: Vec<types::SymbolInformation>,
    enc: PositionEncoding,
    resolve: TextResolver<'_>,
    language: &str,
) -> Vec<SymbolNode> {
    let mut by_file: HashMap<String, Vec<(usize, types::SymbolInformation)>> = HashMap::new();
    for (i, s) in syms.into_iter().enumerate() {
        if let Some(file) = uri::from_uri(&s.location.uri) {
            by_file.entry(file).or_default().push((i, s));
        }
    }
    let mut out: Vec<(usize, SymbolNode)> = Vec::new();
    for (file, group) in by_file {
        let text = resolve(&file);
        let index = text.as_deref().map(LineIndex::new);
        for (i, s) in group {
            let (start, end, line, col) = match &index {
                Some(idx) => {
                    let (a, b) = idx.byte_range(s.location.range, enc);
                    let (l, c) = idx.line_col_utf16(a);
                    (a, b, l, c)
                }
                None => (0, 0, s.location.range.start.line as usize + 1, 1),
            };
            let name = match &s.container_name {
                Some(c) if !c.is_empty() => format!("{c}::{}", s.name),
                _ => s.name.clone(),
            };
            out.push((
                i,
                SymbolNode {
                    name,
                    kind: symbol_kind_name_for(s.kind, language).to_string(),
                    detail: None,
                    start,
                    end,
                    name_start: start,
                    name_end: end,
                    line,
                    col,
                    file: file.clone(),
                    children: Vec::new(),
                    deprecated: s.deprecated == Some(true),
                },
            ));
        }
    }
    out.sort_by_key(|(i, _)| *i);
    out.into_iter().map(|(_, s)| s).collect()
}

/// One diagnostic. `index` is over the text the caller has for the diagnostic's file.
pub fn diagnostic(
    d: &types::Diagnostic,
    index: &LineIndex<'_>,
    enc: PositionEncoding,
    resolve: TextResolver<'_>,
) -> DiagEntry {
    let (start, end) = index.byte_range(d.range, enc);
    let related_raw: Vec<(String, Range, Range)> = d
        .related_information
        .iter()
        .map(|r| (r.location.uri.clone(), r.location.range, r.location.range))
        .collect();
    let messages: Vec<String> =
        d.related_information.iter().map(|r| r.message.clone()).collect();
    let related: Vec<(SpanTarget, String)> = targets(related_raw, enc, resolve)
        .into_iter()
        .zip(messages)
        .collect();

    DiagEntry {
        message: d.message.clone(),
        severity: severity_name(d.severity).to_string(),
        code: d.code.as_ref().map(|c| c.to_string()).unwrap_or_default(),
        source: d.source.clone().unwrap_or_default(),
        start,
        end,
        unnecessary: d.tags.contains(&types::DIAGNOSTIC_TAG_UNNECESSARY),
        deprecated: d.tags.contains(&types::DIAGNOSTIC_TAG_DEPRECATED),
        related,
        // Kept verbatim: a `codeAction` request has to echo the diagnostic back, opaque
        // `data` included, for the server to produce its fix.
        raw: diagnostic_wire(d),
    }
}

/// A diagnostic re-encoded in the shape a server will recognise as its own.
///
/// Needed because a `codeAction` request carries the diagnostics at the caret, and a server
/// matches them against what it published — including the opaque `data` it attached. Send a
/// diagnostic back without that and the quick fixes for it simply do not appear.
pub fn diagnostic_wire(d: &types::Diagnostic) -> serde_json::Value {
    serde_json::to_value(RawDiagnostic::from(d)).unwrap_or(serde_json::Value::Null)
}

/// The subset of a diagnostic that has to survive a round trip back to the server.
///
/// Re-serialized rather than kept as the original JSON because the client decodes into
/// typed structs; this puts back exactly the fields a server needs to recognise its own
/// diagnostic, and nothing else.
#[derive(serde::Serialize)]
struct RawDiagnostic<'a> {
    range: Range,
    #[serde(skip_serializing_if = "Option::is_none")]
    severity: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    source: Option<&'a str>,
    message: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    data: Option<&'a serde_json::Value>,
}

impl<'a> From<&'a types::Diagnostic> for RawDiagnostic<'a> {
    fn from(d: &'a types::Diagnostic) -> Self {
        Self {
            range: d.range,
            severity: d.severity,
            code: d.code.as_ref().map(|c| c.to_string()),
            source: d.source.as_deref(),
            message: &d.message,
            data: d.data.as_ref(),
        }
    }
}

/// LSP's numeric severity → Bennu's name. An absent severity reads as a warning: the spec
/// leaves it to the client, and silently promoting an unlabelled diagnostic to an error
/// would put a red squiggle on something the server declined to call one.
pub fn severity_name(severity: Option<u8>) -> &'static str {
    match severity {
        Some(1) => "error",
        Some(3) => "info",
        Some(4) => "hint",
        _ => "warning",
    }
}

/// A workspace edit → the flat edit list plus any file operations, grouped by file so each
/// one is indexed once.
pub fn workspace_edit(
    edit: &types::WorkspaceEdit,
    enc: PositionEncoding,
    resolve: TextResolver<'_>,
) -> (Vec<FileEdit>, Vec<FileOp>) {
    let mut per_file: Vec<(String, Vec<types::TextEdit>)> = Vec::new();
    let mut ops: Vec<FileOp> = Vec::new();

    // `documentChanges` wins when both are present: it is the versioned form, and a server
    // that sends both means them to be the same edit expressed twice.
    if let Some(changes) = &edit.document_changes {
        for change in changes {
            match change {
                types::DocumentChange::Edits(e) => {
                    if let Some(file) = uri::from_uri(&e.text_document.uri) {
                        per_file.push((file, e.edits.clone()));
                    }
                }
                types::DocumentChange::Resource(op) => {
                    if let Some(mapped) = file_op(op) {
                        ops.push(mapped);
                    }
                }
            }
        }
    } else if let Some(changes) = &edit.changes {
        // A map has no order; sort by path so a rename preview does not shuffle between
        // runs.
        let mut entries: Vec<_> = changes.iter().collect();
        entries.sort_by_key(|(u, _)| (*u).clone());
        for (raw_uri, edits) in entries {
            if let Some(file) = uri::from_uri(raw_uri) {
                per_file.push((file, edits.clone()));
            }
        }
    }

    let mut out: Vec<FileEdit> = Vec::new();
    for (file, edits) in per_file {
        let Some(text) = resolve(&file) else { continue };
        let index = LineIndex::new(&text);
        for e in edits {
            let (start, end) = index.byte_range(e.range, enc);
            out.push(FileEdit { file: file.clone(), start, end, new_text: e.new_text });
        }
    }
    (out, ops)
}

fn file_op(op: &types::ResourceOperation) -> Option<FileOp> {
    match op {
        types::ResourceOperation::Create { uri: u } => {
            Some(FileOp::Create { file: uri::from_uri(u)? })
        }
        types::ResourceOperation::Rename { old_uri, new_uri } => Some(FileOp::Rename {
            from: uri::from_uri(old_uri)?,
            to: uri::from_uri(new_uri)?,
        }),
        types::ResourceOperation::Delete { uri: u } => {
            Some(FileOp::Delete { file: uri::from_uri(u)? })
        }
    }
}

/// A code action, with its edits already converted.
pub fn code_action(
    action: types::CodeAction,
    enc: PositionEncoding,
    resolve: TextResolver<'_>,
) -> ActionEntry {
    let (edits, file_ops) = action
        .edit
        .as_ref()
        .map(|e| workspace_edit(e, enc, resolve))
        .unwrap_or_default();
    ActionEntry {
        title: action.title,
        kind: action.kind.unwrap_or_default(),
        preferred: action.is_preferred == Some(true),
        disabled: action.disabled.map(|d| d.reason),
        edits,
        file_ops,
        command: action.command.map(|c| (c.command, c.arguments)),
    }
}

/// A bare `Command` offered as an action — the legacy shape, which carries no edits and can
/// only be run.
pub fn command_action(cmd: types::Command) -> ActionEntry {
    ActionEntry {
        title: cmd.title,
        kind: String::new(),
        preferred: false,
        disabled: None,
        edits: Vec::new(),
        file_ops: Vec::new(),
        command: Some((cmd.command, cmd.arguments)),
    }
}

/// Signature help, reduced to the active signature.
pub fn signature_help(help: types::SignatureHelp) -> Option<SignatureText> {
    let count = help.signatures.len();
    if count == 0 {
        return None;
    }
    // Clamped, not indexed blindly: an `activeSignature` past the end would otherwise throw
    // away signatures the server did send.
    let active = (help.active_signature.unwrap_or(0) as usize).min(count - 1);
    let sig = help.signatures.into_iter().nth(active)?;
    // Per-signature `activeParameter` overrides the top-level one — the spec's precedence,
    // and the only one that is right when a server offers several overloads.
    let active_param = sig.active_parameter.or(help.active_parameter).map(|p| p as usize);

    let params: Vec<String> = sig
        .parameters
        .iter()
        .map(|p| match &p.label {
            types::ParameterLabel::Text(t) => t.clone(),
            // A span into the signature label. Sliced in UTF-16 units, because that is what
            // `labelOffsetSupport` means and what the editor will slice the label with.
            types::ParameterLabel::Range([a, b]) => slice_utf16(&sig.label, *a, *b),
        })
        .collect();

    let active_param_range = active_param
        .and_then(|i| sig.parameters.get(i))
        .and_then(|p| match &p.label {
            types::ParameterLabel::Range([a, b]) => Some((*a, *b)),
            types::ParameterLabel::Text(_) => None,
        });

    Some(SignatureText {
        label: sig.label,
        doc: sig.documentation.as_ref().map(|d| d.text().to_string()),
        params,
        active_param,
        active_param_range,
    })
}

/// `label[a..b]` where the bounds are UTF-16 code-unit offsets.
fn slice_utf16(label: &str, a: u32, b: u32) -> String {
    let mut units = 0u32;
    let mut out = String::new();
    for c in label.chars() {
        let next = units + c.len_utf16() as u32;
        if units >= a && next <= b {
            out.push(c);
        }
        units = next;
        if units >= b {
            break;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::line_index::Position;

    const UTF16: PositionEncoding = PositionEncoding::Utf16;

    fn range(l1: u32, c1: u32, l2: u32, c2: u32) -> Range {
        Range::new(Position::new(l1, c1), Position::new(l2, c2))
    }

    #[test]
    fn a_target_carries_its_line_col_and_preview() {
        let text = "fn a() {}\nfn beta() {}\n";
        let index = LineIndex::new(text);
        let t = span_target("/p/src/lib.rs", range(1, 3, 1, 7), &index, UTF16);
        assert_eq!(&text[t.start..t.end], "beta");
        assert_eq!((t.line, t.col), (2, 4), "1-based, UTF-16 columns");
        assert_eq!(t.preview, "fn beta() {}", "trimmed source line");
    }

    #[test]
    fn targets_keep_the_servers_order_across_files() {
        // Grouping by file must not reorder the answer: for references the server reports
        // document order, and a shuffled results list is a worse list.
        let raw = vec![
            ("file:///a.rs".to_string(), range(0, 0, 0, 1), range(0, 0, 0, 1)),
            ("file:///b.rs".to_string(), range(0, 0, 0, 1), range(0, 0, 0, 1)),
            ("file:///a.rs".to_string(), range(1, 0, 1, 1), range(1, 0, 1, 1)),
        ];
        let resolve = |_f: &str| Some("x\ny\n".to_string());
        let out = targets(raw, UTF16, &resolve);
        let files: Vec<&str> = out.iter().map(|t| t.file.as_str()).collect();
        assert_eq!(files, vec!["/a.rs", "/b.rs", "/a.rs"]);
    }

    #[test]
    fn an_unreadable_target_is_kept_rather_than_dropped() {
        // A reference the server found is a fact; dropping it under-reports the count.
        let raw = vec![("file:///gone.rs".to_string(), range(0, 0, 0, 1), range(0, 0, 0, 1))];
        let resolve = |_f: &str| None;
        let out = targets(raw, UTF16, &resolve);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].file, "/gone.rs");
        assert!(out[0].preview.is_empty());
    }

    #[test]
    fn a_non_file_uri_target_survives_with_its_uri() {
        // rust-analyzer answers with its own scheme for macro expansions.
        let raw = vec![(
            "rust-macro-expansion:///x".to_string(),
            range(0, 0, 0, 1),
            range(0, 0, 0, 1),
        )];
        let resolve = |_f: &str| None;
        let out = targets(raw, UTF16, &resolve);
        assert_eq!(out[0].file, "rust-macro-expansion:///x");
    }

    #[test]
    fn a_completion_prefers_the_server_edit_over_the_label() {
        // The label is a *display* string; inserting it verbatim is how a completion
        // produces text that does not compile.
        let text = "v.pu";
        let index = LineIndex::new(text);
        let item: types::CompletionItem = serde_json::from_str(
            r#"{"label":"push(…)","insertText":"push","textEdit":{"newText":"push($0)",
                "range":{"start":{"line":0,"character":2},"end":{"line":0,"character":4}}},
                "insertTextFormat":2,"kind":2}"#,
        )
        .unwrap();
        let e = completion_item(0, item, "/p/a.rs", &index, UTF16);
        // The edit's text wins over `insertText` and over the label — and, being a snippet, it
        // arrives already parsed: plain text plus the stop as a byte range into it. The editor
        // therefore never sees `${…}` and needs no parser of its own.
        assert_eq!(e.insert_text, "push()");
        assert!(e.is_snippet);
        assert_eq!(e.snippet_stops.len(), 1);
        assert_eq!((e.snippet_stops[0].start, e.snippet_stops[0].end), (5, 5));
        assert_eq!(e.replace, Some((2, 4)));
        assert_eq!(e.kind, "method");
    }

    /// A non-snippet body is left exactly as it is, `$` and all — the parse must not run on text
    /// the server never called a snippet.
    #[test]
    fn a_plain_completion_is_not_parsed_as_a_snippet() {
        let index = LineIndex::new("x");
        let item: types::CompletionItem =
            serde_json::from_str(r#"{"label":"cost","insertText":"cost_$1","kind":6}"#).unwrap();
        let e = completion_item(0, item, "/p/a.rs", &index, UTF16);
        assert_eq!(e.insert_text, "cost_$1", "no insertTextFormat means plain text");
        assert!(!e.is_snippet);
        assert!(e.snippet_stops.is_empty());
    }

    #[test]
    fn a_completion_falls_back_from_insert_text_to_the_label() {
        let index = LineIndex::new("x");
        let bare: types::CompletionItem = serde_json::from_str(r#"{"label":"len"}"#).unwrap();
        assert_eq!(completion_item(0, bare, "/a.rs", &index, UTF16).insert_text, "len");

        let with_insert: types::CompletionItem =
            serde_json::from_str(r#"{"label":"len()","insertText":"len"}"#).unwrap();
        assert_eq!(completion_item(0, with_insert, "/a.rs", &index, UTF16).insert_text, "len");
    }

    #[test]
    fn an_auto_import_edit_survives_as_an_additional_edit() {
        // Dropping it leaves an accepted completion referring to a type that is not in scope.
        let index = LineIndex::new("fn main() {}\n");
        let item: types::CompletionItem = serde_json::from_str(
            r#"{"label":"HashMap","additionalTextEdits":[{"newText":"use std::collections::HashMap;\n",
                "range":{"start":{"line":0,"character":0},"end":{"line":0,"character":0}}}]}"#,
        )
        .unwrap();
        let e = completion_item(0, item, "/p/a.rs", &index, UTF16);
        assert_eq!(e.additional_edits.len(), 1);
        assert_eq!(e.additional_edits[0].file, "/p/a.rs");
        assert_eq!(e.additional_edits[0].start, 0);
    }

    #[test]
    fn label_details_fill_in_when_detail_is_absent() {
        let index = LineIndex::new("x");
        let item: types::CompletionItem = serde_json::from_str(
            r#"{"label":"from_str","labelDetails":{"detail":"(s: &str)","description":"std::str"}}"#,
        )
        .unwrap();
        let e = completion_item(0, item, "/a.rs", &index, UTF16);
        assert_eq!(e.detail.as_deref(), Some("(s: &str) std::str"));
    }

    #[test]
    fn an_empty_hover_card_is_no_card() {
        let index = LineIndex::new("x");
        let empty: types::Hover = serde_json::from_str(r#"{"contents":"   "}"#).unwrap();
        assert!(hover(empty, &index, UTF16).is_none(), "an empty tooltip reads as broken");
    }

    #[test]
    fn the_symbol_tree_keeps_its_hierarchy_and_lands_on_the_name() {
        let text = "struct Foo {\n    bar: u32,\n}\n";
        let index = LineIndex::new(text);
        let syms: Vec<types::DocumentSymbol> = serde_json::from_str(
            r#"[{"name":"Foo","kind":23,
                "range":{"start":{"line":0,"character":0},"end":{"line":2,"character":1}},
                "selectionRange":{"start":{"line":0,"character":7},"end":{"line":0,"character":10}},
                "children":[{"name":"bar","kind":8,
                  "range":{"start":{"line":1,"character":4},"end":{"line":1,"character":13}},
                  "selectionRange":{"start":{"line":1,"character":4},"end":{"line":1,"character":7}}}]}]"#,
        )
        .unwrap();
        let tree = symbol_tree(syms, "/p/a.rs", &index, UTF16, "rust");
        assert_eq!(tree[0].kind, "struct");
        assert_eq!(&text[tree[0].name_start..tree[0].name_end], "Foo");
        assert_eq!(tree[0].children[0].kind, "field");
        assert_eq!(&text[tree[0].children[0].name_start..tree[0].children[0].name_end], "bar");
    }

    #[test]
    fn a_flat_symbol_folds_in_its_container() {
        // Forty methods called `new` are indistinguishable without it.
        let syms: Vec<types::SymbolInformation> = serde_json::from_str(
            r#"[{"name":"new","kind":6,"containerName":"Foo","location":{"uri":"file:///a.rs",
                "range":{"start":{"line":0,"character":0},"end":{"line":0,"character":3}}}}]"#,
        )
        .unwrap();
        let resolve = |_f: &str| Some("new".to_string());
        let out = flat_symbols(syms, UTF16, &resolve, "rust");
        assert_eq!(out[0].name, "Foo::new");
    }

    #[test]
    fn a_diagnostic_converts_its_span_and_keeps_a_replayable_copy() {
        let text = "let x: u32 = \"s\";\n";
        let index = LineIndex::new(text);
        let d: types::Diagnostic = serde_json::from_str(
            r#"{"message":"mismatched types","severity":1,"code":"E0308","source":"rustc",
                "data":{"rendered":"..."},
                "range":{"start":{"line":0,"character":13},"end":{"line":0,"character":16}}}"#,
        )
        .unwrap();
        let resolve = |_f: &str| None;
        let e = diagnostic(&d, &index, UTF16, &resolve);
        assert_eq!(&text[e.start..e.end], "\"s\"");
        assert_eq!(e.severity, "error");
        assert_eq!(e.code, "E0308");
        assert_eq!(e.source, "rustc");
        // The opaque `data` must survive: a quick fix cannot be produced without it.
        assert_eq!(e.raw["data"]["rendered"], serde_json::json!("..."));
        assert_eq!(e.raw["range"]["start"]["character"], serde_json::json!(13));
    }

    #[test]
    fn an_unnecessary_tag_is_kept_so_the_editor_can_dim_instead_of_underline() {
        let index = LineIndex::new("use std::x;\n");
        let d: types::Diagnostic = serde_json::from_str(
            r#"{"message":"unused import","severity":2,"tags":[1],
                "range":{"start":{"line":0,"character":0},"end":{"line":0,"character":11}}}"#,
        )
        .unwrap();
        let resolve = |_f: &str| None;
        assert!(diagnostic(&d, &index, UTF16, &resolve).unnecessary);
    }

    #[test]
    fn an_absent_severity_is_a_warning_not_an_error() {
        assert_eq!(severity_name(None), "warning");
        assert_eq!(severity_name(Some(1)), "error");
        assert_eq!(severity_name(Some(4)), "hint");
        assert_eq!(severity_name(Some(99)), "warning");
    }

    #[test]
    fn related_information_becomes_targets_paired_with_its_messages() {
        let index = LineIndex::new("a\nb\n");
        let d: types::Diagnostic = serde_json::from_str(
            r#"{"message":"borrow error","severity":1,
                "range":{"start":{"line":0,"character":0},"end":{"line":0,"character":1}},
                "relatedInformation":[
                  {"message":"first borrow here","location":{"uri":"file:///a.rs",
                   "range":{"start":{"line":1,"character":0},"end":{"line":1,"character":1}}}}]}"#,
        )
        .unwrap();
        let resolve = |_f: &str| Some("a\nb\n".to_string());
        let e = diagnostic(&d, &index, UTF16, &resolve);
        assert_eq!(e.related.len(), 1);
        assert_eq!(e.related[0].1, "first borrow here");
        assert_eq!(e.related[0].0.line, 2);
    }

    #[test]
    fn a_workspace_edit_converts_both_forms() {
        let resolve = |_f: &str| Some("aaa\nbbb\n".to_string());

        let simple: types::WorkspaceEdit = serde_json::from_str(
            r#"{"changes":{"file:///a.rs":[{"newText":"X",
                "range":{"start":{"line":0,"character":1},"end":{"line":0,"character":2}}}]}}"#,
        )
        .unwrap();
        let (edits, ops) = workspace_edit(&simple, UTF16, &resolve);
        assert_eq!(edits.len(), 1);
        assert_eq!((edits[0].start, edits[0].end), (1, 2));
        assert!(ops.is_empty());

        let versioned: types::WorkspaceEdit = serde_json::from_str(
            r#"{"documentChanges":[
                {"textDocument":{"uri":"file:///a.rs","version":1},
                 "edits":[{"newText":"Y","range":{"start":{"line":1,"character":0},
                           "end":{"line":1,"character":1}}}]},
                {"kind":"rename","oldUri":"file:///old.rs","newUri":"file:///new.rs"}]}"#,
        )
        .unwrap();
        let (edits, ops) = workspace_edit(&versioned, UTF16, &resolve);
        assert_eq!(edits.len(), 1);
        assert_eq!(edits[0].start, 4, "line 1 starts at byte 4");
        // The file move is surfaced, not silently dropped: without it the rename is half
        // applied and the project stops compiling.
        assert_eq!(
            ops,
            vec![FileOp::Rename { from: "/old.rs".into(), to: "/new.rs".into() }]
        );
    }

    #[test]
    fn document_changes_win_when_a_server_sends_both() {
        let resolve = |_f: &str| Some("abcdef".to_string());
        let both: types::WorkspaceEdit = serde_json::from_str(
            r#"{"changes":{"file:///a.rs":[{"newText":"WRONG",
                 "range":{"start":{"line":0,"character":0},"end":{"line":0,"character":1}}}]},
                "documentChanges":[{"textDocument":{"uri":"file:///a.rs","version":1},
                 "edits":[{"newText":"RIGHT","range":{"start":{"line":0,"character":2},
                           "end":{"line":0,"character":3}}}]}]}"#,
        )
        .unwrap();
        let (edits, _) = workspace_edit(&both, UTF16, &resolve);
        assert_eq!(edits.len(), 1, "not applied twice");
        assert_eq!(edits[0].new_text, "RIGHT");
    }

    #[test]
    fn the_changes_map_is_ordered_by_path_for_a_stable_preview() {
        let resolve = |_f: &str| Some("xy".to_string());
        let e: types::WorkspaceEdit = serde_json::from_str(
            r#"{"changes":{
                "file:///z.rs":[{"newText":"1","range":{"start":{"line":0,"character":0},
                  "end":{"line":0,"character":1}}}],
                "file:///a.rs":[{"newText":"2","range":{"start":{"line":0,"character":0},
                  "end":{"line":0,"character":1}}}]}}"#,
        )
        .unwrap();
        let (edits, _) = workspace_edit(&e, UTF16, &resolve);
        let files: Vec<&str> = edits.iter().map(|x| x.file.as_str()).collect();
        assert_eq!(files, vec!["/a.rs", "/z.rs"], "a HashMap has no order; the preview needs one");
    }

    #[test]
    fn signature_help_slices_the_active_parameter_out_of_the_label() {
        let help: types::SignatureHelp = serde_json::from_str(
            r#"{"signatures":[{"label":"fn insert(k: K, v: V)","parameters":[
                {"label":[10,14]},{"label":[16,20]}]}],"activeParameter":1}"#,
        )
        .unwrap();
        let s = signature_help(help).unwrap();
        assert_eq!(s.params, vec!["k: K", "v: V"]);
        assert_eq!(s.active_param, Some(1));
        assert_eq!(s.active_param_range, Some((16, 20)));
    }

    #[test]
    fn a_per_signature_active_parameter_wins_over_the_top_level_one() {
        // The spec's precedence, and the only one that is right across overloads.
        let help: types::SignatureHelp = serde_json::from_str(
            r#"{"signatures":[{"label":"f(a, b)","activeParameter":0,"parameters":[
                {"label":"a"},{"label":"b"}]}],"activeParameter":1}"#,
        )
        .unwrap();
        assert_eq!(signature_help(help).unwrap().active_param, Some(0));
    }

    #[test]
    fn a_utf16_label_span_slices_correctly_around_an_astral_char() {
        // 😀 is two UTF-16 units; a byte-based slice would cut it in half.
        assert_eq!(slice_utf16("a😀bc", 3, 5), "bc");
        assert_eq!(slice_utf16("a😀bc", 0, 1), "a");
    }

    #[test]
    fn a_code_action_carries_its_edits_and_its_command() {
        let resolve = |_f: &str| Some("abc".to_string());
        let action: types::CodeAction = serde_json::from_str(
            r#"{"title":"Import HashMap","kind":"quickfix","isPreferred":true,
                "edit":{"changes":{"file:///a.rs":[{"newText":"use x;",
                  "range":{"start":{"line":0,"character":0},"end":{"line":0,"character":0}}}]}},
                "command":{"title":"t","command":"rust-analyzer.x","arguments":[1]}}"#,
        )
        .unwrap();
        let e = code_action(action, UTF16, &resolve);
        assert_eq!(e.title, "Import HashMap");
        assert!(e.preferred);
        assert_eq!(e.edits.len(), 1);
        assert_eq!(e.command.unwrap().0, "rust-analyzer.x");
    }

    #[test]
    fn a_disabled_action_keeps_its_reason() {
        // Shown greyed rather than hidden: "cannot extract: selection crosses a block" is
        // information, and a silently missing action is not.
        let resolve = |_f: &str| None;
        let action: types::CodeAction = serde_json::from_str(
            r#"{"title":"Extract function","disabled":{"reason":"selection crosses a block"}}"#,
        )
        .unwrap();
        let e = code_action(action, UTF16, &resolve);
        assert_eq!(e.disabled.as_deref(), Some("selection crosses a block"));
    }
}
