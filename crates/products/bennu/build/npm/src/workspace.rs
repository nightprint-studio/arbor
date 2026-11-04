//! Workspace members — which folders under a root are packages of the same monorepo.
//!
//! npm and Yarn declare them in the root `package.json` (`"workspaces": ["packages/*"]`, or
//! `{ "packages": [...] }`); pnpm in `pnpm-workspace.yaml`. Both use globs, with `!` for exclusions.
//! The matching is pure ([`glob_matches`]); [`members`] adds the directory walk.

use std::path::Path;

/// Folders never worth entering while looking for packages.
const SKIPPED: [&str; 8] = ["node_modules", "target", "dist", "build", "out", "vendor", "coverage", "__pycache__"];

/// The workspace globs a root `package.json` declares.
pub fn package_json_patterns(text: &str) -> Vec<String> {
    let Ok(doc) = serde_json::from_str::<serde_json::Value>(text) else { return Vec::new() };
    let list = match doc.get("workspaces") {
        Some(serde_json::Value::Array(a)) => a.as_slice(),
        Some(serde_json::Value::Object(o)) => o.get("packages").and_then(|p| p.as_array()).map_or(&[][..], |a| a.as_slice()),
        _ => &[][..],
    };
    list.iter().filter_map(|v| v.as_str()).map(str::to_string).collect()
}

/// The `packages:` globs of a `pnpm-workspace.yaml`. A line-based read: the file is a list of
/// strings under one key, and a YAML parser for that would be the only one in the crate.
pub fn pnpm_patterns(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut in_packages = false;
    for raw in text.lines() {
        let line = raw.trim_end();
        if line.trim().is_empty() || line.trim_start().starts_with('#') {
            continue;
        }
        let top_level = !line.starts_with(' ') && !line.starts_with('-');
        if top_level {
            in_packages = line.trim_end_matches(':') == "packages" && line.ends_with(':');
            continue;
        }
        if in_packages {
            if let Some(item) = line.trim_start().strip_prefix('-') {
                let pattern = super::lock::yaml_lite::unquote(item.trim());
                if !pattern.is_empty() {
                    out.push(pattern);
                }
            }
        }
    }
    out
}

/// Whether the folder `path` (relative, forward slashes) is selected by `pattern`.
pub fn glob_matches(pattern: &str, path: &str) -> bool {
    let pattern = pattern.trim().trim_start_matches("./").trim_end_matches('/');
    let pattern = pattern.strip_suffix("/package.json").unwrap_or(pattern);
    let pat: Vec<&str> = pattern.split('/').filter(|s| !s.is_empty()).collect();
    let segs: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
    segments_match(&pat, &segs)
}

fn segments_match(pat: &[&str], segs: &[&str]) -> bool {
    match pat.split_first() {
        None => segs.is_empty(),
        Some((&"**", rest)) => (0..=segs.len()).any(|skip| segments_match(rest, &segs[skip..])),
        Some((p, rest)) => segs.split_first().is_some_and(|(s, tail)| wildcard(p, s) && segments_match(rest, tail)),
    }
}

/// `*` inside one path segment.
fn wildcard(pattern: &str, text: &str) -> bool {
    let p: Vec<char> = pattern.chars().collect();
    let t: Vec<char> = text.chars().collect();
    // Classic backtracking-free matcher: remember the last `*` and retry one character further on.
    let (mut pi, mut ti, mut star, mut mark) = (0, 0, None, 0);
    while ti < t.len() {
        if pi < p.len() && p[pi] == '*' {
            star = Some(pi);
            mark = ti;
            pi += 1;
        } else if pi < p.len() && p[pi] == t[ti] {
            pi += 1;
            ti += 1;
        } else if let Some(s) = star {
            pi = s + 1;
            mark += 1;
            ti = mark;
        } else {
            return false;
        }
    }
    p[pi..].iter().all(|c| *c == '*')
}

