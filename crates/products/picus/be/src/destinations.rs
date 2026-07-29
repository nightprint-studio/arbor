//! `destinations` domain — named sets of destinations, and resolving one.
//!
//! The repetition this removes: every generation in a real repository goes to the
//! same four or six places, and before this the user rebuilt that list by hand
//! each time. A set names the places once.
//!
//! ## Resolving is the interesting half
//!
//! A stored entry names a **folder**, not a file, because half of those paths
//! change every release — `4_13.sql` becomes `4_14.sql` — and a template of
//! literal paths is stale exactly when it is most useful. So applying a set is not
//! a paste: for each entry it re-reads the repository and works out
//!
//!  * which **file** — the fixed one it names, or the next update file under the
//!    folder's own naming scheme;
//!  * which **engine, role and product** — the folder's, exactly as when a
//!    destination is added by hand;
//!  * which **versions** the guard should carry — from the same naming scheme,
//!    which is what already knows that `4_12__4_13.sql` follows `4_11__4_12.sql`.
//!
//! Anything it cannot work out is reported **per entry** rather than failing the
//! whole set: a folder that was renamed should cost the user that one destination,
//! not the other five.
//!
//! Nothing here writes SQL or touches a script. Saving a set writes
//! `.arbor/picus/project.toml`, like every other declaration about a repository.

use std::path::PathBuf;

use picus_core::prelude::PicusState;
use picus_project::prelude::{
    discover, DestinationEntry, DestinationSet, FolderEngine, FolderRole, Project, ProjectConfig,
};
use serde::{Deserialize, Serialize};

use crate::project::{save_and_replan, ConfirmedProject};

/// One entry of a set, resolved against the repository as it is now.
///
/// Every field the interface needs to call `addTarget` and then apply the rules —
/// so applying a set is a loop over these and nothing else.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolvedDestination {
    /// The folder as stored, so a failed entry can still be named.
    pub folder: String,
    /// Project-relative path of the file this entry resolves to. Empty when the
    /// entry could not be resolved.
    pub file: String,
    /// `true` when that file does not exist yet — a new update script, which is
    /// the ordinary case for the entries that leave the name to the scheme.
    pub creates_file: bool,
    /// The folder's engine. Absent when the folder has none, which is the one
    /// state that makes a destination impossible.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dialect: Option<FolderEngine>,
    pub role: FolderRole,
    /// The folder's product, for the version row. Absent for the ordinary
    /// repository.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub product: Option<String>,
    pub wrap: String,
    pub version_guard: bool,
    pub skip_if_present: bool,
    pub require_object: bool,
    pub transactional: bool,
    /// The versions the naming scheme says this file moves between, when it could
    /// say. These fill the guard's bounds so a release template arrives complete.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from_version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub to_version: Option<String>,
    /// This entry names one **fixed file** rather than following the folder's
    /// naming scheme, so it will keep writing into that file next release.
    ///
    /// Not a failure — for a folder whose names the scheme cannot read it is the
    /// only thing that works — but a difference the user has to be able to see,
    /// because the whole promise of a set is "it still works next release" and
    /// these entries do not keep it.
    pub pinned: bool,
    /// Why this entry cannot be used, in the user's terms. `None` when it can.
    ///
    /// Per entry rather than per set: a folder that was renamed should cost the
    /// user one destination, not the whole template.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub problem: Option<String>,
}

/// A set with every entry resolved.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolvedSet {
    pub name: String,
    pub destinations: Vec<ResolvedDestination>,
}

/// Every set this repository declares, resolved against it as it is now.
///
/// Resolved on read rather than on apply so the picker can show what each set
/// would actually do — including that one of its folders has gone — before the
/// user commits to it.
#[arbor_rpc::handler]
fn picus_destination_sets(_state: &PicusState, root: String) -> Result<Vec<ResolvedSet>, String> {
    let root = PathBuf::from(&root);
    let proposal = discover(&root).map_err(|e| e.to_string())?;
    Ok(proposal
        .config
        .destination_sets
        .iter()
        .map(|set| resolve_set(set, &proposal.project, &proposal.config))
        .collect())
}

