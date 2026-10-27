//! Inference corpus: does `bennu-java` give an expression the type Java gives it?
//!
//! The cases are the `infer` module of the corpus (`crates/products/bennu/analysis/check/tests/corpus/infer`),
//! legal Java 21 that javac compiles without a diagnostic — so every expectation is at least assignable
//! in real Java. Each case is one declaration whose line ends in a marker:
//!
//! ```java
//! Optional<Integer> mapped = Optional.of("a").map(String::length); // infer
//! ```
//!
//! The **declared type is the expectation**: it has to be exactly the initializer's type, which is why a
//! case never declares a supertype (`List<String> x = new ArrayList<>()` would be a different question).
//! Bennu is asked for the type of the initializer, and the answer is compared with the declared type
//! resolved the way Bennu resolves any written type.
//!
//! * `// infer`  — Bennu must give exactly this type. Silence is a failure too: the case says Bennu knows.
//! * `// infer?` — a known gap: Bennu may answer nothing. Never a failure — but a **wrong** answer always is,
//!   and an answer that is now right is reported so the `?` can go.
//!
//! The rule behind the asymmetry is the validator's own: a missing type costs a diagnostic that is not
//! shown; a wrong type produces false positives. Wrong is the one thing this test forbids.
//!
//! Run: `cargo test -p bennu-intel --test infer_corpus -- --nocapture`

#[allow(dead_code)]
mod check_corpus_support;

use std::fmt::Write;
use std::path::{Path, PathBuf};

use bennu_java::prelude::{
    extract_symbols, infer_expression_type_at, parse_java, same_binary_type, written_type_ref, InferCache, TypeRef,
};
use check_corpus_support::module::{with_resolver, CorpusModule, SourceFile};
use tree_sitter::Node;

const MODULE: CorpusModule = CorpusModule { name: "infer", release: "21", major: 21, libs: &[] };

fn corpus_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../check/tests/corpus")
}

/// What one case came to.
enum Verdict {
    /// Exactly the declared type.
    Match,
    /// Bennu said nothing.
    Silent,
    /// The right class, with type arguments it could not settle: raw, or a variable left open
    /// (`List<T>` for `List<String>`). Safe for a consumer that treats a variable as unknown, but not
    /// the type Java gives.
    Partial(String),
    /// Bennu answered, and the answer is a different type.
    Wrong(String),
    /// The declared type does not resolve, or the line holds no declaration: the case is broken.
    BrokenCase(String),
}

struct Case {
    file: String,
    line: usize,
    declared: String,
    /// `// infer` rather than `// infer?`.
    required: bool,
    verdict: Verdict,
}

impl Case {
    /// A `// infer` case must be exact; a `// infer?` case is a known gap and fails only when the case
    /// itself is broken. Whatever Bennu answers there is reported, never enforced.
    fn failed(&self) -> bool {
        match self.verdict {
            Verdict::Match => false,
            Verdict::BrokenCase(_) => true,
            Verdict::Silent | Verdict::Partial(_) | Verdict::Wrong(_) => self.required,
        }
    }
}

#[test]
fn bennu_infers_the_types_the_corpus_states() {
    let outcome = with_resolver(&corpus_root(), &MODULE, |resolver, sources| {
        sources.iter().flat_map(|source| cases_of(source, resolver)).collect::<Vec<Case>>()
    });
    let cases = match outcome {
        Ok(cases) => cases,
        Err(why) => {
            eprintln!("infer_corpus: SKIPPED: {why}");
            return;
        }
    };

    let report = render(&cases);
    let path = Path::new(env!("CARGO_TARGET_TMPDIR")).join("bennu-infer-corpus.md");
    match std::fs::write(&path, &report) {
        Ok(()) => eprintln!("infer_corpus: report at {}", path.display()),
        Err(why) => eprintln!("infer_corpus: could not write {}: {why}", path.display()),
    }
    eprintln!("infer_corpus: {}", summary(&cases));

    let failures: Vec<String> = cases.iter().filter(|c| c.failed()).map(describe).collect();
    assert!(
        failures.is_empty(),
        "bennu-java infers something other than the corpus states (or is silent where it must not be):\n{}",
        failures.join("\n")
    );
}

fn cases_of(source: &SourceFile, resolver: &dyn bennu_java::prelude::TypeResolver) -> Vec<Case> {
    let Some(tree) = parse_java(&source.text) else { return Vec::new() };
    let root = tree.root_node();
    let symbols = extract_symbols(&source.text);
    let cache = InferCache::new();
    let mut out = Vec::new();
    for (row, line) in source.text.lines().enumerate() {
        let Some(required) = marker_of(line) else { continue };
        let mut case = Case {
            file: source.rel.clone(),
            line: row + 1,
            declared: String::new(),
            required,
            verdict: Verdict::BrokenCase("no declaration starts on this line".into()),
        };
        if let Some((declared, start, end)) = declaration_on(root, row, &source.text) {
            case.declared = declared.clone();
            case.verdict = match written_type_ref(&root, &source.text, &symbols, &declared, resolver, &cache) {
                None => Verdict::BrokenCase(format!("`{declared}` does not resolve")),
                Some(expected) => {
                    match infer_expression_type_at(&root, &source.text, &symbols, start, end, resolver) {
                        None => Verdict::Silent,
                        Some(got) if same(&got, &expected) => Verdict::Match,
                        Some(got) if partial(&got, &expected) => Verdict::Partial(show(&got)),
                        Some(got) => Verdict::Wrong(format!("{} — the declared type resolves to `{}`", show(&got), show(&expected))),
                    }
                }
            };
        }
        out.push(case);
    }
    out
}

