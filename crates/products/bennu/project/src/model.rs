//! Project open orchestration + file reading.
//!
//! [`open_project`] identifies the root by its **manifest** and ties the leaf pieces
//! together into the [`ProjectInfo`] the `bennu_open_project` handler returns:
//!
//! * `pom.xml` → [`ProjectKind::Maven`]: pom parse, capability detection, JDK
//!   detection, declared encoding — the full Java model.
//! * `Cargo.toml` → [`ProjectKind::Cargo`]: name, workspace members, UTF-8. No
//!   capabilities and no JDK, because every one of those rules is a claim about a
//!   Java stack and a false one is worse than an absent one (docs §7).
//!
//! Maven is checked first: a polyglot repo that has both is a Java project with a
//! Rust component far more often than the reverse, and the Java model is the one
//! that has something to say.
//!
//! [`read_file`] decodes a file in the project's resolved encoding (docs §5 #21).
//! Both are pure over (filesystem + config inputs); the backend glue (which config,
//! which overrides) stays in `bennu-be`.

use std::path::Path;

use bennu_proto::prelude::{CapabilitySet, FileContents, ProjectInfo, ProjectKind, WriteResult};

use crate::error::ProjectError;
use crate::{capability, cargo, encoding, jdk, pom};

/// Inputs the backend supplies from its config (the per-project overrides + default
/// encoding). Keeping them as an explicit struct means this leaf never reaches into
/// `bennu-core`'s config type — the backend maps its config into this.
#[derive(Debug, Clone, Default)]
pub struct OpenOptions<'a> {
    /// Fallback encoding label when the pom declares none (`"UTF-8"` typically).
    pub default_encoding: &'a str,
    /// Explicit JDK version override for this project root, if the user set one.
    pub jdk_override: Option<&'a str>,
}

/// Open the project rooted at `root`, dispatching on the manifest it holds (see the
/// module doc). Errors when `root` isn't a directory, or holds neither `pom.xml` nor
/// `Cargo.toml`.
pub fn open_project(root: &Path, opts: &OpenOptions) -> Result<ProjectInfo, ProjectError> {
    if !root.is_dir() {
        return Err(ProjectError::NotADirectory(root.display().to_string()));
    }
    if root.join("pom.xml").is_file() {
        return open_maven(root, opts);
    }
    if root.join("Cargo.toml").is_file() {
        return open_cargo(root);
    }
    Err(ProjectError::NoManifest(root.display().to_string()))
}

/// The Maven half: pom parse → capability detection + JDK + declared encoding.
fn open_maven(root: &Path, opts: &OpenOptions) -> Result<ProjectInfo, ProjectError> {
    let xml =
        std::fs::read_to_string(root.join("pom.xml")).map_err(|e| ProjectError::Io(e.to_string()))?;
    let pom = pom::parse(&xml);

    let capabilities = capability::detect(root, &pom);
    let jdk = jdk::detect(&pom, opts.jdk_override);

    Ok(ProjectInfo {
        root: root.display().to_string(),
        name: non_empty_or_dir_name(&pom.name, root),
        modules: pom.modules.clone(),
        kind: ProjectKind::Maven,
        jdk,
        capabilities,
        // The project label the status bar shows: pom `sourceEncoding` → config default.
        source_encoding: encoding::project_encoding_label(&pom, opts.default_encoding),
    })
}

/// The Cargo half: `[package] name` → the header, the workspace `members` → the
/// module list. No capabilities, no JDK, and UTF-8 unconditionally — Rust source is
/// UTF-8 by language definition, so the encoding machinery has nothing to resolve
/// here and a config default of `Cp1252` must not leak into a Rust project.
fn open_cargo(root: &Path) -> Result<ProjectInfo, ProjectError> {
    let text = std::fs::read_to_string(root.join("Cargo.toml"))
        .map_err(|e| ProjectError::Io(e.to_string()))?;
    let manifest = cargo::parse(&text);

    Ok(ProjectInfo {
        root: root.display().to_string(),
        name: non_empty_or_dir_name(&manifest.name, root),
        modules: cargo::expand_members(root, &manifest.members),
        kind: ProjectKind::Cargo,
        jdk: None,
        capabilities: CapabilitySet::default(),
        source_encoding: encoding::UTF8.to_string(),
    })
}