/// What the interface sends to save a set. The stored shape, in `camelCase`.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DestinationSetInput {
    pub name: String,
    pub entries: Vec<DestinationEntryInput>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DestinationEntryInput {
    pub folder: String,
    /// The file the destination is currently pointed at.
    ///
    /// Sent for **every** entry, including update ones: whether it can be dropped
    /// in favour of the folder's naming scheme is decided here, by the half that
    /// can read the folder. See [`follows_the_scheme`].
    #[serde(default)]
    pub file: Option<String>,
    #[serde(default)]
    pub wrap: Option<String>,
    #[serde(default)]
    pub version_guard: bool,
    /// The guard's bounds as they stand. Kept only for an entry the naming scheme
    /// cannot re-derive them for — the same decision as [`DestinationEntryInput::file`],
    /// made in the same place and for the same reason.
    #[serde(default)]
    pub from_version: Option<String>,
    #[serde(default)]
    pub to_version: Option<String>,
    #[serde(default)]
    pub skip_if_present: bool,
    #[serde(default)]
    pub require_object: bool,
    #[serde(default)]
    pub transactional: bool,
}

/// Save a set under its name, replacing one of the same name.
///
/// Refuses a set with no name and one with no entries: both would appear in the
/// picker and do nothing, which is worse than not being there. A folder that does
/// not exist is **not** refused — a set may legitimately be written before the
/// release folder is created, and the resolution above already reports it.
#[arbor_rpc::handler]
fn picus_save_destination_set(
    state: &PicusState,
    root: String,
    set: DestinationSetInput,
) -> Result<ConfirmedProject, String> {
    let name = set.name.trim().to_string();
    if name.is_empty() {
        return Err("a set of destinations needs a name to be picked by".to_string());
    }
    if set.entries.is_empty() {
        return Err(format!("{name} has no destinations in it — nothing would be written"));
    }

    let root = PathBuf::from(&root);
    let proposal = discover(&root).map_err(|e| e.to_string())?;
    let mut config = proposal.config;
    let entries = set
        .entries
        .into_iter()
        .map(|entry| stored(entry, &proposal.project, &config))
        .collect();
    config.put_destination_set(DestinationSet { name, entries });
    config.tidy();
    save_and_replan(state, &root, &config)
}

/// Forget a set. Naming one that is not there is an error rather than a no-op:
/// the interface only offers sets that exist, so it means the two have diverged.
#[arbor_rpc::handler]
fn picus_delete_destination_set(
    state: &PicusState,
    root: String,
    name: String,
) -> Result<ConfirmedProject, String> {
    let root = PathBuf::from(&root);
    let proposal = discover(&root).map_err(|e| e.to_string())?;
    let mut config = proposal.config;
    if !config.remove_destination_set(&name) {
        return Err(format!("this repository declares no set of destinations called {name}"));
    }
    config.tidy();
    save_and_replan(state, &root, &config)
}

fn stored(
    input: DestinationEntryInput,
    project: &Project,
    config: &ProjectConfig,
) -> DestinationEntry {
    let folder = input.folder.trim().to_string();
    // Blank and absent are the same intention, and only one of them should reach
    // the file.
    let file = input.file.map(|f| f.trim().to_string()).filter(|f| !f.is_empty());
    // One question, asked once, deciding both what is kept and what is dropped —
    // the file name and the guard's bounds come from the same place, so they must
    // not be able to disagree about whether that place has an answer.
    let followable = follows_the_scheme(&folder, project, config);
    DestinationEntry {
        // Dropped only when the repository can actually work the name out again.
        //
        // This decision is made **here**, not by the interface that sent the
        // entry: whether a folder's next file can be named is a question about
        // the folder's contents and the project's naming scheme, and the half
        // that cannot read either of them was answering it with "it is an update
        // folder, therefore yes". For a folder the scheme cannot read that threw
        // away the one thing that made the entry work — the user got a set whose
        // update destinations vanished the moment it was applied.
        file: if followable { None } else { file },
        folder,
        wrap: input.wrap.filter(|w| w == "block" || w == "plain"),
        version_guard: input.version_guard,
        // Kept for exactly the entries that have nowhere else to get them. An
        // entry the scheme can name gets fresh numbers every release; one it
        // cannot got *no* numbers at all, and came back with an empty guard — the
        // set remembered that the destination wanted a guard and forgot the only
        // two values that make one mean anything.
        from_version: if followable { None } else { trimmed(input.from_version) },
        to_version: if followable { None } else { trimmed(input.to_version) },
        skip_if_present: input.skip_if_present,
        require_object: input.require_object,
        transactional: input.transactional,
    }
}

