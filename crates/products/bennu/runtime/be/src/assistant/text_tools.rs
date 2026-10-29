//! The text tools the assistant gets instead of a shell: search, find, and an exact replace.
//!
//! `grep`, `find` and `sed` are what a model reaches for when it can run commands — and each is a door
//! to everything else a shell can do. These are the three things it actually wanted from them, as typed
//! functions with bounded output: they cannot write, cannot leave the folder they are given, and cannot
//! return more than a fixed number of lines, so a pattern that matches everything costs a sentence
//! rather than the context window.
//!
//! The edit is the one place text is changed, and it is deliberately the strict one: the text to
//! replace must exist, and exist **once**, unless the caller says how many there should be. A
//! replace that silently matched nothing — or the wrong one of three — is how an agent reports success
//! on an edit that never happened.

use std::fs;
use std::path::{Path, PathBuf};

use regex::RegexBuilder;
use serde::Deserialize;

/// Folders no search enters: version control, dependencies, tool caches and the output of frameworks'
/// builds. Matching inside them is never what anyone asked for, and a `node_modules/` can be larger than the project.
pub(super) const SKIPPED_DIRS: &[&str] = &[
    ".git", ".hg", ".svn", "node_modules", ".idea", ".gradle", ".angular", ".svelte-kit", ".next", ".nuxt", ".output", ".turbo",
    ".cache", ".parcel-cache", ".vite", ".venv", "__pycache__", ".yarn", ".pnpm-store", "coverage",
];

/// Folders that are build output only when a project manifest sits beside them. `build` or `out` with no
/// `pom.xml`, `Cargo.toml` or `package.json` next to it is a source folder with an unlucky name — this
/// repository's own `crates/products/bennu/build/` is one — and skipping it would hide real code.
const OUTPUT_DIRS: &[&str] = &["target", "build", "out", "dist"];
const MANIFESTS: &[&str] = &["pom.xml", "build.gradle", "build.gradle.kts", "Cargo.toml", "package.json", "tsconfig.json"];

pub(super) fn is_output_dir(parent: &Path, name: &str) -> bool {
    OUTPUT_DIRS.contains(&name) && MANIFESTS.iter().any(|m| parent.join(m).is_file())
}

/// A file larger than this is not searched: it is a data file or a log, and reading it would be the
/// whole cost of the call.
const MAX_FILE_BYTES: u64 = 2 * 1024 * 1024;

/// The most lines one search answers with, whatever was asked.
const HARD_LIMIT: usize = 500;

#[derive(Debug, Deserialize)]
pub struct SearchArgs {
    /// The text or regular expression to look for.
    pub pattern: String,
    /// A folder to search in, absolute or relative to the project. Defaults to the project root.
    #[serde(default)]
    pub path: Option<String>,
    /// Treat `pattern` as a regular expression (otherwise it is plain text).
    #[serde(default)]
    pub regex: bool,
    #[serde(default)]
    pub ignore_case: bool,
    /// Only files with one of these extensions, without the dot: `["java", "xml"]`.
    #[serde(default)]
    pub extensions: Vec<String>,
    /// Lines of context around each match. 0–5.
    #[serde(default)]
    pub context: usize,
    /// At most this many matching lines. Default 100.
    #[serde(default)]
    pub max_results: Option<usize>,
    /// Report only the files that match and how many times, not the lines.
    #[serde(default)]
    pub files_only: bool,
}

#[derive(Debug, Deserialize)]
pub struct FindArgs {
    /// Part of the file name, or a `*.ext` suffix pattern.
    pub name: String,
    #[serde(default)]
    pub path: Option<String>,
    #[serde(default)]
    pub max_results: Option<usize>,
}

fn walk(dir: &Path, out: &mut Vec<PathBuf>, keep: &dyn Fn(&Path) -> bool, cap: usize) {
    let Ok(read) = fs::read_dir(dir) else { return };
    let mut entries: Vec<_> = read.flatten().collect();
    entries.sort_by_key(|e| e.file_name());
    for entry in entries {
        if out.len() >= cap {
            return;
        }
        let path = entry.path();
        let Ok(kind) = entry.file_type() else { continue };
        let name = entry.file_name().to_string_lossy().to_string();
        if kind.is_dir() {
            if !SKIPPED_DIRS.contains(&name.as_str()) && !is_output_dir(dir, &name) {
                walk(&path, out, keep, cap);
            }
        } else if kind.is_file() && keep(&path) {
            out.push(path);
        }
    }
}

