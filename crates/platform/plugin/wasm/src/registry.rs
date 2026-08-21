//! Which package implements which interface.
//!
//! Built once from the manifests the host discovered, and answered from on every call that
//! needs a guest. The whole of what it decides:
//!
//! * **which module to instantiate** for a given `interface@version/id`;
//! * **which ids exist** for an interface, so a format picker can list them;
//! * **what is broken**, loudly — a module that is not on disk, two packages claiming the
//!   same id, a package built for a target this host does not run.
//!
//! ## Why conflicts are reported rather than resolved
//!
//! Two packages can both declare `studio-format@1/json`. There is no defensible rule for
//! picking one: install order is an accident, alphabetical is arbitrary, and "the newest
//! version" compares two things that are not versions of each other. So neither is
//! registered, both are named, and the user is told — which is the only outcome that does not
//! silently give somebody a different JSON backend than they think they have.
//!
//! The exception is a **disabled** package: it does not compete, because it is not going to
//! run. That is what lets a user resolve a conflict by switching one off.

use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;

use arbor_plugin_types::prelude::{Manifest, Provides, WasmTarget};

/// What identifies one implementation.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ExtensionKey {
    pub interface: String,
    pub version:   u32,
    pub id:        String,
}

impl std::fmt::Display for ExtensionKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}@{}/{}", self.interface, self.version, self.id)
    }
}

/// One registered implementation, resolved to something instantiable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtensionEntry {
    pub key:    ExtensionKey,
    /// The package that provides it — also the identity its capabilities are scoped to.
    pub plugin: String,
    /// Absolute path of the module on disk.
    pub module: PathBuf,
    pub target: WasmTarget,
}

/// Something wrong with a declaration, kept rather than dropped.
///
/// A silently missing extension is the worst outcome available here: a file type simply does
/// not open and nothing says why. Every one of these ends up in the Plugin Logs and, for a
/// conflict, in the Plugin Manager.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IndexProblem {
    /// The manifest names a module that is not in the package directory.
    MissingModule { plugin: String, key: ExtensionKey, module: PathBuf },
    /// Two enabled packages claim the same key. Neither is registered.
    Conflict { key: ExtensionKey, plugins: Vec<String> },
    /// The package declares a wasm target this host cannot instantiate.
    UnsupportedTarget { plugin: String, key: ExtensionKey, target: WasmTarget },
}

impl std::fmt::Display for IndexProblem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            IndexProblem::MissingModule { plugin, key, module } => write!(
                f,
                "'{plugin}' declares {key} but its module is not there: {}. \
                 A module ships as a release asset — a source install of a package that \
                 provides something is incomplete by construction.",
                module.display()
            ),
            IndexProblem::Conflict { key, plugins } => write!(
                f,
                "{key} is claimed by {plugins:?}. None of them is registered: there is no \
                 defensible way to pick one. Disable all but one."
            ),
            IndexProblem::UnsupportedTarget { plugin, key, target } => write!(
                f,
                "'{plugin}' built {key} for {target:?}, which this host does not run."
            ),
        }
    }
}

/// The resolved set of extensions.
#[derive(Debug, Clone, Default)]
pub struct ExtensionIndex {
    entries:  BTreeMap<ExtensionKey, ExtensionEntry>,
    problems: Vec<IndexProblem>,
}

/// The targets this host can instantiate.
///
/// One today. Listed as a set rather than compared to a constant so that adding a second is
/// a data change, and so the error above can say what was expected.
fn is_supported(target: WasmTarget) -> bool {
    matches!(target, WasmTarget::Wasm32Wasip2)
}

