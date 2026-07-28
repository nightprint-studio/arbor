//! `apply` domain — the two calls that put a generation into a repository.
//!
//! **Two calls, and the split is the safety.** `picus_preview_apply` does
//! everything except writing and hands back the exact bytes that would land, file
//! by file, each with the digest of what is on disk right now. `picus_apply`
//! re-does that work and **refuses if any of those digests has moved**, naming the
//! file. So what the user approved in the diff is what gets written, or nothing
//! is.
//!
//! Re-preparing rather than trusting the preview's bytes is the same reasoning
//! `picus_confirm_project` already follows: between the two calls the repository
//! may have changed, and writing a plan made against a tree that is no longer
//! there would be worse than asking again.
//!
//! The refusal is not a lock. Picus does not hold the user's files open, does not
//! stop their editor, and does not pretend a repository is a database. It only
//! promises that it will not write over a change it never showed anyone.
//!
//! ## What this module does not do
//!
//! It does not decide *what* the SQL is (`picus-emit`), *where* the block goes
//! ([`crate::placement`]), or *how* a file is edited without being re-printed
//! (`picus-rewrite`, whose refusal to touch a file it cannot reproduce byte for
//! byte is left exactly as it is). It is the ordering, the guard rails and the
//! wire shapes.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use arbor_fs::prelude::encoding::EncodingContext;
use picus_analyze::prelude::fold_identifier;
use picus_ast::prelude::{DmlModel, DmlOperation, FolderRole, Target};
use picus_core::prelude::{digest, InsertionRule, PicusConfig, PicusState, ScriptSnapshot};
use picus_emit::prelude::emit_for_target;
use picus_parse::prelude::SqlParser;
use picus_project::prelude::{
    label_to_encoding, parent_of, LineEnding, MarkerFields, MarkerTemplate, ProjectConfig,
};
use picus_rewrite::prelude::{commit, prepare_one, Eol, PreparedFile, SourceText, Splice};
use serde::{Deserialize, Serialize};

use crate::scripts::snapshot_for;

/// One destination file, resolved to the bytes that would land in it.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewedFile {
    /// Project-relative path, POSIX separators.
    pub path: String,
    /// The file as it is now, decoded. Empty when it does not exist yet.
    pub before: String,
    /// The file as it would be. This is the real thing, not an approximation of
    /// it: the same string is what gets encoded and written.
    pub after: String,
    pub encoding: String,
    pub eol: LineEnding,
    /// One line per edit, in file order — the diff's hunk headers, each naming
    /// the rule that put the block where it is.
    pub reasons: Vec<String>,
    pub creates_file: bool,
    /// The digest of what is on disk **now**, empty when the file is not there.
    /// Hand it back to `picus_apply` unchanged: it is how the write knows nothing
    /// moved under it.
    pub digest: String,
}

/// What the diff view renders.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApplyPreview {
    pub files: Vec<PreviewedFile>,
}

/// What a write did. Three lists rather than a count, because "unchanged" is a
/// result worth seeing: a re-run that reports every file unchanged is the proof
/// that generation is deterministic.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppliedFiles {
    pub written: Vec<String>,
    pub created: Vec<String>,
    pub unchanged: Vec<String>,
}

/// Resolve a generation into the exact bytes it would write. Touches nothing.
#[arbor_rpc::handler]
fn picus_preview_apply(
    state: &PicusState,
    root: String,
    model: DmlModel,
    targets: Vec<Target>,
) -> Result<ApplyPreview, String> {
    let snapshot = snapshot_for(state, &root)?;
    let planned = plan(&snapshot, &model, &targets)?;
    Ok(ApplyPreview {
        files: planned
            .into_iter()
            .map(|file| PreviewedFile {
                path: file.path,
                creates_file: file.prepared.creates_file(),
                encoding: file.prepared.encoding.clone(),
                eol: line_ending_of(file.prepared.eol),
                reasons: file.prepared.reasons.clone(),
                before: file.prepared.before,
                after: file.prepared.after,
                digest: file.digest,
            })
            .collect(),
    })
}

