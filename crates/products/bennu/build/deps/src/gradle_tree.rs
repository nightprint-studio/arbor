//! The resolved dependency tree — `gradle dependencies` output, read back as data.
//!
//! Gradle's counterpart of [`crate::tree::parse_tree_output`], and for the same reason it parses
//! what Gradle printed instead of recomputing: mediation, capabilities, platforms and substitutions
//! all depend on the whole build.
//!
//! ## What the text looks like
//!
//! ```text
//! Project ':app'
//! runtimeClasspath - Runtime classpath of source set 'main'.
//! +--- org.springframework:spring-web:5.3.10
//! |    \--- org.springframework:spring-core:5.3.10
//! |         \--- org.springframework:spring-jcl:5.3.10
//! +--- com.google.guava:guava:30.0 -> 32.0
//! +--- project :core
//! \--- org.slf4j:slf4j-api:1.7.36 (*)
//! ```
//!
//! Units of five characters, a `+--- ` or `\--- ` marker, and trailing flags: `(*)` the subtree was
//! printed once already, `(c)` a constraint (not a dependency), `(n)` not resolved.
//!
//! ## How a conflict is represented
//!
//! `guava:30.0 -> 32.0` is a node that *asked for* 30.0 and *got* 32.0. The panel already reads a
//! Maven-style conflict as two nodes — the one that was kept and the one that was dropped for it
//! (`omitted: "conflict"`, `kept_version`) — so that is what this emits: the kept node with its
//! children, preceded by a childless omitted sibling for the version that lost.

use crate::tree::{ModuleTree, TreeNode};

/// Parse the stdout of one or more `gradle :x:dependencies --configuration …` tasks into one tree
/// per project.
pub fn parse_gradle_dependencies(text: &str) -> Vec<ModuleTree> {
    let mut out: Vec<ModuleTree> = Vec::new();
    // `last_at[d]` is the index of the most recent kept node at depth `d + 1`.
    let mut last_at: Vec<usize> = Vec::new();
    let mut scope = String::new();

    for raw in text.lines() {
        let line = raw.trim_end();
        if let Some(name) = project_header(line) {
            out.push(ModuleTree { module: name, version: String::new(), nodes: Vec::new() });
            last_at.clear();
            scope.clear();
            continue;
        }
        let Some(module) = out.last_mut() else { continue };
        let Some((depth, content)) = split_marker(line) else {
            // A configuration heading (`runtimeClasspath - …`) names the scope of what follows.
            if let Some((config, _)) = line.split_once(" - ") {
                if !config.contains(' ') && !config.is_empty() {
                    scope = scope_of(config);
                }
            }
            continue;
        };
        let (content, flags) = strip_flags(content);
        if flags.constraint {
            continue;
        }
        let parent = if depth <= 1 { None } else { last_at.get(depth - 2).copied() };
        for node in nodes_of(content, depth, parent, &flags, &scope) {
            let kept = node.omitted.is_empty() || node.omitted == "duplicate" || node.omitted == "unresolved";
            let idx = module.nodes.len();
            module.nodes.push(node);
            if kept {
                last_at.truncate(depth - 1);
                last_at.push(idx);
            }
        }
    }
    out
}

/// `Project ':a'` / `Root project 'name'` (optionally followed by ` - description`) → the module's
/// name: the path for a subproject, the build's name for the root. Matches the ids the dependency
/// report uses, so the two line up.
fn project_header(line: &str) -> Option<String> {
    let head = line.split(" - ").next().unwrap_or(line);
    let quoted = |s: &str| s.split('\'').nth(1).map(str::to_string);
    if head.starts_with("Root project") {
        return Some(quoted(head).unwrap_or_default());
    }
    if head.starts_with("Project ") {
        return quoted(head);
    }
    None
}

fn scope_of(config: &str) -> String {
    let lower = config.to_ascii_lowercase();
    if lower.contains("test") {
        "test".to_string()
    } else if lower.starts_with("runtime") {
        "runtime".to_string()
    } else if lower.starts_with("compile") {
        "compile".to_string()
    } else {
        config.to_string()
    }
}

/// Split a tree line into (depth, content). `None` when the line is not part of a tree.
///
/// The marker (`+--- ` or `\--- `) sits at a multiple of five, after nothing but `|` and spaces.
fn split_marker(line: &str) -> Option<(usize, &str)> {
    let at = ["+--- ", "\\--- "].iter().filter_map(|m| line.find(m)).min()?;
    let prefix = &line[..at];
    if at % 5 != 0 || !prefix.bytes().all(|b| b == b'|' || b == b' ') {
        return None;
    }
    Some((at / 5 + 1, &line[at + 5..]))
}

#[derive(Default)]
struct Flags {
    duplicate: bool,
    constraint: bool,
    unresolved: bool,
}

/// `text (*)` → (`text`, duplicate). Flags can stack (`(c) (*)`).
fn strip_flags(content: &str) -> (&str, Flags) {
    let mut flags = Flags::default();
    let mut rest = content.trim_end();
    loop {
        if let Some(r) = rest.strip_suffix("(*)") {
            flags.duplicate = true;
            rest = r.trim_end();
        } else if let Some(r) = rest.strip_suffix("(c)") {
            flags.constraint = true;
            rest = r.trim_end();
        } else if let Some(r) = rest.strip_suffix("(n)") {
            flags.unresolved = true;
            rest = r.trim_end();
        } else {
            return (rest, flags);
        }
    }
}

