//! What is a Rust test, and where does it sit in the build.
//!
//! The Maven half of this crate ([`crate::discover`]) answers one question — which classes hold
//! tests — because Surefire's own model is "a class is a suite". Cargo's is different in a way
//! that shapes the whole panel: a test's identity is **package + target + module path + fn**, and
//! all four are needed to run it. `cargo test util::tests::works` in a twenty-crate workspace
//! runs whatever matches in every crate.
//!
//! So discovery here is two independent problems, and they are two functions:
//!
//! 1. **[`place_of`] — where does this file compile to?** Pure path logic. `src/util/parse.rs`
//!    is module `util::parse` of the lib; `tests/api.rs` is the *crate root* of an integration
//!    target named `api`. Cargo infers targets from a directory layout, so this is derivable
//!    without reading a manifest — with one fact the path cannot carry, which the caller supplies:
//!    whether the package has a `src/lib.rs` at all.
//! 2. **[`discover_rust_in_source`] — which functions are tests?** A text scan for attributes.
//!
//! ## Why a scan and not a parse
//!
//! The Java side runs tree-sitter, because deciding whether a method is a test there needs the
//! class it is in (a JUnit 3 test is a `testXxx()` in a `TestCase` subclass). Rust needs no such
//! context: an attribute directly above a `fn` is the whole rule. What the scan does have to get
//! right is the **module path**, which means tracking `mod x { … }` nesting — so braces inside
//! strings, comments and raw strings are blanked before anything is counted. That is the entire
//! subtlety, and it is what the tests below pin.
//!
//! ## What counts as a test attribute
//!
//! Any attribute whose path **ends in `test`** — `#[test]`, `#[tokio::test]`, `#[sqlx::test]`,
//! and the `#[foo::test]` of a runtime nobody has written yet. A closed list would have to grow
//! every time an async runtime appears, and the failure mode of a missing entry is a test the
//! panel does not know about — invisible, and indistinguishable from a test that passed.

use serde::{Deserialize, Serialize};

/// Which target of a package a source file compiles into.
///
/// This is the unit `cargo test` selects with a flag (`--lib`, `--bin foo`, `--test api`), and
/// the level the panel groups by under the crate — because it is also the unit that *runs*: one
/// target is one binary, with its own `running N tests` block and its own summary.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TestTarget {
    /// The package's library — `src/lib.rs` and every module under `src/`.
    Lib,
    /// A binary: `src/main.rs` (named after the package) or `src/bin/<name>.rs`.
    Bin { name: String },
    /// An integration test: `tests/<name>.rs`, its own crate, compiled against the lib.
    Test { name: String },
    /// A benchmark: `benches/<name>.rs`.
    Bench { name: String },
    /// An example: `examples/<name>.rs`. `cargo test --example x` compiles and runs it.
    Example { name: String },
    /// A documentation test. Never discovered from a scan — only ever *reported* by a run, since
    /// what makes a doc test is a fenced block inside a comment.
    Doc,
}

impl TestTarget {
    /// The `cargo test` flags that narrow a run to this target.
    ///
    /// `--lib` takes no name (a package has at most one library); the rest are named. `Doc` is
    /// `--doc`, which is the one target selectable but not discoverable.
    pub fn selector_args(&self) -> Vec<String> {
        match self {
            Self::Lib => vec!["--lib".to_string()],
            Self::Doc => vec!["--doc".to_string()],
            Self::Bin { name } => vec!["--bin".to_string(), name.clone()],
            Self::Test { name } => vec!["--test".to_string(), name.clone()],
            Self::Bench { name } => vec!["--bench".to_string(), name.clone()],
            Self::Example { name } => vec!["--example".to_string(), name.clone()],
        }
    }

    /// How the target reads in the tree (`lib`, `bin cli`, `test api`).
    pub fn label(&self) -> String {
        match self {
            Self::Lib => "lib".to_string(),
            Self::Doc => "doc-tests".to_string(),
            Self::Bin { name } => format!("bin {name}"),
            Self::Test { name } => format!("test {name}"),
            Self::Bench { name } => format!("bench {name}"),
            Self::Example { name } => format!("example {name}"),
        }
    }

