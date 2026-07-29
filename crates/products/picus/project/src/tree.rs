//! What is on disk, in the shape the interface renders.
//!
//! These are wire types: `camelCase`, field-for-field with `ScriptFile` /
//! `FolderNode` / `Project` in `src/lib/types/picus/index.ts`. They describe
//! **the repository**, never a database — the two were conflated once early in
//! this product and it leaked immediately (the generator started asking the
//! project for column types), so the separation is kept deliberately sharp.
//!
//! ## The tree is the real directory hierarchy
//!
//! There is no "branch" here and no flattening. Every directory that holds
//! scripts is a [`FolderNode`] in its real place, and a node may **declare** a
//! dialect, a role, or both. Descendants inherit the nearest ancestor's
//! declaration until one overrides it — which is what makes
//!
//! ```text
//! AGGIORNAMENTO           role = update
//! AGGIORNAMENTO/2024/ORA  dialect = oracle
//! AGGIORNAMENTO/2024/POS  dialect = ?
//! ```
//!
//! describable at all: the role is at the top of the tree, the dialect at the
//! bottom, and the two are independent. `effective_dialect` stays an `Option`
//! because a folder nobody classified genuinely has no dialect, and nothing is
//! generated into one — guessing writes Oracle syntax into a PostgreSQL file,
//! the exact failure this product exists to catch.
//!
//! ## One engine field, four answers
//!
//! A folder has one engine, so it has one field, and it is one of four things: a
//! dialect Picus reads; **portable** SQL valid on every dialect it reads; an
//! engine it only recognises (`AGGIORNAMENTO/2024/MSQ` is SQL Server); or nothing
//! yet. Reading it is deliberately done through methods rather than the field,
//! because the useful questions are not "which engine" but:
//!
//! * [`FolderNode::scope`] — what its SQL has to be valid in. `None` is the gate
//!   that keeps unsupported folders out of the parser and the emitter.
//! * [`FolderNode::covers`] — does content here count for that dialect? True of
//!   **both** for a portable folder, which is what puts it in two lanes.
//! * [`FolderNode::effective_dialect`] — the *single* dialect, if there is one.
//!   `None` for portable as well as for unclassified, and callers that meant
//!   `covers` are the bug this distinction exists to catch.

use arbor_fs::prelude::encoding::EncodingSource;
use picus_types::prelude::{DialectScope, EngineKind, FolderEngine, FolderRole};
use serde::{Deserialize, Serialize};

/// How a file's lines end. Preserved on write: a repository that is CRLF stays
/// CRLF, or every generated block arrives as a whole-file diff.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum LineEnding {
    Crlf,
    Lf,
}

impl LineEnding {
    pub fn as_str(self) -> &'static str {
        match self {
            LineEnding::Crlf => "\r\n",
            LineEnding::Lf => "\n",
        }
    }

    /// Decide from the bytes, by majority.
    ///
    /// A file mixing both is not an error — it is a file two editors have touched
    /// — so the answer is "which one wins", and the minority endings are what a
    /// later diagnostic can report.
    pub fn detect(bytes: &[u8]) -> LineEnding {
        let mut crlf = 0usize;
        let mut lf = 0usize;
        let mut previous = 0u8;
        for &byte in bytes {
            if byte == b'\n' {
                if previous == b'\r' {
                    crlf += 1;
                } else {
                    lf += 1;
                }
            }
            previous = byte;
        }
        // Ties, including a file with no line ending at all, go to CRLF: these
        // repositories are Windows-authored, and inventing LF in one of them would
        // rewrite every line of the first file Picus touches.
        if lf > crlf {
            LineEnding::Lf
        } else {
            LineEnding::Crlf
        }
    }
}

/// One script file, as discovered.
///
/// Serialize only, like its siblings below: the tree is *reported* to the
/// interface and never sent back, and `EncodingSource` is deliberately
/// write-only in `arbor-fs` for the same reason.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScriptFile {
    /// Path relative to the project root, POSIX separators — the identity of a
    /// file everywhere in Picus, including on Windows.
    pub path: String,
    pub name: String,
    pub size: u64,
    /// The encoding actually detected.
    pub encoding: String,
    /// How that was decided, so the guess is never silent.
    pub encoding_source: EncodingSource,
    pub eol: LineEnding,
    /// What the folder expects. Different from `encoding` means the file was
    /// rewritten by something that did not know — the `ENC001` diagnostic.
    pub expected_encoding: String,
    /// Declared **on this file** — almost always `None`, which means "whatever my
    /// folder is" and is the answer for essentially every file in a tidy
    /// repository.
    ///
    /// It exists for the untidy ones, where both engines sit in one directory and
    /// only the file name knows which is which. Same four answers as
    /// [`FolderNode::engine`], including `Unsupported` for the single T-SQL script
    /// that wandered in.
    pub engine: Option<FolderEngine>,
    /// After falling back to the folder — what actually applies to this file.
    ///
    /// This, not the folder's, is what decides how the file is parsed, which lane
    /// it counts in, and whether it is read at all. `None` means nobody has said,
    /// here or anywhere above it.
    pub effective_engine: Option<FolderEngine>,
    /// Declared on this file: leave it out of the project. `None` = inherit from
    /// the folder.
    pub excluded: Option<bool>,
    /// After inheritance. `true` means this script is not part of the project:
    /// not parsed, not indexed, no findings, never a destination.
    pub effective_excluded: bool,
}

