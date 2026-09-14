//! Which class declares which static method.
//!
//! Resolution through imports is only as good as the owners it is asked about: `import static
//! org.mockito.Mockito.*;` reaches `any`, because Mockito extends ArgumentMatchers and an on-demand
//! static import brings inherited statics with it — so `any` has three legitimate owners, and a table
//! that listed only ArgumentMatchers would go silent on the most common way Mockito is imported.

pub(crate) const MOCKITO: &str = "org.mockito.Mockito";
pub(crate) const BDD_MOCKITO: &str = "org.mockito.BDDMockito";
pub(crate) const ARGUMENT_MATCHERS: &str = "org.mockito.ArgumentMatchers";
pub(crate) const ADDITIONAL_MATCHERS: &str = "org.mockito.AdditionalMatchers";
pub(crate) const MOCKITO_HAMCREST: &str = "org.mockito.hamcrest.MockitoHamcrest";

/// `when`, `verify` and the `do…` family. BDDMockito extends Mockito, so it reaches them too.
pub(crate) const STUBBING: &[&str] = &[MOCKITO, BDD_MOCKITO];
/// `given` and `then`, which only BDDMockito declares.
pub(crate) const BDD: &[&str] = &[BDD_MOCKITO];
/// The single-value `eq` the fix writes — ArgumentMatchers', reachable through its two subclasses.
/// Not AdditionalMatchers: its `eq` takes a delta.
pub(crate) const EQ: &[&str] = &[ARGUMENT_MATCHERS, MOCKITO, BDD_MOCKITO];

/// Every class of Mockito's own. A static on-demand import of one of these cannot make a name
/// ambiguous with another — they agree with each other by construction.
pub(crate) const LIBRARY: &[&str] =
    &[MOCKITO, BDD_MOCKITO, ARGUMENT_MATCHERS, ADDITIONAL_MATCHERS, MOCKITO_HAMCREST];

/// The stubbers — `doReturn(x)` and the calls that chain one more answer onto it.
pub(crate) const DO_METHODS: &[&str] =
    &["doReturn", "doThrow", "doNothing", "doAnswer", "doCallRealMethod"];

/// Static on-demand imports that sit beside Mockito's in real test files, with the names among the
/// ones this crate recognises that each of them ALSO declares.
///
/// Any other static on-demand import is an unknown: it may declare an `any` of its own (Hamcrest
/// does), and then a bare `any()` is not certainly Mockito's. Listing the usual neighbours keeps the
/// checks working in the ordinary JUnit + AssertJ file instead of going silent in all of them.
pub(crate) const NEIGHBOURS: &[(&str, &[&str])] = &[
    ("org.junit.Assert", &[]),
    ("org.junit.Assume", &[]),
    ("org.junit.jupiter.api.Assertions", &[]),
    ("org.junit.jupiter.api.Assumptions", &[]),
    ("org.hamcrest.MatcherAssert", &[]),
    ("org.assertj.core.api.Assertions", &["not"]),
    ("org.assertj.core.api.BDDAssertions", &["then", "not"]),
];

const ARGUMENT: &[&str] = &[ARGUMENT_MATCHERS, MOCKITO, BDD_MOCKITO];
const ARGUMENT_OR_ADDITIONAL: &[&str] =
    &[ARGUMENT_MATCHERS, MOCKITO, BDD_MOCKITO, ADDITIONAL_MATCHERS];
const THAT: &[&str] = &[ARGUMENT_MATCHERS, MOCKITO, BDD_MOCKITO, MOCKITO_HAMCREST];
const ADDITIONAL: &[&str] = &[ADDITIONAL_MATCHERS];

const PLAIN_MATCHERS: &[&str] = &[
    "any", "anyInt", "anyString", "anyList", "anyMap", "anySet", "anyCollection", "anyIterable",
    "anyBoolean", "anyByte", "anyChar", "anyDouble", "anyFloat", "anyLong", "anyShort", "anyObject",
    "anyVararg", "anyListOf", "anySetOf", "anyMapOf", "anyCollectionOf", "anyIterableOf", "same",
    "isNull", "isNotNull", "notNull", "isA", "nullable", "contains", "matches", "startsWith",
    "endsWith", "refEq",
];
const THAT_MATCHERS: &[&str] = &[
    "argThat", "booleanThat", "byteThat", "charThat", "doubleThat", "floatThat", "intThat",
    "longThat", "shortThat",
];
const ADDITIONAL_ONLY: &[&str] =
    &["and", "or", "not", "gt", "geq", "lt", "leq", "aryEq", "cmpEq", "find"];

/// The classes that declare a matcher of this name, or `None` when it is not a matcher we know.
pub(crate) fn matcher_owners(name: &str) -> Option<&'static [&'static str]> {
    if name == "eq" {
        Some(ARGUMENT_OR_ADDITIONAL)
    } else if PLAIN_MATCHERS.contains(&name) {
        Some(ARGUMENT)
    } else if THAT_MATCHERS.contains(&name) {
        Some(THAT)
    } else if ADDITIONAL_ONLY.contains(&name) {
        Some(ADDITIONAL)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_matcher_is_owned_by_the_classes_that_declare_it() {
        assert!(matcher_owners("any").unwrap().contains(&MOCKITO), "inherited through Mockito");
        assert!(matcher_owners("argThat").unwrap().contains(&MOCKITO_HAMCREST));
        assert_eq!(matcher_owners("gt"), Some(ADDITIONAL));
        assert!(matcher_owners("eq").unwrap().contains(&ADDITIONAL_MATCHERS));
        assert_eq!(matcher_owners("thenReturn"), None);
    }
}
