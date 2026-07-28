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
//! ## The tree is the repository's own
//!
//! Every directory holding scripts becomes a [`FolderNode`] where it actually
//! sits — no branches, no flattening, no two-level assumption. Directories in
//! between are kept even when they hold no scripts themselves, because they are
//! what a declaration inherits *through*.
//!
//! What each folder declares comes from the project file if it says anything
//! about that path, and from [`crate::infer`] otherwise; what *applies* to it is
//! then [`crate::resolve`]'s answer. When a project file already exists it
//! **wins** over every inference, and discovery only fills in what the file cannot
//! know — which files exist and what encoding they turned out to be.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use arbor_fs::prelude::encoding::{detect_in_context, EncodingContext};
use picus_types::prelude::{FolderEngine, FolderRole};

use crate::alias::AliasVocabulary;
use crate::config::{
    AnalysisSettings, EncodingSettings, FolderDeclaration, GenerationSettings, ProjectConfig,
    VersionTableSettings, CURRENT_VERSION, DEFAULT_ENCODING,
};
use crate::error::ProjectError;
use crate::infer::{infer_engine_in, infer_file_engine_in, infer_role_in};
use crate::naming::NamingScheme;
use crate::path::{last_segment, parent_of};
use crate::resolve::resolve_from;
use crate::tree::{FolderNode, LineEnding, Project, ScriptFile};

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

/// Files of one directory, keyed by the directory's project-relative path.
type FilesByDir<'a> = BTreeMap<&'a str, Vec<&'a SourceFile>>;

/// Work out what a repository is, from its files. Pure.
pub fn plan(root: &Path, files: &[SourceFile], existing: Option<&ProjectConfig>) -> Proposal {
    let name = existing
        .map(|c| c.name.clone())
        .or_else(|| root.file_name().map(|n| n.to_string_lossy().to_string()))
        .unwrap_or_else(|| "project".to_string());

    // `BTreeMap`/`BTreeSet` throughout: the order of folders and files is
    // user-visible, and it must not depend on how the filesystem happened to
    // enumerate them.
    let mut by_dir: FilesByDir = BTreeMap::new();
    for file in files.iter().filter(|f| is_script(&f.path)) {
        by_dir.entry(parent_of(&file.path)).or_default().push(file);
    }
    let dirs = directories(&by_dir);

    let builder = Builder {
        existing,
        // Compiled once per scan and consulted for every folder. A repository
        // that declares nothing gets `EMPTY`, so there is one code path rather
        // than a "with vocabulary" and a "without" one.
        aliases: existing.map(ProjectConfig::vocabulary).unwrap_or(AliasVocabulary::EMPTY),
        default_encoding: existing
            .map(|c| c.encoding.default.clone())
            .unwrap_or_else(|| DEFAULT_ENCODING.to_string()),
    };

    let mut tree = builder.children_of("", &dirs, &by_dir);
    // Scripts sitting in the repository root itself: a node of their own, so they
    // are visible and classifiable like everything else. Its path is `""`, and it
    // declares nothing — a declaration on `""` is the *repository's*, which every
    // top-level folder inherits from, so it is passed to the resolver instead.
    if let Some(root_files) = by_dir.get("") {
        let node = FolderNode {
            files: builder.script_files("", root_files),
            ..FolderNode::new("", name.clone())
        };
        tree.insert(0, node);
    }

    let root_declaration = existing.and_then(|c| c.declaration(""));
    resolve_from(
        &mut tree,
        root_declaration.and_then(|d| d.dialect),
        root_declaration.and_then(|d| d.role),
        // A declaration on `""` is the repository's own and applies to everything,
        // exclusion included. Passing only the other two here would have made
        // `path = "" excluded = true` a line that parses, saves, and does nothing.
        root_declaration.and_then(|d| d.excluded).unwrap_or(false),
    );

    let config = match existing {
        // An existing file is authoritative and is never rewritten by a scan: a
        // folder that disappeared from disk stays in the file until the user says
        // otherwise, because deleting their configuration behind their back is
        // exactly the behaviour this design refuses.
        Some(existing) => existing.clone(),
        None => proposed_config(&name, &tree, &builder.default_encoding),
    };

    Proposal {
        notes: notes(&tree),
        project: Project { name, root: root.display().to_string(), tree },
        config,
        is_new: existing.is_none(),
    }
}

/// Every directory the tree needs: the ones holding scripts, plus every
/// directory above them, which is what a declaration inherits through.
fn directories<'a>(by_dir: &FilesByDir<'a>) -> BTreeSet<&'a str> {
    let mut out = BTreeSet::new();
    for dir in by_dir.keys().filter(|d| !d.is_empty()) {
        let mut current: &str = dir;
        while !current.is_empty() && out.insert(current) {
            current = parent_of(current);
        }
    }
    out
}

/// Turns directories into folders. Holds the two things every node needs and
/// nothing else.
struct Builder<'a> {
    existing: Option<&'a ProjectConfig>,
    /// The project's own folder-name vocabulary, compiled once for the scan.
    aliases: AliasVocabulary,
    default_encoding: String,
}