/// The engine questions, asked of one file.
///
/// Thin by design: every answer is [`FolderEngine`]'s, so the rules live in one
/// place and this is only the question being asked at a different granularity.
/// [`FolderNode`] delegates to exactly the same methods, which is what keeps a
/// folder and a file from ever disagreeing about what "portable" means.
impl ScriptFile {
    /// Has this file drifted from what its folder expects?
    pub fn encoding_drifted(&self) -> bool {
        !self.encoding.eq_ignore_ascii_case(&self.expected_encoding)
    }

    /// Does this file say something about itself, rather than taking its folder's
    /// word? What the interface shows as an inherited chip versus an own one.
    pub fn declares_engine(&self) -> bool {
        self.engine.is_some()
    }

    /// What this file's SQL has to be valid in. `None` for an unsupported engine
    /// and for one nobody declared — the single gate everything downstream goes
    /// through.
    pub fn scope(&self) -> Option<DialectScope> {
        self.effective_engine.and_then(FolderEngine::scope)
    }

    /// The **single** dialect this file is written in, if there is one. `None` for
    /// portable as well as for unclassified.
    pub fn effective_dialect(&self) -> Option<EngineKind> {
        self.scope().and_then(DialectScope::dialect)
    }

    /// Does what is written here count as present for `dialect`? True of **both**
    /// for a portable file.
    pub fn covers(&self, dialect: EngineKind) -> bool {
        self.effective_engine.map(|e| e.covers(dialect)).unwrap_or(false)
    }

    /// Portable SQL: written to run on every dialect Picus supports.
    pub fn is_generic(&self) -> bool {
        self.effective_engine.map(FolderEngine::is_generic).unwrap_or(false)
    }

    /// Is this file written in an engine Picus recognises and does not read? The
    /// state that stops the parse.
    pub fn engine_is_unsupported(&self) -> bool {
        self.effective_engine.map(|e| !e.is_readable()).unwrap_or(false)
    }

    /// Does anybody know what engine this file is?
    pub fn engine_is_unknown(&self) -> bool {
        self.effective_engine.is_none()
    }

    /// Is this script part of the project at all?
    ///
    /// The gate every consumer goes through before doing anything with a file.
    /// An excluded script is still **read** — it opens in the editor like any
    /// other, and hiding it would leave no way to change one's mind — but it is
    /// not parsed, not indexed, not compared and never generated into.
    pub fn is_excluded(&self) -> bool {
        self.effective_excluded
    }

    /// Does Picus have anything at all to say about this file?
    ///
    /// The two states that stop the pipeline, in one question, because every
    /// caller that skips one skips the other: excluded means *not ours to look
    /// at*, an unsupported engine means *not ours to read*. Different reasons,
    /// identical consequences.
    pub fn is_out_of_scope(&self) -> bool {
        self.is_excluded() || self.engine_is_unsupported()
    }
}

