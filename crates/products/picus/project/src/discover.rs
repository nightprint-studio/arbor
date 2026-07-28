//! Reading a repository of scripts and working out what it is.
//!
//! Split in two on purpose:
//!
//! * [`plan`] is **pure** — a list of files and their bytes in, a proposal out. No
//!   filesystem, no clock, no ordering surprises. Everything worth testing about
//!   discovery is tested here, without a temporary directory in sight.
//! * [`scan`] is the glue that produces that list from a real directory.
//!
//! The output is a *proposal*, and that word is doing work: nothing here writes
//! anything. The user sees what Picus concluded, corrects the bits it got wrong,
//! and only then is [`crate::config::ProjectConfig::save`] called. A tool that
//! guessed and wrote would be a tool nobody could trust with a repository.
//!
//! When a project file already exists it **wins** over every inference: roles,
//! labels and dialects come from the file, and discovery only fills in what the
//! file cannot know — which files exist and what encoding they turned out to be.

use std::collections::BTreeMap;
use std::path::Path;

use arbor_fs::prelude::encoding::{detect_in_context, EncodingContext};
use picus_types::prelude::FolderRole;

use crate::config::{
    BranchConfig, EncodingSettings, FolderConfig, GenerationSettings, ProjectConfig,
    VersionTableSettings, CURRENT_VERSION, DEFAULT_ENCODING,
};
use crate::error::ProjectError;
use crate::infer::{infer_dialect, infer_role};
use crate::naming::NamingScheme;
use crate::tree::{Branch, LineEnding, Project, ScriptFile, ScriptFolder};

/// Extensions that count as script files. Anything else is not part of the
/// installation and is not shown — a repository full of `.docx` and `.zip` should
/// not read as a project with two hundred files.
pub const SCRIPT_EXTENSIONS: [&str; 9] =
    ["sql", "pks", "pkb", "prc", "fnc", "trg", "vw", "ddl", "dml"];

/// How much of each file is read to decide its encoding and line ending.
///
/// A prefix is enough for both: the encoding of a file is decided by its first
/// non-ASCII byte, and a file whose first 64 KiB are CRLF is a CRLF file. Reading
/// whole repositories into memory to answer a question the first page answers
/// would make opening a large project feel broken.
pub const SAMPLE_BYTES: usize = 64 * 1024;

/// One file as read from disk — the input to the pure half.
#[derive(Debug, Clone)]
pub struct SourceFile {
    /// Path relative to the project root, POSIX separators.
    pub path: String,
    /// The file's real size, which the sample below does not tell us.
    pub size: u64,
    /// The first [`SAMPLE_BYTES`] of the file.
    pub sample: Vec<u8>,
}

/// Something the user should look at before confirming.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProposalNote {
    /// Project-relative path the note is about.
    pub path: String,
    /// Written for the person deciding, not for a log.
    pub message: String,
    /// `true` when Picus could not work something out and is asking, rather than
    /// merely reporting what it found.
    pub needs_attention: bool,
}

/// What discovery concluded.
#[derive(Debug, Clone)]
pub struct Proposal {
    /// The configuration to confirm — or the existing one, unchanged, when the
    /// project was already set up.
    pub config: ProjectConfig,
    /// The tree as it is on disk right now.
    pub project: Project,
    /// What deserves a look before confirming.
    pub notes: Vec<ProposalNote>,
    /// `true` when this project has no `project.toml` yet, i.e. there is
    /// genuinely something to confirm.
    pub is_new: bool,
}

