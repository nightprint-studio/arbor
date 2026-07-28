//! A repository fixture, so the tests can be about rules instead of scaffolding.
//!
//! Files are declared by path and the layout does the rest: the tree is the real
//! one, `ORACLE/…` declares Oracle at the top and `POSTGRES/…` PostgreSQL, the
//! folder name gives the role, and `COMMON/…` is the folder nobody could
//! identify — which is a case the cross-dialect rules have to stay quiet about,
//! so it has to be easy to build.
//!
//! Nothing here hand-writes what a folder resolves to: the declarations go on the
//! nodes and `picus_project::resolve` works out the rest, exactly as discovery
//! does. A fixture that resolved its own tree would be able to describe a
//! repository the product cannot produce.

use std::collections::BTreeMap;

use arbor_fs::prelude::encoding::EncodingSource;
use picus_inventory::prelude::{Inventory, ParsedProject, ParsedScript};
use picus_parse::prelude::{DialectScope, EngineKind, ParsedFile, SqlParser};
use picus_project::prelude::{
    resolve, EncodingSettings, FolderDeclaration, FolderNode, GenerationSettings, LineEnding,
    NamingScheme, Project, ProjectConfig, ScriptFile, VersionTableSettings, CURRENT_VERSION,
};
use picus_types::prelude::{FolderEngine, FolderRole};

use crate::report::{analyze, Report};

/// What a folder's own name declares about its dialect — at **any** depth, which
/// is what lets a test write either `ORACLE/AGGIORNAMENTO/…` or the shape real
/// repositories have, `AGGIORNAMENTO/2024/ORA/…`.
fn engine_of(name: &str) -> Option<FolderEngine> {
    match name {
        "ORACLE" | "ORA" => Some(FolderEngine::Supported(EngineKind::Oracle)),
        "POSTGRES" | "POS" => Some(FolderEngine::Supported(EngineKind::Postgres)),
        // Never inferred in the product — declared. The fixture spells the
        // declaration as a folder name so a test can say "portable" in a path.
        "COMUNE" | "GENERIC" => Some(FolderEngine::Generic),
        _ => None,
    }
}

/// The engine a file is parsed as: its nearest folder that declares one, exactly
/// as `picus-be` resolves it.
fn engine_for(path: &str) -> DialectScope {
    let mut folder = folder_path(path);
    loop {
        if let Some(engine) = engine_of(picus_project::prelude::last_segment(&folder)) {
            return engine.scope().expect("the fixture declares no unsupported engines");
        }
        if folder.is_empty() {
            // The grammar is one permissive superset of both dialects, so the
            // fallback changes nothing but which constructs count as foreign —
            // and `DIA001` refuses to report those without a dialect anyway.
            return DialectScope::One(EngineKind::Oracle);
        }
        folder = picus_project::prelude::parent_of(&folder).to_string();
    }
}

fn role_of(folder: &str) -> Option<FolderRole> {
    match folder {
        "INIZIALIZZAZIONE" => Some(FolderRole::Init),
        "AGGIORNAMENTO" => Some(FolderRole::Update),
        "PROCEDURE" => Some(FolderRole::Routines),
        "DATI" => Some(FolderRole::Data),
        _ => None,
    }
}

fn folder_path(path: &str) -> String {
    path.rsplit_once('/').map(|(dir, _)| dir.to_string()).unwrap_or_default()
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
        let mut parses = Vec::new();
        // Path → files, so the tree can be assembled once from the paths alone.
        let mut by_folder: BTreeMap<String, Vec<ScriptFile>> = BTreeMap::new();

        for (path, source) in files {
            let engine = engine_for(path);
            parses.push((path.to_string(), source.to_string(), parser.parse(source, engine)));
            by_folder.entry(folder_path(path)).or_default().push(script_file(path, source));
        }

        let mut project = Project {
            name: "PROD_CORE".to_string(),
            root: "/repo/prod-core".to_string(),
            tree: tree(&by_folder),
        };
        resolve(&mut project.tree, None, None);

        let config = config_for(&project);
        Fixture { project, config, parses }
    }

    /// Pin one file's detected and expected encodings — the two inputs the
    /// encoding rules read.
    pub fn encoded(mut self, path: &str, detected: &str, expected: &str) -> Fixture {
        for file in files_mut(&mut self.project) {
            if file.path == path {
                file.encoding = detected.to_string();
                file.expected_encoding = expected.to_string();
            }
        }
        self
    }

    pub fn encoding_source(mut self, path: &str, source: EncodingSource) -> Fixture {
        for file in files_mut(&mut self.project) {
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

/// Build the real hierarchy from the folder paths, declaring what each level's
/// name says about it.
fn tree(by_folder: &BTreeMap<String, Vec<ScriptFile>>) -> Vec<FolderNode> {
    let mut paths: Vec<String> = Vec::new();
    for folder in by_folder.keys() {
        let mut current = folder.as_str();
        while !current.is_empty() {
            if !paths.iter().any(|p| p == current) {
                paths.push(current.to_string());
            }
            current = picus_project::prelude::parent_of(current);
        }
    }
    paths.sort();
    nodes_under("", &paths, by_folder)
}

fn nodes_under(
    parent: &str,
    paths: &[String],
    by_folder: &BTreeMap<String, Vec<ScriptFile>>,
) -> Vec<FolderNode> {
    paths
        .iter()
        .filter(|path| picus_project::prelude::parent_of(path) == parent)
        .map(|path| {
            let name = picus_project::prelude::last_segment(path);
            FolderNode {
                // Each level declares what its own name says, and nothing else —
                // the dialect and the role are independent and may sit at
                // opposite ends of the tree, which is the point.
                engine: engine_of(name),
                role: role_of(name),
                files: by_folder.get(path).cloned().unwrap_or_default(),
                children: nodes_under(path, paths, by_folder),
                ..FolderNode::new(path.clone(), name)
            }
        })
        .collect()
}

fn files_mut(project: &mut Project) -> impl Iterator<Item = &mut ScriptFile> {
    fn walk<'a>(nodes: &'a mut [FolderNode], out: &mut Vec<&'a mut ScriptFile>) {
        for node in nodes {
            out.extend(node.files.iter_mut());
            walk(&mut node.children, out);
        }
    }
    let mut out = Vec::new();
    walk(&mut project.tree, &mut out);
    out.into_iter()
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

/// The project file the tree implies: one declaration per folder that declares
/// something, exactly as discovery would propose.
fn config_for(project: &Project) -> ProjectConfig {
    ProjectConfig {
        version: CURRENT_VERSION,
        name: "PROD_CORE".to_string(),
        encoding: EncodingSettings::default(),
        version_table: VersionTableSettings::default(),
        generation: GenerationSettings::default(),
        naming: NamingScheme::default(),
        folders: project
            .walk()
            .filter(|node| node.engine.is_some() || node.role.is_some())
            .map(|node| FolderDeclaration {
                path: node.path.clone(),
                dialect: node.engine,
                role: node.role,
                ..FolderDeclaration::default()
            })
            .collect(),
        aliases: Vec::new(),
    }
}
