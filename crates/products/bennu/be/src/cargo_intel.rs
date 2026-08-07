//! `Cargo.toml` intelligence — the routing gate for validation and completion.
//!
//! ## The contract, and why it is the same one `lsp_route` uses
//!
//! Every function here returns `Option`, and the distinction is load-bearing:
//!
//! - **`None` means "not ours"** — the file is not a Cargo manifest, and the caller must fall
//!   through to whatever engine owns it.
//! - **`Some(empty)` means "ours, and there is nothing to say"** — which is a real and common
//!   answer: a clean manifest has no diagnostics, and most carets have nothing to complete.
//!
//! Collapsing the two would send a `Cargo.toml` to the Java analyzers, which know nothing about it,
//! or (worse) would let the Java path answer for it. Same reasoning, and the same shape, as
//! [`crate::lsp_route`].
//!
//! ## The catalogue is cached, and the lockfile decides when
//!
//! Completion offers crate names and versions read off this machine — the workspace's own manifest,
//! `Cargo.lock`, and the registry cache. That last one is thousands of directory entries, so listing
//! it on every keystroke is the one thing that would make completion feel slow. It is therefore
//! cached per workspace root and rebuilt only when `Cargo.lock`'s modification time changes, which
//! is exactly when a `cargo add` or a build has taught the machine about a new crate. One `stat` per
//! completion request, against a listing that is otherwise free.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::SystemTime;

use bennu_cargo::prelude::{
    complete as cargo_complete, validate as cargo_validate, Catalog, CompleteContext, Manifest,
    ValidateContext,
};
use bennu_proto::prelude::{CompletionItem, Diagnostic};

/// How far up the tree a workspace root is looked for.
const MAX_ANCESTORS: usize = 12;

/// Whether this file is a Cargo manifest — the gate every function below shares.
///
/// The **name**, not the extension: a project has plenty of `.toml` files that are not manifests
/// (`rustfmt.toml`, `.cargo/config.toml`, a fixture), and applying the manifest schema to one of them
/// would flag every key in it.
pub(crate) fn owns(file: &str) -> bool {
    file_name(file) == "cargo.toml"
}

/// Diagnostics for a Cargo manifest. `None` when the file is not one — see the module doc.
///
/// `source` is the live buffer; without it the on-disk file is read, because a manifest can be
/// validated from the Problems panel with no editor open on it.
pub(crate) fn diagnostics(file: &str, source: Option<&str>) -> Option<Vec<Diagnostic>> {
    if !owns(file) {
        return None;
    }
    let path = PathBuf::from(file);
    let text = match source {
        Some(s) => s.to_string(),
        None => std::fs::read_to_string(&path).ok()?,
    };
    let ctx = ValidateContext {
        dir: path.parent().map(Path::to_path_buf),
        workspace: workspace_root(&path)
            // A workspace root validates against itself otherwise, and every one of its own
            // inheritance sources would be reported as missing from itself.
            .filter(|root| Some(root.as_path()) != path.parent())
            .and_then(|root| std::fs::read_to_string(root.join("Cargo.toml")).ok())
            .map(|t| Manifest::parse(&t)),
    };
    Some(cargo_validate(&text, &ctx))
}

/// Completion candidates in a Cargo manifest. `None` when the file is not one.
pub(crate) fn completion(
    file: &str,
    offset: usize,
    source: Option<&str>,
) -> Option<Vec<CompletionItem>> {
    if !owns(file) {
        return None;
    }
    let path = PathBuf::from(file);
    let text = match source {
        Some(s) => s.to_string(),
        None => std::fs::read_to_string(&path).ok()?,
    };
    let root = workspace_root(&path).or_else(|| path.parent().map(Path::to_path_buf))?;
    let ctx = CompleteContext { dir: path.parent().map(Path::to_path_buf), catalog: catalog(&root) };
    Some(cargo_complete(&text, offset, &ctx))
}

/// The workspace root governing `manifest`: the nearest ancestor whose `Cargo.toml` declares a
/// `[workspace]`, else the manifest's own directory.
///
/// `None` only when the path has no parent at all.
pub(crate) fn workspace_root(manifest: &Path) -> Option<PathBuf> {
    let start = manifest.parent()?;
    let mut dir = Some(start);
    let mut levels = 0;
    // `while let`, not `dir?` inside the loop: running out of ancestors means "no workspace above
    // this crate", which has to fall through to the crate's own directory rather than return `None`
    // from the whole function — a crate opened on its own still needs somewhere to read a catalogue
    // from.
    while let Some(d) = dir {
        if levels >= MAX_ANCESTORS {
            break;
        }
        if let Ok(text) = std::fs::read_to_string(d.join("Cargo.toml")) {
            if Manifest::parse(&text).has_table("workspace") {
                return Some(d.to_path_buf());
            }
        }
        dir = d.parent();
        levels += 1;
    }
    Some(start.to_path_buf())
}

