//! The true trace of every member the popup misses: from the receiver up the chain (`a.b().c().d`), each link typed with the engine's
//! own `expression_type`, until the first one that does not type; then why.
//!
//! The cause is looked for in two buffers: the **reduced** one the popup is asked about (the member and its arguments taken out, as
//! the editor holds the line when the dot has just been typed) and the **original**. A link that types in the original and not in the
//! reduced buffer is the bench's doing, not the inference's.
//!
//! `cargo test -p bennu-be mlbench::m3_trace -- --ignored --nocapture`  (`BENNU_MLB_N`, `BENNU_MLB_ONLY` as in M3)

use std::collections::BTreeMap;

use bennu_java::prelude::parse_java;
use tree_sitter::Node;

use super::common::*;
use super::m3_misses::{base_of, declared_type, is_lambda_param};
use super::m3_rank::{after_call, is_plain_name, popup, simple};
use super::sites::{sites, Position};
use crate::index_service::IndexService;

fn txt<'a>(src: &'a str, n: &Node) -> &'a str {
    &src[n.start_byte()..n.end_byte()]
}

/// The receiver node of the member written at `member_start`: the object of the call or field access that has that name.
fn receiver_of<'t>(root: Node<'t>, member_start: usize) -> Option<Node<'t>> {
    let mut n = root.descendant_for_byte_range(member_start, member_start)?;
    loop {
        match n.kind() {
            "method_invocation" if n.child_by_field_name("name").is_some_and(|x| x.start_byte() == member_start) => return n.child_by_field_name("object"),
            "field_access" if n.child_by_field_name("field").is_some_and(|x| x.start_byte() == member_start) => return n.child_by_field_name("object"),
            _ => n = n.parent()?,
        }
    }
}

/// The chain below `n`, innermost first: `a.b().c()` → `a`, `a.b()`, `a.b().c()`.
fn links<'t>(n: Node<'t>) -> Vec<Node<'t>> {
    let mut out = match n.kind() {
        "method_invocation" | "field_access" => n.child_by_field_name("object").map(links).unwrap_or_default(),
        "parenthesized_expression" | "cast_expression" => Vec::new(),
        _ => Vec::new(),
    };
    out.push(n);
    out
}

struct Verdict {
    cause: String,
    bench: bool,
    link: String,
}

fn is_type_variable(b: &str) -> bool {
    b.len() <= 2 && b.starts_with(|c: char| c.is_ascii_uppercase())
}

#[allow(clippy::too_many_arguments)]
fn trace(svc: &IndexService, file: &str, original: &str, reduced: &str, s: &super::sites::Site) -> Verdict {
    let Some(tree) = parse_java(original) else { return Verdict { cause: "original does not parse".into(), bench: false, link: String::new() } };
    let Some(recv) = receiver_of(tree.root_node(), s.start) else { return Verdict { cause: "no receiver found".into(), bench: false, link: String::new() } };
    let chain = links(recv);
    let ty = |buf: &str, n: &Node| svc.expression_type(file, buf, n.start_byte(), n.end_byte());
    let mut prev: Option<String> = None;
    for (i, link) in chain.iter().enumerate() {
        // `Objects` in `Objects.requireNonNull(..)` names a type, not a value: it does not "type", and the next link is the first real one
        if i == 0 && link.kind() == "identifier" {
            let name = txt(original, link);
            if name.starts_with(|c: char| c.is_ascii_uppercase()) && svc.type_resolves(file, reduced, name) {
                if let Some((binary, _)) = svc.header_type(file, reduced, name) {
                    prev = Some(binary);
                    continue;
                }
            }
        }
        let in_reduced = ty(reduced, link);
        if let Some(t) = in_reduced {
            prev = Some(t);
            continue;
        }
        let in_original = ty(original, link);
        let text = txt(original, link).to_string();
        let bench = in_original.is_some();
        let cause = why(svc, file, original, reduced, link, i, prev.as_deref());
        return Verdict { cause, bench, link: text };
    }
    // every link types
    let full = chain.last().and_then(|l| ty(reduced, l));
    let binary = full.unwrap_or_default();
    let known = svc.header_type(file, "", &binary.replace('/', ".")).map(|(_, t)| t.iter().any(|t| t.methods.iter().chain(t.fields.iter()).any(|m| m.name == original[s.start..s.end])));
    let cause = match known {
        None => format!("the whole chain types (as `{}`), but that type's tables are not reachable", binary),
        Some(true) => "the whole chain types and the member is in the tables: the popup omits it".to_string(),
        Some(false) => "the whole chain types, the member is not in the type's tables".to_string(),
    };
    Verdict { cause, bench: false, link: txt(original, &recv).to_string() }
}

