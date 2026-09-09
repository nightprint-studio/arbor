//! **What this project actually imports**, counted — the ranking term nothing else can supply.
//!
//! ## The question it answers
//!
//! A simple name usually resolves to several types. `List` is `java.util.List`, `java.awt.List`,
//! and whatever a jar on the classpath calls a `List`; and every ranking term available before
//! this one is blind to the difference. Matching quality cannot tell them apart — the typed
//! letters are the same. Package proximity ranks by where they *live*, which puts an
//! `org.acme.util.List` nobody has ever written above the `java.util.List` written in four hundred
//! files, purely because it shares a package prefix with the file you are in.
//!
//! What tells them apart is that the project **already made this decision, hundreds of times, in
//! writing**. Every `import` statement in the codebase is somebody choosing one of those
//! candidates, and the aggregate of those choices is by far the best predictor of the next one.
//!
//! ## Why it is not persisted
//!
//! It is derived data: a pure function of files the project already has, computed during the walk
//! that parses all of them anyway, at the cost of one hash lookup per import line. A file on disk
//! would be a second copy of something recomputable — and a **stale** copy, which is worse than
//! none: a project that dropped a library would go on recommending it, and nothing in the popup
//! would say why.
//!
//! ## Why it is not live either
//!
//! It is rebuilt with the index and not maintained per keystroke, and that is a decision rather
//! than a shortcut. This is a statistic about the *shape of the project*: whether it is a codebase
//! that uses `java.util.List` is not a fact that changes because you saved a file. Making it
//! incremental would mean keeping every file's import list in memory to withdraw it again — several
//! megabytes to track a number whose whole purpose is to break a tie — and it would mean somebody's
//! completion order visibly changing under them as they typed. A census a rebuild out of date
//! ranks exactly as well.
//!
//! ## What "a lot" means, and the ceiling
//!
//! The count is turned into a **band**, not used raw: [`popularity`] is the base-2 logarithm,
//! capped at [`MAX_BAND`]. That is the honest resolution of the underlying signal — the difference
//! between 4 imports and 8 is real, the difference between 400 and 800 is not, and a ranking that
//! pretended otherwise would let one very popular type dominate a list for ever.
//!
//! The consequence, stated plainly because it is the interesting limit: **beyond
//! [`SATURATE_AT`] occurrences nothing is recorded**. Counting higher would cost memory to store a
//! number no consumer can distinguish. The counter saturates rather than wrapping — a project with
//! ten thousand `java.util.List` imports must not read as having none.

use std::collections::HashMap;

use bennu_java::prelude::FileSymbols;

/// The highest band [`popularity`] returns. Eight steps (0..=7) is `log2` of the saturation point,
/// which is as much resolution as the underlying number honestly carries.
pub const MAX_BAND: u32 = 7;

/// Where a count stops climbing.
///
/// `2^MAX_BAND` — the first count that reaches the top band. Everything above it ranks identically,
/// so recording it would be storing a distinction nothing can act on.
pub const SATURATE_AT: u32 = 1 << MAX_BAND;

/// How many distinct imported names are kept.
///
/// Not a design point: a real project imports a few thousand distinct types and never comes near
/// this. It is a bound against a generated codebase with a hundred thousand of them, so that a
/// pathological project costs a bounded amount of memory rather than an unbounded one. Reached, the
/// census simply stops learning new names — the ones already counted keep working.
const MAX_NAMES: usize = 20_000;

/// How often each type — and each wildcard-imported package — is imported across the project.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ImportCensus {
    /// Fully-qualified type name → how many files import it, saturating at [`SATURATE_AT`].
    types: HashMap<String, u32>,
    /// Package name → how many files import it with a `.*`, same saturation.
    ///
    /// Kept apart from the types on purpose. A `import java.util.*;` is not evidence about
    /// `java.util.List` specifically — it is evidence about the *package*, and every type in it
    /// gets the same weaker credit. Folding the two together would have one file's wildcard vouch
    /// for four hundred types it may never mention.
    packages: HashMap<String, u32>,
}

impl ImportCensus {
    /// Count one file's import statements.
    ///
    /// Static imports are counted for the type they name — `import static org.junit.Assert.assertEquals`
    /// is that file having chosen `org.junit.Assert`, which is exactly the evidence wanted.
    pub fn add_file(&mut self, symbols: &FileSymbols) {
        for import in &symbols.imports {
            let path = import.path.trim();
            if path.is_empty() {
                continue;
            }
            // `star` and `static_` are the parser's, not this module's to re-derive: an
            // `Import`'s `path` is already written without a trailing `.*`, so testing the text
            // for one would silently count every wildcard as a type.
            match (import.star, import.static_) {
                // `import java.util.*` — the path IS the package.
                (true, false) => bump(&mut self.packages, path),
                // `import static org.junit.Assert.*` — the path is the TYPE, which is exactly the
                // evidence wanted, so it counts as a direct one.
                (true, true) => bump(&mut self.types, path),
                // `import static org.junit.Assert.assertEquals` — the path ends in the member, so
                // the type is one segment up.
                (false, true) => {
                    if let Some((ty, _member)) = path.rsplit_once('.') {
                        bump(&mut self.types, ty);
                    }
                }
                (false, false) => bump(&mut self.types, path),
            }
        }
    }

    /// How many files import `fqn` outright.
    pub fn count(&self, fqn: &str) -> u32 {
        self.types.get(fqn).copied().unwrap_or(0)
    }

    /// How many files wildcard-import `fqn`'s package.
    pub fn package_count(&self, fqn: &str) -> u32 {
        let Some((package, _)) = fqn.rsplit_once('.') else { return 0 };
        self.packages.get(package).copied().unwrap_or(0)
    }

