//! The resolved dependency **tree** — `mvn dependency:tree` output, read back as data.
//!
//! The [`Report`](crate::model::Report) answers "what does each module depend on, and what is on
//! the classpath". It cannot answer the two questions a dependency list is opened for the moment
//! something is wrong: *who brought this in*, and *why is there more than one version of it*. Those
//! live in the resolved graph, which only Maven computes (mediation, exclusions, scopes all depend on
//! the whole reactor), so this module does not recompute it — it reads what Maven printed.
//!
//! ## What the text looks like
//!
//! ```text
//! [INFO] --- maven-dependency-plugin:3.6.0:tree (default-cli) @ app ---
//! [INFO] com.acme:app:jar:1.0
//! [INFO] +- org.springframework:spring-web:jar:5.3.10:compile
//! [INFO] |  \- org.springframework:spring-core:jar:5.3.10:compile
//! [INFO] \- (commons-logging:commons-logging:jar:1.2:compile - omitted for conflict with 1.3)
//! ```
//!
//! One section per module. The root line is the module itself; every other line is indented in units
//! of three characters and ends its prefix with `+- ` or `\- `. With `-Dverbose` Maven also prints the
//! nodes it dropped, in parentheses, with the reason — and that is exactly the information that makes
//! "two versions of the same library" visible, because a dropped node is a version somebody asked for
//! and did not get.
//!
//! Tolerant by design: log noise between sections, a plugin that prints `-- module x` after a
//! coordinate, `(optional)` markers and coordinates with or without a classifier are all read; a line
//! that is none of those ends the section rather than failing the parse.

use serde::{Deserialize, Serialize};

/// One node of a module's resolved tree.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct TreeNode {
    pub group: String,
    pub name: String,
    pub version: String,
    pub scope: String,
    /// `<type>` — `jar` almost always.
    pub kind: String,
    pub classifier: String,
    /// 1 for a direct dependency of the module.
    pub depth: usize,
    /// Index of the node that brought this one in (within the same [`ModuleTree`]); `None` at depth 1,
    /// where the module itself is the parent.
    pub parent: Option<usize>,
    pub optional: bool,
    /// Empty for a node Maven kept. Otherwise why it was dropped: `duplicate` (same version already
    /// present), `conflict` (another version won — see [`kept_version`](Self::kept_version)) or
    /// `cycle`.
    pub omitted: String,
    /// For `conflict`: the version that won.
    pub kept_version: String,
}

impl TreeNode {
    /// `group:name` — the key two versions of the same library share.
    pub fn coord(&self) -> String {
        if self.group.is_empty() {
            self.name.clone()
        } else {
            format!("{}:{}", self.group, self.name)
        }
    }
}

/// The resolved tree of one module.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ModuleTree {
    /// `group:artifactId` of the module.
    pub module: String,
    pub version: String,
    /// Depth-first, in Maven's own order; `parent` indexes into this list.
    pub nodes: Vec<TreeNode>,
}

/// Parse the stdout of `mvn dependency:tree` (any number of modules) into one tree per module.
pub fn parse_tree_output(text: &str) -> Vec<ModuleTree> {
    let mut out: Vec<ModuleTree> = Vec::new();
    // After a tree header, the next content line is the module's root.
    let mut expect_root = false;
    let mut in_section = false;
    // `last_at[d]` is the index of the most recent node at depth `d + 1`.
    let mut last_at: Vec<usize> = Vec::new();

    for raw in text.lines() {
        let line = strip_log_prefix(raw);
        if line.contains("dependency") && line.contains(":tree") && line.trim_start().starts_with("---") {
            expect_root = true;
            in_section = false;
            continue;
        }
        if expect_root {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            expect_root = false;
            if let Some(root) = parse_root(trimmed) {
                out.push(root);
                in_section = true;
                last_at.clear();
            }
            continue;
        }
        if !in_section {
            continue;
        }
        let Some((depth, content)) = split_prefix(line) else {
            in_section = false;
            continue;
        };
        let Some(mut node) = parse_node(content) else { continue };
        node.depth = depth;
        let module = out.last_mut().expect("in_section implies a module");
        node.parent = if depth <= 1 { None } else { last_at.get(depth - 2).copied() };
        let idx = module.nodes.len();
        module.nodes.push(node);
        last_at.truncate(depth - 1);
        last_at.push(idx);
    }
    out
}