/// `declared` when it says something, else the root directory's own name — the
/// fallback both manifests share (a virtual Cargo workspace and a pom without
/// `<name>` are the same situation).
fn non_empty_or_dir_name(declared: &str, root: &Path) -> String {
    if !declared.is_empty() {
        return declared.to_string();
    }
    root.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
}

/// Read `file` decoded in the project's encoding. `encoding_override` (per-project or
/// per-file, from config) wins; else the pom's declared encoding; else
/// `default_encoding`. Returns the decoded text + the encoding that applied + the on-disk
/// [`stamp`](file_stamp) the text corresponds to.
pub fn read_file(
    project_root: &Path,
    file: &Path,
    default_encoding: &str,
    encoding_override: Option<&str>,
) -> Result<FileContents, ProjectError> {
    // The stamp is taken **before** the read, and the order is the whole point. If something
    // rewrites the file between the two, stamping first pairs the NEW text with the OLD
    // stamp — so the next poll reports an external change that has in fact already been
    // read, and the caller reloads identical content. Harmless. Stamping after would pair
    // the OLD text with the NEW stamp: the change would look already-seen and the buffer
    // would sit on stale text that a save then writes back over. That is the failure this
    // whole mechanism exists to prevent, so the cheap direction is the wrong one.
    let stamp = file_stamp(file);
    let bytes = std::fs::read(file).map_err(|e| ProjectError::Io(e.to_string()))?;
    let label = resolve_encoding_label(project_root, default_encoding, encoding_override);
    let (text, applied) = encoding::decode(&bytes, &label);
    // Normalize to LF so every downstream byte offset (validation, go-to) agrees with the editor's
    // LF document; the on-disk CRLF is restored on save (see `write_file`).
    // The whole file: the range slicing, and the two fields that describe it, belong to the
    // handler that was asked for a range. A reader that was not asked for one says so with zeros.
    Ok(FileContents {
        text: encoding::normalize_newlines(&text),
        encoding: applied,
        stamp,
        total_lines: 0,
        from_line: 0,
    })
}

/// A cheap fingerprint of `file`'s current on-disk state — `"<mtime_nanos>:<len>"`, or
/// `""` when the file is absent or its metadata can't be read.
///
/// See [`FileStamp`] for why this is a stat and not a content hash. `""` is deliberately
/// *not* an error: an absent file is a legitimate answer (it was deleted, or never
/// existed), and every consumer treats an empty stamp as "no opinion" — which disables the
/// overwrite check rather than blocking a save.
pub fn file_stamp(file: &Path) -> String {
    let Ok(meta) = std::fs::metadata(file) else { return String::new() };
    let nanos = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("{nanos}:{}", meta.len())
}

