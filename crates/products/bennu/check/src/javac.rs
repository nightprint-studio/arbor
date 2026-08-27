//! The javac diagnostic catalog, and what Bennu does about each entry.
//!
//! Bennu answers the question javac answers, without running javac — so "does Bennu see this?" has
//! to have an answer that isn't a shrug. This table is that answer: **every** diagnostic key javac
//! can raise is declared here as one of three things, and there is no fourth.
//!
//!   * [`Coverage::Check`] — Bennu raises these checks for it. A Bennu diagnostic can therefore
//!     name the javac error it stands for, which is what stops a report from being ambiguous.
//!   * [`Coverage::OutOfScope`] — not an editor's to raise, with the reason attached. Module
//!     resolution, command-line options, reading a malformed class file, javadoc, JVM structural
//!     limits: real javac errors that cannot arise from looking at one source file.
//!   * [`Coverage::Missing`] — a real error in ordinary source that Bennu does not detect yet. The
//!     honest entry, and the one worth counting.
//!
//! The mapping is not one-to-one in either direction, so [`Coverage::Check`] holds a slice. javac
//! folds "cannot find symbol" for a variable, a method and a type into one key and distinguishes
//! them by an argument; Bennu splits them into separate checks, because they have different
//! quick-fixes. Going the other way, one Bennu check answers several keys — `cant.apply.symbol`,
//! `cant.apply.symbols` and `cant.apply.diamond` are all "no overload takes these arguments".
//!
//! ## Where the entries come from
//!
//! The key list is javac's own, not a transcription: it is the `keySet()` of the
//! `com.sun.tools.javac.resources.compiler` bundle, which is the single place javac itself looks up
//! the text of a diagnostic. Regenerate it against a newer JDK with
//!
//! ```sh
//! java --add-exports jdk.compiler/com.sun.tools.javac.resources=ALL-UNNAMED \
//!      --add-opens    jdk.compiler/com.sun.tools.javac.resources=ALL-UNNAMED DumpKeys.java
//! ```
//!
//! where `DumpKeys` prints `ResourceBundle.getBundle("com.sun.tools.javac.resources.compiler")`.
//!
//! The table is the **union** of that bundle and the keys the langtools corpus raises: the corpus
//! tracks `jdk/main`, so it exercises diagnostics a released JDK does not have yet. Declaring only
//! the released set would leave the score with an "unrecognised" bucket, which is the one bucket a
//! coverage report must not have — it reads as "nothing to see" while hiding real gaps.
//!
//! The `Check` entries are **evidence**, not judgement: they come from running Bennu over the JDK's
//! own `test/langtools/tools/javac` corpus — ~1500 files that must not compile, each with a golden
//! file recording the diagnostic javac raises and the line it raises it on — and recording which
//! Bennu check lands on the same line. See `examples/langtools.rs`. A mapping written by reading
//! key names instead would be plausible and wrong in places nobody could point to.

use crate::check_id::CheckId;