/// Parse the stdout of `cargo tree --workspace --prefix depth` into one tree per workspace member.
///
/// With `--prefix depth` every line is the depth as a number glued to the package
/// (`2serde_derive v1.0.0 (proc-macro)`), which is far easier to read back than the ASCII art and
/// carries the same information. Depth 0 is a workspace member — the root of its own tree.
///
/// Unlike Maven, Cargo does not *drop* a version: two incompatible versions of a crate are both
/// built and both appear, as separate nodes. So there are no omitted nodes with a reason here —
/// only a `(*)` marker where Cargo printed a subtree once and elided the repeats, which is what
/// `duplicate` means for these trees.
pub fn parse_cargo_tree(text: &str) -> Vec<ModuleTree> {
    let mut out: Vec<ModuleTree> = Vec::new();
    let mut last_at: Vec<usize> = Vec::new();

    for line in text.lines() {
        let digits = line.bytes().take_while(u8::is_ascii_digit).count();
        if digits == 0 {
            continue;
        }
        let Ok(depth) = line[..digits].parse::<usize>() else { continue };
        let rest = &line[digits..];
        let mut words = rest.split_whitespace();
        let (Some(name), Some(ver)) = (words.next(), words.next()) else { continue };
        let Some(version) = ver.strip_prefix('v') else { continue };
        let tail = words.collect::<Vec<_>>().join(" ");

        if depth == 0 {
            out.push(ModuleTree { module: name.to_string(), version: version.to_string(), nodes: Vec::new() });
            last_at.clear();
            continue;
        }
        let Some(module) = out.last_mut() else { continue };
        let node = TreeNode {
            name: name.to_string(),
            version: version.to_string(),
            kind: if tail.contains("proc-macro") { "proc-macro".into() } else { String::new() },
            depth,
            parent: if depth <= 1 { None } else { last_at.get(depth - 2).copied() },
            omitted: if tail.ends_with("(*)") { "duplicate".into() } else { String::new() },
            ..TreeNode::default()
        };
        let idx = module.nodes.len();
        module.nodes.push(node);
        last_at.truncate(depth - 1);
        last_at.push(idx);
    }
    out
}

/// `[INFO] text` → `text`. Other levels (`[WARNING]`, `[ERROR]`) are kept whole: they are never tree
/// lines, and keeping the tag stops them being mistaken for one.
fn strip_log_prefix(line: &str) -> &str {
    let l = line.trim_end();
    l.strip_prefix("[INFO] ").or_else(|| l.strip_prefix("[INFO]")).unwrap_or(l)
}

fn parse_root(text: &str) -> Option<ModuleTree> {
    let token = text.split_whitespace().next()?;
    let parts: Vec<&str> = token.split(':').collect();
    if parts.len() < 4 {
        return None;
    }
    Some(ModuleTree {
        module: format!("{}:{}", parts[0], parts[1]),
        version: parts[parts.len() - 1].to_string(),
        nodes: Vec::new(),
    })
}

/// Split a tree line into (depth, content). `None` when the line is not part of a tree.
fn split_prefix(line: &str) -> Option<(usize, &str)> {
    let bytes = line.as_bytes();
    let mut i = 0;
    // Continuation units: `|  ` or three spaces.
    // Compared as bytes: log noise may hold multibyte text, and slicing a `str` mid-character panics.
    while i + 3 <= bytes.len() {
        let unit = &bytes[i..i + 3];
        if unit == b"|  " || unit == b"   " {
            i += 3;
        } else {
            break;
        }
    }
    if i + 3 > bytes.len() {
        return None;
    }
    let unit = &bytes[i..i + 3];
    if unit == b"+- " || unit == b"\\- " {
        Some((i / 3 + 1, &line[i + 3..]))
    } else {
        None
    }
}

fn parse_node(content: &str) -> Option<TreeNode> {
    let content = content.trim();
    // A dropped node is wrapped in parentheses: `(coord - note)`.
    let (inner, wrapped) = match content.strip_prefix('(') {
        Some(rest) => (rest.strip_suffix(')').unwrap_or(rest), true),
        None => (content, false),
    };
    let mut tokens = inner.splitn(2, char::is_whitespace);
    let coord = tokens.next()?;
    let note = tokens.next().unwrap_or("").trim();

    let parts: Vec<&str> = coord.split(':').collect();
    let mut node = TreeNode::default();
    match parts.as_slice() {
        [g, a, t, v, s] => {
            node.group = (*g).into(); node.name = (*a).into(); node.kind = (*t).into();
            node.version = (*v).into(); node.scope = (*s).into();
        }
        [g, a, t, c, v, s] => {
            node.group = (*g).into(); node.name = (*a).into(); node.kind = (*t).into();
            node.classifier = (*c).into(); node.version = (*v).into(); node.scope = (*s).into();
        }
        [g, a, t, v] => {
            node.group = (*g).into(); node.name = (*a).into(); node.kind = (*t).into();
            node.version = (*v).into();
        }
        _ => return None,
    }
    node.optional = note.contains("(optional)") || inner.ends_with("(optional)");
    if wrapped || note.contains("omitted for") {
        read_omission(note, &mut node);
    }
    Some(node)
}