/// One directory of the repository, in its real place.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FolderNode {
    /// Project-relative path, POSIX separators — **the identity**. Empty for the
    /// repository root itself, which only appears when scripts sit directly in it.
    pub path: String,
    /// Last segment, which is what the tree row shows.
    pub name: String,
    /// Declared ON this folder. `None` = inherit from the nearest ancestor that
    /// declares one.
    ///
    /// **One field**, because a folder has one engine and it is one of four
    /// things. Splitting it would mean four call sites deciding independently
    /// what a folder is, and the interesting states — portable, unsupported —
    /// would be the ones that got forgotten.
    pub engine: Option<FolderEngine>,
    pub role: Option<FolderRole>,
    /// After inheritance — what actually applies here.
    ///
    /// `None` is a real answer and it means exactly one thing: **nobody has
    /// said**. It is not what a portable folder gets, and not what an unsupported
    /// one gets; both of those are answers, and the interface treats them as such.
    pub effective_engine: Option<FolderEngine>,
    /// After inheritance, falling back to [`FolderRole::Ignored`].
    pub effective_role: FolderRole,
    /// Declared here: leave this folder and everything under it out of the
    /// project. `None` = inherit; a descendant may set it back to `false`.
    pub excluded: Option<bool>,
    /// After inheritance.
    pub effective_excluded: bool,
    /// Which installed product's scripts live here. `None` = inherit.
    ///
    /// A repository that ships more than one product records a version per
    /// product — one row of the version table each, told apart by a predicate —
    /// and the scripts of each product are, in practice, in their own folders.
    /// Naming the product on the folder is what lets a generated block stamp the
    /// right row without the user restating the predicate on every destination.
    ///
    /// A free string rather than an enum: the set is the repository's, not
    /// Picus's. What it *means* is declared once, under `[[product]]` in the
    /// project file.
    pub product: Option<String>,
    /// After inheritance. `None` when nothing above says either — a repository
    /// installing one product never declares any of this.
    pub effective_product: Option<String>,
    pub children: Vec<FolderNode>,
    pub files: Vec<ScriptFile>,
}

impl FolderNode {
    /// A node with nothing declared and nothing resolved yet — what discovery
    /// builds before [`crate::resolve::resolve`] runs over it.
    pub fn new(path: impl Into<String>, name: impl Into<String>) -> FolderNode {
        FolderNode {
            path: path.into(),
            name: name.into(),
            engine: None,
            role: None,
            effective_engine: None,
            effective_role: FolderRole::Ignored,
            excluded: None,
            effective_excluded: false,
            product: None,
            effective_product: None,
            children: Vec::new(),
            files: Vec::new(),
        }
    }

    /// Is this folder part of the project at all? See [`ScriptFile::is_excluded`].
    pub fn is_excluded(&self) -> bool {
        self.effective_excluded
    }

    /// What this folder's SQL has to be valid in, after inheritance.
    ///
    /// `None` for an unsupported engine and for one nobody has declared — the two
    /// states where parsing, analysing and generating all have nothing to do. The
    /// single gate everything downstream goes through.
    pub fn scope(&self) -> Option<DialectScope> {
        self.effective_engine.and_then(FolderEngine::scope)
    }

    /// The **single** dialect this folder emits and parses as, if it has one.
    ///
    /// `None` for a portable folder as well as for an unclassified one, which is
    /// correct in both cases and is why callers asking "does this belong to the
    /// Oracle side" must use [`covers`](Self::covers) instead.
    pub fn effective_dialect(&self) -> Option<EngineKind> {
        self.scope().and_then(DialectScope::dialect)
    }

    /// Does what is written here count as present for `dialect`?
    ///
    /// True of **both** dialects for a portable folder, which makes it the first
    /// thing in the model to belong to more than one lane.
    pub fn covers(&self, dialect: EngineKind) -> bool {
        self.effective_engine.map(|e| e.covers(dialect)).unwrap_or(false)
    }

    /// Portable SQL: written to run on every dialect Picus supports.
    pub fn is_generic(&self) -> bool {
        self.effective_engine.map(FolderEngine::is_generic).unwrap_or(false)
    }

    /// Is this folder written in an engine Picus recognises and does not read?
    ///
    /// The state that stops every question: no proposal note, no classify prompt,
    /// no lane, no comparison, and — the one that matters most for both
    /// correctness and speed — no parse.
    pub fn engine_is_unsupported(&self) -> bool {
        self.effective_engine.map(|e| !e.is_readable()).unwrap_or(false)
    }

    /// Does anybody know what engine this folder is? `false` is the question the
    /// interface asks; it is **not** true of an unsupported or a portable engine,
    /// which are answers.
    pub fn engine_is_unknown(&self) -> bool {
        self.effective_engine.is_none()
    }

