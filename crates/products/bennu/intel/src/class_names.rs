//! [`ClassNameIndex`] — a simple type name → the fully-qualified names that declare it.
//!
//! Built once at provider construction from the classpath (JDK + dependency `.class` names, via
//! [`bennu_classpath::prelude::ClassSource::class_names`]) plus the project's own declared types.
//! Powers the **Import class** intention: a simple name under the caret maps to one or more importable
//! FQNs, and the Alt+Enter menu shows every candidate (the "which import?" picker).

use std::collections::HashMap;

/// Simple name (`List`) → sorted, de-duplicated dotted FQNs (`["java.awt.List", "java.util.List"]`).
#[derive(Default, Debug)]
pub struct ClassNameIndex {
    by_simple: HashMap<String, Vec<String>>,
    /// Every distinct simple name, sorted — the prefix-search axis for type-name completion. Built by
    /// [`finalize`](Self::finalize) once, after all classes are added (kept empty until then).
    sorted_simples: Vec<String>,
}

impl ClassNameIndex {
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a class by its **binary name** (`java/util/List`). Skipped when it isn't an importable
    /// top-level type: inner classes (`Foo$Bar`), `module-info`/`package-info`, and default-package
    /// classes (no `/` — an unqualified type can't be imported). Idempotent.
    pub fn add_binary(&mut self, binary: &str) {
        if let Some((simple, fqn)) = normalize_binary(binary) {
            self.insert(simple, fqn);
        }
    }

    /// Add every binary name from an iterator (the classpath enumeration).
    pub fn add_binaries<I: IntoIterator<Item = String>>(&mut self, binaries: I) {
        for b in binaries {
            self.add_binary(&b);
        }
    }

    /// Add a class by its **dotted FQN** (`com.acme.Order`) with a known simple name (a project type,
    /// which the resolver already carries as `(simple, binary)` pairs — pass the dotted form here).
    pub fn add_fqn(&mut self, simple: &str, fqn: &str) {
        if simple.is_empty() || fqn.is_empty() || !fqn.contains('.') {
            return; // an unqualified type isn't importable
        }
        self.insert(simple.to_string(), fqn.to_string());
    }

    fn insert(&mut self, simple: String, fqn: String) {
        let v = self.by_simple.entry(simple).or_default();
        if let Err(pos) = v.binary_search(&fqn) {
            v.insert(pos, fqn); // keep each candidate list sorted + unique
        }
    }

    /// Snapshot the sorted simple-name axis for prefix search. Call ONCE after all classes are added
    /// (the index is immutable afterwards). Cheap: a `keys` collect + sort.
    pub fn finalize(&mut self) {
        let mut v: Vec<String> = self.by_simple.keys().cloned().collect();
        v.sort();
        self.sorted_simples = v;
    }

    /// The candidate FQNs (dotted, sorted) for a simple type name — empty when none is known.
    pub fn candidates(&self, simple: &str) -> &[String] {
        self.by_simple.get(simple).map(Vec::as_slice).unwrap_or(&[])
    }

    /// Up to `limit` distinct simple names starting with `prefix` (case-sensitive — Java type names are
    /// capitalised), in sorted order — the type-name completion candidates. Empty until
    /// [`finalize`](Self::finalize) has run.
    pub fn simple_names_with_prefix(&self, prefix: &str, limit: usize) -> Vec<&str> {
        let start = self.sorted_simples.partition_point(|s| s.as_str() < prefix);
        let mut out = Vec::new();
        for s in &self.sorted_simples[start..] {
            if !s.starts_with(prefix) {
                break;
            }
            out.push(s.as_str());
            if out.len() >= limit {
                break;
            }
        }
        out
    }

    /// Number of distinct simple names indexed.
    pub fn len(&self) -> usize {
        self.by_simple.len()
    }

    pub fn is_empty(&self) -> bool {
        self.by_simple.is_empty()
    }
}

/// A binary class name → `(simple, dotted_fqn)`, or `None` for a name that isn't an importable
/// top-level type (inner class, `module-info`/`package-info`, or a default-package class).
fn normalize_binary(binary: &str) -> Option<(String, String)> {
    if binary.contains('$') {
        return None; // inner class — not imported by its own simple name
    }
    if !binary.contains('/') {
        return None; // default package — an unqualified type isn't importable
    }
    let simple = binary.rsplit('/').next().unwrap_or(binary);
    if simple == "module-info" || simple == "package-info" {
        return None;
    }
    Some((simple.to_string(), binary.replace('/', ".")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn candidates_are_sorted_and_unique() {
        let mut idx = ClassNameIndex::new();
        idx.add_binary("java/util/List");
        idx.add_binary("java/awt/List");
        idx.add_binary("java/util/List"); // dup — collapses
        assert_eq!(idx.candidates("List"), &["java.awt.List".to_string(), "java.util.List".to_string()]);
        assert_eq!(idx.candidates("Set"), &[] as &[String]);
    }

    #[test]
    fn inner_and_special_names_are_skipped() {
        let mut idx = ClassNameIndex::new();
        idx.add_binary("java/util/Map$Entry"); // inner
        idx.add_binary("module-info");
        idx.add_binary("com/acme/package-info");
        idx.add_binary("DefaultPkgClass"); // default package
        assert!(idx.is_empty(), "none of these are importable top-level types");
    }

    #[test]
    fn project_fqn_and_binary_coexist_under_one_simple_name() {
        let mut idx = ClassNameIndex::new();
        idx.add_binary("java/util/List");
        idx.add_fqn("List", "com.acme.List"); // a project type sharing the simple name
        assert_eq!(
            idx.candidates("List"),
            &["com.acme.List".to_string(), "java.util.List".to_string()]
        );
    }

    #[test]
    fn add_binaries_bulk() {
        let mut idx = ClassNameIndex::new();
        idx.add_binaries(["java/util/List".to_string(), "java/util/Map".to_string()]);
        assert_eq!(idx.candidates("Map"), &["java.util.Map".to_string()]);
        assert_eq!(idx.len(), 2);
    }

    #[test]
    fn prefix_search_is_sorted_and_capped() {
        let mut idx = ClassNameIndex::new();
        idx.add_binaries(
            [
                "java/util/Optional",
                "java/util/OptionalInt",
                "java/util/OptionalDouble",
                "java/util/List", // not an "Opti" match
            ]
            .into_iter()
            .map(str::to_string),
        );
        // Empty before finalize.
        assert!(idx.simple_names_with_prefix("Opti", 10).is_empty());
        idx.finalize();
        assert_eq!(
            idx.simple_names_with_prefix("Opti", 10),
            vec!["Optional", "OptionalDouble", "OptionalInt"]
        );
        // Cap is honoured.
        assert_eq!(idx.simple_names_with_prefix("Opti", 2).len(), 2);
        // Case-sensitive: a lowercase prefix matches nothing here.
        assert!(idx.simple_names_with_prefix("opti", 10).is_empty());
    }
}