fn relative(root: &Path, file: &Path) -> String {
    file.strip_prefix(root).unwrap_or(file).to_string_lossy().replace('\\', "/")
}

fn looks_binary(bytes: &[u8]) -> bool {
    bytes.iter().take(8192).any(|b| *b == 0)
}

fn has_extension(path: &Path, wanted: &[String]) -> bool {
    wanted.is_empty()
        || path
            .extension()
            .map(|e| wanted.iter().any(|w| w.trim_start_matches('.').eq_ignore_ascii_case(&e.to_string_lossy())))
            .unwrap_or(false)
}

/// Search `base` (a folder, or a single file) for `args.pattern`. `root` only shortens the paths shown.
pub fn search(root: &Path, base: &Path, args: &SearchArgs) -> Result<String, String> {
    if args.pattern.is_empty() {
        return Err("the pattern is empty".into());
    }
    let source = if args.regex { args.pattern.clone() } else { regex::escape(&args.pattern) };
    let re = RegexBuilder::new(&source)
        .case_insensitive(args.ignore_case)
        .size_limit(1 << 22)
        .build()
        .map_err(|e| format!("not a valid regular expression: {e}"))?;

    let limit = args.max_results.unwrap_or(100).clamp(1, HARD_LIMIT);
    let context = args.context.min(5);

    let mut files = Vec::new();
    if base.is_file() {
        files.push(base.to_path_buf());
    } else {
        walk(base, &mut files, &|p| has_extension(p, &args.extensions), 20_000);
    }

    let mut lines_out: Vec<String> = Vec::new();
    let mut counts: Vec<(String, usize)> = Vec::new();
    let mut matched = 0usize;
    let mut truncated = false;

    'files: for file in &files {
        let Ok(meta) = fs::metadata(file) else { continue };
        if meta.len() > MAX_FILE_BYTES {
            continue;
        }
        let Ok(bytes) = fs::read(file) else { continue };
        if looks_binary(&bytes) {
            continue;
        }
        let text = String::from_utf8_lossy(&bytes);
        let all: Vec<&str> = text.lines().collect();
        let shown = relative(root, file);
        let mut in_file = 0usize;
        let mut printed_until = 0usize;

        for (i, line) in all.iter().enumerate() {
            if !re.is_match(line) {
                continue;
            }
            in_file += 1;
            matched += 1;
            if args.files_only {
                continue;
            }
            if matched > limit {
                truncated = true;
                break 'files;
            }
            let from = i.saturating_sub(context).max(printed_until);
            let to = (i + context).min(all.len().saturating_sub(1));
            if from > printed_until && printed_until > 0 {
                lines_out.push("--".into());
            }
            for (n, text) in all.iter().enumerate().take(to + 1).skip(from) {
                let mark = if n == i { ':' } else { '-' };
                lines_out.push(format!("{shown}{mark}{}{mark} {text}", n + 1));
            }
            printed_until = to + 1;
        }
        if in_file > 0 {
            counts.push((shown, in_file));
        }
    }

    if matched == 0 {
        return Ok(format!("No matches in {} files.", files.len()));
    }
    if args.files_only {
        let list: Vec<String> = counts.iter().take(limit).map(|(f, n)| format!("{f} ({n})")).collect();
        return Ok(format!("{matched} matches in {} files:\n{}", counts.len(), list.join("\n")));
    }
    let mut out = lines_out.join("\n");
    if truncated {
        out.push_str(&format!("\n… stopped at {limit} matches; narrow the pattern, the folder or the extensions."));
    } else {
        out.push_str(&format!("\n{matched} matches in {} files.", counts.len()));
    }
    Ok(out)
}

