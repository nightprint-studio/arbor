//! [`ClassNameIndex`] — a simple type name → the fully-qualified names that declare it.
//!
//! Built once at provider construction from the classpath (JDK + dependency `.class` names, via
//! [`bennu_classpath::prelude::ClassSource::class_names`]) plus the project's own declared types.
//! Powers the **Import class** intention: a simple name under the caret maps to one or more importable
//! FQNs, and the Alt+Enter menu shows every candidate (the "which import?" picker).
//!
//! ## Two axes, because a name is looked up two ways
//!
//! [`sorted_simples`](ClassNameIndex) answers *what is called something like this* — the axis a
//! bare `Sprin|` or `@SBA|` is matched against. [`sorted_fqns`](ClassNameIndex) answers *what
//! lives under this package* — the axis an `import org.springframework.|` is matched against, and
//! the one a qualified reference in code uses too. They are the same classes; what differs is
//! which end of the name the caret gives you, and neither ordering can serve the other's search.
//!
//! The FQN axis duplicates strings the simple-name map already owns. It is a few megabytes on a
//! large Spring project, and the alternative — sharing them behind an `Arc<str>` — would change
//! the type every caller of [`candidates`](ClassNameIndex::candidates) reads.

use std::collections::{HashMap, HashSet};

/// Simple name (`List`) → sorted, de-duplicated dotted FQNs (`["java.awt.List", "java.util.List"]`).
#[derive(Default, Debug)]
pub struct ClassNameIndex {
    by_simple: HashMap<String, Vec<String>>,
    /// Outer binary name (`java/util/Map`) → the types nested DIRECTLY inside it, as binary names
    /// with the JVM's `$` spelling (`java/util/Map$Entry`), sorted and unique.
    ///
    /// Filled from the same enumeration that fills [`by_simple`], which is why it is here and not
    /// behind a bytecode read: the classpath walk already sees every `Outer$Inner` name and was
    /// throwing them away.
    ///
    /// The class file's `InnerClasses` attribute is the precise source — it names the outer
    /// explicitly instead of inferring it from a `$`, so it tells a genuinely nested class from a
    /// top-level one whose name happens to contain one. The reader exposes it
    /// (`cafebabe::attributes::AttributeData::InnerClasses`); `bennu-classpath` does not decode it.
    /// Worth doing the day something needs the distinction — a name is all completion offers, and
    /// reading the attribute would mean decoding the outer class to answer a question about its
    /// members' names.
    nested: HashMap<String, Vec<String>>,
    /// Every distinct simple name, sorted — the prefix-search axis for type-name completion. Built by
    /// [`finalize`](Self::finalize) once, after all classes are added (kept empty until then).
    sorted_simples: Vec<String>,
    /// Every importable FQN, sorted — the axis a **qualified** name is completed against: an
    /// `import org.springframework.b|`, or an `org.springframework.boot.Sprin|` written out in
    /// code. Built by [`finalize`](Self::finalize); empty until then.
    sorted_fqns: Vec<String>,
    /// The simple names the **project itself** declares. Kept apart from the classpath's so a type
    /// you wrote outranks one from a jar with the same claim on what you typed — which is the
    /// single ranking term that matters most and the only one this index is in a position to know.
    project: HashSet<String>,
}

impl ClassNameIndex {
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a class by its **binary name** (`java/util/List`). Skipped when it isn't an importable
    /// top-level type: inner classes (`Foo$Bar`), `module-info`/`package-info`, and default-package
    /// classes (no `/` — an unqualified type can't be imported). Idempotent.
    pub fn add_binary(&mut self, binary: &str) {
        // A nested name is not importable by its own simple name, so `normalize_binary` refuses it
        // — but it IS a member of its outer, and that is a question completion asks.
        if let Some((outer, _)) = binary.rsplit_once('$') {
            // Directly inside, and never an anonymous class: `Outer$1` is an ordinal javac assigns,
            // and offering `1` in a popup offers something nobody can type.
            let last = &binary[outer.len() + 1..];
            if !last.is_empty() && !last.bytes().all(|b| b.is_ascii_digit()) {
                let v = self.nested.entry(outer.to_string()).or_default();
                if let Err(pos) = v.binary_search(&binary.to_string()) {
                    v.insert(pos, binary.to_string());
                }
            }
        }
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
        self.project.insert(simple.to_string());
        self.insert(simple.to_string(), fqn.to_string());
    }

