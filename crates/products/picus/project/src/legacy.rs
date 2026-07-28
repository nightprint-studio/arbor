//! Reading a `version = 1` project file.
//!
//! Version 1 described a repository as an array of **branches**, each holding an
//! array of **folders**, and that shape could only ever say two things: the
//! dialect belongs to a top-level folder, the role to a folder directly inside
//! it. A repository whose dialect sits three levels down —
//! `AGGIORNAMENTO/2024/ORA` — had nowhere to say so, which is why the shape
//! changed.
//!
//! Those files are in people's repositories and are committed, so they still
//! load. The fold is mechanical and lossless in the direction that matters:
//!
//! | version 1 | becomes |
//! |---|---|
//! | `[[branch]]` with a `dialect` | a declaration on the branch's path |
//! | `[[branch.folder]]` with a `role` | a declaration on the folder's path |
//! | a folder's `encoding` / `naming` | the same fields, on the same path |
//! | `id`, `label` | dropped — the tree now shows the real directory name |
//!
//! Inheritance then reproduces the old behaviour exactly: everything under a
//! branch takes its dialect, everything under a folder takes its role. What the
//! old shape *could not* express is what the user gains, not what they lose.
//!
//! Nothing here writes: a migrated configuration is only persisted when the user
//! next confirms one, like every other write in this crate.

use picus_types::prelude::{FolderEngine, FolderRole};
use serde::Deserialize;

use crate::config::{FolderDeclaration, ProjectConfig, CURRENT_VERSION};
use crate::naming::NamingScheme;

/// A project file as it is on disk: the current shape, plus whatever version 1
/// wrote alongside it.
///
/// `flatten` rather than a second copy of every field, so adding a setting to
/// [`ProjectConfig`] cannot silently stop it being read from an old file.
#[derive(Debug, Deserialize)]
struct StoredConfig {
    #[serde(flatten)]
    config: ProjectConfig,
    /// Version 1's branches. Empty for anything this build wrote.
    #[serde(default, rename = "branch")]
    branches: Vec<LegacyBranch>,
}

#[derive(Debug, Deserialize)]
struct LegacyBranch {
    path: String,
    /// Version 1 only ever wrote a supported dialect here, but the field takes
    /// the wider type so a hand-edited old file naming SQL Server migrates to the
    /// same thing a new one would mean, rather than failing the parse.
    #[serde(default)]
    dialect: Option<FolderEngine>,
    #[serde(default, rename = "folder")]
    folders: Vec<LegacyFolder>,
}

#[derive(Debug, Deserialize)]
struct LegacyFolder {
    path: String,
    /// Required in practice — version 1 always wrote it — but optional here so a
    /// hand-edited file missing one folder's role still opens.
    #[serde(default)]
    role: Option<FolderRole>,
    #[serde(default)]
    encoding: Option<String>,
    #[serde(default)]
    naming: Option<NamingScheme>,
}

/// Parse a project file of either shape.
pub(crate) fn parse(text: &str) -> Result<ProjectConfig, toml::de::Error> {
    let stored: StoredConfig = toml::from_str(text)?;
    Ok(migrate(stored))
}

fn migrate(stored: StoredConfig) -> ProjectConfig {
    let StoredConfig { mut config, branches } = stored;
    if branches.is_empty() {
        return config;
    }

    for branch in branches {
        declare(&mut config, &branch.path, |d| d.dialect = branch.dialect);
        for folder in branch.folders {
            declare(&mut config, &folder.path, |d| {
                d.role = folder.role;
                d.encoding = folder.encoding;
                d.naming = folder.naming;
            });
        }
    }
    // A file that has been read in the new shape is a file in the new shape: the
    // next save writes declarations, and the version has to agree with that or a
    // later build would migrate something already migrated.
    config.version = CURRENT_VERSION;
    config.tidy();
    config
}

/// Apply a legacy entry to the declaration for its path, without disturbing a
/// declaration the same file already carries in the new shape.
///
/// A half-migrated file — someone added `[[folder]]` by hand next to the
/// branches — is not a case worth failing on, and the newer form is the one they
/// meant.
fn declare(
    config: &mut ProjectConfig,
    path: &str,
    fill: impl FnOnce(&mut FolderDeclaration),
) {
    if config.declaration(path).is_some() {
        return;
    }
    fill(config.declaration_mut(path));
}

#[cfg(test)]
mod tests {
    use super::*;
    use picus_types::prelude::EngineKind;

    fn supported(kind: EngineKind) -> Option<FolderEngine> {
        Some(FolderEngine::Supported(kind))
    }

