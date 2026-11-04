//! `package-lock.json` — lockfile versions 1, 2 and 3.
//!
//! v2 and v3 describe the install as a flat `packages` map keyed by the path of each package under
//! `node_modules` (`node_modules/a/node_modules/b` is `b` nested inside `a`); v2 also carries the
//! legacy nested `dependencies` tree for npm 6, which is ignored when `packages` is there. v1 has
//! only the nested tree. Both resolve a requirement the way Node does: look in the requirer's own
//! `node_modules`, then walk up.

use std::collections::{BTreeMap, HashMap};

use serde_json::{Map, Value};

use super::Lock;

/// Dependency sections of a `packages` entry, and whether each makes the edge optional.
const SECTIONS: [(&str, bool); 3] = [("dependencies", false), ("optionalDependencies", true), ("peerDependencies", true)];
/// Sections only a project (the root or a workspace member) has.
const IMPORTER_SECTIONS: [&str; 4] = ["dependencies", "devDependencies", "optionalDependencies", "peerDependencies"];

pub fn parse(text: &str) -> Lock {
    let Ok(Value::Object(doc)) = serde_json::from_str::<Value>(text) else { return Lock::default() };
    match doc.get("packages") {
        Some(Value::Object(packages)) => from_packages(packages),
        _ => from_nested(&doc),
    }
}

// ── v2 / v3 ──────────────────────────────────────────────────────────────────

/// The name of the package at `key`: whatever follows the last `node_modules/`.
fn name_at(key: &str) -> Option<&str> {
    key.rfind("node_modules/").map(|i| &key[i + "node_modules/".len()..])
}

/// Where the requirer at `holder` finds `name`: its own `node_modules`, then each ancestor's.
fn resolve_path(packages: &Map<String, Value>, holder: &str, name: &str) -> Option<String> {
    let mut base = holder.to_string();
    loop {
        let candidate = if base.is_empty() { format!("node_modules/{name}") } else { format!("{base}/node_modules/{name}") };
        if packages.contains_key(&candidate) {
            return Some(candidate);
        }
        if base.is_empty() {
            return None;
        }
        base = match base.rfind("node_modules/") {
            Some(0) | None => String::new(),
            Some(i) => base[..i - 1].to_string(),
        };
    }
}

fn from_packages(packages: &Map<String, Value>) -> Lock {
    let mut lock = Lock::default();
    // key → package index, for every entry that is a downloaded package or a workspace link target.
    let mut index: HashMap<&str, usize> = HashMap::new();

    for (key, entry) in packages {
        let Some(name) = name_at(key) else { continue };
        let link = entry.get("link").and_then(Value::as_bool).unwrap_or(false);
        if link {
            // `node_modules/a` → the member at `entry.resolved`: take the member's own name/version.
            let target = entry.get("resolved").and_then(Value::as_str).and_then(|r| packages.get(r));
            let version = target.and_then(|t| t.get("version")).and_then(Value::as_str).unwrap_or("");
            index.insert(key.as_str(), lock.intern(name, version, true));
        } else if let Some(version) = entry.get("version").and_then(Value::as_str) {
            // An entry with an `inBundle`/`extraneous` flag is still on disk, so still a package.
            index.insert(key.as_str(), lock.intern(name, version, false));
        }
    }

    for (key, entry) in packages {
        let Some(&from) = index.get(key.as_str()) else { continue };
        if lock.packages[from].local {
            continue;
        }
        for (section, optional) in SECTIONS {
            for dep in names_in(entry, section) {
                if let Some(&to) = resolve_path(packages, key, &dep).and_then(|k| index.get(k.as_str())) {
                    lock.add_edge(from, to, optional);
                }
            }
        }
    }

    // Importers: the root (`""`) and every workspace member — an entry whose key is not under
    // `node_modules`. Their direct dependencies resolve from their own folder.
    for (key, entry) in packages {
        if key.contains("node_modules/") {
            continue;
        }
        for section in IMPORTER_SECTIONS {
            for dep in names_in(entry, section) {
                if let Some(&to) = resolve_path(packages, key, &dep).and_then(|k| index.get(k.as_str())) {
                    lock.set_importer_dep(key, &dep, to);
                }
            }
        }
    }
    lock
}

fn names_in(entry: &Value, section: &str) -> Vec<String> {
    entry.get(section).and_then(Value::as_object).map(|m| m.keys().cloned().collect()).unwrap_or_default()
}

// ── v1 ───────────────────────────────────────────────────────────────────────

/// One level of the v1 tree: the packages installed *in* it, by name.
struct Scope {
    parent: Option<usize>,
    members: BTreeMap<String, usize>,
}

fn from_nested(doc: &Map<String, Value>) -> Lock {
    let mut lock = Lock::default();
    let Some(Value::Object(top)) = doc.get("dependencies") else { return lock };

    let mut scopes: Vec<Scope> = Vec::new();
    let mut pending: Vec<Pending> = Vec::new();
    register_scope(top, None, &mut lock, &mut scopes, &mut pending);

    for p in pending {
        let Some(Value::Object(requires)) = p.entry.get("requires") else { continue };
        for dep in requires.keys() {
            // Looked up from the package's *own* nested installs first, then each scope above.
            let mut at = p.nested.or(Some(p.scope));
            while let Some(s) = at {
                if let Some(&to) = scopes[s].members.get(dep) {
                    lock.add_edge(p.package, to, false);
                    break;
                }
                at = scopes[s].parent;
            }
        }
    }
    // The root scope is every package installed at the top — a superset of what the root declares,
    // which is fine for a lookup by declared name.
    for (name, &package) in &scopes[0].members {
        lock.set_importer_dep("", name, package);
    }
    lock
}