/// Why the link at `index` does not type, given the type of the one before it.
fn why(svc: &IndexService, file: &str, original: &str, reduced: &str, link: &Node, index: usize, prev: Option<&str>) -> String {
    let ty = |buf: &str, n: &Node| svc.expression_type(file, buf, n.start_byte(), n.end_byte());
    match link.kind() {
        "identifier" => {
            let name = txt(original, link);
            if name.starts_with(|c: char| c.is_ascii_uppercase()) {
                return if svc.header_type(file, reduced, name).is_some() { "static access: the class has tables, the member is not listed (defect)".into() } else { "type name of a library without sources (or an import the index cannot follow)".into() };
            }
            if is_lambda_param(original, link.start_byte(), name) {
                return "lambda parameter: the functional type of the target is not inferred".into();
            }
            match declared_type(original, link.start_byte(), name) {
                None => "name with no declaration in the file (inherited field, static import or unknown)".into(),
                Some(t) => {
                    let base = base_of(&t);
                    if base == "var" {
                        "`var`: the initialiser does not type".into()
                    } else if is_type_variable(&base) {
                        "variable of a type-variable type".into()
                    } else if !svc.type_resolves(file, reduced, &base) && svc.type_name_matches(file, &base, 3).is_empty() {
                        format!("declared type `{base}` unknown to the index")
                    } else {
                        format!("declared type `{base}` resolves but the variable does not type")
                    }
                }
            }
        }
        "this" | "super" => "`this`/`super`: the enclosing class does not type".into(),
        "object_creation_expression" => {
            let class = link.child_by_field_name("type").map(|t| base_of(txt(original, &t))).unwrap_or_default();
            if link.children(&mut link.walk()).any(|c| c.kind() == "class_body") {
                "anonymous class creation".into()
            } else if svc.type_resolves(file, reduced, &class) {
                format!("`new {class}(..)` with the class known: the creation does not type (generics or arguments)")
            } else {
                format!("`new {class}(..)`: class unknown to the index")
            }
        }
        "method_invocation" | "field_access" => {
            let member = link.child_by_field_name("name").or_else(|| link.child_by_field_name("field")).map(|n| txt(original, &n).to_string()).unwrap_or_default();
            let object = link.child_by_field_name("object");
            // a call with no object: the own class's (or an inherited, or a static import) method
            let Some(object) = object else {
                let args = link.child_by_field_name("arguments");
                return own_call(svc, file, original, reduced, &member, args.as_ref());
            };
            let Some(binary) = prev.map(str::to_string).or_else(|| ty(reduced, &object)) else {
                return "the object of the link does not type".into();
            };
            let Some((_, tables)) = svc.header_type(file, "", &binary.replace('/', ".")) else {
                return format!("tables of `{}` not reachable (name clash, nested type or no source)", simple(&binary));
            };
            let methods: Vec<_> = tables.iter().flat_map(|t| t.methods.iter().chain(t.fields.iter())).filter(|m| m.name == member).collect();
            if methods.is_empty() {
                return format!("`{member}` not found in the hierarchy of `{}`", simple(&binary));
            }
            let args: Vec<Node> = link.child_by_field_name("arguments").map(|a| { let mut c = a.walk(); a.children(&mut c).filter(|x| x.is_named() && !x.kind().ends_with("comment")).collect() }).unwrap_or_default();
            let is_call = link.kind() == "method_invocation";
            if is_call {
                if args.iter().any(|a| matches!(a.kind(), "lambda_expression" | "method_reference")) {
                    return format!("`{member}(…)` with a lambda or method reference argument: overload/generic inference not completed");
                }
                if let Some(bad) = args.iter().find(|a| ty(reduced, a).is_none()) {
                    return format!("`{member}(…)`: argument `{}` does not type", txt(original, bad).chars().take(30).collect::<String>());
                }
                let same_arity: Vec<_> = methods.iter().filter(|m| m.params.len() == args.len()).collect();
                if same_arity.is_empty() {
                    return format!("`{member}`: no overload takes {} arguments", args.len());
                }
                if same_arity.len() > 1 {
                    return format!("`{member}`: {} overloads of that arity, none chosen", same_arity.len());
                }
                let r = &same_arity[0].return_type;
                if is_type_variable(&r.binary_name) || r.type_args.iter().any(|a| is_type_variable(&a.binary_name)) {
                    return format!("`{member}` returns a type variable (`{}`) that the receiver's type arguments do not bind", r.binary_name);
                }
                return format!("`{member}` found, one candidate, concrete return `{}`: the inference still fails (defect)", simple(&r.binary_name));
            }
            if index == 0 { "field access".into() } else { format!("field `{member}` found: the access does not type") }
        }
        k => format!("expression of kind `{k}`"),
    }
}