/// Work out what a repository is, from its files. Pure.
pub fn plan(root: &Path, files: &[SourceFile], existing: Option<&ProjectConfig>) -> Proposal {
    let name = existing
        .map(|c| c.name.clone())
        .or_else(|| root.file_name().map(|n| n.to_string_lossy().to_string()))
        .unwrap_or_else(|| "project".to_string());

    let default_encoding =
        existing.map(|c| c.encoding.default.clone()).unwrap_or_else(|| DEFAULT_ENCODING.to_string());

    // Group by directory, then by branch. `BTreeMap` rather than a hash map
    // throughout: the order of branches and folders is user-visible, and it must
    // not depend on how the filesystem happened to enumerate them.
    let mut by_dir: BTreeMap<&str, Vec<&SourceFile>> = BTreeMap::new();
    for file in files.iter().filter(|f| is_script(&f.path)) {
        by_dir.entry(parent_of(&file.path)).or_default().push(file);
    }
    let mut by_branch: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for dir in by_dir.keys() {
        by_branch.entry(branch_segment(dir)).or_default().push(dir);
    }

    let mut notes = Vec::new();
    let mut branches = Vec::new();
    let mut branch_configs = Vec::new();

    for (branch_path, dirs) in &by_branch {
        let branch_id = slug(if branch_path.is_empty() { "root" } else { branch_path });
        let configured_branch =
            existing.and_then(|c| c.branches.iter().find(|b| b.path == *branch_path));
        // A label the user rewrote is a decision, exactly like a role they
        // corrected, and a rescan must not quietly restore the folder's own name.
        let branch_label = match configured_branch {
            Some(b) => b.label.clone(),
            None => last_segment(branch_path).unwrap_or(&name).to_string(),
        };
        let dialect = match configured_branch {
            Some(b) => b.dialect,
            None => {
                let guess = infer_dialect(&branch_label);
                if guess.is_none() {
                    notes.push(ProposalNote {
                        path: branch_path.to_string(),
                        message: format!(
                            "nothing in the name `{branch_label}` says which engine this branch is written in — \
                             pick one, or leave it unset and nothing will be generated into it"
                        ),
                        needs_attention: true,
                    });
                }
                guess.map(|g| g.value)
            }
        };

        let mut folders = Vec::new();
        let mut folder_configs = Vec::new();

        for dir in dirs {
            let files_here = &by_dir[*dir];
            let configured_folder =
                configured_branch.and_then(|b| b.folders.iter().find(|f| f.path == **dir));

            let label = match configured_folder {
                Some(f) => f.label.clone(),
                None => last_segment(dir).unwrap_or(&name).to_string(),
            };
            let role = match configured_folder {
                Some(f) => f.role,
                None => {
                    let guess = infer_role_for(dir, branch_path);
                    if !guess.is_confident() {
                        notes.push(ProposalNote {
                            path: dir.to_string(),
                            message: format!(
                                "`{label}` does not look like any folder Picus recognises, so it is marked \
                                 as ignored — nothing will be generated into it until you say what it is"
                            ),
                            needs_attention: true,
                        });
                    }
                    guess.value
                }
            };

            // The folder's own encoding vote. Files that are pure ASCII abstain,
            // which is exactly right: they are the ones being decided.
            let mut context = EncodingContext::new()
                .with_legacy(label_to_encoding(&default_encoding));
            if let Some(pinned) = configured_folder.and_then(|f| f.encoding.as_deref()) {
                context = context.with_dominant(label_to_encoding(pinned));
            }
            for file in files_here {
                context.observe(&file.sample);
            }
            let expected = context
                .dominant()
                .map(|e| e.name().to_string())
                .unwrap_or_else(|| default_encoding.clone());

            let mut script_files = Vec::new();
            for file in files_here {
                let detection = detect_in_context(&file.sample, &context);
                let script = ScriptFile {
                    path: file.path.clone(),
                    name: last_segment(&file.path).unwrap_or(&file.path).to_string(),
                    size: file.size,
                    encoding: detection.encoding.name().to_string(),
                    encoding_source: detection.source,
                    eol: LineEnding::detect(&file.sample),
                    expected_encoding: expected.clone(),
                };
                if script.encoding_drifted() {
                    notes.push(ProposalNote {
                        path: script.path.clone(),
                        message: format!(
                            "this file is {} while the rest of `{label}` is {expected} — \
                             it was probably rewritten by an editor that did not know",
                            script.encoding
                        ),
                        needs_attention: false,
                    });
                }
                script_files.push(script);
            }
            script_files.sort_by(|a, b| a.path.cmp(&b.path));

            folder_configs.push(FolderConfig {
                id: slug(dir),
                label: label.clone(),
                path: dir.to_string(),
                role,
                encoding: configured_folder.and_then(|f| f.encoding.clone()),
                naming: configured_folder.and_then(|f| f.naming.clone()),
            });
            folders.push(ScriptFolder {
                id: slug(dir),
                label,
                role,
                path: dir.to_string(),
                files: script_files,
            });
        }

        branch_configs.push(BranchConfig {
            id: branch_id.clone(),
            label: branch_label.clone(),
            path: branch_path.to_string(),
            dialect,
            folders: folder_configs,
        });
        branches.push(Branch {
            id: branch_id,
            label: branch_label,
            dialect,
            path: branch_path.to_string(),
            folders,
        });
    }

    let config = match existing {
        // An existing file is authoritative and is never rewritten by a scan: a
        // folder that disappeared from disk stays in the file until the user says
        // otherwise, because deleting their configuration behind their back is
        // exactly the behaviour this design refuses.
        Some(existing) => existing.clone(),
        None => ProjectConfig {
            version: CURRENT_VERSION,
            name: name.clone(),
            encoding: EncodingSettings {
                default: default_encoding,
                eol: dominant_eol(&branches),
            },
            version_table: VersionTableSettings::default(),
            generation: GenerationSettings::default(),
            naming: NamingScheme::default(),
            branches: branch_configs,
        },
    };

    Proposal {
        config,
        project: Project {
            name,
            root: root.display().to_string(),
            branches,
        },
        notes,
        is_new: existing.is_none(),
    }
}

