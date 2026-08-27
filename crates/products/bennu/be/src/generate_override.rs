//! `generate_override` domain — "Implement / override methods": what can be overridden here, and
//! the edits that write the chosen ones.
//!
//! Two handlers because the dialog is a conversation. [`bennu_overridable_members`] answers "what
//! could this class override", the user ticks some, and [`bennu_generate_overrides`] turns the
//! selection into edits. The selection travels back whole rather than as indices into the first
//! answer: the buffer can have changed between the two calls, and an index into a list computed
//! against different text is how a generator writes the wrong method.
//!
//! The work itself is split the way the rest of Bennu is. Which methods are overridable needs the
//! supertype hierarchy → [`bennu_query::prelude::overridable_at`]. What a method looks like, and
//! where in the class body it goes, is a string transform → `bennu-intentions`. This module is the
//! glue and the import handling.
//!
//! ## Two edits, applied back to front
//!
//! A generated method usually mentions types the file does not import — `List`, `SQLException`,
//! whatever the supertype's signature named. Generating the method without them produces code that
//! does not compile, so the imports come back in the same answer: one edit inserting the methods
//! just inside the class brace, and one per import.
//!
//! They are returned **sorted by descending start offset**, so a caller applies them in order
//! without remapping anything — every edit is above the ones already applied.

use bennu_core::prelude::BennuState;
use serde::{Deserialize, Serialize};

use crate::index_service::IndexService;

/// Args for both handlers: the buffer and where the caret is in it.
#[derive(Deserialize)]
pub struct OverridableArgs {
    pub file: String,
    pub source: String,
    /// Caret position as a **UTF-8 byte offset** into `source`.
    pub offset: usize,
}

/// One overridable method, on the wire.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OverridableWire {
    pub name: String,
    /// `[type, name]` pairs in declaration order.
    pub params: Vec<(String, String)>,
    pub return_type: String,
    pub visibility: String,
    /// The compiler will demand this one — the dialog ticks these by default.
    pub is_abstract: bool,
    pub throws: Vec<String>,
    /// Dotted FQCN of the declaring type — the dialog's grouping.
    pub declaring_type: String,
    /// A readable one-line signature for the row.
    pub signature: String,
    /// Binary names of every type the generated method mentions, so the answer can carry the
    /// imports it needs.
    pub types: Vec<String>,
}

impl From<bennu_query::prelude::Overridable> for OverridableWire {
    fn from(o: bennu_query::prelude::Overridable) -> Self {
        Self {
            name: o.name,
            params: o.params,
            return_type: o.return_type,
            visibility: o.visibility,
            is_abstract: o.is_abstract,
            throws: o.throws,
            declaring_type: o.declaring_type,
            signature: o.signature,
            types: o.types,
        }
    }
}

/// Every method the class under the caret could override, abstract ones first.
///
/// Empty when the caret is not inside a class, no project owns the file, or the index is still
/// building — all benign, and the dialog says it has nothing rather than guessing.
#[arbor_rpc::handler]
fn bennu_overridable_members(
    _ctx: &BennuState,
    args: OverridableArgs,
) -> Result<Vec<OverridableWire>, String> {
    Ok(IndexService::global()
        .overridable_at(&args.file, &args.source, args.offset)
        .into_iter()
        .map(OverridableWire::from)
        .collect())
}

/// Args for [`bennu_generate_overrides`] — the buffer, the caret, and what the user ticked.
///
/// No `file`, unlike its sibling: generating needs the text and the caret and nothing else. The
/// project was already consulted when the list was produced, and asking for a path this handler
/// cannot use would be a field the caller has to be right about for no reason.
#[derive(Deserialize)]
pub struct GenerateOverridesArgs {
    pub source: String,
    pub offset: usize,
    /// The chosen methods, sent back whole (see the module docs on why not indices).
    pub selected: Vec<OverridableWire>,
}

/// A byte range in the requested buffer to replace with `replacement`.
#[derive(Debug, Clone, Serialize)]
pub struct GeneratedEdit {
    pub start: usize,
    pub end: usize,
    pub replacement: String,
}

/// Write the selected overrides into the class under the caret.
///
/// Returns the edits **newest-position-first** so they can be applied in order (see the module
/// docs). An empty selection, or a caret with no class body around it, returns no edits rather
/// than an error: there is nothing wrong, there is simply nothing to write.
#[arbor_rpc::handler]
fn bennu_generate_overrides(
    _ctx: &BennuState,
    args: GenerateOverridesArgs,
) -> Result<Vec<GeneratedEdit>, String> {
    use bennu_intentions::prelude::{class_body_insertion, render_override, OverrideSpec};

    if args.selected.is_empty() {
        return Ok(Vec::new());
    }
    let Some((insert_at, indent)) = class_body_insertion(&args.source, args.offset) else {
        return Ok(Vec::new());
    };

    let bodies: Vec<String> = args
        .selected
        .iter()
        .map(|o| {
            render_override(
                &OverrideSpec {
                    name: o.name.clone(),
                    params: o.params.clone(),
                    return_type: o.return_type.clone(),
                    visibility: o.visibility.clone(),
                    is_abstract: o.is_abstract,
                    throws: o.throws.clone(),
                },
                &indent,
            )
        })
        .collect();

    // A blank line between methods and one before the first, so the result reads like code
    // somebody wrote rather than like output. Built with `\n` throughout and converted once at the
    // end — the renderer knows nothing about the file's newline style and should not have to.
    let block = format!("\n{}\n", bodies.join("\n\n"));
    let replacement = if args.source.contains("\r\n") {
        block.replace('\n', "\r\n")
    } else {
        block
    };
    let mut edits = vec![GeneratedEdit { start: insert_at, end: insert_at, replacement }];

    // The imports the new methods need. Computed against the ORIGINAL buffer — they sit above the
    // insertion point, so applying the methods first leaves these offsets untouched.
    for fqn in imports_needed(&args.selected) {
        if let Some((start, end, replacement)) = crate::intentions::import_edit_for(&args.source, &fqn) {
            edits.push(GeneratedEdit { start, end, replacement });
        }
    }

    // Back to front: every edit is above the ones already applied, so no offset needs remapping.
    edits.sort_by(|a, b| b.start.cmp(&a.start));
    Ok(edits)
}

/// The distinct dotted FQNs the selected methods mention, in a stable order.
///
/// Deduped here rather than per method: two overrides returning `List` must not produce two
/// `import java.util.List;` lines, and `import_edit_for` cannot see the one it is about to add.
fn imports_needed(selected: &[OverridableWire]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for o in selected {
        for binary in &o.types {
            // Nested types are imported by their OUTER name — `import a.b.Outer;` is what makes
            // `Outer.Inner` resolve, and `import a.b.Outer.Inner;` is a different (legal but
            // unusual) thing that the simple name in the signature does not match anyway.
            let fqn = binary.replace('/', ".");
            if !out.contains(&fqn) {
                out.push(fqn);
            }
        }
    }
    // Whether a name actually NEEDS importing — java.lang, the file's own package, an existing
    // wildcard — is `import_edit_for`'s single answer, asked per name by the caller. Sorting only
    // keeps the inserted lines deterministic.
    out.sort();
    out
}
