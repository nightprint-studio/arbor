//! `types` domain — note types: list them, apply one, render one's template.
//!
//! A note type is a first-class object living **inside** the vault
//! (`<vault>/.arbor/garrulus/types/*.toml`), so it syncs to the other machine with
//! the notes. `garrulus-vault` owns what one is and how a note is matched to one;
//! this domain is the three verbs the frontend needs.

use garrulus_core::prelude::{
    civil_from_unix, file_name, hooks, render_template, GarrulusState, NoteType, TemplateCtx,
    TypeId, Vault,
};
use serde::Serialize;
use serde_json::json;

use crate::frontmatter;
use crate::note;
use crate::vault_io;

/// A note the frontend can create: where it would go and what it would contain.
/// Nothing is written — `garrulus_create_note` does that once the user confirms,
/// which is what makes "new note of type X" previewable.
#[derive(Debug, Clone, Serialize)]
pub struct RenderedNote {
    /// The type this came from.
    pub type_id: String,
    /// Proposed vault-relative path, from the type's `folder` + `naming` pattern.
    pub path: String,
    /// The rendered template body, `{{cursor}}` included — the editor consumes
    /// that marker when it places the caret.
    pub text: String,
}

/// Every note type declared in the open vault.
#[arbor_rpc::handler]
fn garrulus_list_types(state: &GarrulusState) -> Result<Vec<NoteType>, String> {
    vault_io::with_vault(state, |v| Ok(v.types.clone()))
}

/// Tag an existing note as being of a type, by setting its frontmatter `type`.
///
/// Deliberately additive: it does not rewrite the body, insert the template's
/// headings, or reorder the frontmatter. Promoting a note to a type must never
/// touch what the user already wrote.
#[arbor_rpc::handler]
fn garrulus_apply_type(
    state: &GarrulusState,
    path: String,
    type_id: String,
) -> Result<(), String> {
    let root = state.vault_root()?;
    vault_io::with_vault(state, |v| find_type(v, &type_id).map(|_| ()))?;

    let source = vault_io::read_source(&root, &path)?;
    let updated = frontmatter::set_key(&source, "type", &type_id);
    if updated != source {
        vault_io::write_source(&root, &path, &updated)?;
        note::reindex(state, &path)?;
    }
    state.fire_hook(hooks::TYPE_APPLIED, json!({ "path": path, "type": type_id }));
    Ok(())
}

/// Render a type's template for a new note with the given title, and propose the
/// path it would be filed at.
#[arbor_rpc::handler]
fn garrulus_render_template(
    state: &GarrulusState,
    type_id: String,
    title: String,
) -> Result<RenderedNote, String> {
    vault_io::with_vault(state, |v| {
        let note_type = find_type(v, &type_id)?;
        // The vault crate never reads a clock on its own — the date and time are
        // the caller's to supply, which is what keeps `render_template` pure.
        let (date, time) = civil_from_unix(vault_io::now_ms() / 1000);
        let ctx = TemplateCtx::new(&title, date, time);
        Ok(RenderedNote {
            type_id: type_id.clone(),
            path: proposed_path(note_type, &ctx),
            text: render_template(note_type, &ctx),
        })
    })
}

/// The type with this id, or a message naming what the vault does have — a typo
/// in a type id is otherwise an error with nowhere to go.
fn find_type<'v>(vault: &'v Vault, type_id: &str) -> Result<&'v NoteType, String> {
    // `TypeId::new` trims and lowercases; comparing the raw string instead would
    // silently miss `"Bug"`.
    vault.note_type(&TypeId::new(type_id)).ok_or_else(|| {
        let known: Vec<&str> = vault.types.iter().map(|t| t.id.as_str()).collect();
        format!("unknown note type '{type_id}' (this vault has: {})", known.join(", "))
    })
}

/// Where a new note of this type is filed: the type's folder plus its naming
/// pattern, with the `.md` the pattern is not obliged to spell out.
fn proposed_path(note_type: &NoteType, ctx: &TemplateCtx) -> String {
    let mut name = file_name(&note_type.naming, ctx);
    if !name.to_lowercase().ends_with(".md") {
        name.push_str(".md");
    }
    let folder = note_type.folder.trim_matches('/');
    if folder.is_empty() {
        name
    } else {
        format!("{folder}/{name}")
    }
}