/// Write `text` to `file`, encoded in the project's resolved encoding — the round-trip
/// inverse of [`read_file`]. The label is resolved the same way (explicit override →
/// pom-declared → default), then [`encoding::encode`] encodes; a char the target
/// encoding can't represent transparently falls back to UTF-8 (never fails the save).
/// Returns the encoding that actually applied + the new on-disk stamp.
///
/// ## `expect_stamp` — the overwrite guard
///
/// When given a non-empty stamp, the write **refuses** if the file's current stamp differs
/// from it, with [`ProjectError::ExternallyModified`]. The caller passes the stamp its
/// buffer was read from, so "something else changed this file since I loaded it" becomes a
/// refusal instead of a silent overwrite. This is the load-bearing half of external-change
/// handling: polling notices a change *eventually*, but the check has to sit next to the
/// write or a caller could only ever check-then-write with a window in between.
///
/// Two cases deliberately do **not** refuse:
///
/// * `None` / `""` — no opinion (a brand-new file, or a caller that never read one). A
///   guard nobody asked for must not turn a legitimate save into an error.
/// * the file is **gone** — its stamp is empty now. The buffer is the last copy of that
///   content in existence; refusing would strand it with nowhere to go, so the write
///   recreates the file. Deletion is not the accident this guard is for.
pub fn write_file(
    project_root: &Path,
    file: &Path,
    text: &str,
    default_encoding: &str,
    encoding_override: Option<&str>,
    expect_stamp: Option<&str>,
) -> Result<WriteResult, ProjectError> {
    if let Some(expected) = expect_stamp.filter(|s| !s.is_empty()) {
        let current = file_stamp(file);
        // A non-empty current stamp that differs → somebody else wrote this file.
        if !current.is_empty() && current != expected {
            return Err(ProjectError::ExternallyModified(file.display().to_string()));
        }
    }
    let label = resolve_encoding_label(project_root, default_encoding, encoding_override);
    // Preserve the file's on-disk line endings: the buffer is LF (normalized on read + CodeMirror's
    // own LF document), but if the existing file is CRLF we re-expand so a save doesn't silently
    // rewrite every line ending (a noisy diff on a Windows/legacy codebase). A new file stays LF.
    let restore = std::fs::read(file).map(|b| encoding::has_crlf(&b)).unwrap_or(false);
    let to_write = if restore { encoding::restore_crlf(text) } else { text.to_string() };
    let (bytes, applied) = encoding::encode(&to_write, &label);
    std::fs::write(file, &bytes).map_err(|e| ProjectError::Io(e.to_string()))?;
    // Stamped AFTER the write — the opposite of `read_file`, and for the mirror reason:
    // this is the state WE just put on disk, and the caller needs exactly that as its new
    // baseline or its very next save would refuse on its own writing.
    Ok(WriteResult { encoding: applied, stamp: file_stamp(file) })
}

/// Rename or move `from` to `to`, refusing rather than overwriting.
///
/// The refusals, and why each one is a refusal instead of a best effort:
///
/// * **`from` is gone** — there is nothing to rename, and `fs::rename` would report a bare "No such
///   file" that names neither end of the operation.
/// * **`to` already exists** — a rename that silently replaced a file would destroy it with no undo
///   anywhere in the system. The one exception is a rename to the *same file*, which is what a
///   change of letter case is on macOS and Windows: `Foo.rs` → `foo.rs` is a legitimate rename that
///   an existence check alone reads as a collision.
/// * **the destination's parent is missing** — creating intermediate directories on a rename is
///   guessing. `mod.rs` → `sub/mod.rs` where `sub/` does not exist is far more likely a typo than an
///   instruction to build a tree.
///
/// Deliberately not this function's business: telling a language server (the caller asks it for the
/// edits a rename implies **before** calling this — the server has to answer about the tree as it
/// stands) and updating any editor state.
pub fn rename_path(from: &Path, to: &Path) -> Result<(), ProjectError> {
    if !from.exists() {
        return Err(ProjectError::Io(format!("{} does not exist", from.display())));
    }
    if to.exists() && !same_file(from, to) {
        return Err(ProjectError::Io(format!("{} already exists", to.display())));
    }
    match to.parent() {
        Some(parent) if !parent.as_os_str().is_empty() && !parent.is_dir() => {
            return Err(ProjectError::Io(format!("{} is not a directory", parent.display())));
        }
        _ => {}
    }
    std::fs::rename(from, to).map_err(|e| ProjectError::Io(e.to_string()))
}