    /// This node and every node under it, depth first.
    pub fn walk(&self) -> Walk<'_> {
        Walk { stack: vec![self] }
    }

    /// Every file in this folder and in every folder under it.
    pub fn all_files(&self) -> impl Iterator<Item = &ScriptFile> {
        self.walk().flat_map(|node| node.files.iter())
    }

    /// Does this folder take part in the `(dialect, role)` lane the cross-dialect
    /// rules compare?
    ///
    /// A **portable** file is in the lane of every dialect: what it writes runs on
    /// both, so it fills a gap on both, and reporting it as missing from either
    /// would be reporting the opposite of the truth.
    ///
    /// Asked of the **files**, because that is where the engine now lives. A
    /// folder holding one `*_ORA.sql` and one `*_POS.sql` is in both lanes even
    /// though the folder itself could never say which engine it is — and a folder
    /// with no files at all is in none, which is right: a directory that only
    /// contains other directories has no content to compare.
    pub fn is_in_lane(&self, dialect: EngineKind, role: FolderRole) -> bool {
        self.effective_role == role
            && self.files.iter().any(|file| !file.is_excluded() && file.covers(dialect))
    }

    /// The files of this folder that are actually part of the project.
    ///
    /// What every consumer wants. Excluded scripts stay in `files` so the tree
    /// can still show them — and so there is somewhere to click to change one's
    /// mind — but nothing downstream should ever iterate `files` directly.
    pub fn included_files(&self) -> impl Iterator<Item = &ScriptFile> {
        self.files.iter().filter(|file| !file.is_excluded())
    }
}

/// Depth-first, in tree order.
///
/// Iterative rather than recursive so a pathological directory depth cannot
/// overflow the stack of a backend serving a window.
pub struct Walk<'a> {
    stack: Vec<&'a FolderNode>,
}

impl<'a> Iterator for Walk<'a> {
    type Item = &'a FolderNode;

    fn next(&mut self) -> Option<&'a FolderNode> {
        let node = self.stack.pop()?;
        // Reversed, so the first child is the next thing out.
        self.stack.extend(node.children.iter().rev());
        Some(node)
    }
}

/// The repository as a whole.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Project {
    pub name: String,
    /// Absolute path of the root, in the platform's own form — this one is shown
    /// to the user and pasted into a terminal, so it is not POSIX-normalised.
    pub root: String,
    /// The top-level folders, each carrying its own subtree.
    pub tree: Vec<FolderNode>,
}

