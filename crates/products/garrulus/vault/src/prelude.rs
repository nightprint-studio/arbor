//! Canonical entry point for `garrulus-vault`'s public API.
//!
//! Workspace convention: call sites reach this crate through
//! `garrulus_vault::prelude::...` (or a single `use garrulus_vault::prelude::*;`).
//! The submodules stay `pub` for rustdoc navigation, but the diff always goes
//! through here.
//!
//! The verb-heavy modules are re-exported whole rather than function by function,
//! so a call site reads as `trash::list(root)` and `template::expand(…)` — the
//! verb stays grouped with its domain, which is what keeps `list`, `restore` and
//! `empty` from becoming ambiguous the moment two modules grow one.

pub use crate::builtin::{
    builtin_types, install_builtin_types, BUG, BUILTIN_TYPE_FILES, DAILY, DECISION, GAMEDESIGN,
    IMPROVEMENT, MEETING, SNIPPET,
};
pub use crate::config::{
    config_path, marker_dir, trash_dir, types_dir, DailySettings, LinkStyle, VaultConfig,
    CURRENT_VERSION, MARKER_RELATIVE_PATH, TRASH_RELATIVE_PATH, TYPES_RELATIVE_PATH,
    VAULT_CONFIG_RELATIVE_PATH,
};
pub use crate::discovery::{find_upward, is_excluded, scan_notes, Vault};
pub use crate::error::{VaultError, VaultResult};
pub use crate::extract::{extract, inline_text, is_external, Extracted};
pub use crate::naming::{
    ensure_note_extension, file_name, sanitize_file_name, unique_name, NOTE_EXTENSION,
};
pub use crate::note::{
    front_list, front_scalar, front_tags, parse_note, read_note, write_note, Heading, Link, Note,
    NoteId, Tag, Task,
};
pub use crate::note_type::{
    classify, classify_with, load_types, save_type, FieldKind, FieldSpec, NoteLayout, NoteType,
    TypeId,
};
pub use crate::path::{
    contains, glob_matches, last_segment, parent_of, path_str, self_and_ancestors, to_rel, RelPath,
};
pub use crate::template::{
    civil_from_unix, expand, render_frontmatter, render_note, render_template,
    render_template_with_cursor, slugify, TemplateCtx, CURSOR,
};
pub use crate::trash::{entry_id, trash_note, TrashedNote};

// Reachable as `garrulus_vault::prelude::<module>::fn` too — see the module note.
// `extract` is absent from this list deliberately: the module and its main
// function share a name, and re-exporting both here would read as a typo.
pub use crate::{builtin, config, discovery, naming, note, note_type, path, template, trash};

// Re-exported so a consumer working in vault terms does not have to name the
// leaf crate for the document types that are unavoidably part of this
// vocabulary: a `Note` carries a `Frontmatter`, and every `Span` on it indexes
// the same source bytes.
pub use garrulus_ast::prelude::{Document, FrontValue, Frontmatter, Span};