/// Write the generation, or refuse because something moved since the preview.
#[arbor_rpc::handler]
fn picus_apply(
    state: &PicusState,
    root: String,
    model: DmlModel,
    targets: Vec<Target>,
    digests: Option<Digests>,
) -> Result<AppliedFiles, String> {
    let approved = digests
        .ok_or_else(|| {
            "an apply has to carry the digests its preview returned — nothing was written"
                .to_string()
        })?
        .into_map();

    let snapshot = snapshot_for(state, &root)?;
    let planned = plan(&snapshot, &model, &targets)?;

    let current: Vec<(String, String)> =
        planned.iter().map(|f| (f.path.clone(), f.digest.clone())).collect();
    unchanged_since_preview(&current, &approved)?;

    let prepared: Vec<PreparedFile> = planned.into_iter().map(|f| f.prepared).collect();
    let applied = commit(&prepared).map_err(|e| e.to_string())?;

    // The files on disk are no longer the files that were read. Invalidating is
    // the honest move: the next question re-reads, rather than answering from a
    // snapshot that is now a description of the past.
    state.scripts().invalidate(&snapshot.root);

    let relative = |paths: Vec<PathBuf>| -> Vec<String> {
        paths.iter().map(|p| relative_to(&snapshot.root, p)).collect()
    };
    Ok(AppliedFiles {
        written: relative(applied.written),
        created: relative(applied.created),
        unchanged: relative(applied.unchanged),
    })
}

/// The digests the preview handed out, coming back.
///
/// Accepts either the map the preview's files imply or a list of pairs, because
/// the two are equally natural readings of "the digests" and a mismatch here would
/// present as an apply that refuses everything for no visible reason.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum Digests {
    ByPath(BTreeMap<String, String>),
    Listed(Vec<FileDigest>),
}

/// One entry of the list form.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileDigest {
    pub path: String,
    pub digest: String,
}

impl Digests {
    fn into_map(self) -> BTreeMap<String, String> {
        match self {
            Digests::ByPath(map) => map,
            Digests::Listed(list) => list.into_iter().map(|f| (f.path, f.digest)).collect(),
        }
    }
}

// ── Planning ───────────────────────────────────────────────────────────────────

/// One destination, resolved.
pub(crate) struct PlannedFile {
    /// Project-relative path, POSIX separators.
    pub path: String,
    pub prepared: PreparedFile,
    /// Digest of the bytes read from disk for this plan.
    pub digest: String,
}

/// Resolve every target into the bytes it would write.
///
/// Shared by both handlers rather than approximated twice: the preview showing
/// something other than what the apply prepares is the one bug this design has no
/// answer for, so there is only one code path that can produce either.
pub(crate) fn plan(
    snapshot: &ScriptSnapshot,
    model: &DmlModel,
    targets: &[Target],
) -> Result<Vec<PlannedFile>, String> {
    // The user's own defaults, read once per plan. Where a block lands can be
    // stated by the repository, and this is the tier underneath that.
    let user = picus_core::config::load();
    let table = fold_identifier(&model.table);
    let mut parser = SqlParser::new();
    let mut out: Vec<PlannedFile> = Vec::with_capacity(targets.len());

    for target in targets {
        if out.iter().any(|f| f.path == target.file) {
            // Two targets on one file would each be prepared against the original
            // text, and the second would silently undo the first.
            return Err(format!(
                "{} is the destination of two targets in the same generation — \
                 Picus cannot write both",
                target.file
            ));
        }
        let path = destination(&snapshot.root, &target.file)?;

        // How this folder writes: the encoding the file already is, or the one the
        // folder expects for a file that does not exist yet.
        let (label, eol) = conventions(snapshot, &target.file);
        let encoding = label_to_encoding(&label);
        let context = EncodingContext::new().with_legacy(encoding).with_dominant(encoding);
        let source =
            SourceText::read(&path, &context, encoding, eol).map_err(|e| e.to_string())?;

        let parsed = parser.parse(&source.text, target.dialect);
        let rule = insertion_rule_for(&snapshot.config, &user, target.role);
        let placement = crate::placement::place(
            &source.text,
            &parsed,
            &table,
            rule,
            &snapshot.config.generation.marker,
        );

        let splice = Splice {
            range: placement.range,
            replacement: block_text(model, target, &snapshot.config.generation.marker)?,
            reason: placement.reason,
        };
        let prepared = prepare_one(&source, &[splice]).map_err(|e| e.to_string())?;

        out.push(PlannedFile {
            path: target.file.clone(),
            digest: if source.exists { digest(&source.bytes) } else { String::new() },
            prepared,
        });
    }
    Ok(out)
}