    /// How many distinct names have been counted — what the index inspector reports, and the
    /// number that says whether the census was built at all.
    pub fn len(&self) -> usize {
        self.types.len() + self.packages.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// How strongly this project vouches for `fqn`, as a band in `0..=MAX_BAND`.
    ///
    /// A direct import counts fully; a wildcard on its package counts, but one band lower, because
    /// `import java.util.*` is a file choosing the package and not this type in it. The two are
    /// combined by taking the better of them rather than by adding: a type that is both directly
    /// imported everywhere and wildcard-imported once is not more certain than one that is only
    /// the first.
    pub fn popularity(&self, fqn: &str) -> u32 {
        let direct = band(self.count(fqn));
        let via_package = band(self.package_count(fqn)).saturating_sub(1);
        direct.max(via_package)
    }
}

/// Add one to `key`'s count, saturating, and stop learning new names past [`MAX_NAMES`].
fn bump(map: &mut HashMap<String, u32>, key: &str) {
    if let Some(count) = map.get_mut(key) {
        *count = (*count + 1).min(SATURATE_AT);
        return;
    }
    if map.len() >= MAX_NAMES {
        return;
    }
    map.insert(key.to_string(), 1);
}

/// A count as a band: `log2`, so each step is a doubling, capped at [`MAX_BAND`].
///
/// Logarithmic and not linear because that is the resolution the number honestly carries. Four
/// imports against eight is a real difference in how much a codebase leans on something; four
/// hundred against eight hundred is noise, and a linear term would let one ubiquitous type sit at
/// the top of every list it matches for ever.
fn band(count: u32) -> u32 {
    if count == 0 {
        return 0;
    }
    (count + 1).ilog2().min(MAX_BAND)
}

#[cfg(test)]
mod tests {
    use super::*;
    use bennu_java::prelude::extract_symbols;

    fn census(files: &[&str]) -> ImportCensus {
        let mut c = ImportCensus::default();
        for f in files {
            c.add_file(&extract_symbols(f));
        }
        c
    }

    #[test]
    fn a_type_is_counted_once_per_file_that_imports_it() {
        let c = census(&[
            "package a;\nimport java.util.List;\nclass A {}\n",
            "package b;\nimport java.util.List;\nclass B {}\n",
            "package c;\nimport java.util.Map;\nclass C {}\n",
        ]);
        assert_eq!(c.count("java.util.List"), 2);
        assert_eq!(c.count("java.util.Map"), 1);
        assert_eq!(c.count("java.awt.List"), 0);
    }

    #[test]
    fn a_wildcard_is_evidence_about_the_package_and_not_about_a_type_in_it() {
        // Otherwise one `import java.util.*;` would vouch for four hundred types the file may
        // never mention.
        let c = census(&["package a;\nimport java.util.*;\nclass A {}\n"]);
        assert_eq!(c.count("java.util.List"), 0);
        assert_eq!(c.package_count("java.util.List"), 1);
    }

    #[test]
    fn a_static_import_is_evidence_about_the_type_it_reaches_through() {
        // Both spellings name `Assert`: one through a member, one through a star. Either way the
        // file chose that type, which is the whole of what is being counted.
        let c = census(&[
            "package a;\nimport static org.junit.Assert.assertEquals;\nclass A {}\n",
            "package b;\nimport static org.junit.Assert.*;\nclass B {}\n",
        ]);
        assert_eq!(c.count("org.junit.Assert"), 2);
        assert_eq!(c.package_count("org.junit.Assert"), 0, "a static star is not a package import");
    }

    // ── the bands ────────────────────────────────────────────────────────────────────────────

    #[test]
    fn the_band_is_a_doubling_per_step() {
        assert_eq!(band(0), 0);
        assert_eq!(band(1), 1);
        assert_eq!(band(3), 2);
        assert_eq!(band(7), 3);
        assert_eq!(band(15), 4);
    }

    #[test]
    fn the_band_stops_climbing_at_the_ceiling() {
        assert_eq!(band(SATURATE_AT), MAX_BAND);
        assert_eq!(band(SATURATE_AT * 100), MAX_BAND);
    }

    #[test]
    fn the_count_saturates_rather_than_wrapping() {
        // A project with ten thousand `List` imports must not read as having none.
        let mut c = ImportCensus::default();
        let file = extract_symbols("package a;\nimport java.util.List;\nclass A {}\n");
        for _ in 0..(SATURATE_AT + 50) {
            c.add_file(&file);
        }
        assert_eq!(c.count("java.util.List"), SATURATE_AT);
        assert_eq!(c.popularity("java.util.List"), MAX_BAND);
    }

    #[test]
    fn a_wildcard_vouches_less_than_a_direct_import() {
        let direct = census(&["package a;\nimport java.util.List;\nimport java.util.List;\nclass A {}\n"]);
        let wild = census(&["package a;\nimport java.util.*;\nimport java.util.*;\nclass A {}\n"]);
        assert!(
            direct.popularity("java.util.List") > wild.popularity("java.util.List"),
            "a file naming the type is stronger evidence than one naming its package"
        );
    }

    #[test]
    fn a_name_nothing_imports_has_no_standing() {
        assert_eq!(census(&[]).popularity("java.util.List"), 0);
    }

    #[test]
    fn learning_stops_at_the_name_ceiling_and_what_is_known_keeps_working() {
        let mut c = ImportCensus::default();
        c.types = (0..MAX_NAMES).map(|i| (format!("p.T{i}"), 1)).collect();
        c.add_file(&extract_symbols("package a;\nimport p.T0;\nimport z.Newcomer;\nclass A {}\n"));
        assert_eq!(c.count("p.T0"), 2, "a name already counted goes on being counted");
        assert_eq!(c.count("z.Newcomer"), 0, "a new one past the ceiling is not learnt");
    }
}