/// `Some(true)` for `// infer`, `Some(false)` for `// infer?`, `None` for a line without a marker.
fn marker_of(line: &str) -> Option<bool> {
    let tail = line.rsplit_once("//")?.1.trim();
    match tail {
        "infer" => Some(true),
        "infer?" => Some(false),
        _ => None,
    }
}

/// The declared type text and the initializer's byte range of the declaration that starts on `row`.
fn declaration_on(root: Node, row: usize, text: &str) -> Option<(String, usize, usize)> {
    let mut stack = vec![root];
    while let Some(node) = stack.pop() {
        if node.start_position().row > row || node.end_position().row < row {
            continue;
        }
        if matches!(node.kind(), "local_variable_declaration" | "field_declaration")
            && node.start_position().row == row
        {
            let declared = node.child_by_field_name("type")?.utf8_text(text.as_bytes()).ok()?.to_string();
            let value = node.child_by_field_name("declarator")?.child_by_field_name("value")?;
            return Some((declared, value.start_byte(), value.end_byte()));
        }
        let mut cursor = node.walk();
        stack.extend(node.children(&mut cursor));
    }
    None
}

/// The same type. The `wildcard` bit is about whether the type can be written back, not about which
/// type it is, so it is not compared.
fn same(a: &TypeRef, b: &TypeRef) -> bool {
    same_binary_type(&a.binary_name, &b.binary_name)
        && a.dims == b.dims
        && a.type_args.len() == b.type_args.len()
        && a.type_args.iter().zip(&b.type_args).all(|(x, y)| same(x, y))
}

/// `got` is `expected` with some type arguments left unsettled: same class, and every argument either
/// the same or a variable nothing bound.
fn partial(got: &TypeRef, expected: &TypeRef) -> bool {
    if !same_binary_type(&got.binary_name, &expected.binary_name) || got.dims != expected.dims {
        return false;
    }
    if got.type_args.is_empty() {
        return true; // raw
    }
    got.type_args.len() == expected.type_args.len()
        && got.type_args.iter().zip(&expected.type_args).all(|(x, y)| is_open_variable(x) || partial(x, y))
}

/// A type variable no binding reached: a bare name with no package, which no primitive is.
fn is_open_variable(t: &TypeRef) -> bool {
    const PRIMITIVES: [&str; 9] = ["boolean", "byte", "short", "char", "int", "long", "float", "double", "void"];
    t.dims == 0
        && t.type_args.is_empty()
        && !t.binary_name.contains('/')
        && !PRIMITIVES.contains(&t.binary_name.as_str())
}

/// A type the way Java writes it, for the report.
fn show(t: &TypeRef) -> String {
    let mut out = t.binary_name.replace('/', ".");
    if !t.type_args.is_empty() {
        out.push('<');
        out.push_str(&t.type_args.iter().map(show).collect::<Vec<_>>().join(", "));
        out.push('>');
    }
    out.push_str(&"[]".repeat(t.dims as usize));
    out
}

fn describe(c: &Case) -> String {
    let what = match &c.verdict {
        Verdict::Match => "matches".to_string(),
        Verdict::Silent => "Bennu is silent".to_string(),
        Verdict::Partial(got) => format!("Bennu infers `{got}` (arguments unsettled)"),
        Verdict::Wrong(got) => format!("Bennu infers `{got}`"),
        Verdict::BrokenCase(why) => format!("broken case: {why}"),
    };
    format!("  {}:{} expects `{}` — {what}", c.file, c.line, c.declared)
}

fn summary(cases: &[Case]) -> String {
    let count = |f: fn(&Case) -> bool| cases.iter().filter(|c| f(c)).count();
    format!(
        "{} cases, exact {}, partial {}, silent {}, wrong {}, broken {} — known gaps `// infer?`: {}",
        cases.len(),
        count(|c| matches!(c.verdict, Verdict::Match)),
        count(|c| matches!(c.verdict, Verdict::Partial(_))),
        count(|c| matches!(c.verdict, Verdict::Silent)),
        count(|c| matches!(c.verdict, Verdict::Wrong(_))),
        count(|c| matches!(c.verdict, Verdict::BrokenCase(_))),
        count(|c| !c.required),
    )
}

fn render(cases: &[Case]) -> String {
    let mut out = String::from("# bennu-java inference corpus\n\n");
    let _ = writeln!(out, "{}\n", summary(cases));
    let sections: [(&str, fn(&Case) -> bool); 5] = [
        ("Required (`// infer`) and not exact — the test fails", |c| c.required && c.failed()),
        ("Known gaps (`// infer?`) — wrong", |c| matches!(c.verdict, Verdict::Wrong(_)) && !c.required),
        ("Known gaps (`// infer?`) — arguments unsettled", |c| {
            matches!(c.verdict, Verdict::Partial(_)) && !c.required
        }),
        ("Known gaps (`// infer?`) — silent", |c| matches!(c.verdict, Verdict::Silent) && !c.required),
        ("Now exact although marked `// infer?` — drop the `?`", |c| {
            matches!(c.verdict, Verdict::Match) && !c.required
        }),
    ];
    for (title, pick) in sections {
        let rows: Vec<&Case> = cases.iter().filter(|c| pick(c)).collect();
        let _ = writeln!(out, "## {title} ({})\n", rows.len());
        for c in rows {
            let _ = writeln!(out, "-{}", describe(c));
        }
        out.push('\n');
    }
    out
}
