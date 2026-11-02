//! Why the popup does not list the member that was written (M3's "truth in popup" ceiling).
//!
//! For each member site whose true member is missing from the popup asked right after the dot, the receiver is classified:
//! not typed by the index (and why: a library without sources, a type variable, a lambda parameter, a call chain whose
//! last link does not bind, ...), or typed but the member is not listed (not in the type's tables, filtered by visibility
//! or staticness, or further down than the first 300 items). Measurement only — nothing in production is touched.
//!
//! `cargo test -p bennu-be mlbench::m3_misses -- --ignored --nocapture`  (`BENNU_MLB_N`, `BENNU_MLB_ONLY` as in M3)

use std::collections::BTreeMap;

use super::common::*;
use super::m3_rank::{after_call, is_plain_name, popup, simple};
use super::sites::{sites, Position};
use crate::index_service::IndexService;

fn word_at(text: &str, i: usize, w: &str) -> bool {
    let b = text[..i].chars().next_back();
    let a = text[i + w.len()..].chars().next();
    !b.is_some_and(|c| c.is_alphanumeric() || c == '_') && !a.is_some_and(|c| c.is_alphanumeric() || c == '_')
}

/// `name` is a lambda parameter of a lambda opened shortly before `at`.
pub(super) fn is_lambda_param(text: &str, at: usize, name: &str) -> bool {
    let from = text[..at].char_indices().rev().nth(400).map_or(0, |(i, _)| i);
    let window = &text[from..at];
    window.match_indices("->").any(|(j, _)| {
        let head = window[..j].trim_end();
        if head.ends_with(name) && word_at(head, head.len() - name.len(), name) {
            return true;
        }
        head.ends_with(')') && head.rfind('(').is_some_and(|o| head[o..].match_indices(name).any(|(k, _)| word_at(head, o + k, name)))
    })
}

/// The declared type text of local/field/parameter `name` as written before `at`.
pub(super) fn declared_type(text: &str, at: usize, name: &str) -> Option<String> {
    let head = &text[..at];
    let mut search = head.len();
    while let Some(i) = head[..search].rfind(name) {
        search = i;
        if !word_at(head, i, name) {
            continue;
        }
        let after = head[i + name.len()..].trim_start();
        if !after.starts_with(['=', ';', ',', ')', ':']) {
            continue;
        }
        let before = head[..i].trim_end();
        // the type token: back over generics, arrays and dots
        let mut depth = 0i32;
        let start = before
            .char_indices()
            .rev()
            .take_while(|&(_, c)| {
                match c {
                    '>' => depth += 1,
                    '<' => depth -= 1,
                    _ => {}
                }
                depth > 0 || c.is_alphanumeric() || matches!(c, '_' | '.' | '<' | '>' | '[' | ']' | '?' | ',' | '$') || (c == ' ' && depth > 0)
            })
            .last()
            .map(|(k, _)| k)?;
        let ty = before[start..].trim();
        if ty.is_empty() || ["return", "new", "throw", "else", "case", "assert"].contains(&ty) || ty.starts_with(char::is_numeric) {
            continue;
        }
        return Some(ty.to_string());
    }
    None
}

pub(super) fn base_of(ty: &str) -> String {
    ty.split('<').next().unwrap_or(ty).trim_end_matches("[]").trim().rsplit('.').next().unwrap_or("").to_string()
}

fn jdk(binary: &str) -> bool {
    binary.starts_with("java/") || binary.starts_with("javax/")
}

struct Miss {
    cause: &'static str,
    example: String,
}