/// Blank and absent mean the same thing on the way in; only one of them is stored.
fn trimmed(value: Option<String>) -> Option<String> {
    value.map(|v| v.trim().to_string()).filter(|v| !v.is_empty())
}

/// Whether this folder's *next* update file can be named without being told.
///
/// Three conditions, all necessary: the folder still exists, it holds updates,
/// and something already in it matches the naming scheme — a pattern that reads
/// nothing has no highest version to count from. Anything else means the entry
/// has to carry its file name, which costs it the "still works next release"
/// promise and is worth saying out loud (see `ResolvedDestination::pinned`).
fn follows_the_scheme(folder_path: &str, project: &Project, config: &ProjectConfig) -> bool {
    let Some(folder) = project.folder_at(folder_path) else { return false };
    if folder.effective_role != FolderRole::Update {
        return false;
    }
    let Ok(naming) = config.naming_for(&folder.path).compile() else { return false };
    naming.propose_next(folder.files.iter().map(|f| f.name.as_str())).is_some()
}

fn resolve_set(set: &DestinationSet, project: &Project, config: &ProjectConfig) -> ResolvedSet {
    ResolvedSet {
        name: set.name.clone(),
        destinations: set.entries.iter().map(|e| resolve_entry(e, project, config)).collect(),
    }
}

fn resolve_entry(
    entry: &DestinationEntry,
    project: &Project,
    config: &ProjectConfig,
) -> ResolvedDestination {
    let mut out = ResolvedDestination {
        folder: entry.folder.clone(),
        file: String::new(),
        creates_file: false,
        dialect: None,
        role: FolderRole::Ignored,
        product: None,
        wrap: entry.wrap.clone().unwrap_or_else(|| "plain".to_string()),
        version_guard: entry.version_guard,
        skip_if_present: entry.skip_if_present,
        require_object: entry.require_object,
        transactional: entry.transactional,
        // The stored bounds are the starting point, not the fallback: the branch
        // below replaces them wherever the naming scheme has something fresher to
        // say, and an entry the scheme can read stores none in the first place.
        from_version: entry.from_version.clone(),
        to_version: entry.to_version.clone(),
        pinned: entry.file.is_some(),
        problem: None,
    };

    let Some(folder) = project.folder_at(&entry.folder) else {
        out.problem = Some(format!(
            "{} is no longer a folder of this repository — it was renamed or removed.",
            entry.folder
        ));
        return out;
    };

    out.role = folder.effective_role;
    out.dialect = folder.effective_engine;
    out.product = folder.effective_product.clone();
    if entry.wrap.is_none() {
        // No stored wrap: take the role's own default, the same one a destination
        // added by hand arrives with.
        out.wrap =
            if folder.effective_role == FolderRole::Update { "block" } else { "plain" }.to_string();
    }

    if folder.effective_engine.is_none() {
        out.problem = Some(format!(
            "{} has no engine, so there is no form to write the statements in.",
            entry.folder
        ));
        return out;
    }

    match &entry.file {
        Some(name) => {
            out.file = join(&entry.folder, name);
            out.creates_file = !folder.files.iter().any(|f| f.name.eq_ignore_ascii_case(name));
        }
        None => {
            // The whole reason an entry stores a folder: work out what this
            // release's file is called, and which versions it moves between.
            let naming = match config.naming_for(&folder.path).compile() {
                Ok(naming) => naming,
                Err(e) => {
                    out.problem = Some(format!("{}: {e}", entry.folder));
                    return out;
                }
            };
            let existing: Vec<&str> = folder.files.iter().map(|f| f.name.as_str()).collect();
            let Some(range) = naming.propose_next(existing) else {
                // Actionable, because the thing it used to suggest — "name the
                // file on this entry" — is not something any interface can do:
                // an entry is not editable, a set is re-saved. Saving now keeps
                // the file name, so re-arming and saving again is the fix.
                out.problem = Some(format!(
                    "{}: no file here matches this project's update-file naming, so the next \
                     one cannot be named. Arm this destination by hand and save the set again \
                     — the file name is kept for a folder like this one. Or give the folder \
                     its own naming pattern in the project settings.",
                    entry.folder
                ));
                return out;
            };
            out.file = join(&entry.folder, &naming.render(&range));
            out.creates_file = true;
            out.from_version = range.from.map(|v| v.to_string());
            out.to_version = Some(range.to.to_string());
        }
    }

    out
}