    /// A version-1 file exactly as Picus used to write one.
    const V1: &str = r#"
version = 1
name = "PROD_CORE"

[encoding]
default = "windows-1252"
eol = "CRLF"

[[branch]]
id = "ora"
label = "ORACLE"
path = "ORACLE"
dialect = "oracle"

[[branch.folder]]
id = "ora-init"
label = "INIZIALIZZAZIONE"
path = "ORACLE/INIZIALIZZAZIONE"
role = "init"

[[branch.folder]]
id = "ora-upd"
label = "AGGIORNAMENTO"
path = "ORACLE/AGGIORNAMENTO"
role = "update"
encoding = "UTF-8"

[[branch]]
id = "common"
label = "COMMON"
path = "COMMON"
"#;

    #[test]
    fn a_version_one_file_becomes_declarations_keyed_by_path() {
        let config = parse(V1).expect("a version 1 file still loads");
        assert_eq!(config.version, CURRENT_VERSION);
        assert_eq!(config.name, "PROD_CORE");

        let paths: Vec<&str> = config.folders.iter().map(|f| f.path.as_str()).collect();
        // `COMMON` declared no dialect and held no folders, so it declared
        // nothing and is not written back as an empty entry.
        assert_eq!(paths, ["ORACLE", "ORACLE/AGGIORNAMENTO", "ORACLE/INIZIALIZZAZIONE"]);

        let oracle = config.declaration("ORACLE").unwrap();
        assert_eq!(oracle.dialect, supported(EngineKind::Oracle));
        assert_eq!(oracle.role, None, "a branch never carried a role");

        let update = config.declaration("ORACLE/AGGIORNAMENTO").unwrap();
        assert_eq!(update.role, Some(FolderRole::Update));
        assert_eq!(update.dialect, None, "the dialect is inherited, not copied down");
        assert_eq!(update.encoding.as_deref(), Some("UTF-8"));
    }

    #[test]
    fn the_old_behaviour_survives_the_fold() {
        // What the two-level shape meant, asserted through the resolver: every
        // folder of a branch is in that branch's dialect, and a folder's role
        // reaches the files under it.
        use crate::tree::FolderNode;

        let config = parse(V1).expect("parses");
        let mut oracle = FolderNode::new("ORACLE", "ORACLE");
        oracle.engine = config.declaration("ORACLE").and_then(|d| d.dialect);
        oracle.children = vec![FolderNode {
            role: config.declaration("ORACLE/AGGIORNAMENTO").and_then(|d| d.role),
            ..FolderNode::new("ORACLE/AGGIORNAMENTO", "AGGIORNAMENTO")
        }];
        let mut tree = vec![oracle];
        crate::resolve::resolve(&mut tree, None, None);

        let update = &tree[0].children[0];
        assert_eq!(update.effective_dialect(), Some(EngineKind::Oracle));
        assert_eq!(update.effective_role, FolderRole::Update);
        // …and the encoding follows the same path lookup it always did.
        assert_eq!(config.encoding_for("ORACLE/AGGIORNAMENTO"), "UTF-8");
        assert_eq!(config.encoding_for("ORACLE/INIZIALIZZAZIONE"), "windows-1252");
    }

    #[test]
    fn a_current_file_is_not_touched_by_the_migration() {
        let text = r#"
            version = 2
            name = "PROD_CORE"
            [[folder]]
            path = "AGGIORNAMENTO"
            role = "update"
        "#;
        let config = parse(text).expect("parses");
        assert_eq!(config.folders.len(), 1);
        assert_eq!(config.declaration("AGGIORNAMENTO").unwrap().role, Some(FolderRole::Update));
    }

    #[test]
    fn a_file_carrying_both_shapes_keeps_the_newer_declaration() {
        // Not a shape Picus writes, but one a hand-edit produces. Losing the line
        // somebody just typed would be the worse of the two answers.
        let text = r#"
            version = 1
            name = "PROD_CORE"

            [[folder]]
            path = "ORACLE"
            dialect = "postgres"

            [[branch]]
            id = "ora"
            label = "ORACLE"
            path = "ORACLE"
            dialect = "oracle"
        "#;
        let config = parse(text).expect("parses");
        assert_eq!(config.declaration("ORACLE").unwrap().dialect, supported(EngineKind::Postgres));
    }

    #[test]
    fn a_branch_with_no_dialect_migrates_to_no_declaration_rather_than_a_guess() {
        let text = r#"
            version = 1
            name = "P"
            [[branch]]
            id = "common"
            label = "COMMON"
            path = "COMMON"
            [[branch.folder]]
            id = "c"
            label = "MISC"
            path = "COMMON/MISC"
            role = "ignored"
        "#;
        let config = parse(text).expect("parses");
        assert!(config.declaration("COMMON").is_none());
        assert_eq!(config.declaration("COMMON/MISC").unwrap().role, Some(FolderRole::Ignored));
    }
}
