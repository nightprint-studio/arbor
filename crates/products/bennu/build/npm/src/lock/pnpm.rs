//! `pnpm-lock.yaml` — lockfile versions 5 through 9.
//!
//! What moved between them is only naming: v9 splits `packages` (what is downloaded) from
//! `snapshots` (the graph, one entry per package *and peer-dependency combination*), where v5/v6
//! keep the edges inside `packages`; v5 writes keys as `/name/1.2.3`, v6 as `/name@1.2.3`, v9 as
//! `name@1.2.3`. Importers are the workspace's projects: `importers:` in a workspace, else the
//! top-level `dependencies:` sections of a single project.
//!
//! A peer-dependency suffix (`1.2.3(react@18.0.0)`, or v5's `1.2.3_react@18.0.0`) makes pnpm install
//! one package several times; here they collapse to one `name@version`, because "which version of
//! `foo`" is the question and "against which React" is not.

use super::yaml_lite::{self, Node};
use super::{split_name_version, Lock};

/// Sections of an importer, and of a package's entry, that name dependencies.
const IMPORTER_SECTIONS: [&str; 3] = ["dependencies", "devDependencies", "optionalDependencies"];
const PACKAGE_SECTIONS: [(&str, bool); 2] = [("dependencies", false), ("optionalDependencies", true)];

pub fn parse(text: &str) -> Lock {
    let doc = yaml_lite::parse(text);
    let mut lock = Lock::default();
    let legacy = is_legacy(&doc);

    // The graph lives in `snapshots` (v9) or in `packages` (older).
    let graph = doc.get("snapshots").or_else(|| doc.get("packages"));
    if let Some(graph) = graph {
        for (key, entry) in &graph.children {
            let Some((name, version)) = split_key(key, legacy) else { continue };
            let from = lock.intern(&name, &version, false);
            for (section, optional) in PACKAGE_SECTIONS {
                for (dep, value) in entry.get(section).map(|n| n.children.as_slice()).unwrap_or_default() {
                    if let Some((n, v, local)) = resolve_spec(dep, &value.value) {
                        let to = lock.intern(&n, &v, local);
                        lock.add_edge(from, to, optional);
                    }
                }
            }
        }
    }

    match doc.get("importers") {
        Some(importers) => {
            for (dir, importer) in &importers.children {
                read_importer(&mut lock, &importer_dir(dir), importer);
            }
        }
        None => read_importer(&mut lock, "", &doc),
    }
    lock
}

fn read_importer(lock: &mut Lock, dir: &str, importer: &Node) {
    for section in IMPORTER_SECTIONS {
        for (dep, value) in importer.get(section).map(|n| n.children.as_slice()).unwrap_or_default() {
            // v6+: `{ specifier, version }`. v5 and older: the version itself.
            let spec = if value.children.is_empty() { value.value.as_str() } else { value.scalar("version") };
            if let Some((n, v, local)) = resolve_spec(dep, spec) {
                let to = lock.intern(&n, &v, local);
                lock.set_importer_dep(dir, dep, to);
            }
        }
    }
}

/// `.` is the root; everything else is already a relative path.
fn importer_dir(key: &str) -> String {
    let key = key.trim_start_matches("./").trim_end_matches('/');
    if key == "." { String::new() } else { key.to_string() }
}

/// The package a `snapshots`/`packages` key names, without its peer suffix. `legacy` is lockfile
/// v5 and older, whose keys are `/name/1.2.3` and not `name@1.2.3` — told apart by the version
/// number and not by the shape of the key, because a v5 peer suffix (`_react@18.0.0`) contains an `@`.
fn split_key(key: &str, legacy: bool) -> Option<(String, String)> {
    let key = key.trim_start_matches('/');
    if legacy {
        let slash = key.rfind('/')?;
        return Some((key[..slash].to_string(), strip_underscore_peers(&key[slash + 1..]).to_string()));
    }
    let core = key.split('(').next().unwrap_or(key);
    let (name, version) = split_name_version(core)?;
    Some((name.to_string(), version.to_string()))
}

/// Lockfile v5 and older.
fn is_legacy(doc: &Node) -> bool {
    doc.scalar("lockfileVersion").trim().parse::<f32>().is_ok_and(|v| v < 6.0)
}

/// v5 appends peers as `_react@18.0.0`; a version never contains `_`.
fn strip_underscore_peers(version: &str) -> &str {
    version.split('_').next().unwrap_or(version)
}

