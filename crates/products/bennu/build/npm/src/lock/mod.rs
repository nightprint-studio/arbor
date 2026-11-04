//! Lockfiles — which version of each package an install actually chose, and who requires whom.
//!
//! A `package.json` says what a project *asks for* (`^4.17.0`); the lockfile says what it *got*
//! (`4.17.21`) and, for every package in the graph, which packages it pulled in. That is the whole
//! of a dependency panel's second half — resolved versions, "who requires this", duplicate
//! versions — and it is on disk, so none of it needs the network or a package manager.
//!
//! Four formats, one model ([`Lock`]):
//!
//! | Format | File | Module |
//! |---|---|---|
//! | npm 5+ | `package-lock.json` (v1, v2, v3) | [`npm`] |
//! | pnpm | `pnpm-lock.yaml` (v5–v9) | [`pnpm`] |
//! | Yarn classic | `yarn.lock` (v1) | [`yarn`] |
//! | Yarn Berry | `yarn.lock` (`__metadata`) | [`yarn`] |
//!
//! Pure text-in: each parser takes the file's contents, so all of it is tested without a disk.

pub mod npm;
pub mod pnpm;
pub mod yaml_lite;
pub mod yarn;

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

/// One edge of the graph: a package requiring another.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Edge {
    /// Index into [`Lock::packages`].
    pub to: usize,
    /// Declared under `optionalDependencies` (or a peer marked optional): may legitimately be absent.
    pub optional: bool,
}

/// One package at one version.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Package {
    /// The registry name, scope included (`@babel/core`).
    pub name: String,
    pub version: String,
    /// A workspace member or a `link:` target: part of the project, not something downloaded. Never
    /// descended into — its own dependencies are its own module's report.
    pub local: bool,
    pub deps: Vec<Edge>,
}

/// Which package manager wrote the file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LockFormat {
    Npm,
    Pnpm,
    Yarn,
}

/// The graph a lockfile describes.
#[derive(Debug, Default)]
pub struct Lock {
    pub packages: Vec<Package>,
    interned: HashMap<(String, String), usize>,
    /// Importer directory (`""` for the root, else relative with forward slashes) → each direct
    /// dependency's package. npm and pnpm record this; Yarn does not (see [`Lock::descriptors`]).
    importers: BTreeMap<String, BTreeMap<String, usize>>,
    /// Yarn: `name@range` as a manifest writes it → the package it resolved to.
    descriptors: HashMap<String, usize>,
}

impl Lock {
    /// The package `name@version`, created on first sight.
    pub(crate) fn intern(&mut self, name: &str, version: &str, local: bool) -> usize {
        let key = (name.to_string(), version.to_string());
        if let Some(&i) = self.interned.get(&key) {
            if local {
                self.packages[i].local = true;
            }
            return i;
        }
        let i = self.packages.len();
        self.packages.push(Package { name: key.0.clone(), version: key.1.clone(), local, deps: Vec::new() });
        self.interned.insert(key, i);
        i
    }

    /// `from` requires `to`. A repeated edge is kept once, and stays non-optional if either said so.
    pub(crate) fn add_edge(&mut self, from: usize, to: usize, optional: bool) {
        let deps = &mut self.packages[from].deps;
        match deps.iter_mut().find(|e| e.to == to) {
            Some(e) => e.optional &= optional,
            None => deps.push(Edge { to, optional }),
        }
    }

    pub(crate) fn set_importer_dep(&mut self, dir: &str, name: &str, package: usize) {
        self.importers.entry(dir.to_string()).or_default().insert(name.to_string(), package);
    }

    pub(crate) fn add_descriptor(&mut self, descriptor: String, package: usize) {
        self.descriptors.insert(descriptor, package);
    }

    pub(crate) fn descriptor(&self, descriptor: &str) -> Option<usize> {
        self.descriptors.get(descriptor).copied()
    }

    /// The package the manifest in `dir` got for its direct dependency `name` declared as `range`.
    pub fn direct(&self, dir: &str, name: &str, range: &str) -> Option<&Package> {
        let from_importer = self.importers.get(dir).and_then(|m| m.get(name));
        let found = from_importer.or_else(|| {
            [format!("{name}@{range}"), format!("{name}@npm:{range}")]
                .iter()
                .find_map(|d| self.descriptors.get(d))
        });
        found.map(|&i| &self.packages[i])
    }

    /// Index of the package `direct` would return — for walking on from it.
    pub fn direct_index(&self, dir: &str, name: &str, range: &str) -> Option<usize> {
        self.direct(dir, name, range)
            .and_then(|p| self.interned.get(&(p.name.clone(), p.version.clone())).copied())
    }
}

/// Which lockfile `dir` has, and where. The package manager is the one whose file is there; with
/// several (a migration half done) the order is the one `package_manager_for` uses.
pub fn lockfile_in(dir: &Path) -> Option<(LockFormat, PathBuf)> {
    [
        ("pnpm-lock.yaml", LockFormat::Pnpm),
        ("yarn.lock", LockFormat::Yarn),
        ("package-lock.json", LockFormat::Npm),
        ("npm-shrinkwrap.json", LockFormat::Npm),
    ]
    .into_iter()
    .map(|(file, format)| (format, dir.join(file)))
    .find(|(_, path)| path.is_file())
}

/// Parse `text` as a lockfile of `format`.
pub fn parse(format: LockFormat, text: &str) -> Lock {
    match format {
        LockFormat::Npm => npm::parse(text),
        LockFormat::Pnpm => pnpm::parse(text),
        LockFormat::Yarn => yarn::parse(text),
    }
}

/// Read the lockfile in `dir`, if there is one and it can be read.
pub fn read(dir: &Path) -> Option<Lock> {
    let (format, path) = lockfile_in(dir)?;
    let text = std::fs::read_to_string(path).ok()?;
    Some(parse(format, &text))
}

/// Split `name@version` at the `@` that separates them (the scope's own `@` does not count).
pub(crate) fn split_name_version(spec: &str) -> Option<(&str, &str)> {
    let at = spec.rfind('@').filter(|&i| i > 0)?;
    Some((&spec[..at], &spec[at + 1..]))
}