fn own_call(svc: &IndexService, file: &str, original: &str, reduced: &str, member: &str, args: Option<&Node>) -> String {
    let n = args.map_or(0, |a| a.named_child_count());
    if let Some(a) = args {
        let mut c = a.walk();
        if a.children(&mut c).any(|x| matches!(x.kind(), "lambda_expression" | "method_reference")) {
            return format!("own call `{member}(…)` with a lambda or method reference argument");
        }
        let mut c2 = a.walk();
        let bad = a.children(&mut c2).filter(|x| x.is_named()).find(|x| svc.expression_type(file, reduced, x.start_byte(), x.end_byte()).is_none());
        if let Some(bad) = bad {
            return format!("own call `{member}(…)`: argument `{}` does not type", txt(original, &bad).chars().take(30).collect::<String>());
        }
    }
    // a static import of that name?
    let imported = original.lines().filter(|l| l.trim_start().starts_with("import static ")).find(|l| {
        let l = l.trim().trim_end_matches(';');
        l.ends_with(&format!(".{member}")) || l.ends_with(".*")
    });
    match imported {
        Some(line) => {
            let class = line.trim().trim_end_matches(';').trim_start_matches("import static ").rsplit_once('.').map(|x| x.0.to_string()).unwrap_or_default();
            let simple_class = class.rsplit('.').next().unwrap_or("").to_string();
            if svc.type_name_matches(file, &simple_class, 20).iter().any(|c| *c == simple_class) || svc.type_resolves(file, reduced, &simple_class) {
                format!("static import `{class}.{member}`: the class is known but the call does not type (defect)")
            } else {
                format!("static import from `{class}`: a library without sources (not a defect)")
            }
        }
        None => format!("own call `{member}({n} args)`: not in the class or its supertypes, no static import (inherited from an unresolved supertype?)"),
    }
}