/// What Bennu does about one javac diagnostic.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Coverage {
    /// Bennu raises one of these checks for it.
    Check(&'static [CheckId]),
    /// Outside what reading a source file can decide. The reason is the entry's justification —
    /// an `OutOfScope` with no defensible reason is a `Missing` in disguise.
    OutOfScope(&'static str),
    /// A real error in ordinary source that Bennu does not detect yet.
    Missing,
}

/// Reasons an entry is out of an editor's reach. Constants rather than free strings so the same
/// justification reads identically on every entry that shares it, and so a new one is a deliberate
/// addition rather than a paraphrase of an existing one.
pub mod why {
    /// The module system: resolution, `requires`/`exports`/`opens`, service bindings.
    pub const MODULES: &str = "module graph — decided across the module path, not in one file";
    /// Command-line options, `-source`/`-target`/`--release`, output paths.
    pub const OPTIONS: &str = "a compiler invocation's options, not the source";
    /// Reading or writing class files, bad bytecode, class loaders.
    pub const CLASS_FILE: &str = "reading or writing a class file";
    /// Filesystem and I/O failures.
    pub const IO: &str = "filesystem or I/O failure";
    /// Structural limits of the class-file format (65535 constants, stack depth, …).
    pub const LIMITS: &str = "a class-file structural limit, hit only when generating bytecode";
    /// Javadoc comment syntax — a separate grammar, checked by a separate tool.
    pub const JAVADOC: &str = "javadoc comment grammar — a separate tool's job";
    /// Annotation processing rounds.
    pub const PROCESSING: &str = "annotation processing, which needs a running processor";
    /// javac's own internal failures and the aggregate tallies it prints.
    pub const INTERNAL: &str = "javac's own bookkeeping, not a property of the code";
}

/// What Bennu does about `key` (a full `compiler.err.…` / `compiler.warn.…` key).
///
/// `None` means the key is not in the table at all — which is a **gap in the table**, not a
/// statement about the code. `examples/langtools.rs` counts these separately for that reason.
pub fn coverage(key: &str) -> Option<Coverage> {
    TABLE.binary_search_by_key(&key, |(k, _)| k).ok().map(|i| TABLE[i].1)
}

/// Every javac key this check stands for — what a Bennu diagnostic names to be unambiguous.
pub fn javac_keys(id: CheckId) -> Vec<&'static str> {
    TABLE
        .iter()
        .filter(|(_, cov)| matches!(cov, Coverage::Check(ids) if ids.contains(&id)))
        .map(|(k, _)| *k)
        .collect()
}

/// Every key declared as not yet detected — the work list.
pub fn missing() -> Vec<&'static str> {
    TABLE.iter().filter(|(_, c)| *c == Coverage::Missing).map(|(k, _)| *k).collect()
}

include!("javac_table.rs");

#[cfg(test)]
mod tests {
    use super::*;

    /// `coverage` binary-searches, so an out-of-order entry would be silently unfindable rather
    /// than a compile error — the one defect this table can have that nothing else would catch.
    #[test]
    fn the_table_is_sorted_and_has_no_duplicates() {
        for pair in TABLE.windows(2) {
            assert!(pair[0].0 < pair[1].0, "out of order: {} then {}", pair[0].0, pair[1].0);
        }
    }

    #[test]
    fn every_entry_is_a_javac_key() {
        for (key, _) in TABLE {
            assert!(
                key.starts_with("compiler.err.") || key.starts_with("compiler.warn."),
                "not a javac diagnostic key: {key}"
            );
        }
    }

    /// An `OutOfScope` whose reason isn't one of the shared constants is a paraphrase, and a
    /// paraphrase is how a table stops being comparable with itself.
    #[test]
    fn out_of_scope_entries_use_a_declared_reason() {
        let declared = [
            why::MODULES,
            why::OPTIONS,
            why::CLASS_FILE,
            why::IO,
            why::LIMITS,
            why::JAVADOC,
            why::PROCESSING,
            why::INTERNAL,
        ];
        for (key, cov) in TABLE {
            if let Coverage::OutOfScope(reason) = cov {
                assert!(declared.contains(reason), "undeclared reason on {key}: {reason}");
            }
        }
    }

    #[test]
    fn lookup_finds_a_known_key() {
        assert!(coverage("compiler.err.not.stmt").is_some());
        assert!(coverage("compiler.err.this.key.does.not.exist").is_none());
    }

    /// A check that claims a javac key must be a real check.
    #[test]
    fn mapped_checks_are_in_the_catalog() {
        for (key, cov) in TABLE {
            if let Coverage::Check(ids) = cov {
                assert!(!ids.is_empty(), "empty mapping on {key}");
                for id in *ids {
                    assert!(CheckId::ALL.contains(id), "{key} maps to a check not in ALL: {id:?}");
                }
            }
        }
    }
}