/// A package whose `requires` are resolved once every scope exists.
struct Pending<'a> {
    package: usize,
    /// The scope it is installed in.
    scope: usize,
    /// The scope of its own nested installs, when it has any.
    nested: Option<usize>,
    entry: &'a Map<String, Value>,
}

/// Create the packages of one level of the tree and, recursively, of each one's nested level.
fn register_scope<'a>(
    level: &'a Map<String, Value>,
    parent: Option<usize>,
    lock: &mut Lock,
    scopes: &mut Vec<Scope>,
    pending: &mut Vec<Pending<'a>>,
) -> usize {
    let id = scopes.len();
    scopes.push(Scope { parent, members: BTreeMap::new() });
    for (name, value) in level {
        let (Some(entry), Some(version)) = (value.as_object(), value.get("version").and_then(Value::as_str)) else { continue };
        let package = lock.intern(name, version, false);
        scopes[id].members.insert(name.clone(), package);
        let slot = pending.len();
        pending.push(Pending { package, scope: id, nested: None, entry });
        if let Some(Value::Object(nested)) = value.get("dependencies") {
            let child = register_scope(nested, Some(id), lock, scopes, pending);
            pending[slot].nested = Some(child);
        }
    }
    id
}

#[cfg(test)]
mod tests {
    use super::*;

    const V3: &str = r#"{
      "name": "app", "lockfileVersion": 3,
      "packages": {
        "": { "name": "app", "dependencies": { "a": "^1.0.0" }, "devDependencies": { "d": "^2.0.0" } },
        "node_modules/a": { "version": "1.2.0", "dependencies": { "b": "^2.0.0" } },
        "node_modules/b": { "version": "2.1.0" },
        "node_modules/a/node_modules/b": { "version": "1.0.0" },
        "node_modules/d": { "version": "2.0.5", "dev": true, "optionalDependencies": { "o": "1" } },
        "node_modules/o": { "version": "1.1.1", "optional": true }
      }
    }"#;

    fn versions(lock: &Lock, name: &str) -> Vec<String> {
        lock.packages.iter().filter(|p| p.name == name).map(|p| p.version.clone()).collect()
    }

    #[test]
    fn direct_dependencies_resolve_to_what_was_installed_at_the_root() {
        let lock = parse(V3);
        assert_eq!(lock.direct("", "a", "^1.0.0").map(|p| p.version.as_str()), Some("1.2.0"));
        assert_eq!(lock.direct("", "d", "^2.0.0").map(|p| p.version.as_str()), Some("2.0.5"));
        assert!(lock.direct("", "missing", "1").is_none());
    }

    #[test]
    fn a_nested_install_wins_over_the_hoisted_one_for_its_parent() {
        let lock = parse(V3);
        assert_eq!(versions(&lock, "b").len(), 2, "both copies of b are in the graph");
        let a = lock.direct_index("", "a", "^1.0.0").unwrap();
        let b_of_a = &lock.packages[lock.packages[a].deps[0].to];
        assert_eq!((b_of_a.name.as_str(), b_of_a.version.as_str()), ("b", "1.0.0"));
    }

    #[test]
    fn optional_dependencies_make_optional_edges() {
        let lock = parse(V3);
        let d = lock.direct_index("", "d", "^2.0.0").unwrap();
        assert!(lock.packages[d].deps[0].optional);
    }

    #[test]
    fn workspace_members_are_importers_and_links_are_local() {
        let lock = parse(r#"{
          "lockfileVersion": 3,
          "packages": {
            "": { "name": "root", "workspaces": ["pkgs/*"] },
            "pkgs/x": { "name": "x", "version": "0.1.0", "dependencies": { "lib": "^3.0.0", "y": "*" } },
            "pkgs/y": { "name": "y", "version": "0.2.0" },
            "node_modules/lib": { "version": "3.4.5" },
            "node_modules/x": { "resolved": "pkgs/x", "link": true },
            "node_modules/y": { "resolved": "pkgs/y", "link": true }
          }
        }"#);
        assert_eq!(lock.direct("pkgs/x", "lib", "^3.0.0").map(|p| p.version.as_str()), Some("3.4.5"));
        let y = lock.direct("pkgs/x", "y", "*").expect("a sibling member resolves through its link");
        assert!(y.local);
        assert_eq!(y.version, "0.2.0");
    }

    #[test]
    fn v1_nests_by_requirer() {
        let lock = parse(r#"{
          "lockfileVersion": 1,
          "dependencies": {
            "a": { "version": "1.0.0", "requires": { "b": "^2.0.0" },
                   "dependencies": { "b": { "version": "1.5.0" } } },
            "b": { "version": "2.0.0" }
          }
        }"#);
        let a = lock.direct_index("", "a", "").unwrap();
        let b = &lock.packages[lock.packages[a].deps[0].to];
        assert_eq!(b.version, "1.5.0");
        assert_eq!(lock.direct("", "b", "").map(|p| p.version.as_str()), Some("2.0.0"));
    }

    #[test]
    fn garbage_is_an_empty_lock_and_not_a_panic() {
        assert!(parse("not json").packages.is_empty());
        assert!(parse("{}").packages.is_empty());
    }
}
