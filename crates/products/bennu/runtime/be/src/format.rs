//! `format` domain — `bennu_format`: reformat a buffer, whoever knows how.
//!
//! One handler over both engines, because "format this file" is one question. A language with a
//! server is formatted by its server (`rustfmt` for Rust); Java is formatted by Bennu's own
//! formatter, since Bennu *is* the Java engine and there is no server to ask.
//!
//! The **style** splits the same way. A server reads the project's own file — `rustfmt.toml`,
//! `.clang-format`, `.prettierrc` — and all it is told from here is the indentation, because that
//! is what the editor is showing; duplicating the rest in Bennu's settings would create a second
//! truth that the CI does not read. Java has no such file, so its style *is* Bennu's settings.
//!
//! The editor calls this and never has to know which of the two answered — which is what stopped
//! Ctrl+Alt+L saying "no formatter for this file type" on the one language the product exists for.
//!
//! Edits rather than formatted text, in both directions: the editor applies them through CodeMirror
//! so the format lands in the undo history as one step and the caret keeps its place.

use bennu_core::prelude::BennuState;
use bennu_proto::prelude::SourceEdit;
use serde::Deserialize;

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
}

/// Reformat `source`. Empty when it is already formatted, or when nothing can format this file.
#[arbor_rpc::handler]
fn bennu_format(_ctx: &BennuState, args: FormatArgs) -> Result<Vec<SourceEdit>, String> {
    let cfg = bennu_core::config::load();
    let tab_size = args.tab_size.unwrap_or(cfg.indent_width).max(1) as usize;
    let spaces = args.insert_spaces.unwrap_or(!cfg.indent_with_tabs);

    if crate::intel::is_java_file(&args.file) {
        // The whole style comes from the profile. The two the caller may override are the two the
        // editor knows better than the config does — it is the surface the user is typing in, and
        // its indentation is what the formatter has to agree with.
        let style = bennu_intentions::prelude::FormatStyle {
            indent_width: tab_size,
            use_tabs: !spaces,
            max_blank_lines: cfg.java_max_blank_lines,
            indent_case_body: cfg.java_indent_case_body,
        };
        return Ok(bennu_intentions::prelude::format_edits(&args.source, style)
            .into_iter()
            .map(|e| SourceEdit {
                file: args.file.clone(),
                start: e.start,
                end: e.end,
                new_text: e.replacement,
            })
            .collect());
    }
    Ok(crate::lsp_route::format(&args.file, &args.source, tab_size as u32, spaces))
}