/// Read a repository from disk and plan it.
pub fn discover(root: &Path) -> Result<Proposal, ProjectError> {
    if !root.is_dir() {
        return Err(ProjectError::NotADirectory { path: root.to_path_buf() });
    }
    let existing = ProjectConfig::load(root)?;
    let files = scan(root)?;
    Ok(plan(root, &files, existing.as_ref()))
}

/// Walk a root and read enough of every script file to decide about it.
pub fn scan(root: &Path) -> Result<Vec<SourceFile>, ProjectError> {
    let mut out = Vec::new();
    collect(root, root, &mut out)?;
    out.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(out)
}

fn collect(root: &Path, dir: &Path, out: &mut Vec<SourceFile>) -> Result<(), ProjectError> {
    let entries = std::fs::read_dir(dir)
        .map_err(|e| ProjectError::Io { path: dir.to_path_buf(), reason: e.to_string() })?;
    for entry in entries {
        let entry =
            entry.map_err(|e| ProjectError::Io { path: dir.to_path_buf(), reason: e.to_string() })?;
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();
        if path.is_dir() {
            // Anything dot-prefixed is tooling, not scripts — including our own
            // `.arbor/`, which must never appear inside the project it describes.
            if name.starts_with('.') || name.eq_ignore_ascii_case("node_modules") {
                continue;
            }
            collect(root, &path, out)?;
        } else if is_script(&name) {
            let relative = path
                .strip_prefix(root)
                .map(|p| p.to_string_lossy().replace('\\', "/"))
                .unwrap_or_else(|_| name.clone());
            let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
            let sample = read_sample(&path)?;
            out.push(SourceFile { path: relative, size, sample });
        }
    }
    Ok(())
}

fn read_sample(path: &Path) -> Result<Vec<u8>, ProjectError> {
    use std::io::Read;
    let file = std::fs::File::open(path)
        .map_err(|e| ProjectError::Io { path: path.to_path_buf(), reason: e.to_string() })?;
    let mut sample = Vec::new();
    file.take(SAMPLE_BYTES as u64)
        .read_to_end(&mut sample)
        .map_err(|e| ProjectError::Io { path: path.to_path_buf(), reason: e.to_string() })?;
    Ok(sample)
}

/// A folder's role, falling back to its ancestors' names before giving up.
///
/// `ORACLE/AGGIORNAMENTO/2026` is an update folder even though `2026` says
/// nothing: the role of a subfolder is the role of the thing it is inside.
fn infer_role_for(dir: &str, branch: &str) -> crate::infer::Guess<FolderRole> {
    let mut current = dir;
    loop {
        let guess = infer_role(last_segment(current).unwrap_or(current));
        if guess.is_confident() {
            return guess;
        }
        match parent_of(current) {
            parent if parent.is_empty() || parent == branch || parent == current => {
                return crate::infer::Guess { value: FolderRole::Ignored, matched: None }
            }
            parent => current = parent,
        }
    }
}

