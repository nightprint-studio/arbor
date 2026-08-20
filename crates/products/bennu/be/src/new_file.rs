//! `new_file` domain — `bennu_new_file`: scaffold a new file's name + initial content for the
//! project-tree "New…" menu.
//!
//! Thin wrapper over [`bennu_java::prelude::scaffold_new_file`] (the pure, tested scaffolder):
//! resolves the final path under the target dir + whether something is already there (the FE
//! refuses to overwrite). The FE writes the returned content (encoding-aware) and opens it.

use std::path::Path;

use bennu_core::prelude::BennuState;
use bennu_cargo::prelude::{scaffold_rust_file, RustFileKind, RustScaffold};
use bennu_java::prelude::{scaffold_new_file, NewFileKind, ScaffoldResult};
use serde::{Deserialize, Serialize};

/// Args for [`bennu_new_file`].
#[derive(Deserialize)]
pub struct NewFileArgs {
    /// Target directory (absolute, forward slashes).
    pub dir: String,
    /// The base name the user entered (extension optional).
    pub name: String,
    /// One of `class|interface|enum|record|annotation|exception|jsp|xml|file` for Java,
    /// or `rust_file|rust_struct|rust_enum|rust_trait|rust_module|rust_tests` for Rust.
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
        "exception" => NewFileKind::JavaException,
        "jsp" => NewFileKind::Jsp,
        "xml" => NewFileKind::Xml,
        "file" => NewFileKind::PlainFile,
        _ => return None,
    })
}

/// Map the wire kind string to a Rust template.
///
/// A separate table from [`kind_of`] and not one enum with both languages in it: what
/// the two ask of the user differs (a Java file is named by its type, a Rust one names
/// its module), so the templates live in each language's own crate and meet here.
fn rust_kind_of(s: &str) -> Option<RustFileKind> {
    Some(match s {
        "rust_file" => RustFileKind::File,
        "rust_struct" => RustFileKind::Struct,
        "rust_enum" => RustFileKind::Enum,
        "rust_trait" => RustFileKind::Trait,
        "rust_module" => RustFileKind::Module,
        "rust_tests" => RustFileKind::Tests,
        _ => return None,
    })
}

/// Scaffold a new file. `None` for an unknown kind or an empty resolved name.
#[arbor_rpc::handler]
fn bennu_new_file(_ctx: &BennuState, args: NewFileArgs) -> Result<Option<NewFileResult>, String> {
    let (file_name, content) = if let Some(kind) = rust_kind_of(&args.kind) {
        let RustScaffold { file_name, content } = scaffold_rust_file(kind, &args.name);
        (file_name, content)
    } else if let Some(kind) = kind_of(&args.kind) {
        let ScaffoldResult { file_name, content } =
            scaffold_new_file(kind, Path::new(&args.dir), &args.name);
        (file_name, content)
    } else {
        return Ok(None);
    };
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
        for k in
            ["class", "interface", "enum", "record", "annotation", "exception", "jsp", "xml", "file"]
        {
            assert!(kind_of(k).is_some(), "{k} should map");
        }
        assert!(kind_of("nope").is_none());
    }

    #[test]
    fn rust_kind_mapping_covers_the_menu() {
        for k in ["rust_file", "rust_struct", "rust_enum", "rust_trait", "rust_module", "rust_tests"]
        {
            assert!(rust_kind_of(k).is_some(), "{k} should map");
        }
        // The two tables must not overlap: a name that resolved in both would resolve
        // differently depending on which branch was tried first.
        for k in ["class", "jsp", "file"] {
            assert!(rust_kind_of(k).is_none());
        }
    }
}
