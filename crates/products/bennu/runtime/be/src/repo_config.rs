//! One definition of "a bennu setting that belongs to this repository".
//!
//! Per-repo preferences live in **`<repo>/.arbor/bennu/config.toml`** — run configurations,
//! breakpoints, the Tomcat link. Filesystem and not `localStorage`, per the working agreement,
//! and per-repo rather than per-profile because they describe *this project* and travel with it.
//!
//! ## Why a file of bennu's own, inside `.arbor/`
//!
//! These used to sit in `<repo>/.arbor/config.toml` under `[bennu.<section>]`, beside corvus's
//! own keys. That file is shared by every product that opens the repository, which made writing
//! one section a whole-file rewrite whose failure mode is *deleting somebody else's settings* —
//! a merge that had to be got right in three places before this module existed. A file of
//! bennu's own removes the hazard rather than centralising it: it has one writer, the `bennu.`
//! prefix that only existed to keep out of corvus's way is gone, and a project can be open in
//! both editors without either one's persistence being a risk to the other's.
//!
//! It stays **under `.arbor/`** rather than becoming a second dot-directory at the repository
//! root: a checkout should acquire one of those for Arbor, not one per product, and everything
//! already written to ignore or clean `.arbor/` keeps working.
//!
//! Sections are therefore top-level: `[run]`, `[debug]`, `[tomcat]`.
//!
//! ## Reading what is already on disk
//!
//! A project configured before the move has its sections in the old place, and a run
//! configuration that silently disappears is worse than any amount of tidiness. So a section
//! missing from the new file is looked for in `.arbor/config.toml` `[bennu.<section>]` — a
//! **read-only** fallback, on the section rather than the file, so a project half-migrated
//! still finds both. The next save writes the new file, and the fallback stops being consulted
//! for that section. Nothing is deleted from the old file: removing keys from a file this
//! module no longer owns is exactly the hazard the move was made to avoid.

use std::path::PathBuf;

use serde::de::DeserializeOwned;
use serde::Serialize;

/// `<repo>/.arbor/bennu/config.toml` — bennu's own per-repo settings.
pub(crate) fn config_path(root: &str) -> PathBuf {
    PathBuf::from(root).join(".arbor").join("bennu").join("config.toml")
}

/// `<repo>/.arbor/config.toml` — the shared file these sections used to live in. Read only.
fn legacy_path(root: &str) -> PathBuf {
    PathBuf::from(root).join(".arbor").join("config.toml")
}

/// Parse the per-repo file into a dynamic table. A missing or unparseable file yields an empty
/// one: a corrupt sibling section must not strand a preference this module owns, and an editor
/// preference never hard-fails a read.
pub(crate) fn read_table(root: &str) -> toml::value::Table {
    read_table_at(&config_path(root))
}

fn read_table_at(path: &PathBuf) -> toml::value::Table {
    let Ok(text) = std::fs::read_to_string(path) else {
        return toml::value::Table::new();
    };
    text.parse::<toml::Value>().ok().and_then(|v| v.as_table().cloned()).unwrap_or_default()
}

/// Read `[<section>]`, decoded. Absent, or shaped wrongly, yields the default — the same
/// self-healing the whole-file read does, and for the same reason.
///
/// Falls back to the pre-move location (`.arbor/config.toml` `[bennu.<section>]`) when the
/// section is not in bennu's own file yet. See the module doc.
pub(crate) fn load<T: DeserializeOwned + Default>(root: &str, section: &str) -> T {
    if let Some(value) = read_table(root).get(section) {
        return value.clone().try_into().unwrap_or_default();
    }
    match read_table_at(&legacy_path(root)).get("bennu").and_then(|b| b.get(section)) {
        Some(value) => value.clone().try_into().unwrap_or_default(),
        None => T::default(),
    }
}