/// Whether two paths name the same file on disk.
///
/// By canonicalised path rather than by string, which is the whole point: on a case-insensitive
/// filesystem `Foo.rs` and `foo.rs` are one file, and only asking the filesystem can tell. A path
/// that cannot be canonicalised (it does not exist) is not the same file as anything.
fn same_file(a: &Path, b: &Path) -> bool {
    match (std::fs::canonicalize(a), std::fs::canonicalize(b)) {
        (Ok(x), Ok(y)) => x == y,
        _ => false,
    }
}

/// Resolve the encoding label for a file in `project_root`: explicit `encoding_override`
/// (per-file/per-project) → the pom's declared `sourceEncoding` → `default_encoding`.
/// Shared by [`read_file`] and [`write_file`] so a read and its matching write agree.
///
/// A **Cargo** root short-circuits to UTF-8: without the pom there is nothing to read a
/// declared encoding from, and letting it fall through to `default_encoding` would decode
/// Rust source in whatever the user set for their legacy Java tree (`Cp1252` is the whole
/// reason that setting exists) — which corrupts every non-ASCII string literal on save.
fn resolve_encoding_label(
    project_root: &Path,
    default_encoding: &str,
    encoding_override: Option<&str>,
) -> String {
    if let Some(label) = encoding_override.filter(|s| !s.is_empty()) {
        return label.to_string();
    }
    // Re-read the project encoding from the pom if present; cheap and keeps this
    // self-contained (no cross-call state).
    match std::fs::read_to_string(project_root.join("pom.xml")) {
        Ok(xml) => encoding::project_encoding_label(&pom::parse(&xml), default_encoding),
        Err(_) if project_root.join("Cargo.toml").is_file() => encoding::UTF8.to_string(),
        Err(_) => default_encoding.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bennu_proto::prelude::ERR_EXTERNALLY_MODIFIED;

    /// A unique temp dir for a fixture tree, removed by the caller.
    fn temp_dir(tag: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!(
            "bennu-model-{tag}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn a_cargo_root_opens_as_a_cargo_project() {
        let dir = temp_dir("cargo");
        std::fs::write(
            dir.join("Cargo.toml"),
            "[workspace]\nmembers = [\"crates/*\"]\n[package]\nname = \"my-crate\"\n",
        )
        .unwrap();
        std::fs::create_dir_all(dir.join("crates/one")).unwrap();
        std::fs::write(dir.join("crates/one/Cargo.toml"), "[package]\nname=\"one\"\n").unwrap();

        // A Cp1252 default (the legacy-Java setting) must NOT reach a Rust project.
        let opts = OpenOptions { default_encoding: "Cp1252", jdk_override: None };
        let info = open_project(&dir, &opts).unwrap();

        assert_eq!(info.kind, ProjectKind::Cargo);
        assert_eq!(info.name, "my-crate");
        assert_eq!(info.modules, vec!["crates/one".to_string()], "workspace members are the modules");
        assert!(info.jdk.is_none(), "a Cargo project has no JDK");
        assert_eq!(info.capabilities, CapabilitySet::default(), "no Java capability may fire");
        assert_eq!(info.source_encoding, "UTF-8", "Rust is UTF-8 by definition, not by default");

        // …and the same holds for the per-file resolution a read/write goes through.
        assert_eq!(resolve_encoding_label(&dir, "Cp1252", None), "UTF-8");
        // An explicit override still wins (the user asked for it by hand).
        assert_eq!(resolve_encoding_label(&dir, "Cp1252", Some("Cp1252")), "Cp1252");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn maven_wins_when_a_root_holds_both_manifests() {
        let dir = temp_dir("both");
        std::fs::write(dir.join("pom.xml"), "<project><artifactId>app</artifactId></project>").unwrap();
        std::fs::write(dir.join("Cargo.toml"), "[package]\nname = \"rusty\"\n").unwrap();

        let info = open_project(&dir, &OpenOptions { default_encoding: "UTF-8", jdk_override: None })
            .unwrap();
        assert_eq!(info.kind, ProjectKind::Maven, "a polyglot root is the Java project");
        assert_eq!(info.name, "app");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_root_with_no_manifest_is_refused() {
        let dir = temp_dir("bare");
        let err = open_project(&dir, &OpenOptions { default_encoding: "UTF-8", jdk_override: None })
            .unwrap_err();
        assert!(matches!(err, ProjectError::NoManifest(_)), "got {err:?}");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn read_normalizes_crlf_and_write_preserves_it() {
        let dir = temp_dir("eol");
        let file = dir.join("Foo.java");
        // A CRLF file on disk (the Windows/legacy norm).
        std::fs::write(&file, b"class Foo {\r\n  int x;\r\n}\r\n").unwrap();

        // read_file hands the FE an LF buffer (so byte offsets match the editor's LF document).
        let read = read_file(&dir, &file, "UTF-8", None).unwrap();
        assert_eq!(read.text, "class Foo {\n  int x;\n}\n", "read must normalize CRLF → LF");

        // Saving the (edited) LF buffer must restore the file's on-disk CRLF, not rewrite every EOL.
        write_file(&dir, &file, "class Foo {\n  int y;\n}\n", "UTF-8", None, None).unwrap();
        let on_disk = std::fs::read(&file).unwrap();
        assert_eq!(on_disk, b"class Foo {\r\n  int y;\r\n}\r\n", "write must preserve CRLF on disk");

        // A genuinely LF file stays LF through a save (no spurious CRLF introduced).
        let lf = dir.join("Bar.java");
        std::fs::write(&lf, b"class Bar {\n}\n").unwrap();
        write_file(&dir, &lf, "class Bar {\n  int z;\n}\n", "UTF-8", None, None).unwrap();
        assert_eq!(std::fs::read(&lf).unwrap(), b"class Bar {\n  int z;\n}\n", "LF file stays LF");

        let _ = std::fs::remove_dir_all(&dir);
    }

    // ── the external-change guard ────────────────────────────────────────────────

    /// A stamp handed straight back from the read must let the save through, and the stamp
    /// the save returns must let the NEXT one through — otherwise Bennu would refuse its
    /// own second keystroke.
    #[test]
    fn a_matching_stamp_saves_and_rolls_forward() {
        let dir = temp_dir("stamp-ok");
        let file = dir.join("Foo.java");
        std::fs::write(&file, b"class Foo {}\n").unwrap();

        let read = read_file(&dir, &file, "UTF-8", None).unwrap();
        assert!(!read.stamp.is_empty(), "an existing file must have a stamp");

        let first = write_file(&dir, &file, "class Foo { int a; }\n", "UTF-8", None, Some(&read.stamp))
            .unwrap();
        assert!(!first.stamp.is_empty());
        // The write moved the file on, so the stamp it returned is the new baseline.
        write_file(&dir, &file, "class Foo { int b; }\n", "UTF-8", None, Some(&first.stamp)).unwrap();
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "class Foo { int b; }\n");

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The bug this exists for: a file changed behind the editor's back is NOT overwritten.
    #[test]
    fn a_stale_stamp_refuses_the_write() {
        let dir = temp_dir("stamp-stale");
        let file = dir.join("Foo.java");
        std::fs::write(&file, b"class Foo {}\n").unwrap();
        let read = read_file(&dir, &file, "UTF-8", None).unwrap();

        // Somebody else writes the file. Forcing the mtime forward keeps the test honest on
        // a filesystem whose timestamp granularity is coarser than this test is fast.
        std::fs::write(&file, b"// edited by somebody else\nclass Foo {}\n").unwrap();
        bump_mtime(&file);

        let err = write_file(&dir, &file, "class Foo { int mine; }\n", "UTF-8", None, Some(&read.stamp))
            .unwrap_err();
        assert!(matches!(err, ProjectError::ExternallyModified(_)), "got {err:?}");
        // The refusal has to be total: the other edit is still on disk, untouched.
        assert!(
            std::fs::read_to_string(&file).unwrap().starts_with("// edited by somebody else"),
            "the external edit must survive a refused save",
        );
        // …and the message carries the prefix the FE branches on.
        assert!(err.to_string().starts_with(ERR_EXTERNALLY_MODIFIED));

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// No stamp = no opinion, and a vanished file is not a conflict — both must still save,
    /// or the guard would block a new file and strand the last copy of a deleted one.
    #[test]
    fn no_stamp_and_a_deleted_file_both_still_save() {
        let dir = temp_dir("stamp-edge");

        // A brand-new file: nothing was ever read, so there is no stamp to check.
        let fresh = dir.join("New.java");
        write_file(&dir, &fresh, "class New {}\n", "UTF-8", None, None).unwrap();
        assert!(fresh.is_file());
        // An explicitly EMPTY stamp means the same thing.
        write_file(&dir, &fresh, "class New { int x; }\n", "UTF-8", None, Some("")).unwrap();

        // A file deleted under us: the buffer is the last copy, so the save recreates it
        // rather than refusing and leaving the content nowhere.
        let gone = dir.join("Gone.java");
        std::fs::write(&gone, b"class Gone {}\n").unwrap();
        let read = read_file(&dir, &gone, "UTF-8", None).unwrap();
        std::fs::remove_file(&gone).unwrap();
        write_file(&dir, &gone, "class Gone { int kept; }\n", "UTF-8", None, Some(&read.stamp))
            .unwrap();
        assert_eq!(std::fs::read_to_string(&gone).unwrap(), "class Gone { int kept; }\n");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_rename_moves_the_file_and_refuses_to_clobber_anything() {
        let dir = temp_dir("rename");
        let from = dir.join("old.rs");
        let to = dir.join("new.rs");
        std::fs::write(&from, b"pub fn keep() {}\n").unwrap();

        rename_path(&from, &to).unwrap();
        assert!(!from.exists());
        assert_eq!(std::fs::read_to_string(&to).unwrap(), "pub fn keep() {}\n");

        // Renaming onto an existing file would destroy it, with no undo anywhere in the system.
        let other = dir.join("other.rs");
        std::fs::write(&other, b"pub fn other() {}\n").unwrap();
        assert!(rename_path(&to, &other).is_err());
        assert_eq!(std::fs::read_to_string(&other).unwrap(), "pub fn other() {}\n", "left intact");

        // A destination whose parent does not exist is a typo, not an instruction to build a tree.
        assert!(rename_path(&to, &dir.join("nope").join("new.rs")).is_err());
        // And there is nothing to rename when the source is gone.
        assert!(rename_path(&dir.join("ghost.rs"), &dir.join("x.rs")).is_err());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn changing_only_the_letter_case_is_a_rename_not_a_collision() {
        // On macOS and Windows the destination "already exists" because it IS the source. An
        // existence check alone reads that as a clobber and refuses a legitimate rename.
        let dir = temp_dir("rename-case");
        let from = dir.join("Thing.rs");
        let to = dir.join("thing.rs");
        std::fs::write(&from, b"pub struct Thing;\n").unwrap();

        rename_path(&from, &to).unwrap();
        assert_eq!(std::fs::read_to_string(&to).unwrap(), "pub struct Thing;\n");

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Push a file's mtime a second into the future.
    ///
    /// Two writes inside the same filesystem timestamp tick can share an mtime, and if they
    /// also share a length the stamps match and the test would pass for the wrong reason —
    /// or fail intermittently, which is worse. Written by hand (rather than by sleeping) so
    /// the suite stays fast: the point is only that the stamp differs.
    fn bump_mtime(file: &Path) {
        let meta = std::fs::metadata(file).unwrap();
        let later = meta.modified().unwrap() + std::time::Duration::from_secs(1);
        // `set_modified` needs the file open for writing.
        let handle = std::fs::OpenOptions::new().write(true).open(file).unwrap();
        handle.set_modified(later).unwrap();
    }
}