impl Builder<'_> {
    /// The folders directly inside `parent`, each with its own subtree.
    fn children_of(
        &self,
        parent: &str,
        dirs: &BTreeSet<&str>,
        by_dir: &FilesByDir<'_>,
    ) -> Vec<FolderNode> {
        dirs.iter()
            .filter(|dir| parent_of(dir) == parent)
            .map(|dir| self.node(dir, dirs, by_dir))
            .collect()
    }

    fn node(&self, path: &str, dirs: &BTreeSet<&str>, by_dir: &FilesByDir<'_>) -> FolderNode {
        let name = last_segment(path);
        let (engine, role) = self.declared(path, name);
        let files = by_dir.get(path).map(|files| self.script_files(path, files)).unwrap_or_default();

        FolderNode {
            engine,
            role,
            // Never inferred, only ever declared — the same rule `generic`
            // follows, and for the same reason: dropping somebody's scripts out
            // of the report is not a conclusion a folder name is allowed to
            // reach on their behalf.
            excluded: self.existing.and_then(|c| c.declaration(path)).and_then(|d| d.excluded),
            files,
            children: self.children_of(path, dirs, by_dir),
            ..FolderNode::new(path, name)
        }
    }

    /// What this folder declares: the project file's word if it has one about
    /// this path, then the project's own vocabulary, then the built-in one.
    ///
    /// That order is the whole precedence rule, and each step earns its place. A
    /// **per-path declaration** is a specific answer about this folder and beats
    /// everything. An **alias** is a fact its owner knows about this repository —
    /// `POS` is PostgreSQL here — and beats a global heuristic that cannot know
    /// it. The **built-in vocabulary** is that heuristic, and it is last.
    ///
    /// A declaration is authoritative for **both** fields, including for the one
    /// it leaves absent — that absence is the user saying "inherit", and
    /// re-inferring over it would undo a correction on every rescan. Note that an
    /// alias does *not* get that treatment: it is inference, so a folder the
    /// project file mentions at all ignores it entirely.
    fn declared(&self, path: &str, name: &str) -> (Option<FolderEngine>, Option<FolderRole>) {
        if let Some(declared) = self.existing.and_then(|c| c.declaration(path)) {
            return (declared.dialect, declared.role);
        }
        let role = infer_role_in(name, &self.aliases);
        (
            infer_engine_in(name, &self.aliases).map(|guess| guess.value),
            // Only a confident guess declares anything. The fallback is `Ignored`,
            // and declaring that here would stop a role inherited from above ever
            // reaching a folder called `2024`.
            role.is_confident().then_some(role.value),
        )
    }

    /// The folder's files, with the encoding each turned out to be.
    fn script_files(&self, path: &str, files: &[&SourceFile]) -> Vec<ScriptFile> {
        // The folder's own encoding vote. Files that are pure ASCII abstain,
        // which is exactly right: they are the ones being decided.
        let mut context =
            EncodingContext::new().with_legacy(label_to_encoding(&self.default_encoding));
        if let Some(pinned) = self.existing.and_then(|c| c.declared_encoding(path)) {
            context = context.with_dominant(label_to_encoding(pinned));
        }
        for file in files {
            context.observe(&file.sample);
        }
        let expected = context
            .dominant()
            .map(|e| e.name().to_string())
            .unwrap_or_else(|| self.default_encoding.clone());

        let mut out: Vec<ScriptFile> = files
            .iter()
            .map(|file| {
                let detection = detect_in_context(&file.sample, &context);
                let name = last_segment(&file.path).to_string();
                ScriptFile {
                    engine: self.file_engine(&file.path, &name),
                    excluded: self
                        .existing
                        .and_then(|c| c.file_declaration(&file.path))
                        .and_then(|d| d.excluded),
                    effective_excluded: false,
                    path: file.path.clone(),
                    name,
                    size: file.size,
                    encoding: detection.encoding.name().to_string(),
                    encoding_source: detection.source,
                    eol: LineEnding::detect(&file.sample),
                    expected_encoding: expected.clone(),
                    // Filled by `resolve`, which owns inheritance for the whole
                    // tree — a folder's and a file's alike.
                    effective_engine: None,
                }
            })
            .collect();
        out.sort_by(|a, b| a.path.cmp(&b.path));
        out
    }

    /// What this **file** declares about itself, if anything.
    ///
    /// The same two-step precedence as a folder's, minus the third: a per-path
    /// `[[file]]` declaration is a specific answer and beats everything; the
    /// project's own vocabulary answers for names that repeat; and there is no
    /// built-in step at all, because a file name is a sentence and the global list
    /// has no business reading engines out of one — see
    /// [`crate::infer::infer_file_engine_in`].
    ///
    /// `None` is the answer for almost every file in almost every repository, and
    /// it means "my folder's", which is where this has always got its answer.
    fn file_engine(&self, path: &str, name: &str) -> Option<FolderEngine> {
        if let Some(declared) = self.existing.and_then(|c| c.file_declaration(path)) {
            // Unlike a folder's, this declaration carries one field, so an empty
            // one is dropped by `tidy` rather than meaning "inherit deliberately".
            // If it is here at all it says something.
            if declared.dialect.is_some() {
                return declared.dialect;
            }
        }
        infer_file_engine_in(name, &self.aliases).map(|guess| guess.value)
    }
}

/// The configuration a brand-new project is proposed with.
fn proposed_config(name: &str, tree: &[FolderNode], default_encoding: &str) -> ProjectConfig {
    let mut folders = Vec::new();
    declarations(tree, (None, None), &mut folders);
    ProjectConfig {
        version: CURRENT_VERSION,
        name: name.to_string(),
        encoding: EncodingSettings {
            default: default_encoding.to_string(),
            eol: dominant_eol(tree),
        },
        version_table: VersionTableSettings::default(),
        generation: GenerationSettings::default(),
        naming: NamingScheme::default(),
        // Nothing is switched off on a first read, and the initialisation model
        // takes its default: it is a fact about how the team works, and guessing
        // it from a directory listing would be a guess dressed up as a setting.
        analysis: AnalysisSettings::default(),
        folders,
        // Nothing classifies a single file on a first read. A `[[file]]`
        // declaration is a correction to a file Picus placed wrongly, and there is
        // nothing to correct yet.
        files: Vec::new(),
        // A repository being read for the first time has no vocabulary: an alias
        // is something its owner tells Picus, never something Picus invents. The
        // interface offers to add one at the moment a folder is classified, which
        // is where the knowledge actually is.
        aliases: Vec::new(),
    }
}