/// Replace `[<section>]` and write the file back, leaving every other section intact.
///
/// The whole file is still parsed and re-serialised rather than being written from a typed
/// struct: `run`, `debug` and `tomcat` share it, and a save of one must not be a save of the
/// others' last-known state.
pub(crate) fn save<T: Serialize>(root: &str, section: &str, value: &T) -> Result<(), String> {
    let mut table = read_table(root);
    let encoded = toml::Value::try_from(value).map_err(|e| e.to_string())?;
    table.insert(section.to_string(), encoded);

    let path = config_path(root);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let text = toml::to_string_pretty(&table).map_err(|e| e.to_string())?;
    std::fs::write(&path, text).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;

    #[derive(Debug, Default, PartialEq, Serialize, Deserialize)]
    #[serde(default)]
    struct Sample {
        name: String,
        count: i64,
    }

    /// A scratch project root, cleaned on the way in.
    fn scratch(tag: &str) -> String {
        let dir = std::env::temp_dir().join(format!("bennu-repo-config-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir.display().to_string()
    }

    fn write(path: PathBuf, text: &str) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }

    /// Writing one section leaves the file's other sections exactly as they were — three of
    /// bennu's own share this file, and saving a breakpoint must not save the run
    /// configurations' last-known state along with it.
    #[test]
    fn writing_one_section_leaves_its_siblings_alone() {
        let root = scratch("siblings");
        write(config_path(&root), "[run]\nactive_id = \"rc-1\"\n");

        save(&root, "debug", &Sample { name: "bp".into(), count: 3 }).unwrap();

        let table = read_table(&root);
        assert_eq!(
            table.get("run").unwrap().get("active_id").unwrap().as_str(),
            Some("rc-1"),
            "a sibling section must survive"
        );
        assert_eq!(load::<Sample>(&root, "debug"), Sample { name: "bp".into(), count: 3 });
        let _ = std::fs::remove_dir_all(&root);
    }

    /// Sections are top-level here. The `bennu.` prefix only existed to keep out of another
    /// product's way in a file bennu did not own.
    #[test]
    fn a_section_is_written_at_the_top_level() {
        let root = scratch("toplevel");
        save(&root, "run", &Sample { name: "App".into(), count: 1 }).unwrap();
        let text = std::fs::read_to_string(config_path(&root)).unwrap();
        assert!(text.contains("[run]"), "{text}");
        assert!(!text.contains("[bennu"), "{text}");
        let _ = std::fs::remove_dir_all(&root);
    }

    /// A project configured before the move still finds its settings. A run configuration that
    /// silently disappears is worse than any amount of tidiness.
    #[test]
    fn a_section_from_before_the_move_is_still_read() {
        let root = scratch("legacy");
        write(
            PathBuf::from(&root).join(".arbor").join("config.toml"),
            "[corvus]\ndisplay_name = \"keepme\"\n\n[bennu.run]\nname = \"App\"\ncount = 7\n",
        );
        assert_eq!(load::<Sample>(&root, "run"), Sample { name: "App".into(), count: 7 });
        let _ = std::fs::remove_dir_all(&root);
    }

    /// Once the section exists in bennu's own file it is the answer, and the old copy is not
    /// consulted — otherwise an edit would appear to have been ignored.
    #[test]
    fn the_new_file_wins_over_the_old_one() {
        let root = scratch("wins");
        write(
            PathBuf::from(&root).join(".arbor").join("config.toml"),
            "[bennu.run]\nname = \"old\"\n",
        );
        save(&root, "run", &Sample { name: "new".into(), count: 0 }).unwrap();
        assert_eq!(load::<Sample>(&root, "run").name, "new");
        let _ = std::fs::remove_dir_all(&root);
    }

    /// Saving never touches the shared file — removing keys from something this module no
    /// longer owns is the hazard the move was made to avoid.
    #[test]
    fn saving_does_not_rewrite_the_shared_file() {
        let root = scratch("untouched");
        let shared = PathBuf::from(&root).join(".arbor").join("config.toml");
        let before = "[corvus]\ndisplay_name = \"keepme\"\n\n[bennu.run]\nname = \"old\"\n";
        write(shared.clone(), before);

        save(&root, "run", &Sample { name: "new".into(), count: 0 }).unwrap();

        assert_eq!(std::fs::read_to_string(&shared).unwrap(), before);
        let _ = std::fs::remove_dir_all(&root);
    }

    /// A fresh repo has no file at all; a read is the default, not an error.
    #[test]
    fn a_repo_with_no_config_reads_as_the_default() {
        let root = std::env::temp_dir().join("bennu-no-such-repo").display().to_string();
        assert_eq!(load::<Sample>(&root, "debug"), Sample::default());
    }

    /// A section hand-edited into the wrong shape self-heals to the default rather than failing
    /// the read — the same philosophy as the whole-file parse.
    #[test]
    fn a_section_of_the_wrong_shape_degrades_to_the_default() {
        let root = scratch("badshape");
        write(config_path(&root), "debug = \"not a table\"\n");
        assert_eq!(load::<Sample>(&root, "debug"), Sample::default());
        let _ = std::fs::remove_dir_all(&root);
    }
}
