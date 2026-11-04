//! `format` domain — `bennu_format`: reformat a buffer, whoever knows how.
//!
//! One handler over both engines, because "format this file" is one question. A language with a
//! server is formatted by its server (`rustfmt` for Rust); Java is formatted by Bennu's own
//! formatter, since Bennu *is* the Java engine and there is no server to ask.
//!
//! The **style** splits the same way. A server reads the project's own file — `rustfmt.toml`,
//! `.clang-format`, `.prettierrc` — and all it is told from here is the indentation, because that
//! is what the editor is showing; duplicating the rest in Bennu's settings would create a second
//! truth that the CI does not read. Java has no such file of its own, so its style is built from
//! Bennu's settings, the project's `.editorconfig` and the project override — see `format_style`.
//!
//! Edits rather than formatted text, in both directions: the editor applies them through CodeMirror
//! so the format lands in the undo history as one step and the caret keeps its place.

use bennu_core::prelude::BennuState;
use bennu_proto::prelude::SourceEdit;
use serde::Deserialize;

/// A selection, as UTF-8 byte offsets into `source`.
#[derive(Deserialize)]
pub struct ByteRange {
    pub start: usize,
    pub end: usize,
}

/// Args for [`bennu_format`].
#[derive(Deserialize)]
pub struct FormatArgs {
    pub file: String,
    pub source: String,
    /// Indent width. Absent → the editor's configured `indent_width`.
    #[serde(default)]
    pub tab_size: Option<u32>,
    /// Absent → spaces, matching Bennu's own editor default.
    #[serde(default)]
    pub insert_spaces: Option<bool>,
    /// Format only these lines (widened to whole lines). Java only; a server formats the whole file.
    #[serde(default)]
    pub range: Option<ByteRange>,
}

/// Reformat `source`. Empty when it is already formatted, or when nothing can format this file.
#[arbor_rpc::handler]
fn bennu_format(_ctx: &BennuState, args: FormatArgs) -> Result<Vec<SourceEdit>, String> {
    let cfg = bennu_core::config::load();
    let tab_size = args.tab_size.unwrap_or(cfg.indent_width).max(1) as usize;
    let spaces = args.insert_spaces.unwrap_or(!cfg.indent_with_tabs);

    if crate::intel::is_java_file(&args.file) {
        let root = crate::index_service::IndexService::global().root_for_file(&args.file);
        let legacy = crate::format_style::legacy_layer(args.tab_size, args.insert_spaces);
        let style = crate::format_style::resolve(root.as_deref(), Some(&args.file), &legacy);
        let edits = match &args.range {
            Some(r) => bennu_intentions::prelude::format_range(&args.source, r.start, r.end, &style),
            None => bennu_intentions::prelude::format_edits(&args.source, &style),
        };
        return Ok(edits
            .into_iter()
            .map(|e| SourceEdit { file: args.file.clone(), start: e.start, end: e.end, new_text: e.replacement })
            .collect());
    }
    Ok(crate::lsp_route::format(&args.file, &args.source, tab_size as u32, spaces))
}