/// The line ending most of the project uses, for generated content to match.
fn dominant_eol(branches: &[Branch]) -> LineEnding {
    let mut lf = 0usize;
    let mut crlf = 0usize;
    for file in branches.iter().flat_map(|b| b.folders.iter().flat_map(|f| f.files.iter())) {
        match file.eol {
            LineEnding::Lf => lf += 1,
            LineEnding::Crlf => crlf += 1,
        }
    }
    if lf > crlf {
        LineEnding::Lf
    } else {
        LineEnding::Crlf
    }
}

/// Resolve an encoding label, falling back to **windows-1252**.
///
/// Deliberately not `arbor_fs`'s `encoding_for_label`, which falls back to UTF-8:
/// that is the right default for a general file, and the wrong one here. A Picus
/// project whose declared encoding is a typo is a legacy repository with a typo,
/// and answering UTF-8 would mark every one of its accented files as drifted.
///
/// Public because every consumer that reads or writes a file of this project has
/// to resolve labels the same way — a backend that resolved a typo to UTF-8 while
/// discovery resolved it to windows-1252 would decode a file one way and refuse to
/// write it back the other.
pub fn label_to_encoding(label: &str) -> &'static encoding_rs::Encoding {
    encoding_rs::Encoding::for_label(label.as_bytes()).unwrap_or(encoding_rs::WINDOWS_1252)
}

fn is_script(path: &str) -> bool {
    match path.rsplit_once('.') {
        Some((_, ext)) => SCRIPT_EXTENSIONS.iter().any(|e| ext.eq_ignore_ascii_case(e)),
        None => false,
    }
}

fn parent_of(path: &str) -> &str {
    match path.rfind('/') {
        Some(i) => &path[..i],
        None => "",
    }
}

fn last_segment(path: &str) -> Option<&str> {
    if path.is_empty() {
        return None;
    }
    Some(path.rsplit('/').next().unwrap_or(path))
}

fn branch_segment(dir: &str) -> &str {
    match dir.find('/') {
        Some(i) => &dir[..i],
        None => dir,
    }
}