impl Project {
    /// Every folder, depth first, in tree order.
    pub fn walk(&self) -> Walk<'_> {
        Walk { stack: self.tree.iter().rev().collect() }
    }

    /// Every file, flattened — what searches and pickers want.
    pub fn all_files(&self) -> impl Iterator<Item = &ScriptFile> {
        self.walk().flat_map(|node| node.files.iter())
    }

    /// The folder at a project-relative path.
    pub fn folder_at(&self, path: &str) -> Option<&FolderNode> {
        self.walk().find(|node| node.path == path)
    }

    /// Which folder a project-relative **file** path sits in.
    ///
    /// Linear in the size of the repository. Callers that need this for *every*
    /// file must build an index once instead — see [`Project::files_by_path`] —
    /// or the walk turns quadratic on exactly the repositories where it hurts.
    pub fn folder_of(&self, path: &str) -> Option<&FolderNode> {
        self.walk().find(|node| node.files.iter().any(|f| f.path == path))
    }

    /// The file at a project-relative path.
    pub fn file_at(&self, path: &str) -> Option<&ScriptFile> {
        self.walk().find_map(|node| node.files.iter().find(|f| f.path == path))
    }

    /// Every file, indexed by path — one walk instead of one walk per lookup.
    ///
    /// The whole tree is read once and the map borrows it, so this costs a
    /// `HashMap` of pointers and nothing else.
    pub fn files_by_path(&self) -> std::collections::HashMap<&str, &ScriptFile> {
        self.walk()
            .flat_map(|node| node.files.iter())
            .map(|file| (file.path.as_str(), file))
            .collect()
    }

    /// The dialect a file must be written in. `None` when neither the file nor any
    /// folder above it declares one — the caller must refuse to generate rather
    /// than pick one.
    ///
    /// Asked of the **file**, which is the folder's answer for all but the handful
    /// of files that say otherwise. A caller that asked the folder instead would
    /// be right almost always, and wrong exactly where this feature exists.
    pub fn dialect_of(&self, path: &str) -> Option<EngineKind> {
        self.file_at(path).and_then(ScriptFile::effective_dialect)
    }

    /// What a file's SQL has to be valid in — the parse and emit target.
    pub fn scope_of(&self, path: &str) -> Option<DialectScope> {
        self.file_at(path).and_then(ScriptFile::scope)
    }

    /// Every dialect the repository actually answers for somewhere.
    ///
    /// A portable file contributes **both**: a repository whose only `init`
    /// scripts are portable still has an Oracle install story and a PostgreSQL
    /// one, written once.
    ///
    /// Counted over **files**, not folders, and the difference is the whole point
    /// of file-level classification: a repository whose PostgreSQL content is four
    /// scattered `*_POS.sql` files in otherwise unclassified folders genuinely has
    /// a PostgreSQL side, and a folder-level count would say it does not and leave
    /// every cross-dialect rule silent about it.
    pub fn dialects(&self) -> Vec<EngineKind> {
        let mut out: Vec<EngineKind> = self
            .walk()
            .flat_map(FolderNode::included_files)
            .flat_map(|file| file.effective_engine.map(FolderEngine::dialects).unwrap_or(&[]))
            .copied()
            .collect();
        out.sort();
        out.dedup();
        out
    }

    /// The folders that play `role` for `dialect` — one lane of the comparison
    /// the consistency rules make.
    ///
    /// A folder is in the lane when **any file in it** is: with the engine on the
    /// file, a directory holding both an `*_ORA.sql` and a `*_POS.sql` belongs to
    /// both lanes, and asking the folder's own engine would put it in neither.
    /// Callers that need the files themselves — anything counting content — want
    /// [`lane_files`](Self::lane_files).
    pub fn lane(&self, dialect: EngineKind, role: FolderRole) -> impl Iterator<Item = &FolderNode> {
        self.walk().filter(move |node| node.is_in_lane(dialect, role))
    }

    /// Every file that plays `role` for `dialect`, with the folder holding it.
    ///
    /// The lane at the granularity content actually has. `role` is still the
    /// folder's — a role is what a directory of scripts is *for* — while the
    /// dialect is the file's.
    pub fn lane_files(
        &self,
        dialect: EngineKind,
        role: FolderRole,
    ) -> impl Iterator<Item = (&FolderNode, &ScriptFile)> {
        self.walk().filter(move |node| node.effective_role == role).flat_map(move |node| {
            node.included_files().filter(move |file| file.covers(dialect)).map(move |file| (node, file))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::resolve::resolve;

    #[test]
    fn line_endings_are_decided_by_majority() {
        assert_eq!(LineEnding::detect(b"a\r\nb\r\nc"), LineEnding::Crlf);
        assert_eq!(LineEnding::detect(b"a\nb\nc"), LineEnding::Lf);
        assert_eq!(LineEnding::detect(b"a\r\nb\nc\n"), LineEnding::Lf);
        assert_eq!(LineEnding::detect(b"a\r\nb\r\nc\n"), LineEnding::Crlf);
    }

    #[test]
    fn a_file_with_no_line_ending_does_not_become_lf() {
        // Inventing LF here would rewrite every line of the first file touched.
        assert_eq!(LineEnding::detect(b"SELECT 1;"), LineEnding::Crlf);
        assert_eq!(LineEnding::detect(b""), LineEnding::Crlf);
    }

    #[test]
    fn the_line_ending_wire_words_match_the_frontend() {
        assert_eq!(serde_json::to_string(&LineEnding::Crlf).unwrap(), "\"CRLF\"");
        assert_eq!(serde_json::to_string(&LineEnding::Lf).unwrap(), "\"LF\"");
    }

    fn file(path: &str, encoding: &str, expected: &str) -> ScriptFile {
        ScriptFile {
            path: path.to_string(),
            name: path.rsplit('/').next().unwrap().to_string(),
            size: 10,
            encoding: encoding.to_string(),
            encoding_source: EncodingSource::Heuristic,
            eol: LineEnding::Crlf,
            expected_encoding: expected.to_string(),
            engine: None,
            effective_engine: None,
            excluded: None,
            effective_excluded: false,
        }
    }

    /// A file that says what engine it is, whatever folder it lands in.
    fn file_of(path: &str, engine: FolderEngine) -> ScriptFile {
        ScriptFile { engine: Some(engine), ..file(path, "windows-1252", "windows-1252") }
    }

    use picus_types::prelude::ForeignEngine;

    fn node(path: &str, dialect: Option<EngineKind>, role: Option<FolderRole>) -> FolderNode {
        engine_node(path, dialect.map(FolderEngine::Supported), role)
    }

    fn engine_node(
        path: &str,
        engine: Option<FolderEngine>,
        role: Option<FolderRole>,
    ) -> FolderNode {
        let name = path.rsplit('/').next().unwrap_or(path).to_string();
        FolderNode { engine, role, ..FolderNode::new(path, name) }
    }

    /// The repository this whole change exists for: the role at the top, the
    /// dialect at the bottom, and one leaf nobody could identify.
    fn project() -> Project {
        let mut aggiornamento = node("AGGIORNAMENTO", None, Some(FolderRole::Update));
        let mut year = node("AGGIORNAMENTO/2024", None, None);
        let mut ora = node("AGGIORNAMENTO/2024/ORA", Some(EngineKind::Oracle), None);
        ora.files.push(file("AGGIORNAMENTO/2024/ORA/4_12.sql", "windows-1252", "windows-1252"));
        let mut pos = node("AGGIORNAMENTO/2024/POS", None, None);
        pos.files.push(file("AGGIORNAMENTO/2024/POS/4_12.sql", "UTF-8", "windows-1252"));
        year.children = vec![ora, pos];
        aggiornamento.children = vec![year];

        let mut project =
            Project { name: "PROD_CORE".to_string(), root: r"C:\p".to_string(), tree: vec![aggiornamento] };
        resolve(&mut project.tree, None, None);
        project
    }

    #[test]
    fn the_walk_is_depth_first_in_tree_order() {
        let p = project();
        let paths: Vec<&str> = p.walk().map(|n| n.path.as_str()).collect();
        assert_eq!(
            paths,
            [
                "AGGIORNAMENTO",
                "AGGIORNAMENTO/2024",
                "AGGIORNAMENTO/2024/ORA",
                "AGGIORNAMENTO/2024/POS"
            ]
        );
    }

    #[test]
    fn a_files_dialect_comes_from_the_nearest_ancestor_that_declares_one() {
        let p = project();
        assert_eq!(p.dialect_of("AGGIORNAMENTO/2024/ORA/4_12.sql"), Some(EngineKind::Oracle));
        // Nobody classified POS, so it has no dialect. Not a default: this is the
        // folder the user is asked about.
        assert_eq!(p.dialect_of("AGGIORNAMENTO/2024/POS/4_12.sql"), None);
        assert_eq!(p.dialect_of("nowhere.sql"), None);
    }

    #[test]
    fn a_role_declared_at_the_top_reaches_the_leaves() {
        let p = project();
        for path in ["AGGIORNAMENTO/2024", "AGGIORNAMENTO/2024/ORA", "AGGIORNAMENTO/2024/POS"] {
            assert_eq!(p.folder_at(path).unwrap().effective_role, FolderRole::Update, "{path}");
        }
    }

    #[test]
    fn a_lane_is_the_folders_that_play_one_role_for_one_dialect() {
        let p = project();
        let oracle: Vec<&str> = p
            .lane(EngineKind::Oracle, FolderRole::Update)
            .map(|n| n.path.as_str())
            .collect();
        assert_eq!(oracle, ["AGGIORNAMENTO/2024/ORA"]);
        // The dialect nobody declared has no lane at all, which is what keeps the
        // cross-dialect rules quiet about it.
        assert_eq!(p.dialects(), [EngineKind::Oracle]);
        assert_eq!(p.lane(EngineKind::Postgres, FolderRole::Update).count(), 0);
    }

    #[test]
    fn drift_is_what_the_folder_expected_versus_what_was_found() {
        let p = project();
        let drifted: Vec<&str> =
            p.all_files().filter(|f| f.encoding_drifted()).map(|f| f.path.as_str()).collect();
        assert_eq!(drifted, ["AGGIORNAMENTO/2024/POS/4_12.sql"]);
    }

    #[test]
    fn the_wire_shape_carries_both_what_was_declared_and_what_applies() {
        // The interface needs both: the declaration is what a folder's editor
        // shows, and the effective value is what the row is labelled with.
        let p = project();
        let json = serde_json::to_value(&p).unwrap();
        let year = &json["tree"][0]["children"][0];
        assert_eq!(year["path"], "AGGIORNAMENTO/2024");
        assert_eq!(year["role"], serde_json::Value::Null, "nothing is declared here");
        assert_eq!(year["effectiveRole"], "update");
        assert_eq!(year["effectiveEngine"], serde_json::Value::Null);
        assert_eq!(json["tree"][0]["children"][0]["children"][0]["engine"], "oracle");
    }

    #[test]
    fn the_four_engine_states_are_four_different_things() {
        // Collapsing any two of these is the bug the single `FolderEngine` field
        // exists to make impossible.
        let mut tree = vec![
            node("ORA", Some(EngineKind::Oracle), None),
            engine_node("COMUNE", Some(FolderEngine::Generic), None),
            engine_node("MSQ", Some(FolderEngine::Unsupported(ForeignEngine::SqlServer)), None),
            node("POS", None, None),
        ];
        resolve(&mut tree, None, None);
        let (ora, generic, msq, unknown) = (&tree[0], &tree[1], &tree[2], &tree[3]);

        assert_eq!(ora.effective_dialect(), Some(EngineKind::Oracle));
        assert!(!ora.is_generic() && !ora.engine_is_unsupported() && !ora.engine_is_unknown());

        // Portable: no single dialect, but an answer — and it covers both.
        assert!(generic.is_generic());
        assert_eq!(generic.effective_dialect(), None);
        assert!(!generic.engine_is_unknown(), "portable is an answer, not a question");
        assert!(!generic.engine_is_unsupported());
        assert!(EngineKind::ALL.iter().all(|d| generic.covers(*d)));
        assert_eq!(generic.scope(), Some(DialectScope::Portable));

        assert!(msq.engine_is_unsupported());
        assert!(!msq.engine_is_unknown(), "SQL Server is an answer too");
        assert_eq!(msq.scope(), None, "nothing is parsed or emitted with it");

        assert!(unknown.engine_is_unknown(), "this one is the question");
        assert_eq!(unknown.scope(), None);
    }

    #[test]
    fn a_portable_folder_is_in_the_lane_of_every_dialect() {
        // The first thing in the model to belong to more than one lane, and the
        // whole reason `covers` exists next to `effective_dialect`.
        let mut comune = engine_node("COMUNE", Some(FolderEngine::Generic), Some(FolderRole::Data));
        comune.files.push(file("COMUNE/parametri.sql", "windows-1252", "windows-1252"));

        let mut project =
            Project { name: "P".to_string(), root: "/p".to_string(), tree: vec![comune] };
        resolve(&mut project.tree, None, None);

        for dialect in EngineKind::ALL {
            let lane: Vec<&str> =
                project.lane(*dialect, FolderRole::Data).map(|n| n.path.as_str()).collect();
            assert_eq!(lane, ["COMUNE"], "{dialect}");
        }
        // …and the repository has both dialects, from one folder.
        assert_eq!(project.dialects(), EngineKind::ALL);
        // The file itself still has no single dialect to be emitted as.
        assert_eq!(project.dialect_of("COMUNE/parametri.sql"), None);
        assert_eq!(project.scope_of("COMUNE/parametri.sql"), Some(DialectScope::Portable));
    }

    // ── When the engine is on the file ────────────────────────────────────────

    /// The untidy repository: one folder, both engines, and only the file names
    /// know which is which.
    fn mixed() -> Project {
        let mut year = node("AGGIORNAMENTO/2024", None, Some(FolderRole::Update));
        year.files = vec![
            file_of("AGGIORNAMENTO/2024/4_12_ORA.sql", FolderEngine::Supported(EngineKind::Oracle)),
            file_of("AGGIORNAMENTO/2024/4_12_POS.sql", FolderEngine::Supported(EngineKind::Postgres)),
            file("AGGIORNAMENTO/2024/note.sql", "windows-1252", "windows-1252"),
        ];
        let mut project =
            Project { name: "P".to_string(), root: "/p".to_string(), tree: vec![year] };
        resolve(&mut project.tree, None, None);
        project
    }

    #[test]
    fn a_file_answers_for_itself_and_its_neighbour_is_unaffected() {
        let p = mixed();
        assert_eq!(p.dialect_of("AGGIORNAMENTO/2024/4_12_ORA.sql"), Some(EngineKind::Oracle));
        assert_eq!(p.dialect_of("AGGIORNAMENTO/2024/4_12_POS.sql"), Some(EngineKind::Postgres));
        // The folder itself still knows nothing, and that is not a contradiction:
        // it is a directory holding two engines, so there is no answer to give.
        assert_eq!(p.folder_at("AGGIORNAMENTO/2024").unwrap().effective_dialect(), None);
        // …and the file nobody classified inherits that nothing, rather than
        // being handed whichever of its neighbours happened to be first.
        assert_eq!(p.dialect_of("AGGIORNAMENTO/2024/note.sql"), None);
    }

    #[test]
    fn a_mixed_folder_is_in_both_lanes_and_the_repository_has_both_dialects() {
        // The failure this replaces: with the engine read off the folder, a
        // directory like this belonged to NO lane, so every cross-dialect rule
        // went silent about half the repository.
        let p = mixed();
        assert_eq!(p.dialects(), EngineKind::ALL);
        for dialect in EngineKind::ALL {
            let lane: Vec<&str> =
                p.lane(*dialect, FolderRole::Update).map(|n| n.path.as_str()).collect();
            assert_eq!(lane, ["AGGIORNAMENTO/2024"], "{dialect}");
        }
        // …and at the granularity content actually has, each lane holds its own
        // file and not its neighbour's.
        let oracle: Vec<&str> = p
            .lane_files(EngineKind::Oracle, FolderRole::Update)
            .map(|(_, f)| f.name.as_str())
            .collect();
        assert_eq!(oracle, ["4_12_ORA.sql"]);
        let postgres: Vec<&str> = p
            .lane_files(EngineKind::Postgres, FolderRole::Update)
            .map(|(_, f)| f.name.as_str())
            .collect();
        assert_eq!(postgres, ["4_12_POS.sql"]);
    }

    #[test]
    fn a_file_declaration_overrides_the_folder_it_sits_in() {
        // A specific answer beats a general one, one level further down than it
        // used to: the folder is Oracle, this file is not.
        let mut ora = node("ORA", Some(EngineKind::Oracle), Some(FolderRole::Update));
        ora.files = vec![
            file("ORA/4_12.sql", "windows-1252", "windows-1252"),
            file_of("ORA/4_12_POS.sql", FolderEngine::Supported(EngineKind::Postgres)),
        ];
        let mut project = Project { name: "P".to_string(), root: "/p".to_string(), tree: vec![ora] };
        resolve(&mut project.tree, None, None);

        assert_eq!(project.dialect_of("ORA/4_12.sql"), Some(EngineKind::Oracle));
        assert_eq!(project.dialect_of("ORA/4_12_POS.sql"), Some(EngineKind::Postgres));
        assert!(!project.file_at("ORA/4_12.sql").unwrap().declares_engine());
        assert!(project.file_at("ORA/4_12_POS.sql").unwrap().declares_engine());
    }

    #[test]
    fn a_single_file_can_be_portable_or_an_engine_picus_does_not_read() {
        let mut folder = node("AGGIORNAMENTO", None, Some(FolderRole::Update));
        folder.files = vec![
            file_of("AGGIORNAMENTO/comune.sql", FolderEngine::Generic),
            file_of(
                "AGGIORNAMENTO/4_12_MSQ.sql",
                FolderEngine::Unsupported(ForeignEngine::SqlServer),
            ),
        ];
        let mut project =
            Project { name: "P".to_string(), root: "/p".to_string(), tree: vec![folder] };
        resolve(&mut project.tree, None, None);

        let portable = project.file_at("AGGIORNAMENTO/comune.sql").unwrap();
        assert!(portable.is_generic());
        assert_eq!(portable.scope(), Some(DialectScope::Portable));
        assert!(EngineKind::ALL.iter().all(|d| portable.covers(*d)));

        let foreign = project.file_at("AGGIORNAMENTO/4_12_MSQ.sql").unwrap();
        assert!(foreign.engine_is_unsupported() && !foreign.engine_is_unknown());
        assert_eq!(foreign.scope(), None, "nothing is parsed with it");
        assert!(EngineKind::ALL.iter().all(|d| !foreign.covers(*d)));
    }

    #[test]
    fn the_wire_shape_carries_the_files_own_engine_and_the_one_in_force() {
        let p = mixed();
        let json = serde_json::to_value(&p).unwrap();
        let files = &json["tree"][0]["files"];
        assert_eq!(files[0]["name"], "4_12_ORA.sql");
        assert_eq!(files[0]["engine"], "oracle");
        assert_eq!(files[0]["effectiveEngine"], "oracle");
        // The unclassified one carries both as null, which is a real answer and
        // the one the interface renders as a question.
        assert_eq!(files[2]["engine"], serde_json::Value::Null);
        assert_eq!(files[2]["effectiveEngine"], serde_json::Value::Null);
    }

    #[test]
    fn every_file_is_indexed_by_path_in_one_walk() {
        // The index exists because asking the tree per file is a walk per file,
        // which is quadratic on exactly the repositories where it hurts.
        let p = mixed();
        let index = p.files_by_path();
        assert_eq!(index.len(), 3);
        assert_eq!(
            index.get("AGGIORNAMENTO/2024/4_12_POS.sql").unwrap().effective_dialect(),
            Some(EngineKind::Postgres)
        );
        assert!(index.get("nowhere.sql").is_none());
    }

    #[test]
    fn an_unsupported_folder_takes_part_in_no_lane_and_no_dialect_list() {
        let mut msq = engine_node(
            "AGGIORNAMENTO/MSQ",
            Some(FolderEngine::Unsupported(ForeignEngine::SqlServer)),
            Some(FolderRole::Update),
        );
        msq.files.push(file("AGGIORNAMENTO/MSQ/4_12.sql", "windows-1252", "windows-1252"));

        let mut project =
            Project { name: "P".to_string(), root: "/p".to_string(), tree: vec![msq] };
        resolve(&mut project.tree, None, None);

        assert!(project.dialects().is_empty());
        assert_eq!(project.lane(EngineKind::Oracle, FolderRole::Update).count(), 0);
        assert_eq!(project.dialect_of("AGGIORNAMENTO/MSQ/4_12.sql"), None);
        assert_eq!(project.scope_of("AGGIORNAMENTO/MSQ/4_12.sql"), None);
    }
}
