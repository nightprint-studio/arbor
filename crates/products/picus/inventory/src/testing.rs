//! Fixtures shared by the crate's unit tests.
//!
//! A two-dialect repository shaped like the ones Picus was built for: an Oracle
//! folder and a PostgreSQL one, each with an initialisation folder and an update
//! folder, and the dialect declared once at the top so the folders under it
//! inherit it. Everything the tests assert is about *that* shape, because a rule
//! that only works on a one-dialect repository is a rule that does not work.

use arbor_fs::prelude::encoding::EncodingSource;
use picus_parse::prelude::{DialectScope, EngineKind, ParsedFile, SqlParser};
use picus_project::prelude::{resolve, FolderNode, LineEnding, Project, ScriptFile};
use picus_types::prelude::{FolderEngine, FolderRole};

pub(crate) fn parsed(source: &str, engine: EngineKind) -> ParsedFile {
    SqlParser::new().parse(source, DialectScope::One(engine))
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

fn folder(path: &str, role: FolderRole, files: Vec<ScriptFile>) -> FolderNode {
    let name = path.rsplit('/').next().unwrap_or(path).to_string();
    FolderNode { role: Some(role), files, ..FolderNode::new(path, name) }
}

fn top(path: &str, dialect: EngineKind, children: Vec<FolderNode>) -> FolderNode {
    FolderNode {
        engine: Some(FolderEngine::Supported(dialect)),
        children,
        ..FolderNode::new(path, path)
    }
}

/// The canonical two-dialect repository.
pub(crate) fn project() -> Project {
    let mut project = Project {
        name: "PROD_CORE".to_string(),
        root: "/repo/prod-core".to_string(),
        tree: vec![
            top(
                "ORACLE",
                EngineKind::Oracle,
                vec![
                    folder(
                        "ORACLE/AGGIORNAMENTO",
                        FolderRole::Update,
                        vec![file("ORACLE/AGGIORNAMENTO/4_12__4_13.sql")],
                    ),
                    folder(
                        "ORACLE/INIZIALIZZAZIONE",
                        FolderRole::Init,
                        vec![
                            file("ORACLE/INIZIALIZZAZIONE/01_TABELLE.sql"),
                            file("ORACLE/INIZIALIZZAZIONE/02_PARAMETRI.sql"),
                        ],
                    ),
                ],
            ),
            top(
                "POSTGRES",
                EngineKind::Postgres,
                vec![
                    folder(
                        "POSTGRES/AGGIORNAMENTO",
                        FolderRole::Update,
                        vec![file("POSTGRES/AGGIORNAMENTO/4_12__4_13.sql")],
                    ),
                    folder(
                        "POSTGRES/INIZIALIZZAZIONE",
                        FolderRole::Init,
                        vec![
                            file("POSTGRES/INIZIALIZZAZIONE/01_tabelle.sql"),
                            file("POSTGRES/INIZIALIZZAZIONE/02_parametri.sql"),
                        ],
                    ),
                ],
            ),
        ],
    };
    resolve(&mut project.tree, None, None);
    project
}
