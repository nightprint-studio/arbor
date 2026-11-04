//! `yarn.lock` — Yarn classic (v1) and Yarn Berry (v2+).
//!
//! Both key an entry by the **descriptors** that resolve to it — `"foo@^1.0.0", "foo@^1.2.0"` — and
//! give each dependency as `name range`, so a requirement is resolved by looking the descriptor up.
//! Neither records which versions a project's own `package.json` got, so [`Lock::direct`] answers
//! those the same way, from the range the manifest writes.
//!
//! Classic is a bespoke indentation format (`version "1.2.3"`, dependencies as `name "range"`).
//! Berry is YAML (`version: 1.2.3`, `name: range`) with the descriptors carrying a protocol
//! (`foo@npm:^1.0.0`); the protocol is stripped so both look the same to a caller.

use super::yaml_lite;
use super::{split_name_version, Lock};

pub fn parse(text: &str) -> Lock {
    if text.contains("__metadata:") {
        parse_berry(text)
    } else {
        parse_classic(text)
    }
}

/// One entry before its dependencies are resolved: they name descriptors that may appear later.
struct Entry {
    package: usize,
    deps: Vec<(String, String, bool)>,
}

fn finish(mut lock: Lock, entries: Vec<Entry>) -> Lock {
    for e in entries {
        for (name, range, optional) in e.deps {
            let found = [format!("{name}@{range}"), format!("{name}@npm:{range}")]
                .iter()
                .find_map(|d| lock.descriptor(d));
            if let Some(to) = found {
                lock.add_edge(e.package, to, optional);
            }
        }
    }
    lock
}

// ── classic ──────────────────────────────────────────────────────────────────

fn parse_classic(text: &str) -> Lock {
    let mut lock = Lock::default();
    let mut entries: Vec<Entry> = Vec::new();
    // The entry being read: its descriptors and version are only complete at the next header.
    let mut descriptors: Vec<String> = Vec::new();
    let mut version = String::new();
    let mut deps: Vec<(String, String, bool)> = Vec::new();
    let mut optional_section = false;
    let mut in_deps = false;

    let flush = |lock: &mut Lock, entries: &mut Vec<Entry>, descriptors: &mut Vec<String>, version: &mut String, deps: &mut Vec<(String, String, bool)>| {
        if let (false, Some(first)) = (version.is_empty(), descriptors.first()) {
            let name = descriptor_name(first).to_string();
            let package = lock.intern(&name, version, false);
            for d in descriptors.iter() {
                lock.add_descriptor(normalise_descriptor(d), package);
            }
            entries.push(Entry { package, deps: std::mem::take(deps) });
        }
        descriptors.clear();
        version.clear();
        deps.clear();
    };

    for raw in text.lines() {
        if raw.trim().is_empty() || raw.trim_start().starts_with('#') {
            continue;
        }
        let indent = raw.len() - raw.trim_start().len();
        let line = raw.trim();
        match indent {
            0 => {
                flush(&mut lock, &mut entries, &mut descriptors, &mut version, &mut deps);
                descriptors = line
                    .trim_end_matches(':')
                    .split(',')
                    .map(|d| yaml_lite::unquote(d.trim()))
                    .filter(|d| !d.is_empty())
                    .collect();
                in_deps = false;
            }
            2 => {
                in_deps = false;
                if let Some(v) = line.strip_prefix("version ") {
                    version = yaml_lite::unquote(v);
                } else if line == "dependencies:" || line == "optionalDependencies:" {
                    in_deps = true;
                    optional_section = line.starts_with("optional");
                }
            }
            _ if in_deps => {
                if let Some((name, range)) = split_classic_dep(line) {
                    deps.push((name, range, optional_section));
                }
            }
            _ => {}
        }
    }
    flush(&mut lock, &mut entries, &mut descriptors, &mut version, &mut deps);
    finish(lock, entries)
}

/// `"@scope/name" "^1.0.0"` or `name "^1.0.0"` → (`name`, `^1.0.0`).
fn split_classic_dep(line: &str) -> Option<(String, String)> {
    let (name, range) = if let Some(rest) = line.strip_prefix('"') {
        let close = rest.find('"')?;
        (&rest[..close], rest[close + 1..].trim())
    } else {
        line.split_once(' ')?
    };
    Some((name.to_string(), yaml_lite::unquote(range)))
}

// ── Berry ────────────────────────────────────────────────────────────────────