#[test]
#[ignore = "needs the public corpus: see the module docs"]
fn m3_the_true_trace_of_the_misses() {
    let wanted = env_num("BENNU_MLB_N", 500);
    let only = env("BENNU_MLB_ONLY");
    let svc = IndexService::global();
    let (mut asked, mut hit, mut hit_with_line_cut) = (0usize, 0usize, 0usize);
    let mut causes: BTreeMap<(String, bool), (usize, Vec<String>)> = BTreeMap::new();
    for spec in PROJECTS.iter().filter(|p| only.as_deref().is_none_or(|o| o == p.name)) {
        let (root, held) = assemble(spec);
        open_and_wait(&root);
        let mut all = Vec::new();
        for (hi, h) in held.iter().enumerate() {
            for s in sites(&h.text) {
                if s.position == Position::MemberAfterDot && is_plain_name(&h.text[s.start..s.end]) {
                    all.push((hi, s));
                }
            }
        }
        let stride = (all.len() / wanted.max(1)).max(1);
        for (hi, s) in all.iter().step_by(stride).take(wanted) {
            let h = &held[*hi];
            let file = h.path.to_string_lossy().into_owned();
            let name = &h.text[s.start..s.end];
            let reduced = format!("{}{}", &h.text[..s.start], &h.text[after_call(&h.text, s.end)..]);
            asked += 1;
            // the editor, typing left to right, has nothing of the line after the caret but what it closes itself: this buffer has the
            // rest of the line gone
            let line_end = h.text[s.start..].find('\n').map_or(h.text.len(), |p| s.start + p);
            let cut_line = format!("{}{}", &h.text[..s.start], &h.text[line_end..]);
            let hit_line = popup(svc, &file, &cut_line, s.start).iter().take(300).any(|(n, ..)| n == name);
            let hit_member_only = popup(svc, &file, &reduced, s.start).iter().take(300).any(|(n, ..)| n == name);
            hit_with_line_cut += usize::from(hit_line);
            if hit_member_only {
                hit += 1;
                continue;
            }
            let mut v = trace(svc, &file, &h.text, &reduced, s);
            if env("BENNU_MLB_SHOW").is_some() && v.cause.starts_with("static access") {
                let lp = popup(svc, &file, &cut_line, s.start);
                let ctx_start = h.text[..s.start].rfind('\n').map_or(0, |p| p + 1);
                eprintln!("[static] {}:{} want {name:?} line {:?} popup len {} first {:?}", h.path.file_name().unwrap().to_string_lossy(), h.text[..s.start].matches('\n').count() + 1, &h.text[ctx_start..line_end].trim().chars().take(90).collect::<String>(), lp.len(), lp.iter().take(6).map(|x| x.0.as_str()).collect::<Vec<_>>());
            }
            if hit_line {
                v.cause = format!("BENCH: the rest of the line left in the buffer breaks the popup (found when the line is cut) — {}", v.cause);
                v.bench = true;
            }
            let line = h.text[..s.start].matches('\n').count() + 1;
            let e = causes.entry((v.cause, v.bench)).or_default();
            e.0 += 1;
            if e.1.len() < 3 {
                e.1.push(format!("{}:{line} — link `{}` — `{}.{name}`", h.path.file_name().unwrap().to_string_lossy(), v.link.chars().take(60).collect::<String>(), h.text[..s.start].lines().last().unwrap_or("").trim().chars().rev().take(50).collect::<String>().chars().rev().collect::<String>()));
            }
        }
        svc.forget(&root.to_string_lossy());
    }
    let missed = asked - hit;
    let mut rows: Vec<_> = causes.into_iter().collect();
    rows.sort_by(|a, b| b.1 .0.cmp(&a.1 .0));
    let bench: usize = rows.iter().filter(|r| r.0 .1).map(|r| r.1 .0).sum();
    let mut out = format!("## M3 misses — the true trace ({missed} misses of {asked} sites; popup lists the member in {hit} with the member cut out, in {hit_with_line_cut} with the rest of the line cut; {bench} misses are the bench's buffer)\n\n| cause | in the original buffer | misses | share of misses |\n|---|---|---|---|\n");
    for ((c, b), (n, _)) in &rows {
        out.push_str(&format!("| {c} | {} | {n} | {:.1}% |\n", if *b { "types (bench)" } else { "does not type (inference)" }, pct(*n, missed)));
    }
    out.push_str("\n### Examples\n");
    for ((c, b), (_, ex)) in &rows {
        out.push_str(&format!("\n**{c}** ({})\n", if *b { "bench" } else { "inference" }));
        for e in ex {
            out.push_str(&format!("- {e}\n"));
        }
    }
    let path = write_result("m3_trace.md", &out);
    eprintln!("{out}\nreport: {}", path.display());
}