    /// A stable id for the target within its package — the tree's node key.
    pub fn id(&self) -> String {
        match self {
            Self::Lib => "lib".to_string(),
            Self::Doc => "doc".to_string(),
            Self::Bin { name } => format!("bin:{name}"),
            Self::Test { name } => format!("test:{name}"),
            Self::Bench { name } => format!("bench:{name}"),
            Self::Example { name } => format!("example:{name}"),
        }
    }
}

/// Where a file sits in the build: which package, which target, and the module path of the file
/// itself inside that target.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FilePlace {
    pub package: String,
    pub target: TestTarget,
    /// `::`-separated module path of the file, empty for a crate root (`src/lib.rs`,
    /// `tests/api.rs`). Inline `mod` blocks extend it per test.
    pub module: String,
}

/// How a test function is written — which decides how many libtest cases it produces and how one
/// of them can be named.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RustTestKind {
    /// `#[test]` — one function, one case.
    Test,
    /// `#[tokio::test]` and relatives. Still one case; worth distinguishing because it needs a
    /// runtime, which is the usual reason one of these fails where the sync ones pass.
    Async,
    /// `#[bench]` — run by `cargo test` (as a smoke test) and by `cargo bench` (for real).
    Bench,
    /// `#[rstest]` / `#[test_case(…)]` — **one function, many cases**. Its libtest paths are
    /// `name::case_1…`, so a run of "just this one" is a prefix filter, not an exact one.
    Parameterized,
}

/// One discovered test function.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RustTest {
    pub package: String,
    pub target: TestTarget,
    /// Module path holding the function, `::`-separated. Empty at a crate root.
    pub module: String,
    /// The function's name.
    pub name: String,
    /// `module::name` — how libtest names the case, and what a filter matches.
    pub path: String,
    /// Absolute path, forward-slashed.
    pub file: String,
    /// 1-based line of the `fn`.
    pub line: u32,
    /// Byte offset of the function's name.
    pub offset: usize,
    pub kind: RustTestKind,
    /// `#[ignore]` — declared and skipped unless asked for by name.
    pub ignored: bool,
    /// `#[should_panic]` — it passes *by* panicking, which is worth seeing in a list where every
    /// other row passes by not.
    pub should_panic: bool,
}

/// Where a file compiles to, from its path relative to the package root.
///
/// `has_lib` is the one thing the layout cannot tell us: in a package with only a `src/main.rs`,
/// `src/util.rs` is a module of the **binary**, and calling it a lib module would produce a
/// `--lib` run that cargo rejects. The caller knows (it found the manifest); this function does
/// not guess.
///
/// `None` for a path cargo does not compile — `build.rs`, anything outside the known roots.
pub fn place_of(rel: &str, package: &str, has_lib: bool) -> Option<FilePlace> {
    let rel = rel.replace('\\', "/");
    let rel = rel.trim_start_matches("./");
    let (root, rest) = rel.split_once('/')?;
    let stem = rest.strip_suffix(".rs")?;

    match root {
        "src" => {
            // `src/bin/x.rs` and `src/bin/x/main.rs` are both binary `x`; anything else under
            // `src/bin/x/` is one of its modules.
            if let Some(inner) = stem.strip_prefix("bin/") {
                let (name, module) = split_crate_root(inner);
                return Some(FilePlace {
                    package: package.to_string(),
                    target: TestTarget::Bin { name },
                    module,
                });
            }
            if stem == "main" {
                return Some(FilePlace {
                    package: package.to_string(),
                    target: TestTarget::Bin { name: package.to_string() },
                    module: String::new(),
                });
            }
            // Every other file under `src/` is a module of the crate root — the lib when there is
            // one, otherwise the package's binary.
            let target = if has_lib {
                TestTarget::Lib
            } else {
                TestTarget::Bin { name: package.to_string() }
            };
            Some(FilePlace {
                package: package.to_string(),
                target,
                module: module_path_of(stem),
            })
        }
        "tests" | "benches" | "examples" => {
            let (name, module) = split_crate_root(stem);
            let target = match root {
                "tests" => TestTarget::Test { name },
                "benches" => TestTarget::Bench { name },
                _ => TestTarget::Example { name },
            };
            Some(FilePlace { package: package.to_string(), target, module })
        }
        _ => None,
    }
}

