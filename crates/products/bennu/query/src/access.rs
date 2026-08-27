//! Who can see what — the accessibility rules the queries share.
//!
//! Completion asks it to decide what to offer; override generation asks it to decide what can be
//! overridden. Both are the same question about the same JLS rules, so they are answered in one
//! place: two copies of "can this be seen from here" is two chances to disagree about somebody's
//! code, and the disagreement would show up as a member you can complete but not override.

/// Whether a `private` member declared in `declaring` is accessible from within `site`: true iff
/// they belong to the same top-level class — equal, or one nested in the other (its binary is the
/// other's with a `/`-boundary suffix). Package vs nesting is `/`-ambiguous in a binary, but two
/// *top-level* classes never prefix each other at a `/` boundary, so this only ever matches a real
/// same-class / nesting relationship.
pub fn same_top_level(declaring: &str, site: Option<&str>) -> bool {
    let Some(site) = site else { return false };
    declaring == site
        || site.starts_with(&format!("{declaring}/"))
        || declaring.starts_with(&format!("{site}/"))
}

/// Whether a package-private member declared in `declaring` is reachable from `site`.
///
/// **True when unsure**, which is the whole design of it. A binary joins the package and the type
/// nesting with the same `/`, so where the package ends can only be read off the naming
/// convention — the leading segments that do not begin with a capital. That is a convention and
/// not a fact, so it is allowed to HIDE a member only when both sides parse to a package and those
/// packages differ. A package named against the convention loses the filter, not the member.
///
/// What it is for: the JDK's own internals are package-private and they were all being offered.
/// `String.` opened on `COMPACT_STRINGS`, `LATIN1`, `UTF16` and `checkBoundsBeginEnd` — none of
/// which anyone outside `java.lang` can write.
pub fn same_package(declaring: &str, site: Option<&str>) -> bool {
    let (Some(site), Some(here)) = (site, package_of(declaring)) else {
        return true;
    };
    match package_of(site) {
        Some(theirs) => here == theirs,
        None => true,
    }
}

/// The package part of a binary name under the Java naming convention: the leading segments that
/// do not begin with an uppercase letter. `java/lang/String` → `java/lang`. `None` for a type in
/// the default package, or one whose first segment is already capitalised.
pub fn package_of(binary: &str) -> Option<&str> {
    let segments: Vec<&str> = binary.split('/').collect();
    if segments.len() < 2 {
        return None;
    }
    // The type begins at the first capitalised segment. With none — a type named against the
    // convention — the last segment is the type name, which is true of any binary with more than
    // one segment.
    let type_at = segments
        .iter()
        .position(|s| s.chars().next().is_some_and(char::is_uppercase))
        .unwrap_or(segments.len() - 1);
    if type_at == 0 {
        return None; // no package: the first segment is already the type
    }
    let end: usize = segments[..type_at].iter().map(|s| s.len() + 1).sum::<usize>() - 1;
    Some(&binary[..end])
}

#[cfg(test)]
mod tests {
    use super::{package_of, same_package};

    #[test]
    fn a_package_is_the_segments_before_the_first_capitalised_one() {
        assert_eq!(package_of("java/lang/String"), Some("java/lang"));
        assert_eq!(package_of("com/acme/Order"), Some("com/acme"));
        // Nesting joins with `/` too, and stops at the OUTER type — an inner class is in its
        // outer's package, which is the answer the accessibility rule wants.
        assert_eq!(package_of("com/acme/ApiClient/CollectionFormat"), Some("com/acme"));
    }

    #[test]
    fn a_type_with_no_package_has_none() {
        assert_eq!(package_of("String"), None);
        assert_eq!(package_of("Order/Inner"), None);
    }

    /// Against the convention, the last segment is taken as the type — the filter degrades to
    /// comparing the enclosing folder rather than throwing the question away.
    #[test]
    fn an_uncapitalised_type_still_yields_something() {
        assert_eq!(package_of("com/acme/order"), Some("com/acme"));
    }

    #[test]
    fn a_package_private_member_is_hidden_only_across_packages() {
        assert!(same_package("java/lang/String", Some("java/lang/Integer")));
        assert!(!same_package("java/lang/String", Some("com/acme/Order")));
    }

    /// Unsure means visible. Every one of these could hide a member somebody can legitimately
    /// write, and a completion that is missing what you wanted is worse than one carrying a little
    /// noise.
    #[test]
    fn unsure_keeps_the_member() {
        assert!(same_package("java/lang/String", None), "caret outside any type");
        assert!(same_package("String", Some("com/acme/Order")), "declarer has no package");
        assert!(same_package("com/acme/Order", Some("Scratch")), "site has no package");
    }
}