fn parse_berry(text: &str) -> Lock {
    let doc = yaml_lite::parse(text);
    let mut lock = Lock::default();
    let mut entries: Vec<Entry> = Vec::new();

    for (key, node) in &doc.children {
        if key == "__metadata" {
            continue;
        }
        let descriptors: Vec<&str> = key.split(',').map(str::trim).filter(|d| !d.is_empty()).collect();
        let (Some(first), version) = (descriptors.first(), node.scalar("version")) else { continue };
        if version.is_empty() {
            continue;
        }
        let name = descriptor_name(first);
        // A workspace member resolves to itself (`pkg@workspace:packages/a`).
        let local = node.scalar("resolution").contains("@workspace:");
        let package = lock.intern(name, version, local);
        for d in &descriptors {
            lock.add_descriptor(normalise_descriptor(d), package);
        }
        let mut deps = Vec::new();
        for (section, optional) in [("dependencies", false), ("optionalDependencies", true)] {
            for (dep, range) in node.get(section).map(|n| n.children.as_slice()).unwrap_or_default() {
                deps.push((dep.clone(), range.value.clone(), optional));
            }
        }
        if !local {
            entries.push(Entry { package, deps });
        }
    }
    finish(lock, entries)
}

// ── shared ───────────────────────────────────────────────────────────────────

/// The package name of a descriptor: everything before the `@` that starts the range.
fn descriptor_name(descriptor: &str) -> &str {
    split_name_version(descriptor).map_or(descriptor, |(name, _)| name)
}

/// `foo@npm:^1.0.0` → `foo@^1.0.0`, so a manifest's plain range finds it. Other protocols
/// (`workspace:`, `patch:`, `git…`) are kept: they are not what a registry range looks like.
fn normalise_descriptor(descriptor: &str) -> String {
    match split_name_version(descriptor) {
        Some((name, range)) => format!("{name}@{}", range.strip_prefix("npm:").unwrap_or(range)),
        None => descriptor.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CLASSIC: &str = r#"# yarn lockfile v1


"@babel/core@^7.0.0", "@babel/core@^7.1.0":
  version "7.1.2"
  resolved "https://registry.yarnpkg.com/@babel/core/-/core-7.1.2.tgz#abc"
  dependencies:
    lodash "^4.17.10"
    "@babel/types" "^7.0.0"

"@babel/types@^7.0.0":
  version "7.0.5"

lodash@^4.17.10:
  version "4.17.21"
  optionalDependencies:
    fsevents "^1.0.0"

fsevents@^1.0.0:
  version "1.2.4"
"#;

    #[test]
    fn classic_resolves_a_manifest_range_through_its_descriptor() {
        let lock = parse(CLASSIC);
        assert_eq!(lock.direct("", "@babel/core", "^7.1.0").map(|p| p.version.as_str()), Some("7.1.2"));
        assert_eq!(lock.direct("", "@babel/core", "^7.0.0").map(|p| p.version.as_str()), Some("7.1.2"));
        assert!(lock.direct("", "@babel/core", "^8.0.0").is_none());
    }

    #[test]
    fn classic_edges_follow_dependencies_by_descriptor() {
        let lock = parse(CLASSIC);
        let core = lock.direct_index("", "@babel/core", "^7.0.0").unwrap();
        let names: Vec<&str> = lock.packages[core].deps.iter().map(|e| lock.packages[e.to].name.as_str()).collect();
        assert_eq!(names, ["lodash", "@babel/types"]);

        let lodash = lock.direct_index("", "lodash", "^4.17.10").unwrap();
        let fsevents = lock.packages[lodash].deps[0];
        assert!(fsevents.optional);
        assert_eq!(lock.packages[fsevents.to].version, "1.2.4");
    }

    const BERRY: &str = r#"# This file is generated by running "yarn install"

__metadata:
  version: 6
  cacheKey: 8

"app@workspace:.":
  version: 0.0.0-use.local
  resolution: "app@workspace:."
  dependencies:
    left-pad: ^1.0.0
  languageName: unknown
  linkType: soft

"left-pad@npm:^1.0.0, left-pad@npm:^1.3.0":
  version: 1.3.0
  resolution: "left-pad@npm:1.3.0"
  dependencies:
    tiny: ^2.0.0
  checksum: abc
  languageName: node
  linkType: hard

"tiny@npm:^2.0.0":
  version: 2.1.0
  resolution: "tiny@npm:2.1.0"
  languageName: node
  linkType: hard
"#;

    #[test]
    fn berry_strips_the_npm_protocol_so_plain_ranges_resolve() {
        let lock = parse(BERRY);
        assert_eq!(lock.direct("", "left-pad", "^1.3.0").map(|p| p.version.as_str()), Some("1.3.0"));
        let lp = lock.direct_index("", "left-pad", "^1.0.0").unwrap();
        let tiny = &lock.packages[lock.packages[lp].deps[0].to];
        assert_eq!((tiny.name.as_str(), tiny.version.as_str()), ("tiny", "2.1.0"));
    }

    #[test]
    fn berry_marks_workspace_members_local_and_does_not_descend_into_them() {
        let lock = parse(BERRY);
        let app = lock.packages.iter().find(|p| p.name == "app").expect("the member is in the graph");
        assert!(app.local);
        assert!(app.deps.is_empty());
    }

    #[test]
    fn empty_or_unrecognised_text_is_an_empty_lock() {
        assert!(parse("").packages.is_empty());
        assert!(parse("# yarn lockfile v1\n").packages.is_empty());
    }
}
