//! The abbreviations a Java file expands — `psf`, `sout`, `psvm`.
//!
//! A table, not a feature: what belongs here is the **vocabulary**, and everything that turns a
//! row into something an editor inserts is the caller's. Same split as `bennu-maven`'s lifecycle
//! and `bennu-cargo`'s command table, for the same reason — a frontend that carried its own copy
//! would eventually offer an expansion the backend does not produce.
//!
//! ## Why a table and not a setting
//!
//! These are IntelliJ's own Java live-template abbreviations, and that is the whole point of the
//! choice: somebody who has typed `psf` for fifteen years should get `public static final` here
//! too, without configuring anything. A file of user-defined templates is a different feature and
//! it can read this list as its defaults.
//!
//! ## What is deliberately not here
//!
//! Abbreviations that need to **read the surrounding code** to be right — IntelliJ's `iter`, which
//! infers the collection in scope, or `soutv`, which prints the variable on the line above. A
//! version of those that guesses is worse than not offering them: it writes a name that is not
//! there, in code that then does not compile, and the abbreviation is supposed to save typing
//! rather than start a correction.
//!
//! The bodies are **LSP snippet syntax** (`$0`, `${1:name}`) because that is the one snippet
//! grammar this workspace already parses and tests (`bennu-lsp`'s `snippet` module). Nothing here
//! parses it; the caller does, once, the same way it does for a language server's own completions.

/// One abbreviation: what is typed, what it expands to, and how the popup describes it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Template {
    /// The abbreviation — what the popup filters on and what is replaced.
    pub abbrev: &'static str,
    /// The expansion, in LSP snippet syntax.
    pub body: &'static str,
    /// The right-hand column of the popup: what you get, in the fewest words that are still true.
    pub detail: &'static str,
}

/// Java's abbreviations, in the order a tie is broken.
///
/// Order is load-bearing where one abbreviation is a prefix of another: typing `psf` matches
/// `psf`, `psfi` and `psfs`, and the bare one has to be first or the most common expansion in Java
/// is the third row down.
pub const TEMPLATES: &[Template] = &[
    // ── modifiers, which is what an abbreviation is really for ────────────────
    Template { abbrev: "psf", body: "public static final $0", detail: "public static final" },
    Template {
        abbrev: "psfi",
        body: "public static final int $0",
        detail: "public static final int",
    },
    Template {
        abbrev: "psfs",
        body: "public static final String $0",
        detail: "public static final String",
    },
    Template { abbrev: "prsf", body: "private static final $0", detail: "private static final" },
    Template {
        abbrev: "prsfi",
        body: "private static final int $0",
        detail: "private static final int",
    },
    Template {
        abbrev: "prsfs",
        body: "private static final String $0",
        detail: "private static final String",
    },
    Template { abbrev: "psvm", body: "public static void main(String[] args) {\n    $0\n}", detail: "main method" },
    // ── printing, which is what the other half is for ─────────────────────────
    Template { abbrev: "sout", body: "System.out.println($0);", detail: "System.out.println()" },
    Template { abbrev: "souf", body: "System.out.printf(\"$0\");", detail: "System.out.printf()" },
    Template { abbrev: "serr", body: "System.err.println($0);", detail: "System.err.println()" },
    // ── the three statements nobody enjoys typing ─────────────────────────────
    Template {
        abbrev: "fori",
        body: "for (int i = 0; i < ${1:n}; i++) {\n    $0\n}",
        detail: "indexed for loop",
    },
    Template {
        abbrev: "ifn",
        body: "if (${1:value} == null) {\n    $0\n}",
        detail: "if null",
    },
    Template {
        abbrev: "inn",
        body: "if (${1:value} != null) {\n    $0\n}",
        detail: "if not null",
    },
    Template { abbrev: "thr", body: "throw new $0", detail: "throw new" },
];

/// The templates whose abbreviation starts with `prefix`, in table order.
///
/// An **empty prefix yields nothing**, and that is the rule that keeps this from being noise: the
/// popup opened on a bare caret is already a list of everything in scope, and thirteen
/// abbreviations at the top of it would be thirteen rows nobody asked for. They are for somebody
/// who has started typing one.
pub fn matching(prefix: &str) -> Vec<&'static Template> {
    if prefix.is_empty() {
        return Vec::new();
    }
    let lower = prefix.to_ascii_lowercase();
    TEMPLATES.iter().filter(|t| t.abbrev.starts_with(&lower)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_prefix_offers_its_family_with_the_bare_one_first() {
        let found: Vec<&str> = matching("psf").iter().map(|t| t.abbrev).collect();
        assert_eq!(found, ["psf", "psfi", "psfs"]);
    }

    /// The rule that keeps the popup usable: nothing without a prefix.
    #[test]
    fn an_empty_prefix_offers_nothing() {
        assert!(matching("").is_empty());
    }

    /// Typed in whatever case the caret is in — an abbreviation is a shorthand, not an identifier.
    #[test]
    fn the_match_is_case_insensitive() {
        assert_eq!(matching("PSVM").len(), 1);
        assert_eq!(matching("PSVM")[0].abbrev, "psvm");
    }

    #[test]
    fn a_prefix_nothing_starts_with_offers_nothing() {
        assert!(matching("qqq").is_empty());
    }

    /// Every body has somewhere for the caret to end up. One without would insert its text and
    /// leave the caret after the closing brace, which is the one place you never want it.
    #[test]
    fn every_template_says_where_the_caret_goes() {
        for t in TEMPLATES {
            assert!(t.body.contains("$0"), "{} has no $0", t.abbrev);
        }
    }

    /// An abbreviation that is a prefix of another must come first, or the commoner expansion is
    /// the second row.
    #[test]
    fn a_shorter_abbreviation_precedes_the_one_it_prefixes() {
        for (i, t) in TEMPLATES.iter().enumerate() {
            for other in &TEMPLATES[..i] {
                assert!(
                    !t.abbrev.starts_with(other.abbrev) || other.abbrev.len() <= t.abbrev.len(),
                    "{} sits before {}",
                    other.abbrev,
                    t.abbrev,
                );
            }
            for other in &TEMPLATES[i + 1..] {
                assert!(
                    !t.abbrev.starts_with(other.abbrev),
                    "{} is a prefix of {} and comes after it",
                    other.abbrev,
                    t.abbrev,
                );
            }
        }
    }
}