/// Split `api` / `api/main` / `api/helpers` into the target name and the module path inside it.
///
/// A directory under `tests/` is one target whose root is its `main.rs` (or `mod.rs`), so
/// `tests/api/helpers.rs` is module `helpers` of target `api` — not a target called
/// `api/helpers`, which cargo has no way to build.
fn split_crate_root(stem: &str) -> (String, String) {
    match stem.split_once('/') {
        None => (stem.to_string(), String::new()),
        Some((name, inner)) => (name.to_string(), module_path_of(inner)),
    }
}

/// The module path a file's own path implies: `util/parse` → `util::parse`.
///
/// A trailing `mod`, `lib` or `main` contributes nothing, because those files *are* the module
/// they sit in rather than one below it — `src/util/mod.rs` is `util`, not `util::mod`.
fn module_path_of(stem: &str) -> String {
    let mut segs: Vec<&str> = stem.split('/').filter(|s| !s.is_empty()).collect();
    if segs.last().is_some_and(|s| matches!(*s, "mod" | "lib" | "main")) {
        segs.pop();
    }
    segs.join("::")
}

/// Every test function in one source file.
///
/// The scan is line-oriented over a **blanked** copy of the source (strings, chars, raw strings
/// and comments replaced by spaces), because the only structural thing it counts is braces and a
/// `}` inside a string would close a module that never opened.
pub fn discover_rust_in_source(file: &str, src: &str, place: &FilePlace) -> Vec<RustTest> {
    let mut found = Vec::new();
    // Inline module bodies we are inside: (name, brace depth of the body).
    let mut mods: Vec<(String, i32)> = Vec::new();
    let mut depth: i32 = 0;
    // Attributes seen since the last item, flattened to one lower-cased string per attribute.
    let mut attrs: Vec<String> = Vec::new();
    // Set while an attribute's brackets are still unbalanced (a `#[cfg_attr(…\n…)]`).
    let mut pending_attr: Option<(String, i32)> = None;
    let mut blanker = Blanker::default();

    let mut offset = 0usize;
    for (i, raw) in src.split_inclusive('\n').enumerate() {
        let line_no = (i + 1) as u32;
        let code = blanker.blank(raw);
        let trimmed = code.trim();

        // ── a continuing attribute ────────────────────────────────────────────────
        if let Some((mut text, mut open)) = pending_attr.take() {
            text.push(' ');
            text.push_str(trimmed);
            open += bracket_balance(trimmed);
            if open > 0 {
                pending_attr = Some((text, open));
            } else {
                attrs.push(text.to_lowercase());
            }
            offset += raw.len();
            continue;
        }

        // ── an attribute ──────────────────────────────────────────────────────────
        if trimmed.starts_with("#[") || trimmed.starts_with("#![") {
            let open = bracket_balance(trimmed);
            if open > 0 {
                pending_attr = Some((trimmed.to_string(), open));
            } else {
                attrs.push(trimmed.to_lowercase());
            }
            offset += raw.len();
            continue;
        }

        // ── an inline module ──────────────────────────────────────────────────────
        // `mod x;` is a *file* module and contributes nothing here: the file it names is scanned
        // on its own, and its path comes from `place_of`.
        if let Some(name) = inline_mod_name(trimmed) {
            mods.push((name, depth + 1));
        }

        // ── a function ────────────────────────────────────────────────────────────
        if let Some((name, at)) = fn_name_at(&code) {
            if let Some(kind) = test_kind(&attrs) {
                let module = join_module(&place.module, &mods);
                let path =
                    if module.is_empty() { name.clone() } else { format!("{module}::{name}") };
                found.push(RustTest {
                    package: place.package.clone(),
                    target: place.target.clone(),
                    module,
                    offset: offset + at,
                    name,
                    path,
                    file: file.to_string(),
                    line: line_no,
                    kind,
                    ignored: attrs.iter().any(|a| a.starts_with("#[ignore")),
                    should_panic: attrs.iter().any(|a| a.starts_with("#[should_panic")),
                });
            }
        }

        // A line of code ends the attribute run. A **blank** line does not: Rust lets one sit
        // between an attribute and its item, and clearing there would lose the marker.
        if !trimmed.is_empty() {
            attrs.clear();
        }

        depth += brace_balance(&code);
        // Leaving a module body: its name stops applying.
        while mods.last().is_some_and(|(_, d)| depth < *d) {
            mods.pop();
        }
        offset += raw.len();
    }
    found
}