    fn insert(&mut self, simple: String, fqn: String) {
        let v = self.by_simple.entry(simple).or_default();
        if let Err(pos) = v.binary_search(&fqn) {
            v.insert(pos, fqn); // keep each candidate list sorted + unique
        }
    }

    /// Snapshot both search axes. Call ONCE after all classes are added (the index is immutable
    /// afterwards).
    pub fn finalize(&mut self) {
        let mut v: Vec<String> = self.by_simple.keys().cloned().collect();
        v.sort();
        self.sorted_simples = v;
        let mut f: Vec<String> = self.by_simple.values().flatten().cloned().collect();
        f.sort();
        f.dedup();
        self.sorted_fqns = f;
    }

    /// The candidate FQNs (dotted, sorted) for a simple type name — empty when none is known.
    pub fn candidates(&self, simple: &str) -> &[String] {
        self.by_simple.get(simple).map(Vec::as_slice).unwrap_or(&[])
    }

    /// The types nested directly inside `binary`, as binary names. Accepts either spelling of the
    /// outer — source writes `p/Outer/Inner`, bytecode writes `p/Outer$Inner`, and a caller holds
    /// whichever its own path produced.
    pub fn nested_types(&self, binary: &str) -> &[String] {
        self.nested
            .get(binary)
            .or_else(|| self.nested.get(&binary.replace('/', "$")))
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    /// Up to `limit` simple names matching what has been typed, **best first**.
    ///
    /// Three tiers, in order, because a Java type is looked for in three ways and only the first
    /// of them is a prefix in the string sense:
    ///
    /// 1. the name starts with exactly what was typed — `Spring` → `SpringApplication`;
    /// 2. it starts with it ignoring case — `SPRING`, or a shift key held one letter too long;
    /// 3. the typed letters are its **camel humps** — `SBA` → `SpringBootApplication`, which is
    ///    how anyone who knows the class name actually reaches for it.
    ///
    /// Within a tier a type the **project** declares comes first, then the shorter name, then
    /// alphabetical. The old search was a plain sorted prefix scan truncated at fifty, which on a
    /// short prefix meant fifty names beginning with `Ab`.
    ///
    /// Only names sharing the typed first letter are examined — true of all three tiers — so this
    /// touches a slice of the axis rather than all of it.
    pub fn matches_for_prefix(&self, typed: &str, limit: usize) -> Vec<&str> {
        let Some(first) = typed.as_bytes().first().copied() else { return Vec::new() };
        // Both spellings of the first letter — a class is capitalised, but the caret may not be.
        // A byte that is not a letter yields one range, not the same one twice.
        let (upper, lower) = (first.to_ascii_uppercase(), first.to_ascii_lowercase());
        let starts: &[u8] = if upper == lower { &[upper] } else { &[upper, lower] };
        let mut scored: Vec<(u8, bool, usize, &str)> = Vec::new();
        for &start in starts {
            for name in self.names_starting_with(start) {
                let Some(tier) = match_tier(name, typed) else { continue };
                scored.push((tier, !self.project.contains(name), name.len(), name.as_str()));
            }
        }
        scored.sort_unstable();
        scored.truncate(limit);
        scored.into_iter().map(|(_, _, _, n)| n).collect()
    }

    /// The slice of the simple-name axis whose names begin with byte `c`. Contiguous because the
    /// axis is sorted bytewise.
    fn names_starting_with(&self, c: u8) -> &[String] {
        let lo = self
            .sorted_simples
            .partition_point(|s| s.as_bytes().first().copied().unwrap_or(0) < c);
        let hi = lo
            + self.sorted_simples[lo..]
                .partition_point(|s| s.as_bytes().first().copied() == Some(c));
        &self.sorted_simples[lo..hi]
    }

    /// What may follow a **qualified** prefix: the distinct next segments under `qualifier`,
    /// filtered by what has been typed of one.
    ///
    /// `qualifier` carries its trailing dot (`"org.springframework."`) or is empty for the root.
    /// The answer is one segment at a time rather than whole names, which is both what an editor
    /// can insert without rewriting the line and what reading a package tree one level at a time
    /// actually feels like: `import org.|` offers `springframework`, not forty thousand classes.
    ///
    /// Case-insensitive on the typed part, because a package segment is lower case and the letter
    /// you are unsure about is never the one you are looking for.
    pub fn segments_under(&self, qualifier: &str, typed: &str, limit: usize) -> Vec<Segment> {
        let mut out = Vec::new();
        let lower = typed.to_ascii_lowercase();
        let mut i = self.sorted_fqns.partition_point(|f| f.as_str() < qualifier);
        while i < self.sorted_fqns.len() && out.len() < limit {
            let Some(rest) = self.sorted_fqns[i].strip_prefix(qualifier) else { break };
            let (seg, leaf) = match rest.split_once('.') {
                Some((head, _)) => (head.to_string(), false),
                None => (rest.to_string(), true),
            };
            // Everything under this segment is contiguous — step over it in one jump rather than
            // walking fifty thousand entries to find twenty distinct package names.
            let span = format!("{qualifier}{seg}");
            let next = i + self.sorted_fqns[i..].partition_point(|f| {
                f.starts_with(&span) && matches!(f.as_bytes().get(span.len()), None | Some(b'.'))
            });
            let fqn = self.sorted_fqns[i].clone();
            i = next.max(i + 1);
            if seg.is_empty() || (!lower.is_empty() && !seg.to_ascii_lowercase().starts_with(&lower))
            {
                continue;
            }
            out.push(Segment { fqn: leaf.then_some(fqn), name: seg, is_class: leaf });
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

/// The completion path's view of the same table — see [`bennu_query::prelude::TypeNameCatalog`].
///
/// It is the SAME index the "Import class" intention reads, deliberately: the type completion
/// offers a name and the import intention adds it, and if they disagreed about which classes exist,
/// a name you could complete would be one you could not import.
impl bennu_query::prelude::TypeNameCatalog for ClassNameIndex {
    fn candidates(&self, simple: &str) -> Vec<String> {
        ClassNameIndex::candidates(self, simple).to_vec()
    }

    fn nested_types(&self, binary: &str) -> Vec<String> {
        ClassNameIndex::nested_types(self, binary).to_vec()
    }
}

/// One step down a qualified name — a package segment, or a class that ends it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Segment {
    /// The segment itself, which is what gets inserted: `springframework`, `SpringApplication`.
    pub name: String,
    /// Whether it ends the name rather than continuing it.
    pub is_class: bool,
    /// The full name, when this is a class — shown as the detail, so the row says where it lands.
    pub fqn: Option<String>,
}

/// How well `name` answers what was typed, lower being better, or `None` for no match at all.
/// See [`ClassNameIndex::matches_for_prefix`] for what the three tiers are and why.
fn match_tier(name: &str, typed: &str) -> Option<u8> {
    if name.starts_with(typed) {
        return Some(0);
    }
    if name.len() >= typed.len() && name.as_bytes()[..typed.len()].eq_ignore_ascii_case(typed.as_bytes())
    {
        return Some(1);
    }
    camel_humps_match(name.as_bytes(), typed.as_bytes()).then_some(2)
}

/// Whether the typed letters walk `name`'s camel humps: after the first character, each one either
/// continues the word it is in or jumps to the next capital that matches it.
///
/// Greedy, and deliberately so — it is the last tier, reached only when neither prefix test did,
/// and a rare miss on a pathological name costs a suggestion rather than producing a wrong one.
/// ASCII, because a Java type name that is not is a name this will simply not reach.
fn camel_humps_match(name: &[u8], typed: &[u8]) -> bool {
    let mut at = 0usize;
    for (k, want) in typed.iter().enumerate() {
        if k == 0 {
            match name.first() {
                Some(c) if c.eq_ignore_ascii_case(want) => at = 1,
                _ => return false,
            }
            continue;
        }
        if name.get(at).is_some_and(|c| c.eq_ignore_ascii_case(want)) {
            at += 1;
            continue;
        }
        match name[at..]
            .iter()
            .position(|c| c.is_ascii_uppercase() && c.eq_ignore_ascii_case(want))
        {
            Some(j) => at += j + 1,
            None => return false,
        }
    }
    true
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
        assert_eq!(
            idx.candidates("List"),
            &["java.awt.List".to_string(), "java.util.List".to_string()]
        );
        assert_eq!(idx.candidates("Set"), &[] as &[String]);
    }

    #[test]
    fn inner_and_special_names_are_skipped() {
        let mut idx = ClassNameIndex::new();
        idx.add_binary("java/util/Map$Entry"); // inner
        idx.add_binary("module-info");
        idx.add_binary("com/acme/package-info");
        idx.add_binary("DefaultPkgClass"); // default package
        assert!(
            idx.is_empty(),
            "none of these are importable top-level types"
        );
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
    fn a_name_is_reachable_the_three_ways_people_type_one() {
        let mut idx = ClassNameIndex::new();
        idx.add_binaries([
            "org/springframework/boot/SpringApplication".to_string(),
            "org/springframework/boot/autoconfigure/SpringBootApplication".to_string(),
            "org/springframework/context/support/StaticApplicationContext".to_string(),
        ]);
        assert!(idx.matches_for_prefix("Spring", 10).is_empty(), "empty until finalize");
        idx.finalize();

        // Tier 0 — a real prefix, and both names have it.
        assert_eq!(
            idx.matches_for_prefix("Spring", 10),
            ["SpringApplication", "SpringBootApplication"],
            "shorter first within the tier",
        );
        // Tier 1 — the case is not what anyone remembers about a class name.
        assert_eq!(idx.matches_for_prefix("springapp", 10), ["SpringApplication"]);
        // Tier 2 — the humps, which is how you reach for a name you already know.
        assert_eq!(idx.matches_for_prefix("SBA", 10), ["SpringBootApplication"]);
        assert_eq!(idx.matches_for_prefix("SprBoot", 10), ["SpringBootApplication"]);
        // A hump match must still start at the beginning: this is completion, not a search box.
        assert!(idx.matches_for_prefix("AC", 10).is_empty());
        assert!(idx.matches_for_prefix("", 10).is_empty());
    }

    /// The term that matters most and the only one this index can know: a type you wrote outranks
    /// one out of a jar with the same claim on what you typed.
    #[test]
    fn a_project_type_outranks_a_library_one() {
        let mut idx = ClassNameIndex::new();
        idx.add_binary("org/apache/struts/action/ActionForm");
        idx.add_fqn("ActionFactory", "it.acme.web.ActionFactory");
        idx.finalize();
        assert_eq!(idx.matches_for_prefix("Action", 10), ["ActionFactory", "ActionForm"]);
    }

    #[test]
    fn a_qualified_name_is_completed_one_segment_at_a_time() {
        let mut idx = ClassNameIndex::new();
        idx.add_binaries([
            "org/springframework/boot/SpringApplication".to_string(),
            "org/springframework/boot/autoconfigure/SpringBootApplication".to_string(),
            "org/springframework/context/ApplicationContext".to_string(),
            "javax/servlet/http/HttpServlet".to_string(),
        ]);
        idx.finalize();

        let names = |q: &str, t: &str| -> Vec<String> {
            idx.segments_under(q, t, 20).into_iter().map(|s| s.name).collect()
        };
        assert_eq!(names("", ""), ["javax", "org"]);
        assert_eq!(names("org.springframework.", ""), ["boot", "context"]);
        // A package and the classes that end the name, side by side, and the class says so.
        let boot = idx.segments_under("org.springframework.boot.", "", 20);
        assert_eq!(
            boot.iter().map(|s| (s.name.as_str(), s.is_class)).collect::<Vec<_>>(),
            [("SpringApplication", true), ("autoconfigure", false)],
        );
        assert_eq!(
            boot[0].fqn.as_deref(),
            Some("org.springframework.boot.SpringApplication"),
        );
        assert!(boot[1].fqn.is_none(), "a package is not somewhere you land");
        // Filtered by what has been typed of the segment, ignoring case.
        assert_eq!(names("org.springframework.", "co"), ["context"]);
        assert_eq!(names("org.springframework.boot.", "spring"), ["SpringApplication"]);
        assert!(names("org.nope.", "").is_empty());
    }

    /// The jump that keeps the walk off every entry under a package has to land in the right
    /// place: `Bar`, `Bar.baz` and `Barn` are adjacent, and only the first two are one segment.
    #[test]
    fn stepping_over_a_segment_does_not_swallow_its_neighbour() {
        let mut idx = ClassNameIndex::new();
        idx.add_binaries([
            "p/Bar".to_string(),
            "p/Barn".to_string(),
            "p/Baz".to_string(),
        ]);
        idx.finalize();
        assert_eq!(
            idx.segments_under("p.", "", 20).into_iter().map(|s| s.name).collect::<Vec<_>>(),
            ["Bar", "Barn", "Baz"],
        );
    }

}