/// A stable id from a path: lowercase, one dash per run of anything else.
fn slug(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut pending_dash = false;
    for c in text.chars() {
        if c.is_ascii_alphanumeric() {
            if pending_dash && !out.is_empty() {
                out.push('-');
            }
            pending_dash = false;
            out.extend(c.to_lowercase());
        } else {
            pending_dash = true;
        }
    }
    if out.is_empty() {
        "root".to_string()
    } else {
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use picus_types::prelude::EngineKind;

    fn cp1252(text: &str) -> Vec<u8> {
        encoding_rs::WINDOWS_1252.encode(text).0.into_owned()
    }

    fn file(path: &str, bytes: Vec<u8>) -> SourceFile {
        SourceFile { path: path.to_string(), size: bytes.len() as u64, sample: bytes }
    }

    /// A two-branch repository shaped like the ones this product was built for.
    fn repository() -> Vec<SourceFile> {
        vec![
            file("ORACLE/INIZIALIZZAZIONE/01_TABELLE.sql", cp1252("-- tabelle\r\nCREATE TABLE X;\r\n")),
            file(
                "ORACLE/INIZIALIZZAZIONE/02_PARAMETRI.sql",
                cp1252("-- soglia già applicata\r\nINSERT INTO PARAMETRI VALUES ('X');\r\n"),
            ),
            file("ORACLE/AGGIORNAMENTO/4_11__4_12.sql", cp1252("-- 4.11 -> 4.12\r\n")),
            file("POSTGRES/INIZIALIZZAZIONE/01_tabelle.sql", cp1252("-- tabelle\ncreate table x;\n")),
            file("POSTGRES/AGGIORNAMENTO/4_11__4_12.sql", cp1252("-- perché\n")),
            file("DOCUMENTAZIONE/note.txt", b"not a script".to_vec()),
        ]
    }

    fn planned() -> Proposal {
        plan(Path::new("/repo/prod-core"), &repository(), None)
    }

    #[test]
    fn branches_are_the_top_level_folders_and_carry_the_dialect() {
        let p = planned();
        let ids: Vec<&str> = p.project.branches.iter().map(|b| b.id.as_str()).collect();
        assert_eq!(ids, ["oracle", "postgres"]);
        assert_eq!(p.project.branches[0].dialect, Some(EngineKind::Oracle));
        assert_eq!(p.project.branches[1].dialect, Some(EngineKind::Postgres));
    }

    #[test]
    fn a_folder_with_no_script_files_does_not_become_a_folder() {
        // DOCUMENTAZIONE holds one .txt, so it is not part of the project at all.
        let p = planned();
        assert!(p.project.branches.iter().all(|b| b.id != "documentazione"));
        assert_eq!(p.project.all_files().count(), 5);
    }

    #[test]
    fn roles_come_from_the_folder_names() {
        let p = planned();
        let oracle = &p.project.branches[0];
        let roles: Vec<FolderRole> = oracle.folders.iter().map(|f| f.role).collect();
        // BTreeMap order: AGGIORNAMENTO before INIZIALIZZAZIONE.
        assert_eq!(roles, [FolderRole::Update, FolderRole::Init]);
    }

    #[test]
    fn a_subfolder_inherits_the_role_of_what_it_is_inside() {
        let mut files = repository();
        files.push(file("ORACLE/AGGIORNAMENTO/2026/4_12__4_13.sql", cp1252("-- x\r\n")));
        let p = plan(Path::new("/repo/prod-core"), &files, None);
        let nested = p.project.branches[0]
            .folders
            .iter()
            .find(|f| f.path == "ORACLE/AGGIORNAMENTO/2026")
            .expect("the nested folder");
        assert_eq!(nested.role, FolderRole::Update);
    }

    #[test]
    fn the_output_does_not_depend_on_the_order_the_files_arrived_in() {
        // The filesystem's enumeration order must never reach the user.
        let mut reversed = repository();
        reversed.reverse();
        let a = plan(Path::new("/repo/prod-core"), &repository(), None);
        let b = plan(Path::new("/repo/prod-core"), &reversed, None);
        assert_eq!(a.project, b.project);
        assert_eq!(a.config, b.config);
    }

    #[test]
    fn an_unrecognised_branch_gets_no_dialect_and_says_so() {
        let files = vec![file("COMMON/x.sql", cp1252("select 1;\r\n"))];
        let p = plan(Path::new("/repo/prod-core"), &files, None);
        assert_eq!(p.project.branches[0].dialect, None);
        let note = p.notes.iter().find(|n| n.path == "COMMON").expect("a note about it");
        assert!(note.needs_attention);
        assert!(note.message.contains("engine"));
    }

    #[test]
    fn an_unrecognised_folder_is_ignored_and_flagged() {
        let files = vec![file("ORACLE/MISCELLANEA/x.sql", cp1252("select 1;\r\n"))];
        let p = plan(Path::new("/repo/prod-core"), &files, None);
        assert_eq!(p.project.branches[0].folders[0].role, FolderRole::Ignored);
        let note = p
            .notes
            .iter()
            .find(|n| n.path == "ORACLE/MISCELLANEA")
            .expect("a note about it");
        assert!(note.needs_attention);
    }

    #[test]
    fn encoding_is_detected_and_ascii_files_inherit_the_folders() {
        let p = planned();
        let init = p.project.branches[0]
            .folders
            .iter()
            .find(|f| f.path == "ORACLE/INIZIALIZZAZIONE")
            .expect("the init folder");

        // 02_PARAMETRI has an accented character, so it decides the folder…
        let parametri = init.files.iter().find(|f| f.name == "02_PARAMETRI.sql").unwrap();
        assert_eq!(parametri.encoding, "windows-1252");

        // …and 01_TABELLE, which is pure ASCII, inherits rather than being guessed.
        let tabelle = init.files.iter().find(|f| f.name == "01_TABELLE.sql").unwrap();
        assert_eq!(tabelle.encoding, "windows-1252");
        assert_eq!(tabelle.encoding_source.as_str(), "inherited");
    }

    #[test]
    fn a_file_that_drifted_is_reported_without_needing_attention() {
        // A UTF-8 file inside a windows-1252 folder: the ENC001 case.
        let mut files = repository();
        files.push(file(
            "ORACLE/INIZIALIZZAZIONE/03_CLIENTI.sql",
            "-- perché\r\nINSERT INTO CLIENTI VALUES ('X');\r\n".as_bytes().to_vec(),
        ));
        let p = plan(Path::new("/repo/prod-core"), &files, None);
        let drifted = p
            .project
            .all_files()
            .find(|f| f.name == "03_CLIENTI.sql")
            .expect("the drifted file");
        assert_eq!(drifted.encoding, "UTF-8");
        assert_eq!(drifted.expected_encoding, "windows-1252");
        assert!(drifted.encoding_drifted());

        let note = p.notes.iter().find(|n| n.path.ends_with("03_CLIENTI.sql")).expect("a note");
        // Reported, but it is not a question — Picus knows exactly what happened.
        assert!(!note.needs_attention);
    }

    #[test]
    fn line_endings_are_per_file_and_the_project_default_is_the_majority() {
        let p = planned();
        let oracle_file = p.project.all_files().find(|f| f.path.starts_with("ORACLE/A")).unwrap();
        let pg_file = p.project.all_files().find(|f| f.path.starts_with("POSTGRES/A")).unwrap();
        assert_eq!(oracle_file.eol, LineEnding::Crlf);
        assert_eq!(pg_file.eol, LineEnding::Lf);
        // 3 CRLF files against 2 LF ones.
        assert_eq!(p.config.encoding.eol, LineEnding::Crlf);
    }

    #[test]
    fn an_existing_configuration_wins_over_every_inference() {
        let first = planned();
        let mut config = first.config.clone();
        // The user disagreed: this is a data folder, not an initialisation one.
        let folder = config.branches[0]
            .folders
            .iter_mut()
            .find(|f| f.path == "ORACLE/INIZIALIZZAZIONE")
            .unwrap();
        folder.role = FolderRole::Data;
        folder.label = "Reference data".to_string();

        let second = plan(Path::new("/repo/prod-core"), &repository(), Some(&config));
        let init = second.project.branches[0]
            .folders
            .iter()
            .find(|f| f.path == "ORACLE/INIZIALIZZAZIONE")
            .unwrap();
        assert_eq!(init.role, FolderRole::Data);
        assert_eq!(init.label, "Reference data");
        // …and nothing is proposed, because there is nothing to confirm.
        assert!(!second.is_new);
        assert_eq!(second.config, config);
    }

    #[test]
    fn a_configured_folder_encoding_outranks_the_vote() {
        let mut config = planned().config;
        config.branches[0]
            .folders
            .iter_mut()
            .find(|f| f.path == "ORACLE/INIZIALIZZAZIONE")
            .unwrap()
            .encoding = Some("UTF-8".to_string());

        let p = plan(Path::new("/repo/prod-core"), &repository(), Some(&config));
        let init = p.project.branches[0]
            .folders
            .iter()
            .find(|f| f.path == "ORACLE/INIZIALIZZAZIONE")
            .unwrap();
        // Every file in the folder is now measured against UTF-8, so the
        // windows-1252 one reads as drift — which is the point of pinning it.
        assert!(init.files.iter().all(|f| f.expected_encoding == "UTF-8"));
        assert!(init.files.iter().any(|f| f.encoding_drifted()));
    }

    #[test]
    fn files_at_the_root_still_belong_somewhere() {
        let files = vec![file("install.sql", cp1252("select 1;\r\n"))];
        let p = plan(Path::new("/repo/prod-core"), &files, None);
        assert_eq!(p.project.branches.len(), 1);
        assert_eq!(p.project.branches[0].id, "root");
        assert_eq!(p.project.branches[0].dialect, None);
        assert_eq!(p.project.all_files().count(), 1);
    }

    #[test]
    fn slugs_are_stable_and_never_empty() {
        assert_eq!(slug("ORACLE/AGGIORNAMENTO"), "oracle-aggiornamento");
        assert_eq!(slug("01_INIZIALIZZAZIONE"), "01-inizializzazione");
        assert_eq!(slug("///"), "root");
        assert_eq!(slug(""), "root");
    }
}