fn join(folder: &str, file: &str) -> String {
    if folder.is_empty() {
        file.to_string()
    } else {
        format!("{folder}/{file}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Two update folders that disagree about the project's naming scheme — the
    /// shape the whole decision exists for, and the one a fixture with a single
    /// tidy folder cannot express.
    fn repository(name: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!("picus-sets-{name}"));
        let _ = std::fs::remove_dir_all(&root);
        for relative in [
            // Reads as `2.4 → 2.5` under the default pattern, so the next one can
            // be named without being told.
            "ORACLE/AGGIORNAMENTO/2_4__2_5.sql",
            // Named after the release rather than after the transition. Nothing
            // here says what follows it.
            "ORACLE/MIGRATIONS/rilascio_2.05.sql",
            "ORACLE/INIZIALIZZAZIONE/01_SCHEDARIO.sql",
        ] {
            let path = root.join(relative);
            std::fs::create_dir_all(path.parent().unwrap()).expect("mkdir");
            std::fs::write(&path, "-- vuoto\n").expect("write");
        }
        root
    }

    fn entry(folder: &str, file: &str) -> DestinationEntryInput {
        DestinationEntryInput {
            folder: folder.to_string(),
            file: Some(file.to_string()),
            wrap: None,
            version_guard: false,
            from_version: None,
            to_version: None,
            skip_if_present: false,
            require_object: false,
            transactional: false,
        }
    }

    /// The same, carrying the guard a user filled in by hand.
    fn guarded(folder: &str, file: &str, from: &str, to: &str) -> DestinationEntryInput {
        DestinationEntryInput {
            version_guard: true,
            from_version: Some(from.to_string()),
            to_version: Some(to.to_string()),
            ..entry(folder, file)
        }
    }

    #[test]
    fn an_update_folder_the_scheme_can_read_stores_no_file_name() {
        let root = repository("followable");
        let found = discover(&root).expect("discovers");

        let stored =
            stored(entry("ORACLE/AGGIORNAMENTO", "2_5__2_6.sql"), &found.project, &found.config);
        // Dropped on purpose: next release the same set has to arrive at
        // `2_6__2_7.sql` without anybody editing it.
        assert_eq!(stored.file, None);

        let resolved = resolve_entry(&stored, &found.project, &found.config);
        assert_eq!(resolved.problem, None, "{resolved:?}");
        assert!(!resolved.pinned);
        assert_eq!(resolved.file, "ORACLE/AGGIORNAMENTO/2_5__2_6.sql");
        assert_eq!(resolved.to_version.as_deref(), Some("2.6"));
    }

    #[test]
    fn an_update_folder_the_scheme_cannot_read_keeps_its_file_name() {
        // The bug this pins: the interface dropped the file for *every* update
        // destination, on the theory that the scheme would name it again. For a
        // folder like this one the scheme cannot, and the entry the user had just
        // arranged came back unusable — a set that lost two of its three
        // destinations the moment it was applied.
        let root = repository("unfollowable");
        let found = discover(&root).expect("discovers");

        let stored =
            stored(entry("ORACLE/MIGRATIONS", "rilascio_2.06.sql"), &found.project, &found.config);
        assert_eq!(stored.file.as_deref(), Some("rilascio_2.06.sql"));

        let resolved = resolve_entry(&stored, &found.project, &found.config);
        assert_eq!(resolved.problem, None, "{resolved:?}");
        assert!(resolved.pinned, "it writes into that file, and says so");
        assert_eq!(resolved.file, "ORACLE/MIGRATIONS/rilascio_2.06.sql");
        assert!(resolved.creates_file, "the file is not there yet");
    }

    #[test]
    fn a_seeding_folder_always_keeps_its_file_name() {
        // An initialisation names one file for ever; there is no scheme to consult
        // and nothing to re-derive.
        let root = repository("seeding");
        let found = discover(&root).expect("discovers");

        let stored = stored(
            entry("ORACLE/INIZIALIZZAZIONE", "01_SCHEDARIO.sql"),
            &found.project,
            &found.config,
        );
        assert_eq!(stored.file.as_deref(), Some("01_SCHEDARIO.sql"));

        let resolved = resolve_entry(&stored, &found.project, &found.config);
        assert_eq!(resolved.problem, None, "{resolved:?}");
        assert!(resolved.pinned);
        assert!(!resolved.creates_file, "it is already there");
    }

    #[test]
    fn a_pinned_entry_keeps_the_guards_bounds_and_a_followable_one_does_not() {
        // The bounds and the file name are the same decision: an entry the scheme
        // can read gets both fresh every release, one it cannot has nowhere else
        // to get either. Storing the file but not the bounds — which is what
        // happened first — produced a destination pinned to one file and guarded
        // by nothing, which is the one combination that is never right.
        let root = repository("bounds");
        let found = discover(&root).expect("discovers");

        let pinned = stored(
            guarded("ORACLE/MIGRATIONS", "rilascio_2.06.sql", "2.5", "2.6"),
            &found.project,
            &found.config,
        );
        assert_eq!(pinned.from_version.as_deref(), Some("2.5"));
        assert_eq!(pinned.to_version.as_deref(), Some("2.6"));
        let resolved = resolve_entry(&pinned, &found.project, &found.config);
        assert_eq!(resolved.from_version.as_deref(), Some("2.5"));
        assert_eq!(resolved.to_version.as_deref(), Some("2.6"));

        let followable = stored(
            guarded("ORACLE/AGGIORNAMENTO", "2_5__2_6.sql", "2.5", "2.6"),
            &found.project,
            &found.config,
        );
        assert_eq!(followable.from_version, None, "last release's numbers are not kept");
        assert!(followable.version_guard, "the guard itself is");
        // …and come back from the scheme, one release further on than they went in.
        let resolved = resolve_entry(&followable, &found.project, &found.config);
        assert_eq!(resolved.to_version.as_deref(), Some("2.6"));
    }

    #[test]
    fn a_fileless_entry_the_scheme_cannot_resolve_says_how_to_fix_it() {
        // Sets written before the decision moved to the backend still hold these.
        // The reason has to name a way out that some interface actually offers —
        // "name the file on this entry" named one that none of them do.
        let root = repository("legacy");
        let found = discover(&root).expect("discovers");

        let orphan = DestinationEntry {
            folder: "ORACLE/MIGRATIONS".to_string(),
            file: None,
            ..DestinationEntry::default()
        };
        let resolved = resolve_entry(&orphan, &found.project, &found.config);
        let problem = resolved.problem.expect("it cannot be resolved");
        assert!(problem.contains("save the set again"), "{problem}");
        assert!(resolved.file.is_empty());
    }
}