/// Files whose name contains `args.name`, or ends with it when it starts with `*`.
pub fn find(root: &Path, base: &Path, args: &FindArgs) -> Result<String, String> {
    if args.name.is_empty() {
        return Err("the name is empty".into());
    }
    let limit = args.max_results.unwrap_or(100).clamp(1, HARD_LIMIT);
    let needle = args.name.to_lowercase();
    let suffix = needle.strip_prefix('*').map(str::to_string);
    let keep = move |p: &Path| {
        let name = p.file_name().map(|n| n.to_string_lossy().to_lowercase()).unwrap_or_default();
        match &suffix {
            Some(s) => name.ends_with(s.as_str()),
            None => name.contains(&needle),
        }
    };
    let mut files = Vec::new();
    walk(base, &mut files, &keep, limit + 1);
    if files.is_empty() {
        return Ok("No files match.".into());
    }
    let truncated = files.len() > limit;
    files.truncate(limit);
    let mut out: Vec<String> = files.iter().map(|f| relative(root, f)).collect();
    if truncated {
        out.push(format!("… stopped at {limit}; narrow the name or the folder."));
    }
    Ok(out.join("\n"))
}

#[derive(Debug, Deserialize)]
pub struct ReplaceArgs {
    /// The text or regular expression to replace.
    pub pattern: String,
    /// What goes in its place. With `regex`, `$1` and `${name}` stand for the pattern's groups.
    pub replacement: String,
    #[serde(default)]
    pub path: Option<String>,
    #[serde(default)]
    pub regex: bool,
    #[serde(default)]
    pub ignore_case: bool,
    #[serde(default)]
    pub extensions: Vec<String>,
    /// Count only: say which files would change and how many times, and change nothing.
    #[serde(default)]
    pub dry_run: bool,
    /// Refuse to touch more files than this (default 100): a pattern that matches half the project is
    /// more likely a mistake than a refactoring.
    #[serde(default)]
    pub max_files: Option<usize>,
}

/// The matcher a [`ReplaceArgs`] describes.
pub fn matcher(args: &ReplaceArgs) -> Result<regex::Regex, String> {
    if args.pattern.is_empty() {
        return Err("the pattern is empty".into());
    }
    let source = if args.regex { args.pattern.clone() } else { regex::escape(&args.pattern) };
    RegexBuilder::new(&source)
        .case_insensitive(args.ignore_case)
        .size_limit(1 << 22)
        .build()
        .map_err(|e| format!("not a valid regular expression: {e}"))
}

/// The files under `base` (or `base` itself) whose text contains a match, in path order. A cheap read
/// of the bytes decides: the caller reads each survivor properly, with its encoding and its stamp.
pub fn files_matching(base: &Path, re: &regex::Regex, extensions: &[String]) -> Vec<PathBuf> {
    let mut files = Vec::new();
    if base.is_file() {
        files.push(base.to_path_buf());
    } else {
        walk(base, &mut files, &|p| has_extension(p, extensions), 20_000);
    }
    files
        .into_iter()
        .filter(|f| {
            let Ok(meta) = fs::metadata(f) else { return false };
            if meta.len() > MAX_FILE_BYTES {
                return false;
            }
            let Ok(bytes) = fs::read(f) else { return false };
            !looks_binary(&bytes) && re.is_match(&String::from_utf8_lossy(&bytes))
        })
        .collect()
}

/// `text` with every match replaced, and how many there were. A plain-text pattern inserts the
/// replacement as it is; a regular expression expands its groups.
pub fn replace_matches(re: &regex::Regex, text: &str, replacement: &str, regex_mode: bool) -> (String, usize) {
    let count = re.find_iter(text).count();
    if count == 0 {
        return (text.to_string(), 0);
    }
    let out = if regex_mode {
        re.replace_all(text, replacement).into_owned()
    } else {
        re.replace_all(text, regex::NoExpand(replacement)).into_owned()
    };
    (out, count)
}

