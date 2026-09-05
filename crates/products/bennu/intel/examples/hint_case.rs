//! How often the engine can type the right-hand side of a local — measured against the type the
//! author wrote.
//!
//! ## Why a project with no `var` is the right corpus
//!
//! The type hint only draws on `var` / Lombok's `val`, so a Java 8 library has nothing to show and
//! looks like it cannot be measured. It can, and better than a modern one: **every local with a
//! written type is a hint with its answer already on the line.** Erase the type in your head and
//! the declaration is a `var`; the type that was there is what the hint would have to say.
//!
//! That gives an oracle no downloaded sample provides. Three outcomes, and they are not equally
//! bad:
//!
//!   * **agreed** — the hint would have appeared and been right;
//!   * **untyped** — the hint would simply not appear. What the reader sees is a `var` with nothing
//!     after it, which is the complaint this harness exists to answer;
//!   * **DISAGREED** — the hint would have appeared and been **wrong**, which is the only real
//!     defect here. A hint is drawn as if the compiler had said it and there is nothing to click
//!     through to find out it did not.
//!
//! The untyped ones are grouped by the syntactic shape of the initialiser, because that is what a
//! fix is: a shape the inference does not handle yet.
//!
//! ```sh
//! cargo run -p bennu-intel --release --example hint_case -- <project-dir>
//! ```

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use bennu_classpath::prelude::resolve_jdk_classpath;
use bennu_index::prelude::PersistedIndex;
use bennu_intel::prelude::build_project_index_from_sources;
use bennu_java::prelude::{infer_expression_type, parse_java, TypeResolver};
use bennu_query::prelude::{inlay_hints, IndexResolver, JdkMemberIndex};

fn main() {
    let mut args = std::env::args().skip(1);
    let Some(root) = args.next() else {
        eprintln!("usage: hint_case <project-dir>");
        std::process::exit(2);
    };
    let root = PathBuf::from(&root);
    let mut javas = Vec::new();
    collect(&root, &mut javas);
    javas.sort();
    let sources: Vec<(PathBuf, String)> = javas
        .iter()
        .filter_map(|p| fs::read_to_string(p).ok().map(|t| (p.clone(), t)))
        .collect();
    eprintln!("java files : {}", sources.len());

    let temp = std::env::temp_dir().join(format!("bennu-hint-{}", std::process::id()));
    let _ = fs::remove_dir_all(&temp);
    fs::create_dir_all(&temp).expect("temp dir");
    let built = build_project_index_from_sources(&sources, &temp);
    built.builder.persist().expect("persist");
    let simple: Vec<(String, String)> =
        built.type_map.iter().map(|(s, b)| (s.clone(), b.clone())).collect();

    let jdk = resolve_jdk_classpath("21").expect("a JDK");
    let persisted = PersistedIndex::open(built.builder.blob_path(), built.builder.fst_path())
        .expect("open index");
    let mut r = IndexResolver::new(persisted, JdkMemberIndex::new(Box::new(jdk)));
    for (s, b) in &simple {
        r.add_simple_hint(s, b);
    }
    let resolver: Arc<dyn TypeResolver + Send + Sync> = Arc::new(r);
    eprintln!("resolver ready\n");

    let (mut total, mut agreed, mut disagreed, mut untyped) = (0usize, 0usize, 0usize, 0usize);
    let mut shapes: HashMap<String, usize> = HashMap::new();
    let mut wrong: Vec<String> = Vec::new();
    let mut samples: HashMap<String, Vec<String>> = HashMap::new();
    let (mut hint_sites, mut hints_drawn) = (0usize, 0usize);

    for (path, text) in &sources {
        // What the editor would actually draw, so the two halves are measured on one corpus.
        hints_drawn += inlay_hints(text, resolver.as_ref()).len();

        let Some(tree) = parse_java(text) else { continue };
        let bytes = text.as_bytes();
        let mut stack = vec![tree.root_node()];
        while let Some(n) = stack.pop() {
            let mut c = n.walk();
            for ch in n.named_children(&mut c) {
                stack.push(ch);
            }
            if n.kind() == "lambda_expression" {
                hint_sites += 1;
            }
            if n.kind() != "local_variable_declaration" {
                continue;
            }
            let Some(ty) = n.child_by_field_name("type") else { continue };
            let Ok(written) = ty.utf8_text(bytes) else { continue };
            // A `var` has no answer written on the line — nothing to check it against.
            if bennu_java::prelude::is_inferred_type(written) {
                continue;
            }
            let mut cd = n.walk();
            for d in n.named_children(&mut cd) {
                if d.kind() != "variable_declarator" {
                    continue;
                }
                let Some(value) = d.child_by_field_name("value") else { continue };
                total += 1;
                let inferred = infer_expression_type(
                    text,
                    value.start_byte(),
                    value.end_byte(),
                    resolver.as_ref(),
                );
                match inferred {
                    None => {
                        untyped += 1;
                        let shape = shape_of(&value, bytes);
                        *shapes.entry(shape.clone()).or_default() += 1;
                        let line = text[..value.start_byte()].matches('\n').count() + 1;
                        let list = samples.entry(shape).or_default();
                        if list.len() < 3 {
                            list.push(format!(
                                "{}:{line}  {}",
                                short(path, &root),
                                one_line(value.utf8_text(bytes).unwrap_or(""))
                            ));
                        }
                    }
                    Some(t) => {
                        let got = simple_name(&t.binary_name, t.dims as usize);
                        let want = base_of(written);
                        if got == want {
                            agreed += 1;
                        } else {
                            disagreed += 1;
                            if wrong.len() < 25 {
                                let line = text[..value.start_byte()].matches('\n').count() + 1;
                                wrong.push(format!(
                                    "{}:{line}  wrote `{want}`, inferred `{got}`   {}",
                                    short(path, &root),
                                    one_line(value.utf8_text(bytes).unwrap_or(""))
                                ));
                            }
                        }
                    }
                }
            }
        }
    }

    println!("\n=== the type hint, checked against the type the author wrote ===");
    println!("locals with a written type : {total}");
    let pct = |n: usize| if total == 0 { 0.0 } else { 100.0 * n as f64 / total as f64 };
    println!("  agreed  (hint, correct)  : {agreed}  ({:.1}%)", pct(agreed));
    println!("  untyped (no hint at all) : {untyped}  ({:.1}%)", pct(untyped));
    println!("  DISAGREED (wrong hint)   : {disagreed}  ({:.1}%)   ← the only defect", pct(disagreed));
    println!("\nlambda expressions        : {hint_sites}");
    println!("hints the editor would draw: {hints_drawn}");

    if !shapes.is_empty() {
        println!("\n=== what it cannot type, by shape of the right-hand side ===");
        let mut rows: Vec<_> = shapes.into_iter().collect();
        rows.sort_by(|a, b| b.1.cmp(&a.1));
        for (shape, n) in rows {
            println!("  {n:>5}  {shape}");
            for s in samples.get(&shape).into_iter().flatten() {
                println!("           {s}");
            }
        }
    }
    if !wrong.is_empty() {
        println!("\n=== hints that would have been WRONG ===");
        for w in &wrong {
            println!("  {w}");
        }
    }
    let _ = fs::remove_dir_all(&temp);
}