// ── the cached catalogue ───────────────────────────────────────────────────────

/// A catalogue and the lockfile modification time it was read at.
struct Cached {
    catalog: Catalog,
    /// `None` when there was no `Cargo.lock` — which is itself a state worth remembering, so that
    /// the arrival of one invalidates the entry.
    lock_stamp: Option<SystemTime>,
}

fn cache() -> &'static Mutex<HashMap<PathBuf, Cached>> {
    static CACHE: OnceLock<Mutex<HashMap<PathBuf, Cached>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// The crate/version catalogue for `root`, rebuilt when the lockfile has moved on.
///
/// Cloned out rather than handed out behind the lock: building one walks the registry directory, and
/// holding the map's mutex across that would serialise every other project's completion behind it.
fn catalog(root: &Path) -> Catalog {
    let stamp = lock_stamp(root);
    if let Ok(map) = cache().lock() {
        if let Some(hit) = map.get(root) {
            if hit.lock_stamp == stamp {
                return hit.catalog.clone();
            }
        }
    }
    let catalog = Catalog::read(root);
    if let Ok(mut map) = cache().lock() {
        map.insert(root.to_path_buf(), Cached { catalog: catalog.clone(), lock_stamp: stamp });
    }
    catalog
}

/// The modification time of `root/Cargo.lock`, or `None` when there is none.
fn lock_stamp(root: &Path) -> Option<SystemTime> {
    std::fs::metadata(root.join("Cargo.lock")).ok()?.modified().ok()
}

/// Drop the cached catalogue for `root` — after a command that could have changed what is
/// installed (`cargo add`, `cargo update`, a build that fetched something).
pub(crate) fn forget_catalog(root: &str) {
    if let Ok(mut map) = cache().lock() {
        map.remove(Path::new(root));
    }
}

/// The lower-cased final segment of a path.
fn file_name(path: &str) -> String {
    path.rsplit(['/', '\\']).next().unwrap_or(path).to_ascii_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_manifest_is_ours() {
        assert!(owns("/p/Cargo.toml"));
        assert!(owns("C:\\p\\Cargo.toml"));
        // Case-insensitively, because a path from the frontend may be spelled either way on a
        // case-insensitive filesystem.
        assert!(owns("/p/cargo.toml"));
        // Every other TOML in a Rust project is somebody else's file, and applying the manifest
        // schema to one would flag every key in it.
        assert!(!owns("/p/rustfmt.toml"));
        assert!(!owns("/p/.cargo/config.toml"));
        assert!(!owns("/p/Cargo.lock"));
        assert!(!owns("/p/src/main.rs"));
    }

    /// The routing contract: not-ours is `None`, ours-with-nothing-to-say is `Some(empty)`.
    #[test]
    fn a_file_that_is_not_a_manifest_falls_through() {
        assert!(diagnostics("/p/rustfmt.toml", Some("x = 1")).is_none());
        assert!(completion("/p/rustfmt.toml", 0, Some("x = 1")).is_none());
        // A clean manifest is ours, and answers nothing.
        let clean = diagnostics("/p/Cargo.toml", Some("[package]\nname = \"x\"\n"));
        assert_eq!(clean.as_deref(), Some(&[][..]));
    }

    #[test]
    fn a_manifest_with_a_problem_answers_with_it() {
        let d = diagnostics("/p/Cargo.toml", Some("[package]\nname = \"x\"\nedtion = \"2021\"\n"))
            .expect("ours");
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].code, "cargo-unknown-key");
    }

    #[test]
    fn the_workspace_root_is_the_nearest_ancestor_declaring_one() {
        let dir = std::env::temp_dir().join(format!(
            "bennu-be-cargo-intel-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("crates/inner")).unwrap();
        std::fs::write(dir.join("Cargo.toml"), "[workspace]\nmembers = [\"crates/*\"]\n").unwrap();
        std::fs::write(dir.join("crates/inner/Cargo.toml"), "[package]\nname = \"inner\"\n").unwrap();

        assert_eq!(workspace_root(&dir.join("crates/inner/Cargo.toml")).as_deref(), Some(dir.as_path()));
        // A crate with no workspace above it is its own root, so completion still has somewhere to
        // read a catalogue from.
        let lone = dir.join("crates/inner");
        std::fs::remove_file(dir.join("Cargo.toml")).unwrap();
        assert_eq!(workspace_root(&lone.join("Cargo.toml")).as_deref(), Some(lone.as_path()));

        let _ = std::fs::remove_dir_all(&dir);
    }
}