/// What a dependency value resolves to: `(name, version, is_local)`.
///
/// `1.2.3(peer@1)` → the package itself; `other@1.0.0` / `/other@1.0.0` → an alias, which is the
/// real package under another name; `link:../x` → a workspace sibling (local, versionless).
fn resolve_spec(name: &str, value: &str) -> Option<(String, String, bool)> {
    let value = value.trim();
    if value.is_empty() {
        return None;
    }
    if let Some(path) = value.strip_prefix("link:") {
        return Some((name.to_string(), format!("link:{path}"), true));
    }
    let core = value.trim_start_matches('/').split('(').next().unwrap_or(value);
    if let Some((real, version)) = split_name_version(core) {
        return Some((real.to_string(), strip_underscore_peers(version).to_string(), false));
    }
    Some((name.to_string(), strip_underscore_peers(core).to_string(), false))
}

#[cfg(test)]
mod tests {
    use super::*;

    const V9: &str = "\
lockfileVersion: '9.0'

importers:

  .:
    dependencies:
      react:
        specifier: ^18.0.0
        version: 18.2.0
    devDependencies:
      '@types/react':
        specifier: ^18.0.0
        version: 18.2.5(csstype@3.1.2)

  packages/ui:
    dependencies:
      core:
        specifier: workspace:*
        version: link:../core

packages:

  react@18.2.0:
    resolution: {integrity: sha512-x}

snapshots:

  react@18.2.0:
    dependencies:
      loose-envify: 1.4.0

  loose-envify@1.4.0: {}

  '@types/react@18.2.5(csstype@3.1.2)':
    dependencies:
      csstype: 3.1.2
    optionalDependencies:
      fsevents: 2.3.3
";

    #[test]
    fn v9_importers_resolve_to_the_version_pnpm_chose() {
        let lock = parse(V9);
        assert_eq!(lock.direct("", "react", "^18.0.0").map(|p| p.version.as_str()), Some("18.2.0"));
        // The peer suffix is not part of the version.
        assert_eq!(lock.direct("", "@types/react", "^18.0.0").map(|p| p.version.as_str()), Some("18.2.5"));
    }

    #[test]
    fn workspace_links_are_local_and_per_importer() {
        let lock = parse(V9);
        let core = lock.direct("packages/ui", "core", "workspace:*").expect("link");
        assert!(core.local);
        assert!(lock.direct("", "core", "workspace:*").is_none(), "other importers do not see it");
    }

    #[test]
    fn v9_edges_come_from_snapshots() {
        let lock = parse(V9);
        let react = lock.direct_index("", "react", "").unwrap();
        let dep = &lock.packages[lock.packages[react].deps[0].to];
        assert_eq!((dep.name.as_str(), dep.version.as_str()), ("loose-envify", "1.4.0"));

        let types = lock.direct_index("", "@types/react", "").unwrap();
        let edges = &lock.packages[types].deps;
        assert_eq!(edges.len(), 2);
        assert!(!edges[0].optional && edges[1].optional, "fsevents is optional");
    }

    #[test]
    fn older_layouts_are_read_too() {
        let v6 = "\
lockfileVersion: '6.0'

dependencies:
  lodash:
    specifier: ^4.17.0
    version: 4.17.21

packages:

  /lodash@4.17.21:
    resolution: {integrity: sha512-x}
    dev: false

  /@babel/core@7.1.0(@babel/helper@1.0.0):
    dependencies:
      lodash: 4.17.21
";
        let lock = parse(v6);
        assert_eq!(lock.direct("", "lodash", "^4.17.0").map(|p| p.version.as_str()), Some("4.17.21"));
        assert!(lock.packages.iter().any(|p| p.name == "@babel/core" && p.version == "7.1.0"));

        let v5 = "\
lockfileVersion: 5.4

specifiers:
  left-pad: ^1.0.0

dependencies:
  left-pad: 1.3.0

packages:

  /left-pad/1.3.0:
    resolution: {integrity: sha512-x}
    dev: false

  /@scope/pkg/2.0.0_react@18.0.0:
    dependencies:
      left-pad: 1.3.0
";
        let lock = parse(v5);
        assert_eq!(lock.direct("", "left-pad", "^1.0.0").map(|p| p.version.as_str()), Some("1.3.0"));
        assert!(lock.packages.iter().any(|p| p.name == "@scope/pkg" && p.version == "2.0.0"));
    }

    #[test]
    fn an_alias_resolves_to_the_real_package() {
        assert_eq!(resolve_spec("str-width", "string-width@4.2.3"), Some(("string-width".into(), "4.2.3".into(), false)));
        assert_eq!(resolve_spec("x", ""), None);
    }
}