#[allow(clippy::too_many_arguments)]
fn classify(svc: &IndexService, file: &str, text: &str, s: &super::sites::Site, list_has_truth_late: bool, source: &str) -> Miss {
    let name = &text[s.start..s.end];
    let recv = s.target.clone().unwrap_or_default();
    let line_no = text[..s.start].matches('\n').count();
    let line = text.lines().nth(line_no).unwrap_or("").trim();
    let example = format!("`{}` — {}", &line[..line.len().min(110)], format_args!("{recv}.{name}"));
    let dot = text[..s.start].trim_end().len().saturating_sub(1);
    let recv_start = dot.saturating_sub(recv.len());
    let typed = svc.receiver_type_for(file, source, s.start);
    if let Some(binary) = typed {
        if list_has_truth_late {
            return Miss { cause: "typed; member listed but beyond the first 300 items", example };
        }
        let empty_tables = svc.header_type(file, "", &simple(&binary)).is_none();
        let known = svc.header_type(file, "", &simple(&binary)).is_some_and(|(_, tables)| tables.iter().any(|t| t.methods.iter().chain(t.fields.iter()).any(|m| m.name == name)));
        let cause = if known { "typed; member exists in the type's tables but the popup omits it (visibility / static / filter)" } else if empty_tables { "typed; type tables not reachable from the file (name clash or nested type)" } else { "typed; member absent from the tables of the type and its supertypes" };
        return Miss { cause, example: format!("{example} [receiver {binary}]") };
    }
    // the receiver does not type: why
    let cause: &'static str = if recv.starts_with("new ") && recv.ends_with(')') {
        let class = recv[4..].split(['(', '<']).next().unwrap_or("").trim();
        if svc.type_resolves(file, source, class) || !svc.type_name_matches(file, class, 3).is_empty() {
            "receiver is `new T(...)` with T known, yet the result does not type"
        } else {
            "receiver is `new T(...)` with T in a library without sources"
        }
    } else if recv.ends_with(')') {
        // a call chain: does the object of its last call type?
        let open = {
            let (mut depth, mut o) = (0i32, None);
            for (i, ch) in recv.char_indices().rev() {
                match ch {
                    ')' => depth += 1,
                    '(' => {
                        depth -= 1;
                        if depth == 0 {
                            o = Some(i);
                            break;
                        }
                    }
                    _ => {}
                }
            }
            o
        };
        match open.and_then(|o| recv[..o].rfind('.').map(|d| (o, d))) {
            None => "call chain: unqualified call of the own class does not type",
            Some((_, d)) => {
                let obj = &recv[..d];
                let static_type = obj.rsplit('.').next().filter(|t| t.starts_with(|c: char| c.is_ascii_uppercase()) && t.chars().all(|c| c.is_alphanumeric() || c == '_'));
                if let Some(t) = static_type {
                    return Miss { cause: if svc.type_resolves(file, source, t) { "call chain: static call `T.m(...)` on a type that resolves, result does not bind" } else { "call chain: static call `T.m(...)` on a type in a library without sources" }, example };
                }
                if let Some(b) = svc.expression_type(file, source, recv_start, recv_start + obj.len()) {
                    if jdk(&b.replace('.', "/")) || b.starts_with("java.") || b.starts_with("javax.") {
                        "call chain: last call is on a JDK type but its result does not bind (generic return / lambda)"
                    } else {
                        "call chain: last call is on a typed project/library type but its result does not bind"
                    }
                } else {
                    "call chain: an earlier link of the chain does not type"
                }
            }
        }
    } else if is_plain_name(&recv) {
        if is_lambda_param(text, s.start, &recv) {
            "receiver is a lambda parameter"
        } else {
            match declared_type(text, s.start, &recv) {
                None => "receiver is a field/inherited name with no declaration in the file",
                Some(ty) => {
                    let base = base_of(&ty);
                    if base == "var" {
                        "receiver declared with `var` (its initialiser does not type)"
                    } else if base.len() <= 2 && base.starts_with(|c: char| c.is_ascii_uppercase()) {
                        "receiver has a type-variable type"
                    } else if !svc.type_resolves(file, source, &base) && svc.type_name_matches(file, &base, 5).is_empty() {
                        "receiver's declared type is in a library without sources"
                    } else {
                        "receiver's declared type resolves but the variable does not type"
                    }
                }
            }
        }
    } else if recv.starts_with(|c: char| c.is_ascii_uppercase()) && recv.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '.') {
        if svc.type_resolves(file, source, recv.rsplit('.').next().unwrap_or(&recv)) {
            "static access on a type that resolves"
        } else {
            "static access on a type that is in a library without sources"
        }
    } else if recv.starts_with("this.") || recv.starts_with("super.") || recv == "this" {
        "this / super access"
    } else if recv.starts_with('(') {
        "parenthesised or cast receiver"
    } else if recv.starts_with("new ") {
        "receiver is an instance creation"
    } else if recv.contains('.') {
        "field-access chain (a.b.c) does not type"
    } else {
        "other receiver"
    };
    Miss { cause, example }
}