/// The module path of a test: the file's own path, then every inline `mod` we are inside.
fn join_module(file_module: &str, mods: &[(String, i32)]) -> String {
    let mut parts: Vec<&str> = Vec::new();
    if !file_module.is_empty() {
        parts.extend(file_module.split("::"));
    }
    parts.extend(mods.iter().map(|(n, _)| n.as_str()));
    parts.join("::")
}

/// Which kind of test these attributes declare, if any.
///
/// The rule is the attribute **path ending in `test`** plus the named exceptions — see the module
/// doc for why it is a suffix and not a list.
fn test_kind(attrs: &[String]) -> Option<RustTestKind> {
    let mut seen: Vec<RustTestKind> = Vec::new();
    for attr in attrs {
        let path = attr_path(attr);
        let leaf = path.rsplit("::").next().unwrap_or(&path);
        let candidate = match path.as_str() {
            "test" | "wasm_bindgen_test" => RustTestKind::Test,
            "bench" => RustTestKind::Bench,
            "rstest" | "test_case" => RustTestKind::Parameterized,
            // `#[tokio::test]`, `#[sqlx::test]`, `#[actix_web::test]`, and the next one.
            _ if leaf == "test" && path.contains("::") => RustTestKind::Async,
            _ => continue,
        };
        seen.push(candidate);
    }
    // Priority, not last-wins: `#[test_case(…)] #[test]` together produce many cases and must not
    // be reported as the one the plain `#[test]` would suggest; `#[tokio::test]` needs a runtime
    // whether or not something else also marked the function.
    for want in
        [RustTestKind::Parameterized, RustTestKind::Async, RustTestKind::Bench, RustTestKind::Test]
    {
        if seen.contains(&want) {
            return Some(want);
        }
    }
    None
}

/// The path of an attribute: `#[tokio::test(flavor = "x")]` → `tokio::test`.
fn attr_path(attr: &str) -> String {
    let body = attr.trim_start_matches("#").trim_start_matches("!").trim_start_matches('[');
    let end = body.find(|c: char| !(c.is_ascii_alphanumeric() || c == '_' || c == ':')).unwrap_or(body.len());
    body[..end].trim_matches(':').to_string()
}

/// The name of an inline module opened on this line (`mod tests {`), if any.
///
/// Only an opening brace counts: `mod x;` declares a **file** module, whose own path comes from
/// [`place_of`] when that file is scanned, and treating it as a body would swallow every
/// following test into a module that is not there.
fn inline_mod_name(line: &str) -> Option<String> {
    let idx = find_word(line, "mod")?;
    // Only a leading item, so a `use foo::mod_utils` or a trailing `mod` inside an expression
    // cannot open one. `pub`, `pub(crate)` and `pub(super)` are the legal prefixes.
    let before = line[..idx].trim();
    if !(before.is_empty()
        || before == "pub"
        || (before.starts_with("pub(") && before.ends_with(')')))
    {
        return None;
    }
    let rest = line[idx + 3..].trim_start();
    let name: String =
        rest.chars().take_while(|c| c.is_ascii_alphanumeric() || *c == '_').collect();
    (!name.is_empty() && rest[name.len()..].trim_start().starts_with('{')).then_some(name)
}

/// The name of a function declared on this line, and the byte index of that name **in this
/// line** — which is what makes a jump land on the name instead of on `pub async fn`.
fn fn_name_at(line: &str) -> Option<(String, usize)> {
    let idx = find_word(line, "fn")?;
    let after = &line[idx + 2..];
    let lead = after.len() - after.trim_start().len();
    let rest = after.trim_start();
    let name: String =
        rest.chars().take_while(|c| c.is_ascii_alphanumeric() || *c == '_').collect();
    (!name.is_empty()).then(|| (name, idx + 2 + lead))
}

/// Byte index of `word` as a whole token in `line`.
fn find_word(line: &str, word: &str) -> Option<usize> {
    let mut from = 0;
    while let Some(i) = line[from..].find(word) {
        let at = from + i;
        let before_ok = at == 0 || !is_ident_byte(line.as_bytes()[at - 1]);
        let after = at + word.len();
        let after_ok = after >= line.len() || !is_ident_byte(line.as_bytes()[after]);
        if before_ok && after_ok {
            return Some(at);
        }
        from = at + word.len();
    }
    None
}

fn is_ident_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

