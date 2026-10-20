//! `env_file` domain — `bennu_load_env_file`: the variables of a `.env` file, read at launch.
//!
//! A run configuration may point at a `.env` file instead of (or besides) listing its variables.
//! The file is **read every time the configuration runs**, never copied into it: the file is the
//! source of truth, so editing it — or `git pull`ing a new one — is enough, and a secret that lives
//! in an ignored `.env` never lands in the committed run configuration.
//!
//! The dialect is the common subset every dotenv implementation agrees on: `KEY=value`, an optional
//! `export ` prefix, `#` comments, single quotes (literal), double quotes (with `\n`, `\t`, `\"`,
//! `\\` escapes) and an unquoted value ending at a ` #`. No `${VAR}` expansion — that is shell
//! behaviour, and a launcher that quietly expanded would disagree with the shell that runs the same
//! file elsewhere.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use bennu_core::prelude::BennuState;
use serde::Deserialize;

/// Args for [`bennu_load_env_file`].
#[derive(Deserialize)]
pub struct LoadEnvFileArgs {
    /// The project root, against which a relative `path` is resolved.
    pub root: String,
    /// The `.env` file — absolute, or relative to the root.
    pub path: String,
}

/// Read and parse the `.env` file at `path`. Errors name the file, so a typo in the configuration
/// is a sentence in the Run console and not a variable that is silently absent.
#[arbor_rpc::handler]
fn bennu_load_env_file(
    _ctx: &BennuState,
    args: LoadEnvFileArgs,
) -> Result<BTreeMap<String, String>, String> {
    let file = resolve(&args.root, &args.path);
    let text = std::fs::read_to_string(&file)
        .map_err(|e| format!("Could not read the env file {}: {e}", file.display()))?;
    Ok(parse(&text))
}

fn resolve(root: &str, path: &str) -> PathBuf {
    let p = Path::new(path.trim());
    if p.is_absolute() { p.to_path_buf() } else { Path::new(root).join(p) }
}

/// Parse dotenv text. Malformed lines (no `=`, empty key) are skipped: one bad line should not
/// cost the other forty variables.
pub fn parse(text: &str) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let line = line.strip_prefix("export ").map(str::trim_start).unwrap_or(line);
        let Some((key, rest)) = line.split_once('=') else { continue };
        let key = key.trim();
        if key.is_empty() || key.contains(char::is_whitespace) {
            continue;
        }
        out.insert(key.to_string(), value_of(rest.trim_start()));
    }
    out
}

fn value_of(rest: &str) -> String {
    if let Some(inner) = rest.strip_prefix('"') {
        return unescape_double(inner);
    }
    if let Some(inner) = rest.strip_prefix('\'') {
        return inner.split('\'').next().unwrap_or("").to_string();
    }
    // Unquoted: a ` #` starts a trailing comment, a bare `#` inside the value (a URL fragment, a
    // password) does not.
    let end = rest.find(" #").unwrap_or(rest.len());
    rest[..end].trim_end().to_string()
}

fn unescape_double(inner: &str) -> String {
    let mut out = String::new();
    let mut chars = inner.chars();
    while let Some(c) = chars.next() {
        match c {
            '"' => break,
            '\\' => match chars.next() {
                Some('n') => out.push('\n'),
                Some('t') => out.push('\t'),
                Some(other) => out.push(other),
                None => out.push('\\'),
            },
            other => out.push(other),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_pairs_comments_and_export_prefix() {
        let env = parse("# a comment\nA=1\n\nexport B = two\nbroken line\n=nokey\n");
        assert_eq!(env.get("A").map(String::as_str), Some("1"));
        assert_eq!(env.get("B").map(String::as_str), Some("two"));
        assert_eq!(env.len(), 2);
    }

    #[test]
    fn quotes_decide_what_a_hash_means() {
        let env = parse("A=\"x # y\\nz\"\nB='lit # eral'\nC=value # trailing\nD=pa#ss\n");
        assert_eq!(env["A"], "x # y\nz");
        assert_eq!(env["B"], "lit # eral");
        assert_eq!(env["C"], "value");
        assert_eq!(env["D"], "pa#ss");
    }

    #[test]
    fn a_value_may_itself_contain_equals() {
        assert_eq!(parse("URL=jdbc:pg://h/db?a=b&c=d")["URL"], "jdbc:pg://h/db?a=b&c=d");
    }

    #[test]
    fn a_relative_path_hangs_off_the_root() {
        assert_eq!(resolve("/p", ".env.local"), Path::new("/p").join(".env.local"));
    }
}