/// Replace `old` with `new` in `text`. `old` must be there; and, unless `expected` says otherwise,
/// there must be exactly one — an ambiguous edit is refused with the count so the caller can add
/// context, not applied to whichever match came first.
pub fn replace(text: &str, old: &str, new: &str, expected: Option<usize>) -> Result<String, String> {
    if old.is_empty() {
        return Err("the text to replace is empty".into());
    }
    if old == new {
        return Err("the new text is the same as the old".into());
    }
    let found = text.matches(old).count();
    if found == 0 {
        return Err("the text to replace was not found in the file — read it again; it may have changed, or whitespace differs".into());
    }
    let want = expected.unwrap_or(1);
    if found != want {
        return Err(format!(
            "the text appears {found} times but the edit expects {want}; include more surrounding lines so it is unique, or pass `expected_count` to replace them all"
        ));
    }
    Ok(text.replace(old, new))
}

/// One replacement inside a file, as a refactoring plan states it: a byte range, what is there now, and
/// what goes there.
#[derive(Debug, Clone)]
pub struct Span {
    pub start: usize,
    pub end: usize,
    pub old: String,
    pub new_text: String,
}

/// The byte offset of `line`:`column` (both 1-based, the column counted in characters).
pub fn byte_offset(text: &str, line: usize, column: usize) -> Result<usize, String> {
    if line == 0 {
        return Err("lines start at 1".into());
    }
    let mut start = 0usize;
    for (i, raw) in text.split_inclusive('\n').enumerate() {
        if i + 1 == line {
            let body = raw.trim_end_matches(['\n', '\r']);
            let col = column.max(1) - 1;
            if col > body.chars().count() {
                return Err(format!("line {line} has only {} characters", body.chars().count()));
            }
            let within = body.char_indices().nth(col).map(|(b, _)| b).unwrap_or(body.len());
            return Ok(start + within);
        }
        start += raw.len();
    }
    Err(format!("the file has fewer than {line} lines"))
}