/// Folders (relative to `root`, forward slashes, sorted) that hold a `package.json` and are selected
/// by the root's workspace globs. Empty for a project that is not a workspace.
pub fn members(root: &Path) -> Vec<String> {
    let mut patterns = std::fs::read_to_string(root.join("package.json"))
        .map(|t| package_json_patterns(&t))
        .unwrap_or_default();
    if let Ok(yaml) = std::fs::read_to_string(root.join("pnpm-workspace.yaml")) {
        patterns.extend(pnpm_patterns(&yaml));
    }
    let (excluded, included): (Vec<_>, Vec<_>) = patterns.iter().partition(|p| p.starts_with('!'));
    if included.is_empty() {
        return Vec::new();
    }
    let depth = if included.iter().any(|p| p.contains("**")) {
        6
    } else {
        included.iter().map(|p| p.split('/').filter(|s| !s.is_empty()).count()).max().unwrap_or(1)
    };

    let mut found = Vec::new();
    scan(root, "", depth, &mut |rel| {
        let selected = included.iter().any(|p| glob_matches(p, rel))
            && !excluded.iter().any(|p| glob_matches(&p[1..], rel));
        if selected && root.join(rel).join("package.json").is_file() {
            found.push(rel.to_string());
        }
    });
    found.sort();
    found
}

fn scan(root: &Path, rel: &str, depth: usize, visit: &mut impl FnMut(&str)) {
    if depth == 0 {
        return;
    }
    let Ok(entries) = std::fs::read_dir(root.join(rel)) else { return };
    let mut dirs: Vec<String> = entries
        .flatten()
        .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
        .filter_map(|e| e.file_name().into_string().ok())
        .filter(|n| !n.starts_with('.') && !SKIPPED.contains(&n.as_str()))
        .collect();
    dirs.sort();
    for name in dirs {
        let child = if rel.is_empty() { name } else { format!("{rel}/{name}") };
        visit(&child);
        scan(root, &child, depth - 1, visit);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn globs_match_by_segment() {
        assert!(glob_matches("packages/*", "packages/a"));
        assert!(!glob_matches("packages/*", "packages/a/b"));
        assert!(glob_matches("packages/**", "packages/a/b"));
        assert!(glob_matches("packages/**", "packages"));
        assert!(glob_matches("apps/web-*", "apps/web-admin"));
        assert!(!glob_matches("apps/web-*", "apps/api"));
        assert!(glob_matches("./tools/cli/", "tools/cli"));
        assert!(glob_matches("libs/*/package.json", "libs/x"));
    }

    #[test]
    fn npm_and_yarn_declare_workspaces_two_ways() {
        assert_eq!(package_json_patterns(r#"{ "workspaces": ["a/*", "b"] }"#), ["a/*", "b"]);
        assert_eq!(package_json_patterns(r#"{ "workspaces": { "packages": ["c/*"], "nohoist": [] } }"#), ["c/*"]);
        assert!(package_json_patterns(r#"{ "name": "x" }"#).is_empty());
        assert!(package_json_patterns("{ broken").is_empty());
    }

    #[test]
    fn pnpm_lists_packages_under_one_key() {
        let yaml = "# monorepo\npackages:\n  - 'packages/*'\n  - \"apps/**\"\n  - '!**/test/**'\n\ncatalog:\n  react: ^18\n";
        assert_eq!(pnpm_patterns(yaml), ["packages/*", "apps/**", "!**/test/**"]);
    }

    #[test]
    fn members_are_the_selected_folders_that_have_a_manifest() {
        let dir = std::env::temp_dir().join(format!("bennu-npm-ws-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        for d in ["packages/a", "packages/b", "packages/docs", "tools/x", "node_modules/pkg"] {
            std::fs::create_dir_all(dir.join(d)).unwrap();
        }
        for d in ["packages/a", "packages/b", "tools/x", "node_modules/pkg"] {
            std::fs::write(dir.join(d).join("package.json"), "{}").unwrap();
        }
        std::fs::write(dir.join("package.json"), r#"{ "workspaces": ["packages/*", "!packages/b", "node_modules/*"] }"#).unwrap();

        assert_eq!(members(&dir), ["packages/a"], "b is excluded, docs has no manifest, node_modules is never entered");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
