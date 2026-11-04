//! Finding out how a project actually names things.
//!
//! The bulk "Fix naming" has to work for a project that never configured a convention. Two honest
//! answers exist: the language's **standard** (what its community writes), and what the project
//! itself **already does**. The second matters because a legacy tree that spells every method
//! `get_user_name` is not "wrong" to a tool that insists on `getUserName` — it is a tree with a
//! convention nobody wrote down, and rewriting a thousand methods against it would be the loudest
//! way to be unhelpful.
//!
//! So this reads the names the code declares, per kind of declaration, and keeps a convention only
//! when the evidence is strong:
//!
//! * a name counts as a **vote** only when exactly one candidate convention accepts it — `order`
//!   is camelCase and snake_case and lowercase at once, and counts for nobody;
//! * a convention is **detected** when it holds at least [`MIN_VOTES`] votes and at least
//!   [`MIN_SHARE`] of them. Anything weaker falls back to the standard for that kind.
//!
//! Pure data in, data out, so it is testable without a project.

use std::collections::BTreeMap;

use crate::convention::Convention;
use crate::target::Target;

/// Fewest decisive names a kind needs before its convention is read off the project.
pub const MIN_VOTES: usize = 8;

/// Share of the decisive names the leading convention must hold.
pub const MIN_SHARE: f32 = 0.6;

/// The conventions a name can vote for.
const CANDIDATES: [Convention; 4] =
    [Convention::Camel, Convention::Pascal, Convention::UpperSnake, Convention::LowerSnake];

/// A kind of declaration whose convention the project's own code settled.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Detected {
    pub target: Target,
    pub convention: Convention,
    /// How many names settled it (the decisive ones).
    pub votes: usize,
    /// The leading convention's share of those, `0.0..=1.0`.
    pub share: f32,
}

/// The kinds that are never read off the project: a package segment is a directory name, and a
/// type parameter is a letter — neither has a house style worth inferring.
const STANDARD_ONLY: [Target; 2] = [Target::Package, Target::TypeParameter];

/// Read each kind's convention off the names declared under it.
///
/// Kinds with too little evidence are simply absent from the result — the caller keeps the
/// standard for them.
pub fn detect_conventions(names: &BTreeMap<Target, Vec<String>>) -> Vec<Detected> {
    let mut out = Vec::new();
    for (&target, list) in names {
        if STANDARD_ONLY.contains(&target) {
            continue;
        }
        let mut votes = [0usize; CANDIDATES.len()];
        for name in list {
            let accepting: Vec<usize> = CANDIDATES
                .iter()
                .enumerate()
                .filter(|(_, c)| c.accepts(name))
                .map(|(i, _)| i)
                .collect();
            if let [only] = accepting.as_slice() {
                votes[*only] += 1;
            }
        }
        let total: usize = votes.iter().sum();
        let Some((lead, &lead_votes)) = votes.iter().enumerate().max_by_key(|(_, v)| **v) else {
            continue;
        };
        if total < MIN_VOTES {
            continue;
        }
        let share = lead_votes as f32 / total as f32;
        if share >= MIN_SHARE {
            out.push(Detected { target, convention: CANDIDATES[lead], votes: total, share });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(target: Target, list: &[&str]) -> BTreeMap<Target, Vec<String>> {
        BTreeMap::from([(target, list.iter().map(|s| s.to_string()).collect())])
    }

    #[test]
    fn a_project_of_snake_case_methods_is_detected_as_such() {
        let found = detect_conventions(&names(
            Target::Method,
            &[
                "get_user", "set_user", "load_all", "save_one", "find_by_id", "list_items", "run_job",
                "stop_job", "get_name", "set_name",
            ],
        ));
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].convention, Convention::LowerSnake);
        assert_eq!(found[0].votes, 10);
        assert!(found[0].share > 0.99);
    }

    #[test]
    fn single_word_names_vote_for_nobody() {
        // `order`, `list`, `run` are camelCase AND snake_case: no evidence either way.
        let found = detect_conventions(&names(
            Target::Method,
            &["order", "list", "run", "stop", "load", "save", "find", "get", "set", "put", "add"],
        ));
        assert!(found.is_empty(), "{found:?}");
    }

    #[test]
    fn too_few_decisive_names_leave_the_standard_alone() {
        let found = detect_conventions(&names(Target::Method, &["get_user", "set_user", "load_all"]));
        assert!(found.is_empty());
    }

    #[test]
    fn a_split_project_has_no_convention() {
        // Half and half is not a convention, it is a mess — the standard is the better offer.
        let found = detect_conventions(&names(
            Target::Method,
            &[
                "getUser", "setUser", "loadAll", "saveOne", "get_name", "set_name", "list_all", "find_one",
            ],
        ));
        assert!(found.is_empty(), "{found:?}");
    }

    #[test]
    fn a_clear_majority_wins_over_a_few_strays() {
        let found = detect_conventions(&names(
            Target::Method,
            &[
                "getUser", "setUser", "loadAll", "saveOne", "findById", "listItems", "runJob", "stopJob",
                "get_name",
            ],
        ));
        assert_eq!(found[0].convention, Convention::Camel);
    }

    #[test]
    fn packages_and_type_parameters_are_never_inferred() {
        let list = ["com_acme", "org_x", "net_y", "io_z", "de_a", "fr_b", "it_c", "es_d", "uk_e"];
        assert!(detect_conventions(&names(Target::Package, &list)).is_empty());
        assert!(detect_conventions(&names(Target::TypeParameter, &list)).is_empty());
    }

    #[test]
    fn constants_in_upper_snake_are_detected() {
        let found = detect_conventions(&names(
            Target::Constant,
            &[
                "MAX_SIZE", "MIN_SIZE", "DEFAULT_NAME", "API_KEY", "BASE_URL", "TIMEOUT_MS", "RETRY_COUNT",
                "LOG_LEVEL", "USER_AGENT",
            ],
        ));
        assert_eq!(found[0].convention, Convention::UpperSnake);
    }
}