impl ExtensionIndex {
    /// Build the index from every discovered manifest.
    ///
    /// `enabled` is the host's enable ledger. A package absent from it counts as enabled —
    /// the ledger only records deliberate choices, and a package nobody has switched off is
    /// on.
    pub fn build(manifests: &[Manifest], enabled: &HashMap<String, bool>) -> Self {
        // Group by key first, so a conflict is visible before anything is registered.
        let mut claims: BTreeMap<ExtensionKey, Vec<(&Manifest, &Provides)>> = BTreeMap::new();
        for m in manifests {
            if !enabled.get(&m.name).copied().unwrap_or(true) {
                continue;
            }
            for p in &m.provides {
                let key = ExtensionKey {
                    interface: p.interface.clone(),
                    version:   p.version,
                    id:        p.id.clone(),
                };
                claims.entry(key).or_default().push((m, p));
            }
        }

        let mut entries = BTreeMap::new();
        let mut problems = Vec::new();

        for (key, claimants) in claims {
            if claimants.len() > 1 {
                problems.push(IndexProblem::Conflict {
                    key: key.clone(),
                    plugins: claimants.iter().map(|(m, _)| m.name.clone()).collect(),
                });
                continue;
            }
            let (m, p) = claimants[0];
            if !is_supported(m.wasm.target) {
                problems.push(IndexProblem::UnsupportedTarget {
                    plugin: m.name.clone(),
                    key:    key.clone(),
                    target: m.wasm.target,
                });
                continue;
            }
            let module = m.dir.join(&p.module);
            if !module.is_file() {
                problems.push(IndexProblem::MissingModule {
                    plugin: m.name.clone(),
                    key:    key.clone(),
                    module,
                });
                continue;
            }
            entries.insert(
                key.clone(),
                ExtensionEntry { key, plugin: m.name.clone(), module, target: m.wasm.target },
            );
        }

        Self { entries, problems }
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn problems(&self) -> &[IndexProblem] {
        &self.problems
    }

    /// The implementation of one exact key.
    pub fn resolve(&self, interface: &str, version: u32, id: &str) -> Option<&ExtensionEntry> {
        self.entries.get(&ExtensionKey {
            interface: interface.to_string(),
            version,
            id: id.to_string(),
        })
    }

    pub fn all(&self) -> impl Iterator<Item = &ExtensionEntry> {
        self.entries.values()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use arbor_plugin_types::prelude::{Permissions, WasmSection};

    /// A manifest with a real module on disk, so `build` can find it.
    fn pkg(dir: &std::path::Path, name: &str, provides: &[(&str, u32, &str, &str)]) -> Manifest {
        let pdir = dir.join(name);
        std::fs::create_dir_all(&pdir).unwrap();
        let mut m = Manifest {
            name: name.to_string(),
            version: "1.0.0".into(),
            description: String::new(),
            author: String::new(),
            license: None,
            repository: None,
            homepage: None,
            keywords: vec![],
            category: None,
            icon: None,
            min_arbor_version: None,
            arbor_api: 1,
            os: vec![],
            targets: vec![],
            lua: None,
            doc_file: None,
            experimental: false,
            permissions: Permissions::default(),
            sandbox: Default::default(),
            hooks: Default::default(),
            scheduler: Default::default(),
            dependencies: vec![],
            provides: vec![],
            wasm: WasmSection::default(),
            credentials: vec![],
            dir: pdir.clone(),
        };
        for (iface, ver, id, module) in provides {
            std::fs::write(pdir.join(module), b"\0asm").unwrap();
            m.provides.push(Provides {
                interface: iface.to_string(),
                version:   *ver,
                id:        id.to_string(),
                module:    module.to_string(),
            });
        }
        m
    }

    fn scratch(tag: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("arbor-wasmidx-{}-{tag}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn a_declared_module_that_exists_is_registered() {
        let d = scratch("ok");
        let m = pkg(&d, "studio-json", &[("studio-format", 1, "json", "studio_json.wasm")]);
        let idx = ExtensionIndex::build(&[m], &HashMap::new());
        assert!(idx.problems().is_empty(), "{:?}", idx.problems());
        let e = idx.resolve("studio-format", 1, "json").expect("not resolved");
        assert_eq!(e.plugin, "studio-json");
        assert!(e.module.ends_with("studio_json.wasm"));
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn a_missing_module_is_a_named_problem_not_a_silent_absence() {
        // The failure this catches: a file type simply does not open, and nothing says why.
        let d = scratch("missing");
        let mut m = pkg(&d, "studio-ini", &[]);
        m.provides.push(Provides {
            interface: "studio-format".into(), version: 1,
            id: "ini".into(), module: "studio_ini.wasm".into(),
        });
        let idx = ExtensionIndex::build(&[m], &HashMap::new());
        assert!(idx.resolve("studio-format", 1, "ini").is_none());
        assert_eq!(idx.problems().len(), 1);
        assert!(matches!(idx.problems()[0], IndexProblem::MissingModule { .. }));
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn two_packages_claiming_one_id_register_neither() {
        // There is no defensible tie-break, so the honest outcome is that the user is told.
        let d = scratch("conflict");
        let a = pkg(&d, "studio-json", &[("studio-format", 1, "json", "a.wasm")]);
        let b = pkg(&d, "faster-json", &[("studio-format", 1, "json", "b.wasm")]);
        let idx = ExtensionIndex::build(&[a, b], &HashMap::new());
        assert!(idx.resolve("studio-format", 1, "json").is_none(), "neither wins");
        match &idx.problems()[0] {
            IndexProblem::Conflict { plugins, .. } => {
                assert_eq!(plugins.len(), 2);
            }
            other => panic!("expected a conflict, got {other:?}"),
        }
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn disabling_one_of_them_resolves_the_conflict() {
        // This is the escape hatch the conflict message points at, so it has to work.
        let d = scratch("resolved");
        let a = pkg(&d, "studio-json", &[("studio-format", 1, "json", "a.wasm")]);
        let b = pkg(&d, "faster-json", &[("studio-format", 1, "json", "b.wasm")]);
        let mut enabled = HashMap::new();
        enabled.insert("faster-json".to_string(), false);
        let idx = ExtensionIndex::build(&[a, b], &enabled);
        assert!(idx.problems().is_empty(), "{:?}", idx.problems());
        assert_eq!(idx.resolve("studio-format", 1, "json").unwrap().plugin, "studio-json");
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn a_package_nobody_switched_off_counts_as_on() {
        // The ledger records deliberate choices only; absence is not "off".
        let d = scratch("default-on");
        let m = pkg(&d, "studio-json", &[("studio-format", 1, "json", "a.wasm")]);
        let idx = ExtensionIndex::build(&[m], &HashMap::new());
        assert_eq!(idx.len(), 1);
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn versions_of_an_interface_do_not_collide() {
        // `studio-format@1/json` and `@2/json` are different contracts, not a conflict.
        let d = scratch("versions");
        let a = pkg(&d, "old", &[("studio-format", 1, "json", "a.wasm")]);
        let b = pkg(&d, "new", &[("studio-format", 2, "json", "b.wasm")]);
        let idx = ExtensionIndex::build(&[a, b], &HashMap::new());
        assert!(idx.problems().is_empty(), "{:?}", idx.problems());
        assert_eq!(idx.len(), 2);
        assert!(idx.resolve("studio-format", 1, "json").is_some());
        assert!(idx.resolve("studio-format", 2, "json").is_some());
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn a_package_can_provide_several_things() {
        let d = scratch("several");
        let m = pkg(&d, "cloud", &[
            ("cloud-provider", 1, "gcs", "gcs.wasm"),
            ("cloud-provider", 1, "s3",  "s3.wasm"),
        ]);
        let idx = ExtensionIndex::build(&[m], &HashMap::new());
        assert_eq!(idx.len(), 2);
        assert!(idx.resolve("cloud-provider", 1, "gcs").is_some());
        assert!(idx.resolve("cloud-provider", 1, "s3").is_some());
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn an_unsupported_target_is_refused_rather_than_attempted() {
        let d = scratch("target");
        let mut m = pkg(&d, "old-abi", &[("studio-format", 1, "json", "a.wasm")]);
        m.wasm.target = WasmTarget::Wasm32UnknownUnknown;
        let idx = ExtensionIndex::build(&[m], &HashMap::new());
        assert!(idx.resolve("studio-format", 1, "json").is_none());
        assert!(matches!(idx.problems()[0], IndexProblem::UnsupportedTarget { .. }));
        std::fs::remove_dir_all(&d).ok();
    }
}
