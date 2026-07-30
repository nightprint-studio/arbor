//! Matching identifiers: the glob the filters are written in, and the fold that
//! decides when two names are one object.
//!
//! The glob is implemented here rather than pulled in, for two reasons that both
//! matter more than the twenty lines it costs. It is the only pattern language a
//! user of this product will ever type into a diff filter, so it has to keep
//! meaning exactly `*` and `?` and nothing else — no `**`, no `{a,b}`, no
//! character classes silently changing what an old template matched. And a
//! filter that decides which tables get compared is load-bearing for the verdict:
//! a pattern that quietly matches more than the user thinks hides differences.

/// Does `value` match `pattern`, where `*` is any run of characters (including
/// none) and `?` is exactly one?
///
/// Everything else is literal. There is no escape character: a database
/// identifier containing a `*` is not a thing anybody has, and inventing `\*`
/// would make every Windows-ish path pasted into a filter behave surprisingly.
pub fn glob_match(pattern: &str, value: &str) -> bool {
    let p: Vec<char> = pattern.chars().collect();
    let v: Vec<char> = value.chars().collect();

    // Classic two-pointer walk with a single backtrack point. `star` remembers
    // where the last `*` was and `resume` how much of the value it has been made
    // to swallow so far, which is what makes `*_x*` terminate on a long name
    // instead of exploring every split.
    let (mut pi, mut vi) = (0usize, 0usize);
    let mut star: Option<usize> = None;
    let mut resume = 0usize;

    while vi < v.len() {
        if pi < p.len() && (p[pi] == '?' || p[pi] == v[vi]) {
            pi += 1;
            vi += 1;
        } else if pi < p.len() && p[pi] == '*' {
            star = Some(pi);
            pi += 1;
            resume = vi;
        } else if let Some(s) = star {
            pi = s + 1;
            resume += 1;
            vi = resume;
        } else {
            return false;
        }
    }

    // Trailing `*`s match the empty rest.
    while pi < p.len() && p[pi] == '*' {
        pi += 1;
    }
    pi == p.len()
}

/// Does `value` match any of `patterns`?
///
/// An empty pattern list matches **nothing** — the caller decides what "no
/// patterns" means for its mode, and the two readings ("everything" for an
/// include list, "nothing" for an exclude list) must not be baked in here.
pub fn matches_any(patterns: &[String], value: &str, case_insensitive: bool) -> bool {
    let folded = fold_name(value, case_insensitive);
    patterns.iter().any(|p| glob_match(&fold_name(p, case_insensitive), &folded))
}

/// The form a name is compared and keyed by.
///
/// Case folding is a per-comparison decision rather than a constant, because both
/// answers are right somewhere: two PostgreSQL databases hold `parametri` and
/// `Parametri` as genuinely different tables, while the same table read from
/// Oracle and from PostgreSQL arrives as `PARAMETRI` and `parametri` and is one
/// object. Defaulting to insensitive (see [`crate::config::DiffConfig`]) keeps
/// the cross-engine case working out of the box; the switch is there for the
/// person comparing two databases of one engine who means it.
pub fn fold_name(name: &str, case_insensitive: bool) -> String {
    if case_insensitive {
        name.to_lowercase()
    } else {
        name.to_string()
    }
}

/// A list of names in the form it is *compared* in.
///
/// Comparison folds, reporting does not: a difference is decided on the folded
/// names and then shown with the spellings each server actually gave, because
/// "the index is on `COD`" and "the index is on `cod`" is the sort of detail
/// somebody is about to paste into a script.
pub fn fold_all(names: &[String], case_insensitive: bool) -> Vec<String> {
    names.iter().map(|n| fold_name(n, case_insensitive)).collect()
}

/// The names in `left` that `right` does not have, in `left`'s own order.
pub fn missing_from(left: &[String], right: &[String], case_insensitive: bool) -> Vec<String> {
    let present: Vec<String> = right.iter().map(|n| fold_name(n, case_insensitive)).collect();
    left.iter()
        .filter(|n| !present.contains(&fold_name(n, case_insensitive)))
        .cloned()
        .collect()
}