/// The node(s) one line stands for: one, or two when the line records a version conflict.
fn nodes_of(content: &str, depth: usize, parent: Option<usize>, flags: &Flags, scope: &str) -> Vec<TreeNode> {
    let base = TreeNode {
        scope: scope.to_string(),
        depth,
        parent,
        omitted: if flags.unresolved {
            "unresolved".to_string()
        } else if flags.duplicate {
            "duplicate".to_string()
        } else {
            String::new()
        },
        ..TreeNode::default()
    };
    if let Some(path) = content.trim().strip_prefix("project ") {
        return vec![TreeNode { name: path.trim().to_string(), kind: "project".to_string(), ..base }];
    }

    let (left, right) = match content.split_once(" -> ") {
        Some((l, r)) => (l.trim(), Some(r.trim())),
        None => (content.trim(), None),
    };
    let mut parts = left.splitn(3, ':');
    let (Some(group), Some(name)) = (parts.next(), parts.next()) else { return Vec::new() };
    let requested = requested_version(parts.next().unwrap_or(""));

    // `-> 1.2` is a version; `-> org.new:mod:1.2` is a substitution that also changes the artifact.
    let (group, name, resolved) = match right {
        Some(r) if r.contains(':') => {
            let mut p = r.splitn(3, ':');
            (
                p.next().unwrap_or(group).to_string(),
                p.next().unwrap_or(name).to_string(),
                p.next().unwrap_or("").to_string(),
            )
        }
        Some(r) => (group.to_string(), name.to_string(), r.to_string()),
        None => (group.to_string(), name.to_string(), requested.clone()),
    };

    let kept = TreeNode { group: group.clone(), name: name.clone(), version: resolved.clone(), ..base.clone() };
    if !requested.is_empty() && requested != resolved && !resolved.is_empty() {
        let lost = TreeNode {
            group,
            name,
            version: requested,
            omitted: "conflict".to_string(),
            kept_version: resolved,
            ..base
        };
        return vec![lost, kept];
    }
    vec![kept]
}

/// `{strictly 1.0}` / `{require 1.0; reject 2.0}` → `1.0`; a plain version is returned as is.
fn requested_version(raw: &str) -> String {
    let t = raw.trim();
    if let Some(inner) = t.strip_prefix('{').and_then(|r| r.strip_suffix('}')) {
        return inner
            .split(|c: char| c.is_whitespace() || c == ';')
            .find(|w| w.starts_with(|c: char| c.is_ascii_digit()))
            .unwrap_or(inner)
            .to_string();
    }
    t.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "\
Project ':app'

runtimeClasspath - Runtime classpath of source set 'main'.
+--- org.springframework:spring-web:5.3.10
|    \\--- org.springframework:spring-core:5.3.10
|         \\--- org.springframework:spring-jcl:5.3.10
+--- com.google.guava:guava:30.0 -> 32.0
+--- project :core
+--- org.x:managed -> 2.1
+--- org.c:constrained:1.0 (c)
\\--- org.slf4j:slf4j-api:1.7.36 (*)

Root project 'shop'

runtimeClasspath - Runtime classpath of source set 'main'.
No dependencies
";

    #[test]
    fn a_tree_is_read_with_its_parents() {
        let trees = parse_gradle_dependencies(SAMPLE);
        assert_eq!(trees.len(), 2);
        assert_eq!(trees[0].module, ":app");
        assert_eq!(trees[1].module, "shop");
        let nodes = &trees[0].nodes;
        let spring_core = nodes.iter().find(|n| n.name == "spring-core").unwrap();
        assert_eq!(spring_core.depth, 2);
        assert_eq!(nodes[spring_core.parent.unwrap()].name, "spring-web");
        let jcl = nodes.iter().find(|n| n.name == "spring-jcl").unwrap();
        assert_eq!(nodes[jcl.parent.unwrap()].name, "spring-core");
        assert_eq!(spring_core.scope, "runtime");
    }

    #[test]
    fn a_version_conflict_is_a_kept_node_and_a_dropped_one() {
        let trees = parse_gradle_dependencies(SAMPLE);
        let guavas: Vec<&TreeNode> = trees[0].nodes.iter().filter(|n| n.name == "guava").collect();
        assert_eq!(guavas.len(), 2);
        let lost = guavas.iter().find(|n| !n.omitted.is_empty()).unwrap();
        let kept = guavas.iter().find(|n| n.omitted.is_empty()).unwrap();
        assert_eq!((lost.version.as_str(), lost.kept_version.as_str(), lost.omitted.as_str()), ("30.0", "32.0", "conflict"));
        assert_eq!(kept.version, "32.0");
    }

    #[test]
    fn project_constraint_managed_and_repeated_nodes() {
        let nodes = &parse_gradle_dependencies(SAMPLE)[0].nodes;
        assert!(nodes.iter().any(|n| n.kind == "project" && n.name == ":core"));
        // A constraint is not a dependency.
        assert!(!nodes.iter().any(|n| n.name == "constrained"));
        // `org.x:managed -> 2.1` asked for no version: it is kept at 2.1, with no phantom conflict.
        let managed = nodes.iter().find(|n| n.name == "managed").unwrap();
        assert_eq!((managed.version.as_str(), managed.omitted.as_str()), ("2.1", ""));
        assert_eq!(nodes.iter().filter(|n| n.name == "managed").count(), 1);
        let slf4j = nodes.iter().find(|n| n.name == "slf4j-api").unwrap();
        assert_eq!(slf4j.omitted, "duplicate");
    }

    #[test]
    fn a_strict_requirement_reads_as_its_version() {
        assert_eq!(requested_version("{strictly 1.0}"), "1.0");
        assert_eq!(requested_version("{require 1.0; reject 2.0}"), "1.0");
        assert_eq!(requested_version("1.2"), "1.2");
    }
}