fn brace_balance(s: &str) -> i32 {
    s.bytes().map(|b| match b {
        b'{' => 1,
        b'}' => -1,
        _ => 0,
    }).sum()
}

fn bracket_balance(s: &str) -> i32 {
    s.bytes().map(|b| match b {
        b'[' | b'(' => 1,
        b']' | b')' => -1,
        _ => 0,
    }).sum()
}

/// Replaces strings, chars and comments with spaces, keeping every other byte where it was.
///
/// Stateful across lines: a block comment and a raw string both span them. Keeping the *length*
/// identical is what lets the caller go on using byte offsets into the original line.
#[derive(Default)]
struct Blanker {
    in_block_comment: u32,
    /// `Some(hashes)` while inside a raw string opened with that many `#`.
    in_raw: Option<usize>,
    /// Inside an ordinary `"…"` that has not closed yet — Rust lets one span lines.
    in_string: bool,
}

impl Blanker {
    fn blank(&mut self, line: &str) -> String {
        let bytes = line.as_bytes();
        let mut out = vec![b' '; bytes.len()];
        let mut i = 0;
        while i < bytes.len() {
            // Inside a raw string: look for `"` followed by EXACTLY the hashes it opened with. A
            // shorter run does not close it — `r#"say "hi""#` contains a bare quote.
            if let Some(hashes) = self.in_raw {
                let closes = bytes[i] == b'"'
                    && bytes.len() >= i + 1 + hashes
                    && bytes[i + 1..i + 1 + hashes].iter().all(|b| *b == b'#');
                if closes {
                    self.in_raw = None;
                    i += 1 + hashes;
                } else {
                    i += 1;
                }
                continue;
            }
            if self.in_string {
                match bytes[i] {
                    b'\\' => i += 2,
                    b'"' => {
                        self.in_string = false;
                        i += 1;
                    }
                    _ => i += 1,
                }
                continue;
            }
            if self.in_block_comment > 0 {
                if bytes[i..].starts_with(b"*/") {
                    self.in_block_comment -= 1;
                    i += 2;
                } else if bytes[i..].starts_with(b"/*") {
                    self.in_block_comment += 1;
                    i += 2;
                } else {
                    i += 1;
                }
                continue;
            }
            match bytes[i] {
                b'/' if bytes[i..].starts_with(b"//") => break, // rest of the line is a comment
                b'/' if bytes[i..].starts_with(b"/*") => {
                    self.in_block_comment = 1;
                    i += 2;
                }
                b'r' if raw_string_hashes(&bytes[i..]).is_some() => {
                    let hashes = raw_string_hashes(&bytes[i..]).unwrap_or(0);
                    self.in_raw = Some(hashes);
                    i += 2 + hashes; // `r` + hashes + `"`
                }
                b'"' => {
                    self.in_string = true;
                    i += 1;
                }
                b'\'' => {
                    // A lifetime (`'a`) is not a char literal, and mistaking one for an unclosed
                    // literal would blank the rest of the line.
                    if is_char_literal(&bytes[i..]) {
                        i += 1;
                        while i < bytes.len() {
                            match bytes[i] {
                                b'\\' => i += 2,
                                b'\'' => {
                                    i += 1;
                                    break;
                                }
                                _ => i += 1,
                            }
                        }
                    } else {
                        out[i] = bytes[i];
                        i += 1;
                    }
                }
                b => {
                    out[i] = b;
                    i += 1;
                }
            }
        }
        String::from_utf8_lossy(&out).into_owned()
    }
}

/// How many `#` a raw string at the start of `bytes` opens with, when it is one.
fn raw_string_hashes(bytes: &[u8]) -> Option<usize> {
    if bytes.first() != Some(&b'r') {
        return None;
    }
    let hashes = bytes[1..].iter().take_while(|b| **b == b'#').count();
    (bytes.get(1 + hashes) == Some(&b'"')).then_some(hashes)
}