/// One declaration per folder that says something its ancestors did not.
///
/// A folder whose inferred dialect is the one it would have inherited anyway
/// writes nothing: the file then reads as the handful of decisions the repository
/// actually embodies, rather than as a line per directory.
fn declarations(
    nodes: &[FolderNode],
    inherited: (Option<FolderEngine>, Option<FolderRole>),
    out: &mut Vec<FolderDeclaration>,
) {
    for node in nodes {
        let engine = node.engine;
        let mut declaration = FolderDeclaration::new(&node.path);
        if engine.is_some() && engine != inherited.0 {
            declaration.dialect = engine;
        }
        if node.role.is_some() && node.role != inherited.1 {
            declaration.role = node.role;
        }
        if !declaration.is_empty() {
            out.push(declaration);
        }
        let below = (engine.or(inherited.0), node.role.or(inherited.1));
        declarations(&node.children, below, out);
    }
}

/// What the user should look at before confirming.
///
/// Only folders that actually hold scripts produce a question: a directory that
/// exists solely because something below it holds files is not a decision anybody
/// has to make.
///
/// Neither is a folder written in an engine Picus does not support. That is an
/// **answer** — "these are SQL Server scripts" — and there is nothing the user
/// could do with the question. A tool that keeps asking something you have
/// already answered is one people stop reading, and this report is the one part
/// of Picus that must keep being read.
fn notes(tree: &[FolderNode]) -> Vec<ProposalNote> {
    let mut out = Vec::new();
    for node in tree.iter().flat_map(FolderNode::walk) {
        if node.files.is_empty() {
            continue;
        }
        // Excluded means "pretend this is not in the repository", and a question
        // about something that is not in the repository is exactly the noise this
        // report cannot afford. Not even the encoding notes below: a file nobody
        // will ever generate into cannot have drifted from anything that matters.
        if node.is_excluded() && node.files.iter().all(ScriptFile::is_excluded) {
            continue;
        }
        if node.engine_is_unsupported() {
            // Not silent about the file-level facts below, though: an encoding
            // that drifted is still worth saying, whoever owns the scripts.
            out.extend(node.included_files().filter(|f| f.encoding_drifted()).map(|f| drifted(node, f)));
            continue;
        }
        if node.effective_role == FolderRole::Ignored {
            // A folder somebody (or a keyword) called `ignored` is a decision.
            // One that merely fell through to it is the question.
            if node.role != Some(FolderRole::Ignored) {
                out.push(unknown_role(node));
            }
        } else if node.included_files().any(ScriptFile::engine_is_unknown) {
            // Asked only where it matters, on two counts. An ignored folder
            // receives nothing whatever its dialect, so pairing the two questions
            // would double the list for no extra decision. And a folder whose
            // files have each answered for themselves — the untidy repository
            // where the engine is in the file name — has nothing left to ask,
            // even though the folder itself could never say what engine it is.
            out.push(unknown_dialect(node));
        }
        for file in node.included_files() {
            if file.encoding_drifted() {
                out.push(drifted(node, file));
            }
            if let Some(note) = disagrees_with_folder(node, file) {
                out.push(note);
            }
        }
    }
    out
}

/// A file that classified itself as something other than the folder it is in.
///
/// Only where the folder **declared** its engine, and only when the two differ:
/// that is the surprising case, and the one worth a line. In the repository this
/// feature exists for the folder declares nothing and the files answer for
/// themselves, which is ordinary and produces no note — otherwise the report
/// would be one line per file, which is a report nobody reads.
///
/// Not a question. Picus is not confused about what happened; it is telling the
/// user that a specific answer is overruling a general one, so a `POS_TERMINALI.sql`
/// that reads as PostgreSQL inside a declared Oracle folder is visible instead of
/// silent.
fn disagrees_with_folder(node: &FolderNode, file: &ScriptFile) -> Option<ProposalNote> {
    let declared = file.engine?;
    let folder = node.effective_engine?;
    if declared == folder {
        return None;
    }
    Some(ProposalNote {
        path: file.path.clone(),
        message: format!(
            "this file reads as {} from its name, while `{}` is {} — the file wins, and nothing \
             else in the folder is affected",
            declared.label(),
            node.name,
            folder.label()
        ),
        needs_attention: false,
    })
}

fn unknown_role(node: &FolderNode) -> ProposalNote {
    ProposalNote {
        path: node.path.clone(),
        message: format!(
            "nothing above `{}` says what these scripts are for, so it is marked as ignored — \
             nothing will be generated into it until you say what it is",
            node.name
        ),
        needs_attention: true,
    }
}

fn unknown_dialect(node: &FolderNode) -> ProposalNote {
    ProposalNote {
        path: node.path.clone(),
        message: format!(
            "nothing in the name `{}`, or above it, says which engine these scripts are written \
             in — pick one, or leave it unset and nothing will be generated into it. If every \
             folder called `{}` means the same engine, say so once for the whole project rather \
             than folder by folder",
            node.name, node.name
        ),
        needs_attention: true,
    }
}