/// The syntactic shape of an initialiser, at the grain a fix has: the node kind, and for a call the
/// fact that it is a call rather than which method it was.
fn shape_of(value: &tree_sitter::Node, bytes: &[u8]) -> String {
    let kind = value.kind();
    if kind == "method_invocation" {
        // A bare `foo()` and `recv.foo()` are resolved by different code, and only one of them used
        // to be read at all — worth keeping apart.
        return if value.child_by_field_name("object").is_some() {
            "method_invocation (recv.m(…))".to_string()
        } else {
            "method_invocation (bare m(…))".to_string()
        };
    }
    let _ = bytes;
    kind.to_string()
}

/// `java/util/List` + dims → `List[]`; a primitive is already its own name.
fn simple_name(binary: &str, dims: usize) -> String {
    let base = binary.rsplit('/').next().unwrap_or(binary).replace('$', ".");
    format!("{base}{}", "[]".repeat(dims))
}

/// The written type without its type arguments — `Map<String, List<Integer>>` → `Map`. Generic
/// arguments are a separate question from "did it find the right class".
fn base_of(written: &str) -> String {
    let w = written.trim();
    let head = match w.find('<') {
        Some(at) => {
            let tail: String = w[at..].chars().filter(|c| *c == '[').map(|_| '[').collect();
            let dims = tail.len();
            format!("{}{}", &w[..at], "[]".repeat(dims))
        }
        None => w.to_string(),
    };
    head.rsplit('.').next().unwrap_or(&head).replace(' ', "")
}

fn one_line(s: &str) -> String {
    let flat: String = s.split_whitespace().collect::<Vec<_>>().join(" ");
    if flat.len() > 70 { format!("{}…", &flat[..70]) } else { flat }
}

fn short(p: &Path, root: &Path) -> String {
    p.strip_prefix(root).unwrap_or(p).display().to_string().replace('\\', "/")
}

fn collect(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else { return };
    for e in entries.flatten() {
        let p = e.path();
        if p.is_dir() {
            collect(&p, out);
        } else if p.extension().is_some_and(|x| x == "java") {
            out.push(p);
        }
    }
}