/// Apply a plan's replacements to `text`. Refused as a whole — never half-applied — when any span is
/// out of range, splits a character, overlaps another, or no longer matches what the plan saw there
/// (the file changed after it was made).
pub fn apply_spans(text: &str, spans: &[Span]) -> Result<String, String> {
    let mut sorted: Vec<&Span> = spans.iter().collect();
    sorted.sort_by_key(|s| s.start);
    let mut previous_end = 0usize;
    for s in &sorted {
        if s.start > s.end || s.end > text.len() || !text.is_char_boundary(s.start) || !text.is_char_boundary(s.end) {
            return Err(format!("an edit at bytes {}..{} does not fit the file — it changed after the plan was made", s.start, s.end));
        }
        if s.start < previous_end {
            return Err("two edits of the plan overlap".into());
        }
        if text[s.start..s.end] != s.old {
            return Err(format!("the text at bytes {}..{} is no longer `{}` — the file changed after the plan was made", s.start, s.end, s.old));
        }
        previous_end = s.end;
    }
    let mut out = String::with_capacity(text.len());
    let mut at = 0usize;
    for s in sorted {
        out.push_str(&text[at..s.start]);
        out.push_str(&s.new_text);
        at = s.end;
    }
    out.push_str(&text[at..]);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn project(name: &str, files: &[(&str, &str)]) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("bennu-text-tools-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        for (path, body) in files {
            let full = dir.join(path);
            fs::create_dir_all(full.parent().unwrap()).unwrap();
            fs::write(full, body).unwrap();
        }
        dir
    }

    fn args(pattern: &str) -> SearchArgs {
        SearchArgs { pattern: pattern.into(), path: None, regex: false, ignore_case: false, extensions: vec![], context: 0, max_results: None, files_only: false }
    }

    #[test]
    fn a_plain_search_finds_the_line_with_its_number() {
        let root = project("plain", &[("src/A.java", "class A {\n  int total;\n}\n")]);
        let out = search(&root, &root, &args("total")).unwrap();
        assert!(out.contains("src/A.java:2: "), "{out}");
        assert!(out.contains("1 matches in 1 files"));
    }

    #[test]
    fn plain_text_is_not_read_as_a_pattern() {
        let root = project("escape", &[("a.txt", "price (EUR) [x]\nprice EUR\n")]);
        let out = search(&root, &root, &args("(EUR) [x]")).unwrap();
        assert!(out.contains("a.txt:1:") && !out.contains("a.txt:2:"), "{out}");
    }

    #[test]
    fn a_regex_and_case_folding_work_when_asked() {
        let root = project("regex", &[("a.txt", "Total\ntotals\nsum\n")]);
        let mut a = args(r"^total\w*$");
        a.regex = true;
        a.ignore_case = true;
        let out = search(&root, &root, &a).unwrap();
        assert!(out.contains("a.txt:1:") && out.contains("a.txt:2:") && !out.contains("a.txt:3:"), "{out}");
        a.pattern = "(".into();
        assert!(search(&root, &root, &a).unwrap_err().contains("not a valid regular expression"));
    }

    #[test]
    fn context_lines_are_marked_apart_from_matches() {
        let root = project("ctx", &[("a.txt", "one\ntwo\nthree\nfour\n")]);
        let mut a = args("three");
        a.context = 1;
        let out = search(&root, &root, &a).unwrap();
        assert!(out.contains("a.txt-2- two") && out.contains("a.txt:3: three") && out.contains("a.txt-4- four"), "{out}");
    }

    #[test]
    fn build_output_and_binaries_are_not_searched() {
        let root = project("skip", &[("pom.xml", ""), ("src/A.java", "needle\n"), ("target/B.java", "needle\n"), ("node_modules/x.js", "needle\n")]);
        fs::write(root.join("blob.bin"), b"needle\0\0\0").unwrap();
        let out = search(&root, &root, &args("needle")).unwrap();
        assert!(out.contains("src/A.java") && !out.contains("target") && !out.contains("node_modules") && !out.contains("blob.bin"), "{out}");
    }

    #[test]
    fn a_source_folder_called_build_is_searched_unless_a_manifest_sits_beside_it() {
        let lone = project("lone-build", &[("src/build/Step.java", "needle\n")]);
        assert!(search(&lone, &lone, &args("needle")).unwrap().contains("src/build/Step.java"));
        let built = project("real-build", &[("package.json", "{}"), ("dist/app.js", "needle\n"), (".angular/cache/x.js", "needle\n"), ("src/main.ts", "needle\n")]);
        let out = search(&built, &built, &args("needle")).unwrap();
        assert!(out.contains("src/main.ts") && !out.contains("dist") && !out.contains(".angular"), "{out}");
    }

    #[test]
    fn extensions_narrow_the_files() {
        let root = project("ext", &[("a.java", "x\n"), ("b.xml", "x\n")]);
        let mut a = args("x");
        a.extensions = vec!["xml".into()];
        let out = search(&root, &root, &a).unwrap();
        assert!(out.contains("b.xml") && !out.contains("a.java"), "{out}");
    }

    #[test]
    fn a_pattern_that_matches_everything_is_cut_off_and_says_so() {
        let body = "x\n".repeat(300);
        let root = project("limit", &[("a.txt", body.as_str())]);
        let mut a = args("x");
        a.max_results = Some(10);
        let out = search(&root, &root, &a).unwrap();
        assert_eq!(out.lines().filter(|l| l.starts_with("a.txt:")).count(), 10);
        assert!(out.contains("stopped at 10 matches"), "{out}");
    }

    #[test]
    fn files_only_counts_per_file() {
        let root = project("files", &[("a.txt", "x\nx\n"), ("b.txt", "x\n")]);
        let mut a = args("x");
        a.files_only = true;
        let out = search(&root, &root, &a).unwrap();
        assert!(out.contains("3 matches in 2 files") && out.contains("a.txt (2)"), "{out}");
    }

    #[test]
    fn find_matches_a_name_part_or_a_suffix() {
        let root = project("find", &[("src/OrderService.java", ""), ("src/OrderTest.java", ""), ("pom.xml", "")]);
        let by_part = find(&root, &root, &FindArgs { name: "order".into(), path: None, max_results: None }).unwrap();
        assert!(by_part.contains("OrderService.java") && by_part.contains("OrderTest.java") && !by_part.contains("pom"));
        let by_suffix = find(&root, &root, &FindArgs { name: "*.xml".into(), path: None, max_results: None }).unwrap();
        assert_eq!(by_suffix, "pom.xml");
    }

    #[test]
    fn a_replace_needs_the_text_to_be_there_once() {
        assert_eq!(replace("a b c", "b", "X", None).unwrap(), "a X c");
        assert!(replace("a b c", "z", "X", None).unwrap_err().contains("not found"));
        let ambiguous = replace("b b", "b", "X", None).unwrap_err();
        assert!(ambiguous.contains("2 times"), "{ambiguous}");
        assert_eq!(replace("b b", "b", "X", Some(2)).unwrap(), "X X");
    }

    #[test]
    fn a_position_becomes_a_byte_offset_in_characters() {
        let t = "ab\nèx = 1;\r\nlast";
        assert_eq!(byte_offset(t, 1, 1).unwrap(), 0);
        assert_eq!(byte_offset(t, 2, 2).unwrap(), 3 + 2, "after the two-byte è");
        assert_eq!(byte_offset(t, 3, 1).unwrap(), t.find("last").unwrap());
        assert_eq!(byte_offset(t, 1, 3).unwrap(), 2, "just past the last character is allowed");
        assert!(byte_offset(t, 1, 4).is_err() && byte_offset(t, 9, 1).is_err() && byte_offset(t, 0, 1).is_err());
    }

    fn span(start: usize, end: usize, old: &str, new: &str) -> Span {
        Span { start, end, old: old.into(), new_text: new.into() }
    }

    #[test]
    fn a_plan_is_applied_back_to_front_without_shifting_itself() {
        let t = "total + total";
        let out = apply_spans(t, &[span(8, 13, "total", "amountDue"), span(0, 5, "total", "amountDue")]).unwrap();
        assert_eq!(out, "amountDue + amountDue");
    }

    #[test]
    fn a_stale_plan_is_refused_whole() {
        let t = "total + total";
        assert!(apply_spans(t, &[span(0, 5, "other", "x")]).unwrap_err().contains("changed after the plan"));
        assert!(apply_spans(t, &[span(0, 99, "total", "x")]).is_err());
        assert!(apply_spans(t, &[span(0, 5, "total", "x"), span(3, 8, "al + ", "y")]).unwrap_err().contains("overlap"));
        assert!(apply_spans("è", &[span(0, 1, "", "x")]).is_err(), "inside a character");
    }

    #[test]
    fn an_insertion_is_a_span_that_replaces_nothing() {
        assert_eq!(apply_spans("f()", &[span(2, 2, "", "int a")]).unwrap(), "f(int a)");
    }

    fn replace_args(pattern: &str, replacement: &str) -> ReplaceArgs {
        ReplaceArgs { pattern: pattern.into(), replacement: replacement.into(), path: None, regex: false, ignore_case: false, extensions: vec![], dry_run: false, max_files: None }
    }

    #[test]
    fn a_tree_replace_finds_only_the_files_that_match() {
        let root = project("tree", &[("pom.xml", "<project/>
"), ("src/A.java", "old()\n"), ("src/B.java", "fine\n"), ("target/C.java", "old()\n")]);
        let args = replace_args("old()", "fresh()");
        let re = matcher(&args).unwrap();
        let files = files_matching(&root, &re, &[]);
        assert_eq!(files.len(), 1);
        assert!(files[0].ends_with("A.java"));
    }

    #[test]
    fn plain_text_replacement_is_inserted_as_written_and_a_regex_expands_its_groups() {
        let plain = replace_args("a.b", "$1");
        assert_eq!(replace_matches(&matcher(&plain).unwrap(), "a.b axb", "$1", false), ("$1 axb".to_string(), 1));
        let mut rx = replace_args(r"get(\w+)\(\)", "load$1()");
        rx.regex = true;
        assert_eq!(replace_matches(&matcher(&rx).unwrap(), "getName() getAge()", "load$1()", true), ("loadName() loadAge()".to_string(), 2));
    }

    #[test]
    fn an_empty_or_broken_pattern_is_refused_before_any_file_is_read() {
        assert!(matcher(&replace_args("", "x")).is_err());
        let mut bad = replace_args("(", "x");
        bad.regex = true;
        assert!(matcher(&bad).unwrap_err().contains("not a valid regular expression"));
    }

    #[test]
    fn a_replace_that_changes_nothing_or_names_nothing_is_refused() {
        assert!(replace("abc", "", "x", None).is_err());
        assert!(replace("abc", "a", "a", None).is_err());
    }
}