/// The marker line and the generated SQL, as one block ending in a line break.
///
/// The trailing newline is not cosmetic: [`crate::placement`] gives back a range
/// that includes the line break it consumed, so a block that ends without one
/// would join itself to whatever follows the second time it is generated.
/// Refuses rather than writing when the destination cannot take this model — a
/// portable folder asked for an upsert, or wrapped in a block. The refusal comes
/// from the emitter itself, so no caller can reach the wrong bytes by forgetting
/// to check first.
fn block_text(
    model: &DmlModel,
    target: &Target,
    marker: &MarkerTemplate,
) -> Result<String, String> {
    let mut sql = emit_for_target(model, target)
        .map_err(|refusal| format!("{}: {refusal}", target.file))?;
    if !sql.ends_with('\n') {
        sql.push('\n');
    }

    // Short enough to read in a comment, long enough that two different blocks
    // never share one. Only rendered when the project asked for `{hash}`.
    let hash: String = digest(sql.as_bytes()).chars().take(12).collect();
    let version = target.guards.version.as_ref();
    let fields = MarkerFields {
        table: Some(&model.table),
        operation: Some(operation_word(model.operation)),
        from_version: version.map(|v| v.from.as_str()),
        to_version: version.map(|v| v.to.as_str()),
        hash: Some(&hash),
    };

    Ok(match marker.render(&fields) {
        Some(line) => format!("{line}\n{sql}"),
        // Marking switched off: the block is the SQL, and nothing will be able to
        // find it again — which is exactly what emptying the template buys.
        None => sql,
    })
}

fn operation_word(operation: DmlOperation) -> &'static str {
    match operation {
        DmlOperation::Insert => "insert",
        DmlOperation::Upsert => "upsert",
        DmlOperation::Update => "update",
        DmlOperation::Delete => "delete",
    }
}

/// The rule in force for a role: what the repository says, then what this user
/// prefers, then the built-in default.
///
/// The repository wins because where a block lands shows up in every colleague's
/// diff, and a setting that made the same generation land differently per person
/// is the class of surprise Picus exists to remove.
pub(crate) fn insertion_rule_for(
    project: &ProjectConfig,
    user: &PicusConfig,
    role: FolderRole,
) -> InsertionRule {
    if let Some(declared) = project.generation.insertion_for(role) {
        return declared;
    }
    match role {
        FolderRole::Update => user.generation.insertion_rule_update(),
        // A data folder is seeded the way an initialisation folder is, so it takes
        // the same preference. `Routines` and `Ignored` have no preference of
        // their own to take.
        FolderRole::Init | FolderRole::Data => user.generation.insertion_rule_init(),
        FolderRole::Routines | FolderRole::Ignored => InsertionRule::default_for(role),
    }
}

/// Has anything moved since the preview?
///
/// Pure, and separated from the I/O for that reason: this is the check the whole
/// two-call design exists for, and every branch of it is asserted below.
pub(crate) fn unchanged_since_preview(
    current: &[(String, String)],
    approved: &BTreeMap<String, String>,
) -> Result<(), String> {
    for (path, now) in current {
        let Some(then) = approved.get(path) else {
            return Err(format!(
                "{path} was not part of the preview — nothing was written. \
                 Preview the generation again and review what it would change."
            ));
        };
        if then == now {
            continue;
        }
        return Err(match (then.is_empty(), now.is_empty()) {
            // It was going to be created, and now it is there.
            (true, false) => format!(
                "{path} has been created by something else since the preview — \
                 nothing was written. Preview the generation again."
            ),
            // It was there, and now it is not.
            (false, true) => format!(
                "{path} has been deleted since the preview — nothing was written. \
                 Preview the generation again."
            ),
            _ => format!(
                "{path} changed on disk since the preview — nothing was written. \
                 Preview the generation again and review what it would change."
            ),
        });
    }
    Ok(())
}

/// The absolute path of a destination, refusing anything that leaves the
/// repository.
///
/// A target's file comes from the interface, and the interface builds it from the
/// tree — but this is the function that writes, and "the caller would never send
/// that" is not a property a writing function gets to assume.
fn destination(root: &Path, relative: &str) -> Result<PathBuf, String> {
    let refused = relative.is_empty()
        || relative.starts_with('/')
        || relative.starts_with('\\')
        || relative.split(['/', '\\']).any(|segment| segment == "..")
        || relative.chars().nth(1) == Some(':');
    if refused {
        return Err(format!("{relative} is not a path inside this repository"));
    }
    Ok(root.join(relative))
}

