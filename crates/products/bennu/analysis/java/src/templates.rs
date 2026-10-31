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
    // `pf` and `prf` are the non-static pair: a constructor-injected dependency is a `private final`
    // field, and in code written since that became the norm it is the commonest modifier run there is.
    Template { abbrev: "pf", body: "public final $0", detail: "public final" },
    Template { abbrev: "prf", body: "private final $0", detail: "private final" },
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

/// What an abbreviation expands to, which decides where it can be written.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Place {
    /// A modifier run or a method of a type's body: `psf`, `psvm`.
    Member,
    /// One or more statements of a method body: `sout`, `fori`.
    Statement,
}

impl Template {
    /// Where the expansion is valid Java.
    pub fn place(&self) -> Place {
        match self.abbrev {
            "sout" | "souf" | "serr" | "fori" | "ifn" | "inn" | "thr" => Place::Statement,
            _ => Place::Member,
        }
    }
}

/// Where the word being typed stands, as far as an abbreviation is concerned.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaretPlace {
    /// The first word of a member of a type's body.
    Member,
    /// The first word of a statement.
    Statement,
    /// Anywhere else: inside an expression, after a type, after a `.` — where no abbreviation can be written.
    Elsewhere,
}

/// Where the word that starts at `word_start` stands.
///
/// A word opens a member or a statement after `;`, `{`, `}`, a closing `)` (the header of an `if`, an annotation's
/// arguments), `else`, or an annotation line of its own — and only there. After an operator, an opening parenthesis, a
/// comma, a `.`, `return`, `new` or a type name it is part of an expression or a declaration, and `sout` offered there
/// is a row that cannot be accepted.
pub fn caret_place(source: &str, word_start: usize) -> CaretPlace {
    let before = source.get(..word_start).unwrap_or("").trim_end();
    let after_annotation_line = || {
        let line = before.rsplit('\n').next().unwrap_or("").trim_start();
        line.starts_with('@') && !line.contains(' ')
    };
    let opens = before.is_empty()
        || before.ends_with([';', '{', '}', ')'])
        || before.ends_with("else")
        || after_annotation_line();
    if !opens {
        return CaretPlace::Elsewhere;
    }
    // Only what stands before the word says which scope it is in. Read it as a complete file: a word typed in an
    // unfinished buffer would otherwise land in whatever the parser's error recovery made of the open braces.
    let head = source.get(..word_start).unwrap_or("");
    let open = head.matches('{').count().saturating_sub(head.matches('}').count());
    let closed = format!("{head}x;{}", "}".repeat(open));
    let Some(tree) = crate::prelude::parse_java(&closed) else { return CaretPlace::Elsewhere };
    let node = tree.root_node().descendant_for_byte_range(word_start, word_start);
    match node.map(inside_a_body) {
        Some(true) => CaretPlace::Statement,
        _ => CaretPlace::Member,
    }
}

/// Whether `node` is inside code that runs — a block, a switch body, an initializer — rather than directly in a type's
/// body, where a statement cannot start.
pub fn inside_a_body(node: tree_sitter::Node<'_>) -> bool {
    let mut current = Some(node);
    while let Some(n) = current {
        match n.kind() {
            "block" | "switch_block" | "constructor_body" | "static_initializer" => return true,
            "class_body" | "interface_body" | "enum_body" | "enum_body_declarations" | "annotation_type_body" | "program" => {
                return false
            }
            _ => {}
        }
        current = n.parent();
    }
    false
}

/// The templates whose abbreviation starts with `prefix`, in table order.
///
/// An **empty prefix yields nothing**, and that is the rule that keeps this from being noise: the
/// popup opened on a bare caret is already a list of everything in scope, and fifteen
/// abbreviations at the top of it would be fifteen rows nobody asked for. They are for somebody
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

    fn place(marked: &str) -> CaretPlace {
        let at = marked.find('|').expect("a caret");
        caret_place(&marked.replace('|', ""), at)
    }

    #[test]
    fn a_statement_opens_in_a_body_and_a_member_in_a_type() {
        assert_eq!(place("class A {\n    void f() {\n        |\n    }\n}"), CaretPlace::Statement);
        assert_eq!(place("class A {\n    void f() {\n        int x = 1;\n        |\n    }\n}"), CaretPlace::Statement);
        assert_eq!(place("class A {\n    void f() {\n        if (x) |\n    }\n}"), CaretPlace::Statement);
        assert_eq!(place("class A {\n    |\n}"), CaretPlace::Member);
        assert_eq!(place("class A {\n    int x;\n    |\n}"), CaretPlace::Member);
        assert_eq!(place("class A {\n    @Autowired\n    |\n}"), CaretPlace::Member);
    }

    #[test]
    fn inside_an_expression_or_after_a_type_there_is_no_place() {
        assert_eq!(place("class A {\n    void f() {\n        foo(|\n    }\n}"), CaretPlace::Elsewhere);
        assert_eq!(place("class A {\n    void f() {\n        int x = |\n    }\n}"), CaretPlace::Elsewhere);
        assert_eq!(place("class A {\n    void f() {\n        return |\n    }\n}"), CaretPlace::Elsewhere);
        assert_eq!(place("class A {\n    void f() {\n        String |\n    }\n}"), CaretPlace::Elsewhere);
        assert_eq!(place("class A {\n    void f() {\n        a.|\n    }\n}"), CaretPlace::Elsewhere);
    }

    #[test]
    fn each_template_belongs_to_one_place() {
        let place_of = |a: &str| TEMPLATES.iter().find(|t| t.abbrev == a).map(Template::place);
        assert_eq!(place_of("sout"), Some(Place::Statement));
        assert_eq!(place_of("fori"), Some(Place::Statement));
        assert_eq!(place_of("psvm"), Some(Place::Member));
        assert_eq!(place_of("psf"), Some(Place::Member));
    }

    #[test]
    fn a_prefix_offers_its_family_with_the_bare_one_first() {
        let found: Vec<&str> = matching("psf").iter().map(|t| t.abbrev).collect();
        assert_eq!(found, ["psf", "psfi", "psfs"]);
    }

    /// `prf` is not a prefix of `prsf`, so `private final` and `private static final` are two
    /// families: typing the static one never offers the other, and `pr` offers both.
    #[test]
    fn the_final_pair_stays_out_of_the_static_families() {
        let found = |prefix: &str| matching(prefix).iter().map(|t| t.abbrev).collect::<Vec<_>>();
        assert_eq!(found("prf"), ["prf"]);
        assert_eq!(found("pf"), ["pf"]);
        assert_eq!(found("prsf"), ["prsf", "prsfi", "prsfs"]);
        assert_eq!(found("pr"), ["prf", "prsf", "prsfi", "prsfs"]);
        assert_eq!(matching("prf")[0].body, "private final $0");
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
