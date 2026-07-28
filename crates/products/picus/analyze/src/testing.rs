//! A repository fixture, so the tests can be about rules instead of scaffolding.
//!
//! Files are declared by path and the layout does the rest: `ORACLE/…` is the
//! Oracle branch, `POSTGRES/…` the PostgreSQL one, the folder name gives the
//! role, and `COMMON/…` is the branch nobody could identify — which is a case the
//! cross-branch rules have to stay quiet about, so it has to be easy to build.

use arbor_fs::prelude::encoding::EncodingSource;
use picus_inventory::prelude::{Inventory, ParsedProject, ParsedScript};
use picus_parse::prelude::{EngineKind, ParsedFile, SqlParser};
use picus_project::prelude::{
    Branch, BranchConfig, EncodingSettings, FolderConfig, GenerationSettings, LineEnding,
    NamingScheme, Project, ProjectConfig, ScriptFile, ScriptFolder, VersionTableSettings,
    CURRENT_VERSION,
};
use picus_types::prelude::FolderRole;

use crate::report::{analyze, Report};

/// `(branch id, branch label, dialect)` for a top-level folder.
fn branch_of(path: &str) -> (&'static str, &'static str, Option<EngineKind>) {
    match path.split('/').next().unwrap_or("") {
        "ORACLE" => ("ora", "ORACLE", Some(EngineKind::Oracle)),
        "POSTGRES" => ("pg", "POSTGRES", Some(EngineKind::Postgres)),
        _ => ("common", "COMMON", None),
    }
}

fn role_of(folder: &str) -> FolderRole {
    match folder {
        "INIZIALIZZAZIONE" => FolderRole::Init,
        "AGGIORNAMENTO" => FolderRole::Update,
        "PROCEDURE" => FolderRole::Routines,
        "DATI" => FolderRole::Data,
        _ => FolderRole::Ignored,
    }
}

fn folder_path(path: &str) -> String {
    path.rsplit_once('/').map(|(dir, _)| dir.to_string()).unwrap_or_default()
}

fn folder_id(path: &str) -> String {
    folder_path(path).to_lowercase().replace('/', "-")
}

pub(crate) struct Fixture {
    pub project: Project,
    pub config: ProjectConfig,
    /// `(path, source, parse)` — owned here so the borrowed `ParsedProject` the
    /// rules read has something to point at.
    parses: Vec<(String, String, ParsedFile)>,
}

impl std::fmt::Debug for Fixture {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Fixture").field("files", &self.parses.len()).finish()
    }
}

impl Fixture {
    pub fn build(files: &[(&str, &str)]) -> Fixture {
        let mut parser = SqlParser::new();
        let mut branches: Vec<Branch> = Vec::new();
        let mut parses = Vec::new();

        for (path, source) in files {
            let (branch_id, branch_label, dialect) = branch_of(path);
            let engine = dialect.unwrap_or(EngineKind::Oracle);
            parses.push((path.to_string(), source.to_string(), parser.parse(source, engine)));

            let branch_index = match branches.iter().position(|b| b.id == branch_id) {
                Some(i) => i,
                None => {
                    branches.push(Branch {
                        id: branch_id.to_string(),
                        label: branch_label.to_string(),
                        dialect,
                        path: branch_label.to_string(),
                        folders: Vec::new(),
                    });
                    branches.len() - 1
                }
            };
            let folders = &mut branches[branch_index].folders;
            let id = folder_id(path);
            let folder_index = match folders.iter().position(|f| f.id == id) {
                Some(i) => i,
                None => {
                    let dir = folder_path(path);
                    folders.push(ScriptFolder {
                        id: id.clone(),
                        label: dir.rsplit('/').next().unwrap_or(&dir).to_string(),
                        role: role_of(dir.rsplit('/').next().unwrap_or(&dir)),
                        path: dir,
                        files: Vec::new(),
                    });
                    folders.len() - 1
                }
            };
            folders[folder_index].files.push(script_file(path, source));
        }

        let config = config_for(&branches);
        Fixture {
            project: Project {
                name: "PROD_CORE".to_string(),
                root: "/repo/prod-core".to_string(),
                branches,
            },
            config,
            parses,
        }
    }

    /// Pin one file's detected and expected encodings — the two inputs the
    /// encoding rules read.
    pub fn encoded(mut self, path: &str, detected: &str, expected: &str) -> Fixture {
        for file in self.project.branches.iter_mut().flat_map(|b| {
            b.folders.iter_mut().flat_map(|f| f.files.iter_mut())
        }) {
            if file.path == path {
                file.encoding = detected.to_string();
                file.expected_encoding = expected.to_string();
            }
        }
        self
    }

    pub fn encoding_source(mut self, path: &str, source: EncodingSource) -> Fixture {
        for file in self.project.branches.iter_mut().flat_map(|b| {
            b.folders.iter_mut().flat_map(|f| f.files.iter_mut())
        }) {
            if file.path == path {
                file.encoding_source = source;
            }
        }
        self
    }

    pub fn configured(mut self, edit: impl FnOnce(&mut ProjectConfig)) -> Fixture {
        edit(&mut self.config);
        self
    }

    fn scripts(&self) -> Vec<ParsedScript<'_>> {
        self.parses
            .iter()
            .map(|(path, source, parsed)| ParsedScript {
                path: path.as_str(),
                source: source.as_str(),
                parsed,
            })
            .collect()
    }

    pub fn report(&self) -> Report {
        let joined = ParsedProject::new(&self.project, self.scripts());
        let inventory = Inventory::build(&joined);
        analyze(&joined, &self.config, &inventory)
    }
}

fn script_file(path: &str, source: &str) -> ScriptFile {
    ScriptFile {
        path: path.to_string(),
        name: path.rsplit('/').next().unwrap_or(path).to_string(),
        size: source.len() as u64,
        encoding: "windows-1252".to_string(),
        encoding_source: EncodingSource::Inherited,
        eol: LineEnding::Crlf,
        expected_encoding: "windows-1252".to_string(),
    }
}

fn config_for(branches: &[Branch]) -> ProjectConfig {
    ProjectConfig {
        version: CURRENT_VERSION,
        name: "PROD_CORE".to_string(),
        encoding: EncodingSettings::default(),
        version_table: VersionTableSettings::default(),
        generation: GenerationSettings::default(),
        naming: NamingScheme::default(),
        branches: branches
            .iter()
            .map(|branch| BranchConfig {
                id: branch.id.clone(),
                label: branch.label.clone(),
                path: branch.path.clone(),
                dialect: branch.dialect,
                folders: branch
                    .folders
                    .iter()
                    .map(|folder| FolderConfig {
                        id: folder.id.clone(),
                        label: folder.label.clone(),
                        path: folder.path.clone(),
                        role: folder.role,
                        encoding: None,
                        naming: None,
                    })
                    .collect(),
            })
            .collect(),
    }
}
