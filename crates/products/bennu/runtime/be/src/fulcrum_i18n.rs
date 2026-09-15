//! `fulcrum_i18n` domain — `bennu_i18n_studio`.
//!
//! The one verb the editor's i18n panel needs, and the one call that could not travel the framework
//! seam: everything it answers is in fulcrum's own vocabulary — a parsed markup tree, the
//! stylesheet, the same label in the other languages — where the seam's positional verbs all answer
//! in the shared one (a target, a hover card, a completion). See
//! [`fulcrum_of_file`](crate::frameworks::fulcrum_of_file) for why that handle exists rather than a
//! sixth trait method.
//!
//! ## Why the parse happens here and not in the frontend
//!
//! The panel re-asks on a debounce as the value is typed, which looks like the kind of thing a
//! frontend should answer locally. It is not: the markup grammar already has two parsers — the
//! engine's, which is authoritative, and `bennu-fulcrum-i18n`'s, which differs from it deliberately
//! and is pinned against it by tests. A third one in TypeScript would be a copy nothing checks, and
//! the way it would fail is the worst available: the preview would agree with the editor's colouring
//! and both would disagree with what the engine renders. So the tree crosses the seam already parsed,
//! and the frontend draws it.
//!
//! ## Why the answer says why it is empty
//!
//! This used to return a bare `Option`, and an empty panel was then indistinguishable between four
//! situations: the file is not a bundle, the project has no i18n model at all, the text never
//! arrived, and the caret is genuinely not on a value. Only the last is normal, and the panel was
//! reporting all four as the last — telling the user to put the caret on a translation while their
//! caret was on one. Three of the four are answerable cheaply and without the extension, so they are
//! answered.

use bennu_core::prelude::BennuState;
use bennu_fulcrum_i18n::prelude::{bundle_of, live_values, StudioView};
use bennu_project::prelude::normalize_newlines;
use serde::{Deserialize, Serialize};

/// Args for [`bennu_i18n_studio`].
#[derive(Deserialize)]
pub struct StudioArgs {
    /// Absolute path to the bundle file.
    pub file: String,
    /// The live buffer. Absent → read from disk, which is what a jump from the Labels panel sends.
    #[serde(default)]
    pub source: Option<String>,
    /// Caret byte offset.
    #[serde(default)]
    pub offset: usize,
}

/// What the panel draws, and — when there is nothing to draw — which link was missing.
#[derive(Debug, Clone, Serialize)]
pub struct StudioAnswer {
    /// The translation under the caret. `None` is the ordinary answer: most carets in a bundle are on
    /// a table header or a blank line.
    pub view: Option<StudioView>,
    /// Whether the path is `i18n/<lang>/<category>.toml` at all. Decided from the path alone, so it
    /// is always answered.
    pub bundle: bool,
    /// Whether any open project owns this file. `false` for a file opened from outside every project
    /// root, which nothing else here can be true of.
    pub project: bool,
    /// The owning project's root, or empty. The frontend needs it to ask for a rescan.
    pub root: String,
    /// Whether that project carries the fulcrum i18n model.
    ///
    /// `false` means no `i18n/languages.toml` was found **when the project was scanned** — and the
    /// emphasis is load-bearing: capabilities are detected once, lazily, and cached for the life of
    /// the project's slot, so a bundle tree created after the project was opened is invisible until
    /// something rebuilds it. That is why the answer carries `root`: the panel can offer the rescan
    /// instead of being a dead end.
    pub model: bool,
    /// How many translations the **buffer** parsed into.
    ///
    /// The number that makes an empty panel diagnosable: `0` on a file plainly full of them means the
    /// text never arrived or is not being parsed, and no amount of moving the caret will help.
    pub translations: usize,
}

/// What the i18n panel draws for the caret at `offset`.
///
/// Never errors on "nothing here" — see [`StudioAnswer`] for what it says instead. Reading the file
/// failing is reported the same way, as a bundle with no translations in it.
#[arbor_rpc::handler]
fn bennu_i18n_studio(_ctx: &BennuState, args: StudioArgs) -> Result<StudioAnswer, String> {
    let path = args.file.replace('\\', "/");
    let bundle = bundle_of(&path).is_some();

    let source = match args.source {
        Some(s) => Some(s),
        // Normalised for the same reason the scan normalises: every offset handed back is used
        // against the editor's buffer, which was normalised on read. Leave the `\r`s in and each one
        // before the caret shifts the value's span by a byte.
        None => std::fs::read(&args.file)
            .ok()
            .map(|bytes| normalize_newlines(&String::from_utf8_lossy(&bytes))),
    };
    // The framework host's own registry, not the Java symbol index's — that one is only populated for
    // a Java project, which is what made this answer `false` on every Cargo root.
    let root = crate::frameworks::FrameworkService::global()
        .root_for_file(&args.file)
        .unwrap_or_default();
    let project = !root.is_empty();

    let Some(source) = source else {
        return Ok(StudioAnswer {
            view: None,
            bundle,
            project,
            root,
            model: false,
            translations: 0,
        });
    };

    // Both of these are pure functions of the path and the text, so they answer whether or not the
    // project carries the extension — which is precisely what makes "no model" distinguishable from
    // "no translations" from "not on one".
    let translations = if bundle { live_values(&source).len() } else { 0 };

    let ext = crate::frameworks::fulcrum_of_file(&args.file);
    let model = ext.is_some();
    let view = ext.and_then(|ext| ext.studio(&path, &source, args.offset));

    Ok(StudioAnswer { view, bundle, project, root, model, translations })
}
