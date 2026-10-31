//! The header must be the same text the lab built the model's training prompts with. `fixtures/header_parity.tsv`
//! is written by the lab (`bennu-flc-stub header-golden <file>`): neutral snippets, the member lists its index gave
//! for them, and the header it produced. This renders each case from those same lists and compares the bytes.

use bennu_flc::header::{build_in, HeaderMember, TypeInfo, TypeLookup};
use std::collections::HashMap;

const GOLDEN: &str = include_str!("fixtures/header_parity.tsv");

fn unescape(s: &str) -> String {
    let mut out = String::new();
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some(o) => out.push(o),
            None => {}
        }
    }
    out
}

#[derive(Default)]
struct Case {
    name: String,
    outside: String,
    code: String,
    focus: Option<String>,
    partial: String,
    resolve: HashMap<String, String>,
    types: HashMap<String, TypeInfo>,
    expect: String,
}

struct Lookup<'a>(&'a Case);

impl TypeLookup for Lookup<'_> {
    fn resolve(&self, name: &str) -> Option<TypeInfo> {
        self.0.types.get(self.0.resolve.get(name)?).cloned()
    }
}

fn cases() -> Vec<Case> {
    let mut out = Vec::new();
    let mut cur = Case::default();
    let mut key = String::new();
    for line in GOLDEN.lines() {
        let f: Vec<&str> = line.split('\t').collect();
        match f[0] {
            "CASE" => cur = Case { name: f[1].to_string(), ..Case::default() },
            "OUTSIDE" => cur.outside = unescape(f.get(1).unwrap_or(&"")),
            "CODE" => cur.code = unescape(f[1]),
            "FOCUS" => cur.focus = (f[1] != "-").then(|| f[1].to_string()),
            "PARTIAL" => cur.partial = f.get(1).unwrap_or(&"").to_string(),
            "RESOLVE" => {
                cur.resolve.insert(f[1].to_string(), f[2].to_string());
            }
            "TYPE" => {
                key = f[1].to_string();
                cur.types.insert(key.clone(), TypeInfo { key: key.clone(), members: Vec::new() });
            }
            "MEM" => {
                let params = f.get(3).filter(|p| !p.is_empty()).map(|p| p.split(',').map(str::to_string).collect()).unwrap_or_default();
                cur.types.get_mut(&key).unwrap().members.push(HeaderMember { name: f[1].to_string(), is_method: f[2] == "1", params });
            }
            "EXPECT" => cur.expect = unescape(f.get(1).unwrap_or(&"")),
            "END" => out.push(std::mem::take(&mut cur)),
            _ => {}
        }
    }
    out
}

#[test]
fn every_case_renders_the_same_bytes_as_the_lab() {
    let all = cases();
    assert!(all.len() >= 8, "the fixture lost cases: {}", all.len());
    for case in &all {
        let focus = case.focus.as_ref().map(|k| case.types[k].clone());
        let got = build_in(&Lookup(case), &case.outside, &case.code, focus.as_ref(), &case.partial);
        assert_eq!(got, case.expect, "case {}", case.name);
    }
}

#[test]
fn the_fixture_exercises_the_budget_and_the_cut() {
    let all = cases();
    let by = |n: &str| all.iter().find(|c| c.name == n).unwrap_or_else(|| panic!("no case {n}"));
    // the budget stops at the first line that does not fit, so a later small type is not squeezed in
    assert_eq!(by("budget").expect.lines().count(), 1);
    // an oversized focus line is cut at a member boundary and still ends the line
    let cut = &by("oversized_focus").expect;
    assert!(cut.len() <= bennu_flc::header::HEADER_BUDGET && cut.ends_with(")\n"), "{cut:?}");
}