fn read_omission(note: &str, node: &mut TreeNode) {
    if let Some(i) = note.find("omitted for ") {
        let reason = &note[i + "omitted for ".len()..];
        if let Some(rest) = reason.strip_prefix("conflict with ") {
            node.omitted = "conflict".into();
            node.kept_version = rest.split_whitespace().next().unwrap_or("").trim_end_matches(')').into();
        } else if reason.starts_with("duplicate") {
            node.omitted = "duplicate".into();
        } else if reason.starts_with("cycle") {
            node.omitted = "cycle".into();
        } else {
            node.omitted = reason.split_whitespace().next().unwrap_or("omitted").into();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "\
[INFO] Scanning for projects...
[INFO] --- maven-dependency-plugin:3.6.0:tree (default-cli) @ app ---
[INFO] com.acme:app:jar:1.0
[INFO] +- org.springframework:spring-web:jar:5.3.10:compile
[INFO] |  +- org.springframework:spring-core:jar:5.3.10:compile
[INFO] |  |  \\- org.springframework:spring-jcl:jar:5.3.10:compile
[INFO] |  \\- (commons-logging:commons-logging:jar:1.2:compile - omitted for conflict with 1.3)
[INFO] +- commons-logging:commons-logging:jar:1.3:compile
[INFO] \\- junit:junit:jar:4.13:test (optional)
[INFO]
[INFO] --- maven-dependency-plugin:3.6.0:tree (default-cli) @ lib ---
[INFO] com.acme:lib:jar:2.0
[INFO] \\- io.netty:netty-transport-native-epoll:jar:linux-x86_64:4.1.0:compile
[INFO] BUILD SUCCESS
";

    #[test]
    fn each_module_gets_its_own_tree() {
        let t = parse_tree_output(SAMPLE);
        assert_eq!(t.len(), 2);
        assert_eq!(t[0].module, "com.acme:app");
        assert_eq!(t[0].version, "1.0");
        assert_eq!(t[1].module, "com.acme:lib");
    }

    #[test]
    fn a_node_knows_who_brought_it_in() {
        let t = &parse_tree_output(SAMPLE)[0];
        let jcl = t.nodes.iter().find(|n| n.name == "spring-jcl").unwrap();
        assert_eq!(jcl.depth, 3);
        let parent = &t.nodes[jcl.parent.unwrap()];
        assert_eq!(parent.name, "spring-core");
        let grand = &t.nodes[parent.parent.unwrap()];
        assert_eq!(grand.name, "spring-web");
        assert_eq!(grand.parent, None);
    }

    #[test]
    fn a_dropped_version_is_kept_with_the_reason() {
        let t = &parse_tree_output(SAMPLE)[0];
        let dropped = t.nodes.iter().find(|n| n.omitted == "conflict").unwrap();
        assert_eq!(dropped.version, "1.2");
        assert_eq!(dropped.kept_version, "1.3");
        assert_eq!(t.nodes[dropped.parent.unwrap()].name, "spring-web");
    }

    #[test]
    fn optional_scope_and_classifier_are_read() {
        let t = parse_tree_output(SAMPLE);
        let junit = t[0].nodes.iter().find(|n| n.name == "junit").unwrap();
        assert!(junit.optional);
        assert_eq!(junit.scope, "test");
        let netty = &t[1].nodes[0];
        assert_eq!(netty.classifier, "linux-x86_64");
        assert_eq!(netty.version, "4.1.0");
        assert_eq!(netty.scope, "compile");
    }

    const CARGO: &str = "\
0app v0.1.0 (C:\\w\\app)
1serde v1.0.0
2serde_derive v1.0.0 (proc-macro)
1tokio v1.2.0
2serde v1.0.0 (*)
1rand v0.7.0
0lib v0.2.0 (C:\\w\\lib)
1rand v0.8.0
";

    #[test]
    fn cargo_members_are_roots_and_depth_gives_the_parent() {
        let t = parse_cargo_tree(CARGO);
        assert_eq!(t.len(), 2);
        assert_eq!(t[0].module, "app");
        assert_eq!(t[0].version, "0.1.0");
        let derive = t[0].nodes.iter().find(|n| n.name == "serde_derive").unwrap();
        assert_eq!(derive.kind, "proc-macro");
        assert_eq!(t[0].nodes[derive.parent.unwrap()].name, "serde");
        let dup = t[0].nodes.iter().find(|n| n.omitted == "duplicate").unwrap();
        assert_eq!(t[0].nodes[dup.parent.unwrap()].name, "tokio");
    }

    #[test]
    fn cargo_keeps_both_versions_of_a_crate() {
        let t = parse_cargo_tree(CARGO);
        assert_eq!(t[0].nodes.iter().find(|n| n.name == "rand").unwrap().version, "0.7.0");
        assert_eq!(t[1].nodes[0].version, "0.8.0");
        assert!(parse_cargo_tree("   Compiling foo\nerror: no\n").is_empty());
    }

    #[test]
    fn noise_and_non_tree_output_parse_to_nothing() {
        assert!(parse_tree_output("[ERROR] Failed to execute goal\n[INFO] BUILD FAILURE\n").is_empty());
        assert!(parse_tree_output("").is_empty());
    }
}
