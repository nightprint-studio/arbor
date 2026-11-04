//! The small slice of YAML that `pnpm-lock.yaml` and Yarn Berry's `yarn.lock` are written in.
//!
//! Both are machine-generated block mappings: `key: value` pairs nested by indentation, scalars
//! that may be quoted, and the occasional flow collection (`{ integrity: … }`, `[a, b]`) that none of
//! the readers here look inside. That is all this understands — no anchors, no multi-line scalars,
//! no sequences (a `- item` line is skipped, and no lockfile field a reader needs is a list).
//!
//! A full YAML library would be the alternative; for two generated formats and four fields it is a
//! dependency bigger than the problem.

/// One node: the scalar written after the key (flow collections included, verbatim) and the keys
/// nested beneath it, in file order.
#[derive(Debug, Default, Clone)]
pub struct Node {
    pub value: String,
    pub children: Vec<(String, Node)>,
}

impl Node {
    /// The child named `key`.
    pub fn get(&self, key: &str) -> Option<&Node> {
        self.children.iter().find(|(k, _)| k == key).map(|(_, n)| n)
    }

    /// The child named `key`'s scalar, or "" when it is absent.
    pub fn scalar(&self, key: &str) -> &str {
        self.get(key).map_or("", |n| n.value.as_str())
    }
}

/// Parse `text` into a root node whose children are the top-level keys.
pub fn parse(text: &str) -> Node {
    // `path[i]` is the child index chosen at depth `i`; `indents[i]` is that line's indentation.
    // The stack replaces recursion: a lockfile is deep in lines, not in nesting.
    let mut root = Node::default();
    let mut path: Vec<usize> = Vec::new();
    let mut indents: Vec<usize> = Vec::new();

    for raw in text.lines() {
        let trimmed = raw.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with("- ") || trimmed == "-" {
            continue;
        }
        let indent = raw.len() - trimmed.len();
        let Some((key, value)) = split_entry(trimmed) else { continue };

        while indents.last().is_some_and(|&i| i >= indent) {
            indents.pop();
            path.pop();
        }
        let parent = descend(&mut root, &path);
        parent.children.push((key, Node { value, children: Vec::new() }));
        path.push(parent.children.len() - 1);
        indents.push(indent);
    }
    root
}

fn descend<'a>(root: &'a mut Node, path: &[usize]) -> &'a mut Node {
    let mut at = root;
    for &i in path {
        at = &mut at.children[i].1;
    }
    at
}

/// `key: value` → (`key`, `value`), both unquoted. The key ends at the first `:` that is followed by
/// a space or the end of the line — which is how YAML lets `/a@1(b@2):` carry colons-free keys and
/// `"a@npm:^1":` carry quoted ones.
fn split_entry(line: &str) -> Option<(String, String)> {
    let bytes = line.as_bytes();
    let (key, rest) = match bytes.first()? {
        q @ (b'"' | b'\'') => {
            let close = line[1..].find(*q as char)? + 1;
            let after = line[close + 1..].strip_prefix(':')?;
            (line[1..close].to_string(), after)
        }
        _ => {
            let mut end = None;
            for (i, b) in bytes.iter().enumerate() {
                if *b == b':' && bytes.get(i + 1).map_or(true, |n| *n == b' ') {
                    end = Some(i);
                    break;
                }
            }
            let end = end?;
            (line[..end].to_string(), &line[end + 1..])
        }
    };
    Some((key, unquote(rest.trim())))
}

/// A scalar without its quotes, and without a trailing `# comment`.
pub fn unquote(raw: &str) -> String {
    let s = raw.trim();
    if let Some(q) = s.chars().next().filter(|c| matches!(c, '"' | '\'')) {
        if let Some(close) = s[1..].find(q) {
            return s[1..=close].to_string();
        }
    }
    match s.find(" #") {
        Some(i) => s[..i].trim_end().to_string(),
        None => s.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nesting_follows_indentation_and_keys_keep_their_colons() {
        let doc = "\
lockfileVersion: '9.0'
importers:
  .:
    dependencies:
      '@scope/pkg':
        specifier: ^1.0.0
        version: 1.2.3(react@18.0.0)
snapshots:
  foo@1.0.0:
    dependencies:
      bar: 2.0.0
";
        let root = parse(doc);
        assert_eq!(root.scalar("lockfileVersion"), "9.0");
        let dep = root.get("importers").and_then(|n| n.get(".")).and_then(|n| n.get("dependencies"))
            .and_then(|n| n.get("@scope/pkg")).expect("scoped dependency");
        assert_eq!(dep.scalar("version"), "1.2.3(react@18.0.0)");
        let snap = root.get("snapshots").and_then(|n| n.get("foo@1.0.0")).expect("snapshot");
        assert_eq!(snap.get("dependencies").map(|d| d.scalar("bar")), Some("2.0.0"));
    }

    #[test]
    fn a_quoted_key_may_contain_colons_and_commas() {
        let root = parse("\"foo@npm:^1.0.0, foo@npm:^1.2.0\":\n  version: 1.2.3\n");
        let (key, node) = &root.children[0];
        assert_eq!(key, "foo@npm:^1.0.0, foo@npm:^1.2.0");
        assert_eq!(node.scalar("version"), "1.2.3");
    }

    #[test]
    fn comments_sequences_and_flow_values_do_not_break_the_walk() {
        let root = parse("a:\n  # note\n  - item\n  resolution: {integrity: sha512-x}\n  b: 1 # trailing\n");
        let a = root.get("a").unwrap();
        assert_eq!(a.scalar("resolution"), "{integrity: sha512-x}");
        assert_eq!(a.scalar("b"), "1");
    }
}
