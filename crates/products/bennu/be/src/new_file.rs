//! `new_file` domain — `bennu_new_file`: scaffold a new file's name + initial content for the
//! project-tree "New…" menu.
//!
//! Thin wrapper over [`bennu_java::prelude::scaffold_new_file`] (the pure, tested scaffolder):
//! resolves the final path under the target dir + whether something is already there (the FE
//! refuses to overwrite). The FE writes the returned content (encoding-aware) and opens it.

use std::path::Path;

use bennu_core::prelude::BennuState;
use bennu_java::prelude::{scaffold_new_file, NewFileKind, ScaffoldResult};
use serde::{Deserialize, Serialize};

/// Args for [`bennu_new_file`].
#[derive(Deserialize)]
pub struct NewFileArgs {
    /// Target directory (absolute, forward slashes).
    pub dir: String,
    /// The base name the user entered (extension optional).
    pub name: String,
    /// One of `class|interface|enum|record|annotation|jsp|xml|file`.
    pub kind: String,
}

/// The resolved new-file path + content + whether a file is already there.
#[derive(Serialize)]
pub struct NewFileResult {
    /// Absolute path (forward slashes) of the file to create.
    pub path: String,
    /// Initial content (Java template with inferred package, JSP/XML header, or empty).
    pub content: String,
    /// True when a file already exists at `path` (the FE warns instead of overwriting).
    pub exists: bool,
}

/// Map the wire kind string to the scaffolder enum.
fn kind_of(s: &str) -> Option<NewFileKind> {
    Some(match s {
        "class" => NewFileKind::JavaClass,
        "interface" => NewFileKind::JavaInterface,
        "enum" => NewFileKind::JavaEnum,
        "record" => NewFileKind::JavaRecord,
        "annotation" => NewFileKind::JavaAnnotation,
        "jsp" => NewFileKind::Jsp,
        "xml" => NewFileKind::Xml,
        "file" => NewFileKind::PlainFile,
        _ => return None,
    })
}

/// Scaffold a new file. `None` for an unknown kind or an empty resolved name.
#[arbor_rpc::handler]
fn bennu_new_file(_ctx: &BennuState, args: NewFileArgs) -> Result<Option<NewFileResult>, String> {
    let Some(kind) = kind_of(&args.kind) else {
        return Ok(None);
    };
    let ScaffoldResult { file_name, content } =
        scaffold_new_file(kind, Path::new(&args.dir), &args.name);
    if file_name.trim().is_empty() {
        return Ok(None);
    }
    let path = format!("{}/{}", args.dir.trim_end_matches('/'), file_name);
    let exists = Path::new(&path).exists();
    Ok(Some(NewFileResult { path, content, exists }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kind_mapping_covers_the_menu() {
        for k in ["class", "interface", "enum", "record", "annotation", "jsp", "xml", "file"] {
            assert!(kind_of(k).is_some(), "{k} should map");
        }
        assert!(kind_of("nope").is_none());
    }
}