fn drifted(node: &FolderNode, file: &ScriptFile) -> ProposalNote {
    ProposalNote {
        path: file.path.clone(),
        message: format!(
            "this file is {} while the rest of `{}` is {} — it was probably rewritten by an \
             editor that did not know",
            file.encoding, node.name, file.expected_encoding
        ),
        needs_attention: false,
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

/// The line ending most of the project uses, for generated content to match.
fn dominant_eol(tree: &[FolderNode]) -> LineEnding {
    let mut lf = 0usize;
    let mut crlf = 0usize;
    for file in tree.iter().flat_map(FolderNode::all_files) {
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

#[cfg(test)]
mod tests {
    use super::*;
    use picus_types::prelude::{EngineKind, ForeignEngine};

    fn supported(kind: EngineKind) -> Option<FolderEngine> {
        Some(FolderEngine::Supported(kind))
    }

    fn cp1252(text: &str) -> Vec<u8> {
        encoding_rs::WINDOWS_1252.encode(text).0.into_owned()
    }

    fn file(path: &str, bytes: Vec<u8>) -> SourceFile {
        SourceFile { path: path.to_string(), size: bytes.len() as u64, sample: bytes }
    }

    /// A two-dialect repository shaped like the ones this product was built for.
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

    fn at<'a>(proposal: &'a Proposal, path: &str) -> &'a FolderNode {
        proposal.project.folder_at(path).unwrap_or_else(|| panic!("no folder at {path}"))
    }

    #[test]
    fn the_tree_is_the_directory_hierarchy_and_nothing_else() {
        let p = planned();
        let paths: Vec<&str> = p.project.walk().map(|n| n.path.as_str()).collect();
        assert_eq!(
            paths,
            [
                "ORACLE",
                "ORACLE/AGGIORNAMENTO",
                "ORACLE/INIZIALIZZAZIONE",
                "POSTGRES",
                "POSTGRES/AGGIORNAMENTO",
                "POSTGRES/INIZIALIZZAZIONE",
            ]
        );
    }

    #[test]
    fn the_dialect_is_declared_where_the_name_says_it_and_inherited_below() {
        let p = planned();
        assert_eq!(at(&p, "ORACLE").engine, supported(EngineKind::Oracle));
        // Declared once, at the top; the folders under it say nothing and mean it.
        assert_eq!(at(&p, "ORACLE/AGGIORNAMENTO").engine, None);
        assert_eq!(at(&p, "ORACLE/AGGIORNAMENTO").effective_dialect(), Some(EngineKind::Oracle));
        assert_eq!(at(&p, "POSTGRES/INIZIALIZZAZIONE").effective_dialect(), Some(EngineKind::Postgres));
    }

    #[test]
    fn the_dialect_can_sit_at_the_bottom_of_the_tree_and_the_role_at_the_top() {
        // The repository this whole shape exists for. Nothing here is a branch:
        // `AGGIORNAMENTO` is three levels above the folder that says `ORA`.
        let files = vec![
            file("AGGIORNAMENTO/2024/ORA/4_12.sql", cp1252("-- x\r\n")),
            file("AGGIORNAMENTO/2024/POS/4_12.sql", cp1252("-- x\r\n")),
            file("INIZIALIZZAZIONE/2024/ORA/01.sql", cp1252("-- x\r\n")),
        ];
        let p = plan(Path::new("/repo/prod-core"), &files, None);

        let ora = at(&p, "AGGIORNAMENTO/2024/ORA");
        assert_eq!(ora.effective_dialect(), Some(EngineKind::Oracle));
        assert_eq!(ora.effective_role, FolderRole::Update);
        // `POS` matches nothing Picus knows, and inventing a dialect for it is the
        // failure this product exists to catch. The user is asked instead.
        let pos = at(&p, "AGGIORNAMENTO/2024/POS");
        assert_eq!(pos.effective_dialect(), None);
        assert_eq!(pos.effective_role, FolderRole::Update);
        let note = p.notes.iter().find(|n| n.path == "AGGIORNAMENTO/2024/POS").expect("a note");
        assert!(note.needs_attention);
        assert!(note.message.contains("engine"), "{}", note.message);
        // …and the same leaf name under another role keeps that other role.
        assert_eq!(at(&p, "INIZIALIZZAZIONE/2024/ORA").effective_role, FolderRole::Init);
    }

    #[test]
    fn the_proposed_file_declares_only_what_is_not_inherited() {
        let files = vec![
            file("AGGIORNAMENTO/2024/ORA/4_12.sql", cp1252("-- x\r\n")),
            file("AGGIORNAMENTO/2025/ORA/4_13.sql", cp1252("-- x\r\n")),
        ];
        let p = plan(Path::new("/repo/prod-core"), &files, None);
        let declared: Vec<(&str, Option<FolderEngine>, Option<FolderRole>)> =
            p.config.folders.iter().map(|f| (f.path.as_str(), f.dialect, f.role)).collect();
        assert_eq!(
            declared,
            [
                ("AGGIORNAMENTO", None, Some(FolderRole::Update)),
                ("AGGIORNAMENTO/2024/ORA", supported(EngineKind::Oracle), None),
                ("AGGIORNAMENTO/2025/ORA", supported(EngineKind::Oracle), None),
            ]
        );
    }

    #[test]
    fn a_folder_with_no_script_files_does_not_become_a_folder() {
        // DOCUMENTAZIONE holds one .txt, so it is not part of the project at all.
        let p = planned();
        assert!(p.project.folder_at("DOCUMENTAZIONE").is_none());
        assert_eq!(p.project.all_files().count(), 5);
    }

    #[test]
    fn roles_come_from_the_folder_names() {
        let p = planned();
        assert_eq!(at(&p, "ORACLE/AGGIORNAMENTO").effective_role, FolderRole::Update);
        assert_eq!(at(&p, "ORACLE/INIZIALIZZAZIONE").effective_role, FolderRole::Init);
        // The folder that only holds other folders declares nothing.
        assert_eq!(at(&p, "ORACLE").role, None);
    }

    #[test]
    fn a_subfolder_inherits_the_role_of_what_it_is_inside() {
        let mut files = repository();
        files.push(file("ORACLE/AGGIORNAMENTO/2026/4_12__4_13.sql", cp1252("-- x\r\n")));
        let p = plan(Path::new("/repo/prod-core"), &files, None);
        let nested = at(&p, "ORACLE/AGGIORNAMENTO/2026");
        assert_eq!(nested.role, None, "`2026` says nothing about itself");
        assert_eq!(nested.effective_role, FolderRole::Update);
        assert_eq!(nested.effective_dialect(), Some(EngineKind::Oracle));
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
        assert_eq!(a.notes, b.notes);
    }

    #[test]
    fn an_unrecognised_folder_gets_no_dialect_and_says_so() {
        let files = vec![file("COMMON/AGGIORNAMENTO/x.sql", cp1252("select 1;\r\n"))];
        let p = plan(Path::new("/repo/prod-core"), &files, None);
        assert_eq!(at(&p, "COMMON/AGGIORNAMENTO").effective_dialect(), None);
        let note = p.notes.iter().find(|n| n.path == "COMMON/AGGIORNAMENTO").expect("a note");
        assert!(note.needs_attention);
        assert!(note.message.contains("engine"));
    }

    #[test]
    fn an_unrecognised_folder_is_ignored_and_flagged() {
        let files = vec![file("ORACLE/MISCELLANEA/x.sql", cp1252("select 1;\r\n"))];
        let p = plan(Path::new("/repo/prod-core"), &files, None);
        assert_eq!(at(&p, "ORACLE/MISCELLANEA").effective_role, FolderRole::Ignored);
        let note = p.notes.iter().find(|n| n.path == "ORACLE/MISCELLANEA").expect("a note");
        assert!(note.needs_attention);
        // One question, not two: an ignored folder receives nothing whatever its
        // dialect, so it is not also asked about that.
        assert_eq!(p.notes.iter().filter(|n| n.path == "ORACLE/MISCELLANEA").count(), 1);
    }

    #[test]
    fn a_folder_that_only_holds_other_folders_is_never_a_question() {
        let files = vec![file("ORACLE/AGGIORNAMENTO/x.sql", cp1252("select 1;\r\n"))];
        let p = plan(Path::new("/repo/prod-core"), &files, None);
        assert!(p.notes.is_empty(), "{:?}", p.notes);
    }

    #[test]
    fn encoding_is_detected_and_ascii_files_inherit_the_folders() {
        let p = planned();
        let init = at(&p, "ORACLE/INIZIALIZZAZIONE");

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
        config.declaration_mut("ORACLE/INIZIALIZZAZIONE").role = Some(FolderRole::Data);

        let second = plan(Path::new("/repo/prod-core"), &repository(), Some(&config));
        assert_eq!(at(&second, "ORACLE/INIZIALIZZAZIONE").effective_role, FolderRole::Data);
        // …and nothing is proposed, because there is nothing to confirm.
        assert!(!second.is_new);
        assert_eq!(second.config, config);
    }

    #[test]
    fn a_declaration_that_clears_a_dialect_is_not_re_inferred() {
        // The user looked at `ORACLE`, said "actually nobody knows", and a rescan
        // must not overrule them by reading the folder's name again.
        let mut config = planned().config;
        config.declaration_mut("ORACLE").dialect = None;
        config.declaration_mut("ORACLE").role = Some(FolderRole::Ignored);

        let p = plan(Path::new("/repo/prod-core"), &repository(), Some(&config));
        assert_eq!(at(&p, "ORACLE").effective_dialect(), None);
        assert_eq!(at(&p, "ORACLE/AGGIORNAMENTO").effective_dialect(), None);
    }

    #[test]
    fn a_configured_folder_encoding_outranks_the_vote_and_reaches_the_folders_below() {
        let mut config = planned().config;
        config.declaration_mut("ORACLE").encoding = Some("UTF-8".to_string());

        let p = plan(Path::new("/repo/prod-core"), &repository(), Some(&config));
        let init = at(&p, "ORACLE/INIZIALIZZAZIONE");
        // Every file in the folder is now measured against UTF-8, so the
        // windows-1252 one reads as drift — which is the point of pinning it.
        assert!(init.files.iter().all(|f| f.expected_encoding == "UTF-8"));
        assert!(init.files.iter().any(|f| f.encoding_drifted()));
        // …and a folder in another part of the tree is untouched.
        assert!(at(&p, "POSTGRES/INIZIALIZZAZIONE")
            .files
            .iter()
            .all(|f| f.expected_encoding == "windows-1252"));
    }

    #[test]
    fn files_at_the_root_still_belong_somewhere() {
        let files = vec![
            file("install.sql", cp1252("select 1;\r\n")),
            file("ORACLE/AGGIORNAMENTO/x.sql", cp1252("select 1;\r\n")),
        ];
        let p = plan(Path::new("/repo/prod-core"), &files, None);
        // The root is a folder of its own, first, named after the repository.
        assert_eq!(p.project.tree[0].path, "");
        assert_eq!(p.project.tree[0].name, "prod-core");
        assert_eq!(p.project.tree[0].files.len(), 1);
        assert_eq!(p.project.tree[0].effective_dialect(), None);
        assert_eq!(p.project.all_files().count(), 2);
    }

    // ── The project's own vocabulary, end to end ──────────────────────────────

    /// The repository this feature exists for: one folder set per delivered
    /// version, four engines, eleven of each.
    fn versioned_repository() -> Vec<SourceFile> {
        let mut files = Vec::new();
        for version in ["4_11", "4_12", "4_13"] {
            for engine in ["ORA", "POS", "MSQ", "DB"] {
                files.push(file(
                    &format!("AGGIORNAMENTO/{version}/{engine}/{version}.sql"),
                    cp1252("-- x\r\n"),
                ));
            }
        }
        files
    }

    /// A configuration that declares nothing but the vocabulary under test.
    fn with_aliases(entries: &[(&str, Option<&str>, Option<&str>)]) -> ProjectConfig {
        let mut config = ProjectConfig {
            version: CURRENT_VERSION,
            name: "PROD_CORE".to_string(),
            encoding: EncodingSettings::default(),
            version_table: VersionTableSettings::default(),
            generation: GenerationSettings::default(),
            naming: NamingScheme::default(),
            analysis: AnalysisSettings::default(),
            folders: Vec::new(),
            files: Vec::new(),
            aliases: Vec::new(),
        };
        for (name, engine, role) in entries {
            let alias = config.alias_mut(name);
            alias.engine = engine.map(str::to_string);
            alias.role = role.map(str::to_string);
        }
        config
    }

    #[test]
    fn one_alias_classifies_every_folder_of_that_name_at_once() {
        // Eleven folders in the real repository, three here. Declaring `POS` once
        // is the difference between one decision and one per delivered version —
        // and the reason this is a name and not a path.
        let config = with_aliases(&[("POS", Some("postgres"), None)]);
        let p = plan(Path::new("/repo/prod-core"), &versioned_repository(), Some(&config));

        for version in ["4_11", "4_12", "4_13"] {
            let folder = at(&p, &format!("AGGIORNAMENTO/{version}/POS"));
            assert_eq!(folder.effective_dialect(), Some(EngineKind::Postgres), "{version}");
            // …and the role still comes from the top of the tree, untouched.
            assert_eq!(folder.effective_role, FolderRole::Update, "{version}");
        }
        // Nothing was written into the file to achieve it: the alias is the whole
        // declaration, and a `POS` folder added next month needs no further edit.
        assert!(p.config.folders.is_empty());
    }

    #[test]
    fn an_alias_naming_an_unsupported_engine_makes_the_folder_go_quiet() {
        // MSQ is SQL Server and DB is DB2. Neither is a question, so neither
        // produces a note — and neither is ever parsed.
        let config = with_aliases(&[
            ("POS", Some("postgres"), None),
            ("MSQ", Some("sqlserver"), None),
            ("DB", Some("db2"), None),
        ]);
        let p = plan(Path::new("/repo/prod-core"), &versioned_repository(), Some(&config));

        let msq = at(&p, "AGGIORNAMENTO/4_12/MSQ");
        assert_eq!(msq.effective_engine.and_then(FolderEngine::foreign), Some(ForeignEngine::SqlServer));
        assert_eq!(msq.effective_dialect(), None, "nothing is parsed with it");
        assert!(msq.engine_is_unsupported() && !msq.engine_is_unknown());
        assert_eq!(at(&p, "AGGIORNAMENTO/4_11/DB").effective_engine.and_then(FolderEngine::foreign), Some(ForeignEngine::Db2));

        // The whole point: the repository is fully described, so there is nothing
        // left to ask about.
        assert!(p.notes.is_empty(), "{:?}", p.notes);
    }

    #[test]
    fn without_the_vocabulary_the_same_repository_asks_nine_times() {
        // The state this feature replaces, asserted so the improvement is not a
        // claim: POS, MSQ and DB in each of the three versions.
        let p = plan(Path::new("/repo/prod-core"), &versioned_repository(), None);
        let asked: Vec<&str> =
            p.notes.iter().filter(|n| n.needs_attention).map(|n| n.path.as_str()).collect();
        assert_eq!(asked.len(), 9, "{asked:?}");
        assert!(asked.iter().all(|path| path.ends_with("POS")
            || path.ends_with("MSQ")
            || path.ends_with("DB")));
    }

    #[test]
    fn a_per_path_declaration_beats_an_alias() {
        // A specific answer beats a general rule. The user looked at one `POS`
        // folder, said it is actually Oracle, and a rescan must not overrule them
        // with the project-wide vocabulary.
        let mut config = with_aliases(&[("POS", Some("postgres"), None)]);
        config.declaration_mut("AGGIORNAMENTO/4_12/POS").dialect = supported(EngineKind::Oracle);

        let p = plan(Path::new("/repo/prod-core"), &versioned_repository(), Some(&config));
        assert_eq!(
            at(&p, "AGGIORNAMENTO/4_12/POS").effective_dialect(),
            Some(EngineKind::Oracle)
        );
        // …and the alias still answers for every folder nobody singled out.
        assert_eq!(
            at(&p, "AGGIORNAMENTO/4_13/POS").effective_dialect(),
            Some(EngineKind::Postgres)
        );
    }

    #[test]
    fn a_per_path_declaration_that_clears_the_engine_also_beats_an_alias() {
        // The harder half of the same rule: an absent field in a declaration is
        // the user saying "inherit", and re-inferring — from the alias this time —
        // would undo that correction on every rescan.
        let mut config = with_aliases(&[("POS", Some("postgres"), None)]);
        config.declaration_mut("AGGIORNAMENTO/4_12/POS").role = Some(FolderRole::Data);

        let p = plan(Path::new("/repo/prod-core"), &versioned_repository(), Some(&config));
        let pinned = at(&p, "AGGIORNAMENTO/4_12/POS");
        assert_eq!(pinned.effective_dialect(), None, "the declaration is authoritative");
        assert_eq!(pinned.effective_role, FolderRole::Data);
    }

    #[test]
    fn an_alias_beats_the_built_in_vocabulary_during_discovery() {
        let config = with_aliases(&[("ORA", Some("postgres"), None)]);
        let p = plan(Path::new("/repo/prod-core"), &versioned_repository(), Some(&config));
        assert_eq!(
            at(&p, "AGGIORNAMENTO/4_12/ORA").effective_dialect(),
            Some(EngineKind::Postgres)
        );
    }

    #[test]
    fn an_alias_can_name_a_role_as_well_as_an_engine() {
        // A repository whose update folder is called CONSEGNE has exactly the
        // problem the engine alias solves, one axis over.
        let files = vec![file("CONSEGNE/2024/ORA/4_12.sql", cp1252("-- x\r\n"))];
        let bare = plan(Path::new("/repo/prod-core"), &files, None);
        assert_eq!(at(&bare, "CONSEGNE/2024/ORA").effective_role, FolderRole::Ignored);

        let config = with_aliases(&[("CONSEGNE", None, Some("update"))]);
        let p = plan(Path::new("/repo/prod-core"), &files, Some(&config));
        assert_eq!(at(&p, "CONSEGNE/2024/ORA").effective_role, FolderRole::Update);
        assert!(p.notes.is_empty(), "{:?}", p.notes);
    }

    #[test]
    fn a_bad_alias_degrades_to_the_repository_that_has_none() {
        let config = with_aliases(&[("POS", Some("postgres"), None), ("MSQ", Some("t-sql"), None)]);
        let p = plan(Path::new("/repo/prod-core"), &versioned_repository(), Some(&config));

        assert_eq!(
            at(&p, "AGGIORNAMENTO/4_12/POS").effective_dialect(),
            Some(EngineKind::Postgres)
        );
        // The bad one classifies nothing, so those folders are asked about again —
        // which is the correct degradation, not a silent wrong answer.
        assert!(at(&p, "AGGIORNAMENTO/4_12/MSQ").engine_is_unknown());
        assert!(p.notes.iter().any(|n| n.path == "AGGIORNAMENTO/4_12/MSQ"));
        // …and the user is told why, once, by the configuration itself.
        let problems = p.config.problems();
        assert_eq!(problems.len(), 1);
        assert!(problems[0].contains("t-sql"), "{problems:?}");
    }

    #[test]
    fn an_unsupported_folder_still_reports_an_encoding_that_drifted() {
        // Going quiet about the engine is not going quiet about everything: a
        // file that changed encoding is a fact about the bytes, and true whoever
        // owns the scripts.
        let config = with_aliases(&[("MSQ", Some("sqlserver"), None)]);
        let files = vec![
            file("AGGIORNAMENTO/MSQ/a.sql", cp1252("-- perché\r\n")),
            file("AGGIORNAMENTO/MSQ/b.sql", "-- perché\r\n".as_bytes().to_vec()),
        ];
        let p = plan(Path::new("/repo/prod-core"), &files, Some(&config));

        assert!(p.notes.iter().all(|n| !n.needs_attention), "{:?}", p.notes);
        assert!(p.notes.iter().any(|n| n.path.ends_with("b.sql")), "{:?}", p.notes);
    }

    #[test]
    fn a_portable_folder_is_classified_and_never_asked_about() {
        // The folder of plain inserts meant to run on both engines. Declared, not
        // guessed — and once declared it is as settled as `ORA` is.
        let files = vec![
            file("AGGIORNAMENTO/COMUNE/4_12.sql", cp1252("UPDATE PARAMETRI SET V = 1;\r\n")),
            file("AGGIORNAMENTO/2024/ORA/4_12.sql", cp1252("-- x\r\n")),
        ];
        let bare = plan(Path::new("/repo/prod-core"), &files, None);
        // Nothing infers it: without a declaration, `COMUNE` is a question.
        assert!(at(&bare, "AGGIORNAMENTO/COMUNE").engine_is_unknown());

        let config = with_aliases(&[("COMUNE", Some("generic"), None)]);
        let p = plan(Path::new("/repo/prod-core"), &files, Some(&config));
        let comune = at(&p, "AGGIORNAMENTO/COMUNE");
        assert!(comune.is_generic());
        assert!(!comune.engine_is_unknown(), "portable is an answer");
        assert_eq!(comune.effective_dialect(), None, "no single dialect to emit as");
        assert!(EngineKind::ALL.iter().all(|d| comune.covers(*d)));
        assert!(p.notes.is_empty(), "{:?}", p.notes);
    }

    #[test]
    fn a_portable_declaration_can_also_be_made_on_one_path() {
        // The alias is for names that repeat; a single portable folder is an
        // ordinary declaration in the same `dialect` key as every other engine.
        let files = vec![file("COMUNE/parametri.sql", cp1252("INSERT INTO P VALUES (1);\r\n"))];
        let mut config = with_aliases(&[]);
        config.declaration_mut("COMUNE").dialect = Some(FolderEngine::Generic);
        config.declaration_mut("COMUNE").role = Some(FolderRole::Data);

        let p = plan(Path::new("/repo/prod-core"), &files, Some(&config));
        assert!(at(&p, "COMUNE").is_generic());
        // …and it survives a round trip through the file it came from.
        let text = toml::to_string_pretty(&config).expect("serialises");
        assert!(text.contains(r#"dialect = "generic""#), "{text}");
        assert_eq!(ProjectConfig::parse(&text).unwrap(), config);
    }

    // ── When the engine is in the file name ───────────────────────────────────

    /// The untidy repository: no engine folders at all, both engines loose in the
    /// same directories, and the file name the only thing that knows.
    fn scattered() -> Vec<SourceFile> {
        let mut files = Vec::new();
        for version in ["4_11", "4_12"] {
            for engine in ["ORA", "POS"] {
                files.push(file(
                    &format!("AGGIORNAMENTO/{version}/{version}_{engine}.sql"),
                    cp1252("UPDATE PARAMETRI SET VALORE = 1;\r\n"),
                ));
            }
        }
        files.push(file("AGGIORNAMENTO/2024/LEGGIMI.sql", cp1252("-- x\r\n")));
        files
    }

    /// Aliases pointed at file names as well as folder names.
    fn with_file_aliases(entries: &[(&str, &str)]) -> ProjectConfig {
        let mut config = with_aliases(&[]);
        for (name, engine) in entries {
            let alias = config.alias_mut(name);
            alias.engine = Some(engine.to_string());
            alias.applies_to = Some("both".to_string());
        }
        config
    }

    #[test]
    fn without_a_declaration_the_file_names_say_nothing() {
        // The state this replaces, asserted so the improvement is not a claim:
        // every file unclassified, and a question per folder.
        let p = plan(Path::new("/repo/prod-core"), &scattered(), None);
        assert!(p.project.all_files().all(|f| f.engine_is_unknown()));
        assert!(p.notes.iter().filter(|n| n.needs_attention).count() >= 2);
    }

    #[test]
    fn two_alias_lines_classify_every_scattered_file() {
        let config = with_file_aliases(&[("POS", "postgres"), ("ORA", "oracle")]);
        let p = plan(Path::new("/repo/prod-core"), &scattered(), Some(&config));

        for version in ["4_11", "4_12"] {
            assert_eq!(
                p.project.dialect_of(&format!("AGGIORNAMENTO/{version}/{version}_ORA.sql")),
                Some(EngineKind::Oracle),
                "{version}"
            );
            assert_eq!(
                p.project.dialect_of(&format!("AGGIORNAMENTO/{version}/{version}_POS.sql")),
                Some(EngineKind::Postgres),
                "{version}"
            );
        }
        // The repository now has both sides, from files alone — no folder in it
        // could ever have said so.
        assert_eq!(p.project.dialects(), EngineKind::ALL);
        // The role still comes from the top of the tree, as always.
        assert_eq!(at(&p, "AGGIORNAMENTO/4_12").effective_role, FolderRole::Update);
    }

    #[test]
    fn the_folder_is_still_asked_about_only_where_a_file_is_left_over() {
        // The point of asking per file rather than per folder: `AGGIORNAMENTO/2024`
        // holds one unclassified script, so it is still a question, while the
        // folders whose files all answered for themselves have gone quiet.
        let config = with_file_aliases(&[("POS", "postgres"), ("ORA", "oracle")]);
        let p = plan(Path::new("/repo/prod-core"), &scattered(), Some(&config));

        let asked: Vec<&str> =
            p.notes.iter().filter(|n| n.needs_attention).map(|n| n.path.as_str()).collect();
        assert_eq!(asked, ["AGGIORNAMENTO/2024"], "{:?}", p.notes);
    }

    #[test]
    fn one_file_can_be_corrected_by_path_and_it_beats_the_name() {
        // `POS_TERMINALI.sql` is a point-of-sale script in an Oracle repository,
        // and the name rule reads it as PostgreSQL. The user says otherwise about
        // that one path, and a rescan must not overrule them.
        let files = vec![file("ORA/POS_TERMINALI.sql", cp1252("-- x\r\n"))];
        let mut config = with_file_aliases(&[("POS", "postgres"), ("ORA", "oracle")]);

        let before = plan(Path::new("/repo/prod-core"), &files, Some(&config));
        assert_eq!(before.project.dialect_of("ORA/POS_TERMINALI.sql"), Some(EngineKind::Postgres));

        config.file_declaration_mut("ORA/POS_TERMINALI.sql").dialect =
            Some(FolderEngine::Supported(EngineKind::Oracle));
        let after = plan(Path::new("/repo/prod-core"), &files, Some(&config));
        assert_eq!(after.project.dialect_of("ORA/POS_TERMINALI.sql"), Some(EngineKind::Oracle));
    }

    #[test]
    fn a_file_that_disagrees_with_a_declared_folder_is_reported_but_not_asked_about() {
        // The surprising case: the folder was declared Oracle and one file reads
        // as something else. Picus is not confused — it is saying so out loud.
        let files = vec![
            file("ORA/4_12.sql", cp1252("-- x\r\n")),
            file("ORA/4_12_POS.sql", cp1252("-- x\r\n")),
        ];
        let mut config = with_file_aliases(&[("POS", "postgres")]);
        // Both fields: a folder declaration is authoritative for the one it leaves
        // absent too, so declaring only the role would clear the engine and there
        // would be nothing for the file to disagree with.
        config.declaration_mut("ORA").dialect = supported(EngineKind::Oracle);
        config.declaration_mut("ORA").role = Some(FolderRole::Update);

        let p = plan(Path::new("/repo/prod-core"), &files, Some(&config));
        assert_eq!(p.project.dialect_of("ORA/4_12.sql"), Some(EngineKind::Oracle));
        assert_eq!(p.project.dialect_of("ORA/4_12_POS.sql"), Some(EngineKind::Postgres));

        let note = p.notes.iter().find(|n| n.path == "ORA/4_12_POS.sql").expect("a note");
        assert!(!note.needs_attention, "it is a report, not a question");
        assert!(note.message.contains("PostgreSQL"), "{}", note.message);
        // …and the file that agreed with its folder produces nothing.
        assert!(!p.notes.iter().any(|n| n.path == "ORA/4_12.sql"), "{:?}", p.notes);
    }

    #[test]
    fn a_folder_whose_files_all_answered_produces_no_note_at_all() {
        // In the repository this exists for the folder declares nothing and every
        // file answers for itself, which is ordinary — one line per file would be
        // a report nobody reads.
        let files = vec![
            file("AGGIORNAMENTO/4_12_ORA.sql", cp1252("-- x\r\n")),
            file("AGGIORNAMENTO/4_12_POS.sql", cp1252("-- x\r\n")),
        ];
        let config = with_file_aliases(&[("POS", "postgres"), ("ORA", "oracle")]);
        let p = plan(Path::new("/repo/prod-core"), &files, Some(&config));
        assert!(p.notes.is_empty(), "{:?}", p.notes);
    }

    #[test]
    fn the_built_in_vocabulary_recognises_an_unsupported_engine_by_its_real_name() {
        // No alias at all: a folder that spells the product out is recognised
        // everywhere, which is what keeps the global list honest and short.
        let files = vec![file("AGGIORNAMENTO/MSSQL/4_12.sql", cp1252("-- x\r\n"))];
        let p = plan(Path::new("/repo/prod-core"), &files, None);
        assert_eq!(
            at(&p, "AGGIORNAMENTO/MSSQL").effective_engine.and_then(FolderEngine::foreign),
            Some(ForeignEngine::SqlServer)
        );
        assert!(p.notes.is_empty(), "{:?}", p.notes);
    }

    #[test]
    fn a_declaration_on_the_root_reaches_every_folder() {
        let mut config = planned().config;
        config.declaration_mut("").dialect = supported(EngineKind::Postgres);
        let p = plan(Path::new("/repo/prod-core"), &repository(), Some(&config));
        // `ORACLE` still declares its own, and everything that declares nothing
        // takes the root's.
        assert_eq!(at(&p, "ORACLE").effective_dialect(), Some(EngineKind::Oracle));
        let files = vec![file("MISC/AGGIORNAMENTO/x.sql", cp1252("select 1;\r\n"))];
        let p = plan(Path::new("/repo/prod-core"), &files, Some(&config));
        assert_eq!(at(&p, "MISC/AGGIORNAMENTO").effective_dialect(), Some(EngineKind::Postgres));
    }
}