/// The encoding label and line ending a destination is written in: its own where
/// the file exists, its folder's where it is about to be created.
///
/// The folder's answer is the nearest declaration at or above it, so a file about
/// to be created three levels down takes the encoding somebody pinned at the top
/// rather than the project-wide default.
fn conventions(snapshot: &ScriptSnapshot, relative: &str) -> (String, Eol) {
    if let Some(source) = snapshot.source(relative) {
        return (source.encoding.clone(), eol_of(source.eol));
    }
    let label = snapshot.config.encoding_for(parent_of(relative));
    (label.to_string(), eol_of(snapshot.config.encoding.eol))
}

fn eol_of(line_ending: LineEnding) -> Eol {
    match line_ending {
        LineEnding::Crlf => Eol::Crlf,
        LineEnding::Lf => Eol::Lf,
    }
}

fn line_ending_of(eol: Eol) -> LineEnding {
    match eol {
        Eol::Crlf => LineEnding::Crlf,
        Eol::Lf => LineEnding::Lf,
    }
}

/// A written path back in the form the interface speaks.
fn relative_to(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .map(|p| p.to_string_lossy().replace('\\', "/"))
        .unwrap_or_else(|_| path.to_string_lossy().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use picus_project::prelude::{
        EncodingSettings, FolderDeclaration, NamingScheme, CURRENT_VERSION,
    };

    fn approved(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs.iter().map(|(p, d)| (p.to_string(), d.to_string())).collect()
    }

    fn current(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
        pairs.iter().map(|(p, d)| (p.to_string(), d.to_string())).collect()
    }

    #[test]
    fn an_untouched_repository_writes() {
        let plan = current(&[("ORACLE/A.sql", "aaa"), ("POSTGRES/a.sql", "bbb")]);
        let ok = approved(&[("ORACLE/A.sql", "aaa"), ("POSTGRES/a.sql", "bbb"), ("OTHER.sql", "zzz")]);
        // An approval for a file this plan does not touch is harmless: the user
        // previewed more than they applied.
        assert!(unchanged_since_preview(&plan, &ok).is_ok());
    }

    #[test]
    fn a_file_that_changed_is_refused_by_name() {
        let plan = current(&[("ORACLE/A.sql", "aaa"), ("ORACLE/AGGIORNAMENTO/4_12__4_13.sql", "new")]);
        let err = unchanged_since_preview(&plan, &approved(&[
            ("ORACLE/A.sql", "aaa"),
            ("ORACLE/AGGIORNAMENTO/4_12__4_13.sql", "old"),
        ]))
        .unwrap_err();

        assert!(err.contains("ORACLE/AGGIORNAMENTO/4_12__4_13.sql"), "{err}");
        assert!(err.contains("changed on disk since the preview"), "{err}");
        assert!(err.contains("nothing was written"), "{err}");
        // …and it does not name the file that is fine.
        assert!(!err.contains("ORACLE/A.sql"), "{err}");
    }

    #[test]
    fn a_file_that_appeared_and_a_file_that_vanished_read_differently() {
        // Both are "the digest moved", and both deserve their own sentence: the
        // user has to know whether their block would have clobbered something new
        // or landed in a hole.
        let appeared = unchanged_since_preview(
            &current(&[("ORACLE/NEW.sql", "aaa")]),
            &approved(&[("ORACLE/NEW.sql", "")]),
        )
        .unwrap_err();
        assert!(appeared.contains("created by something else"), "{appeared}");

        let vanished = unchanged_since_preview(
            &current(&[("ORACLE/OLD.sql", "")]),
            &approved(&[("ORACLE/OLD.sql", "aaa")]),
        )
        .unwrap_err();
        assert!(vanished.contains("deleted since the preview"), "{vanished}");
    }

    #[test]
    fn a_file_the_preview_never_mentioned_is_refused() {
        // Applying more than was shown is the same failure as applying something
        // stale: the user approved a diff, not a promise.
        let err = unchanged_since_preview(
            &current(&[("ORACLE/A.sql", "aaa")]),
            &approved(&[("POSTGRES/a.sql", "aaa")]),
        )
        .unwrap_err();
        assert!(err.contains("was not part of the preview"), "{err}");
        assert!(err.contains("ORACLE/A.sql"), "{err}");
    }

    #[test]
    fn nothing_to_write_is_not_an_error() {
        assert!(unchanged_since_preview(&[], &approved(&[])).is_ok());
    }

    #[test]
    fn the_digests_arrive_as_a_map_or_as_a_list() {
        let from_map: Digests =
            serde_json::from_str(r#"{"ORACLE/A.sql":"aaa"}"#).expect("map form");
        let from_list: Digests =
            serde_json::from_str(r#"[{"path":"ORACLE/A.sql","digest":"aaa"}]"#).expect("list form");
        assert_eq!(from_map.into_map(), from_list.into_map());
    }

    // ── Which rule applies ────────────────────────────────────────────────────

    fn project_config(declared: &[(FolderRole, InsertionRule)]) -> ProjectConfig {
        let mut config = ProjectConfig {
            version: CURRENT_VERSION,
            name: "PROD_CORE".to_string(),
            encoding: EncodingSettings::default(),
            version_table: Default::default(),
            generation: Default::default(),
            naming: NamingScheme::default(),
            analysis: Default::default(),
            folders: Vec::new(),
            files: Vec::new(),
            aliases: Vec::new(),
        };
        for (role, rule) in declared {
            config
                .generation
                .insertion
                .insert(role.as_str().to_string(), rule.as_wire().to_string());
        }
        config
    }

    #[test]
    fn the_defaults_are_append_for_updates_and_group_for_initialisation() {
        let project = project_config(&[]);
        let user = PicusConfig::default();
        assert_eq!(
            insertion_rule_for(&project, &user, FolderRole::Update),
            InsertionRule::EndOfFile
        );
        assert_eq!(
            insertion_rule_for(&project, &user, FolderRole::Init),
            InsertionRule::AfterLastOnTable
        );
        // A data folder is seeded like an initialisation one.
        assert_eq!(
            insertion_rule_for(&project, &user, FolderRole::Data),
            InsertionRule::AfterLastOnTable
        );
    }

    #[test]
    fn the_repository_outranks_the_user() {
        let project = project_config(&[(FolderRole::Update, InsertionRule::BeforeFinalCommit)]);
        let mut user = PicusConfig::default();
        user.generation.insertion_rule_update = InsertionRule::EndOfFile.as_wire().to_string();

        assert_eq!(
            insertion_rule_for(&project, &user, FolderRole::Update),
            InsertionRule::BeforeFinalCommit,
            "a rule in project.toml is a decision the whole team inherits"
        );
        // …and a role the repository says nothing about still takes the user's.
        user.generation.insertion_rule_init = InsertionRule::EndOfFile.as_wire().to_string();
        assert_eq!(
            insertion_rule_for(&project, &user, FolderRole::Init),
            InsertionRule::EndOfFile
        );
    }

    #[test]
    fn a_destination_outside_the_repository_is_refused() {
        let root = Path::new("/repo");
        for bad in ["../outside.sql", "ORACLE/../../outside.sql", "/etc/passwd", "C:\\x.sql", ""] {
            assert!(destination(root, bad).is_err(), "{bad} must be refused");
        }
        assert!(destination(root, "ORACLE/AGGIORNAMENTO/4_12__4_13.sql").is_ok());
    }

    #[test]
    fn a_new_file_is_created_with_its_folders_conventions() {
        // A declared encoding reaches every folder below it, and the nearest
        // declaration wins.
        let mut config = project_config(&[]);
        config.encoding.default = "UTF-8".to_string();
        config.folders.push(FolderDeclaration {
            path: "ORACLE".to_string(),
            encoding: Some("windows-1252".to_string()),
            ..FolderDeclaration::default()
        });

        let snapshot = ScriptSnapshot {
            root: PathBuf::from("/repo"),
            project: picus_project::prelude::Project {
                name: "PROD_CORE".to_string(),
                root: "/repo".to_string(),
                tree: Vec::new(),
            },
            config,
            notes: Vec::new(),
            is_new: false,
            problems: Vec::new(),
            sources: BTreeMap::new(),
        };

        // Three levels under the folder that declared it.
        let (label, eol) = conventions(&snapshot, "ORACLE/AGGIORNAMENTO/2026/4_13__4_14.sql");
        assert_eq!(label, "windows-1252");
        assert_eq!(eol, Eol::Crlf);

        // A file no declaration covers takes the project's own default.
        let (label, _) = conventions(&snapshot, "loose.sql");
        assert_eq!(label, "UTF-8");
    }
}
