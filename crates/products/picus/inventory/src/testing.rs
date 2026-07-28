//! Fixtures shared by the crate's unit tests.
//!
//! A two-branch repository shaped like the ones Picus was built for: an Oracle
//! branch and a PostgreSQL one, each with an initialisation folder and an update
//! folder. Everything the tests assert is about *that* shape, because a rule that
//! only works on a one-branch repository is a rule that does not work.

use arbor_fs::prelude::encoding::EncodingSource;
use picus_parse::prelude::{EngineKind, ParsedFile, SqlParser};
use picus_project::prelude::{Branch, LineEnding, Project, ScriptFile, ScriptFolder};
use picus_types::prelude::FolderRole;

pub(crate) fn parsed(source: &str, engine: EngineKind) -> ParsedFile {
    SqlParser::new().parse(source, engine)
}

pub(crate) fn file(path: &str) -> ScriptFile {
    ScriptFile {
        path: path.to_string(),
        name: path.rsplit('/').next().unwrap_or(path).to_string(),
        size: 0,
        encoding: "windows-1252".to_string(),
        encoding_source: EncodingSource::Inherited,
        eol: LineEnding::Crlf,
        expected_encoding: "windows-1252".to_string(),
    }
}

fn folder(id: &str, label: &str, role: FolderRole, path: &str, files: Vec<ScriptFile>) -> ScriptFolder {
    ScriptFolder { id: id.to_string(), label: label.to_string(), role, path: path.to_string(), files }
}

/// The canonical two-branch repository.
pub(crate) fn project() -> Project {
    Project {
        name: "PROD_CORE".to_string(),
        root: "/repo/prod-core".to_string(),
        branches: vec![
            Branch {
                id: "ora".to_string(),
                label: "ORACLE".to_string(),
                dialect: Some(EngineKind::Oracle),
                path: "ORACLE".to_string(),
                folders: vec![
                    folder(
                        "ora-init",
                        "INIZIALIZZAZIONE",
                        FolderRole::Init,
                        "ORACLE/INIZIALIZZAZIONE",
                        vec![
                            file("ORACLE/INIZIALIZZAZIONE/01_TABELLE.sql"),
                            file("ORACLE/INIZIALIZZAZIONE/02_PARAMETRI.sql"),
                        ],
                    ),
                    folder(
                        "ora-upd",
                        "AGGIORNAMENTO",
                        FolderRole::Update,
                        "ORACLE/AGGIORNAMENTO",
                        vec![file("ORACLE/AGGIORNAMENTO/4_12__4_13.sql")],
                    ),
                ],
            },
            Branch {
                id: "pg".to_string(),
                label: "POSTGRES".to_string(),
                dialect: Some(EngineKind::Postgres),
                path: "POSTGRES".to_string(),
                folders: vec![
                    folder(
                        "pg-init",
                        "INIZIALIZZAZIONE",
                        FolderRole::Init,
                        "POSTGRES/INIZIALIZZAZIONE",
                        vec![
                            file("POSTGRES/INIZIALIZZAZIONE/01_tabelle.sql"),
                            file("POSTGRES/INIZIALIZZAZIONE/02_parametri.sql"),
                        ],
                    ),
                    folder(
                        "pg-upd",
                        "AGGIORNAMENTO",
                        FolderRole::Update,
                        "POSTGRES/AGGIORNAMENTO",
                        vec![file("POSTGRES/AGGIORNAMENTO/4_12__4_13.sql")],
                    ),
                ],
            },
        ],
    }
}