#[test]
#[ignore = "needs the public corpus: see the module docs"]
fn m3_why_the_popup_misses() {
    let wanted = env_num("BENNU_MLB_N", 500);
    let only = env("BENNU_MLB_ONLY");
    let svc = IndexService::global();
    let (mut asked, mut hit) = (0usize, 0usize);
    let (mut typed_total, mut hit_untyped) = (0usize, 0usize);
    let mut causes: BTreeMap<&'static str, (usize, Vec<String>)> = BTreeMap::new();
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
            let source = format!("{}{}", &h.text[..s.start], &h.text[after_call(&h.text, s.end)..]);
            let list = popup(svc, &file, &source, s.start);
            asked += 1;
            let typed = svc.receiver_type_for(&file, &source, s.start).is_some();
            typed_total += usize::from(typed);
            hit_untyped += usize::from(!typed && list.iter().take(300).any(|(n, ..)| n == name));
            match list.iter().position(|(n, ..)| n == name) {
                Some(p) if p < 300 => hit += 1,
                late => {
                    let m = classify(svc, &file, &h.text, s, late.is_some(), &source);
                    let e = causes.entry(m.cause).or_default();
                    e.0 += 1;
                    if e.1.len() < 3 {
                        e.1.push(format!("{} ({})", m.example, spec.name));
                    }
                }
            }
        }
        svc.forget(&root.to_string_lossy());
    }
    let missed = asked - hit;
    let mut rows: Vec<_> = causes.into_iter().collect();
    rows.sort_by(|a, b| b.1 .0.cmp(&a.1 .0));
    let mut out = format!("## M3 misses — {asked} member sites, popup lists the member in {hit} ({:.1}%), misses {missed}\n\n`receiver_type_for` (the receiver type M3 keys its tables by) types {typed_total} sites ({:.1}%); the popup lists the member at {hit_untyped} sites where it does not.\n\n| cause | misses | share of misses |\n|---|---|---|\n", pct(hit, asked), pct(typed_total, asked));
    for (c, (n, _)) in &rows {
        out.push_str(&format!("| {c} | {n} | {:.1}% |\n", pct(*n, missed)));
    }
    out.push_str("\n### Examples\n");
    for (c, (_, ex)) in &rows {
        out.push_str(&format!("\n**{c}**\n"));
        for e in ex {
            out.push_str(&format!("- {e}\n"));
        }
    }
    let path = write_result("m3_misses.md", &out);
    eprintln!("{out}\nreport: {}", path.display());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_lambda_parameter_is_told_from_a_local() {
        let t = "list.forEach(item -> item.run()); String s = x;";
        assert!(is_lambda_param(t, 24, "item"));
        let t2 = "map.forEach((k, v) -> k.run());";
        assert!(is_lambda_param(t2, 21, "k"));
        assert!(!is_lambda_param("String s = x; s.run();", 17, "s"));
    }

    #[test]
    fn the_declared_type_is_read_back_from_the_declaration() {
        let t = "void f(Map<String, List<Integer>> map) { map.get(1); }";
        assert_eq!(declared_type(t, 42, "map").as_deref(), Some("Map<String, List<Integer>>"));
        assert_eq!(declared_type("int x = 1; y.run();", 11, "y"), None);
        assert_eq!(base_of("java.util.List<String>[]"), "List");
    }
}