/// Whether a `'` at the start of `bytes` opens a char literal rather than a lifetime.
fn is_char_literal(bytes: &[u8]) -> bool {
    match bytes.get(1) {
        Some(b'\\') => true,
        // `'a'` is a literal, `'a` followed by anything else is a lifetime.
        Some(_) => bytes.get(2) == Some(&b'\'') || bytes.get(3) == Some(&b'\''),
        None => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn place(module: &str) -> FilePlace {
        FilePlace {
            package: "demo".to_string(),
            target: TestTarget::Lib,
            module: module.to_string(),
        }
    }

    // ── place_of ──────────────────────────────────────────────────────────────────

    #[test]
    fn a_lib_root_has_no_module_path() {
        let p = place_of("src/lib.rs", "demo", true).expect("a target");
        assert_eq!(p.target, TestTarget::Lib);
        assert_eq!(p.module, "");
    }

    #[test]
    fn a_module_file_carries_its_path() {
        assert_eq!(place_of("src/util/parse.rs", "demo", true).unwrap().module, "util::parse");
        // `mod.rs` IS the module — it must not appear in the path as a segment of its own.
        assert_eq!(place_of("src/util/mod.rs", "demo", true).unwrap().module, "util");
    }

    /// The fact the path cannot carry: in a bin-only package `src/util.rs` is a module of the
    /// **binary**, and a `--lib` run of it is a run cargo refuses.
    #[test]
    fn without_a_lib_a_src_module_belongs_to_the_binary() {
        let p = place_of("src/util.rs", "demo", false).expect("a target");
        assert_eq!(p.target, TestTarget::Bin { name: "demo".to_string() });
        assert_eq!(p.module, "util");
    }

    #[test]
    fn a_binary_is_named_after_its_file_or_its_package() {
        assert_eq!(
            place_of("src/main.rs", "demo", true).unwrap().target,
            TestTarget::Bin { name: "demo".to_string() }
        );
        assert_eq!(
            place_of("src/bin/cli.rs", "demo", true).unwrap().target,
            TestTarget::Bin { name: "cli".to_string() }
        );
    }

    /// A directory under `tests/` is ONE target rooted at its `main.rs`; its other files are
    /// modules of it, not targets cargo could build.
    #[test]
    fn an_integration_directory_is_one_target_with_modules() {
        let root = place_of("tests/api/main.rs", "demo", true).unwrap();
        assert_eq!(root.target, TestTarget::Test { name: "api".to_string() });
        assert_eq!(root.module, "");
        let helper = place_of("tests/api/helpers.rs", "demo", true).unwrap();
        assert_eq!(helper.target, TestTarget::Test { name: "api".to_string() });
        assert_eq!(helper.module, "helpers");
    }

    #[test]
    fn a_file_cargo_does_not_compile_has_no_place() {
        assert!(place_of("build.rs", "demo", true).is_none());
        assert!(place_of("docs/notes.md", "demo", true).is_none());
    }

    // ── discovery ─────────────────────────────────────────────────────────────────

    #[test]
    fn finds_a_test_in_an_inline_module() {
        let src = "\
pub fn add(a: u8, b: u8) -> u8 { a + b }

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adds() {
        assert_eq!(add(1, 2), 3);
    }
}
";
        let found = discover_rust_in_source("/p/src/util.rs", src, &place("util"));
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].module, "util::tests");
        assert_eq!(found[0].path, "util::tests::adds");
        assert_eq!(found[0].kind, RustTestKind::Test);
        assert_eq!(&src[found[0].offset..found[0].offset + 4], "adds");
    }

    #[test]
    fn a_plain_function_is_not_a_test() {
        let src = "fn helper() {}\n#[test]\nfn real() {}\n";
        let found = discover_rust_in_source("/p/src/lib.rs", src, &place(""));
        assert_eq!(found.iter().map(|t| t.name.as_str()).collect::<Vec<_>>(), ["real"]);
    }

    /// The whole reason the scan blanks strings: a `}` in one would close a module that never
    /// opened, and every test after it would be attributed one level too shallow.
    #[test]
    fn a_brace_inside_a_string_does_not_close_a_module() {
        let src = "\
mod tests {
    fn fixture() -> &'static str { \"}}}\" }
    #[test]
    fn still_inside() {}
}
";
        let found = discover_rust_in_source("/p/src/lib.rs", src, &place(""));
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].module, "tests", "the string's braces were counted");
    }

    #[test]
    fn a_brace_inside_a_comment_does_not_close_a_module() {
        let src = "\
mod tests {
    // closes here }
    /* and } here */
    #[test]
    fn still_inside() {}
}
";
        let found = discover_rust_in_source("/p/src/lib.rs", src, &place(""));
        assert_eq!(found[0].module, "tests");
    }

    #[test]
    fn a_raw_string_spanning_lines_is_blanked() {
        let src = "\
mod tests {
    const Q: &str = r#\"
        } } }
    \"#;
    #[test]
    fn still_inside() {}
}
";
        let found = discover_rust_in_source("/p/src/lib.rs", src, &place(""));
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].module, "tests");
    }

    /// A lifetime is not a char literal — reading `'a` as one would blank the rest of the line,
    /// including the `{` that opens the body.
    #[test]
    fn a_lifetime_is_not_a_char_literal() {
        let src = "\
mod tests {
    fn borrow<'a>(s: &'a str) -> &'a str { s }
    #[test]
    fn still_inside() {}
}
";
        let found = discover_rust_in_source("/p/src/lib.rs", src, &place(""));
        assert_eq!(found[0].module, "tests");
    }

    #[test]
    fn nested_modules_stack_and_unstack() {
        let src = "\
mod outer {
    mod inner {
        #[test]
        fn deep() {}
    }
    #[test]
    fn shallow() {}
}
#[test]
fn top() {}
";
        let found = discover_rust_in_source("/p/src/lib.rs", src, &place(""));
        let paths: Vec<&str> = found.iter().map(|t| t.path.as_str()).collect();
        assert_eq!(paths, ["outer::inner::deep", "outer::shallow", "top"]);
    }

    /// `mod x;` names another file. Treating it as an opening brace would swallow every
    /// following test into a module that is not there.
    #[test]
    fn a_file_module_declaration_opens_nothing() {
        let src = "mod util;\n#[test]\nfn top() {}\n";
        let found = discover_rust_in_source("/p/src/lib.rs", src, &place(""));
        assert_eq!(found[0].path, "top");
    }

    #[test]
    fn an_async_test_is_recognised_by_the_leaf_of_its_path() {
        let src = "#[tokio::test]\nasync fn talks() {}\n";
        let found = discover_rust_in_source("/p/src/lib.rs", src, &place(""));
        assert_eq!(found[0].kind, RustTestKind::Async);
        assert_eq!(found[0].name, "talks");
    }

    #[test]
    fn ignore_and_should_panic_are_carried() {
        let src = "\
#[test]
#[ignore = \"needs the network\"]
fn slow() {}

#[test]
#[should_panic(expected = \"boom\")]
fn explodes() {}
";
        let found = discover_rust_in_source("/p/src/lib.rs", src, &place(""));
        assert!(found[0].ignored);
        assert!(!found[0].should_panic);
        assert!(found[1].should_panic);
        assert!(!found[1].ignored);
    }

    /// One function, many libtest cases — the panel must not offer to run it by exact name.
    #[test]
    fn a_parameterized_function_is_marked_as_such() {
        let src = "#[rstest]\n#[case(1)]\n#[case(2)]\nfn adds(#[case] n: u8) {}\n";
        let found = discover_rust_in_source("/p/src/lib.rs", src, &place(""));
        assert_eq!(found[0].kind, RustTestKind::Parameterized);
    }

    #[test]
    fn an_attribute_spanning_lines_still_marks_the_test() {
        let src = "\
#[tokio::test(
    flavor = \"multi_thread\",
    worker_threads = 2
)]
async fn spread() {}
";
        let found = discover_rust_in_source("/p/src/lib.rs", src, &place(""));
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].kind, RustTestKind::Async);
    }

    #[test]
    fn a_bench_is_its_own_kind() {
        let src = "#[bench]\nfn speed(b: &mut Bencher) {}\n";
        let found = discover_rust_in_source("/p/src/lib.rs", src, &place(""));
        assert_eq!(found[0].kind, RustTestKind::Bench);
    }

    /// An integration test file is a crate root: its tests have no module prefix from the path.
    #[test]
    fn an_integration_test_has_no_path_prefix() {
        let p = FilePlace {
            package: "demo".to_string(),
            target: TestTarget::Test { name: "api".to_string() },
            module: String::new(),
        };
        let found = discover_rust_in_source("/p/tests/api.rs", "#[test]\nfn answers() {}\n", &p);
        assert_eq!(found[0].path, "answers");
    }

    #[test]
    fn a_target_names_its_selector_flags() {
        assert_eq!(TestTarget::Lib.selector_args(), ["--lib"]);
        assert_eq!(
            TestTarget::Test { name: "api".to_string() }.selector_args(),
            ["--test", "api"]
        );
    }
}
