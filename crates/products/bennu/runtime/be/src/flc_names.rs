//! The names this project gives to its variables, by type — so a line that is only `OrderService` is offered the name
//! the project always uses (`orderSvc`), not the one the type spells (`orderService`).
//!
//! Counts only: for each type as written (`Order`, `List<Item>`), how many times each variable name was declared with
//! it, across fields, locals, parameters and `for` variables. Each file's contribution is kept apart, so a saved file
//! replaces its own counts. The counts are written to disk only through `flc_profile`, in the user's Arbor data folder
//! and only when the settings allow it.

use std::collections::{HashMap, HashSet};
use std::sync::{Mutex, OnceLock};

use serde::{Deserialize, Serialize};

/// A name has to hold at least this share of a type's declarations...
const MIN_SHARE: f64 = 0.5;
/// ...over at least this many of them.
const MIN_SIGHTINGS: u32 = 3;

#[derive(Default)]
pub(crate) struct Names {
    /// type as written (no spaces) → variable name → declarations
    table: HashMap<String, HashMap<String, u32>>,
    /// what each file contributed, to take it back
    by_file: HashMap<String, Vec<(String, String)>>,
    /// hash of the text each file was counted from: the same text again is not parsed again
    hashes: HashMap<String, u64>,
    /// what other projects saw, consulted only for a type this project has too few declarations of
    prior: HashMap<String, HashMap<String, u32>>,
}

/// A project's counters summed over its files: `(type, name, count)`.
#[derive(Serialize, Deserialize, Default, Clone, Debug, PartialEq)]
pub(crate) struct NameTotals {
    pub pairs: Vec<(String, String, u32)>,
}

fn compact(type_text: &str) -> String {
    type_text.chars().filter(|c| !c.is_whitespace()).collect()
}

fn is_name(s: &str) -> bool {
    s.starts_with(|c: char| c.is_ascii_lowercase()) && s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'$')
}

/// A declared type worth counting: a capitalised, unqualified name (generics and `[]` allowed). `var`, primitives and
/// qualified names are left out — a lookup is never made for them.
fn countable(type_text: &str) -> bool {
    type_text.starts_with(|c: char| c.is_ascii_uppercase()) && !type_text.contains('.')
}

/// Every `Type name` declared in `source`: fields, locals, parameters and enhanced-`for` variables.
fn declarations(source: &str) -> Vec<(String, String)> {
    let Some(tree) = bennu_java::prelude::parse_java(source) else { return Vec::new() };
    let mut out = Vec::new();
    let mut stack = vec![tree.root_node()];
    while let Some(node) = stack.pop() {
        let mut cursor = node.walk();
        match node.kind() {
            "field_declaration" | "local_variable_declaration" => {
                if let Some(ty) = node.child_by_field_name("type").and_then(|t| t.utf8_text(source.as_bytes()).ok()) {
                    for declarator in node.children_by_field_name("declarator", &mut cursor) {
                        if let Some(name) = declarator.child_by_field_name("name").and_then(|n| n.utf8_text(source.as_bytes()).ok()) {
                            out.push((compact(ty), name.to_string()));
                        }
                    }
                }
            }
            "formal_parameter" | "enhanced_for_statement" => {
                let ty = node.child_by_field_name("type").and_then(|t| t.utf8_text(source.as_bytes()).ok());
                let name = node.child_by_field_name("name").and_then(|n| n.utf8_text(source.as_bytes()).ok());
                if let (Some(ty), Some(name)) = (ty, name) {
                    out.push((compact(ty), name.to_string()));
                }
            }
            _ => {}
        }
        let mut cursor = node.walk();
        stack.extend(node.children(&mut cursor));
    }
    out.retain(|(ty, name)| countable(ty) && is_name(name));
    out
}

impl Names {
    pub(crate) fn add_file(&mut self, key: &str, source: &str) {
        let hash = crate::flc_profile::text_hash(source);
        if self.hashes.get(key) == Some(&hash) {
            return;
        }
        self.remove_file(key);
        let found = declarations(source);
        for (ty, name) in &found {
            *self.table.entry(ty.clone()).or_default().entry(name.clone()).or_insert(0) += 1;
        }
        self.by_file.insert(key.to_string(), found);
        self.hashes.insert(key.to_string(), hash);
        crate::flc_profile::mark_dirty();
    }

    /// Every file counted, with the hash of its text and its `(type, name)` pairs — for the profile on disk.
    pub(crate) fn snapshot(&self, include: &dyn Fn(&str) -> bool) -> Vec<(String, u64, Vec<(String, String)>)> {
        self.hashes.iter().filter(|(k, _)| include(k)).map(|(k, h)| (k.clone(), *h, self.by_file.get(k).cloned().unwrap_or_default())).collect()
    }

    /// Puts back what a saved profile says `key` contributed, without reading the file.
    pub(crate) fn restore_file(&mut self, key: &str, hash: u64, pairs: Vec<(String, String)>) {
        self.remove_file(key);
        for (ty, name) in &pairs {
            *self.table.entry(ty.clone()).or_default().entry(name.clone()).or_insert(0) += 1;
        }
        self.by_file.insert(key.to_string(), pairs);
        self.hashes.insert(key.to_string(), hash);
    }

    pub(crate) fn totals(&self, include: &dyn Fn(&str) -> bool) -> NameTotals {
        let mut sum: HashMap<(String, String), u32> = HashMap::new();
        for key in self.hashes.keys().filter(|k| include(k)) {
            for pair in self.by_file.get(key).into_iter().flatten() {
                *sum.entry(pair.clone()).or_insert(0) += 1;
            }
        }
        let mut pairs: Vec<_> = sum.into_iter().map(|((t, n), c)| (t, n, c)).collect();
        pairs.sort();
        NameTotals { pairs }
    }

    pub(crate) fn set_prior(&mut self, totals: &NameTotals) {
        self.prior.clear();
        for (ty, name, n) in &totals.pairs {
            *self.prior.entry(ty.clone()).or_default().entry(name.clone()).or_insert(0) += n;
        }
    }

    pub(crate) fn clear(&mut self) {
        *self = Names::default();
    }

    pub(crate) fn files(&self) -> Vec<String> {
        self.hashes.keys().cloned().collect()
    }

    pub(crate) fn remove_file(&mut self, key: &str) {
        if self.hashes.remove(key).is_some() {
            crate::flc_profile::mark_dirty();
        }
        for (ty, name) in self.by_file.remove(key).unwrap_or_default() {
            let Some(names) = self.table.get_mut(&ty) else { continue };
            if let Some(n) = names.get_mut(&name) {
                *n -= 1;
                if *n == 0 {
                    names.remove(&name);
                }
            }
            if names.is_empty() {
                self.table.remove(&ty);
            }
        }
    }

    /// The name the project uses for `type_text`, when it is clear enough.
    pub(crate) fn habit(&self, type_text: &str) -> Option<String> {
        let key = compact(type_text);
        let mut names: HashMap<&String, u32> = self.table.get(&key).into_iter().flatten().map(|(n, c)| (n, *c)).collect();
        // too few declarations here: the other projects' are added
        if names.values().sum::<u32>() < MIN_SIGHTINGS {
            for (n, c) in self.prior.get(&key).into_iter().flatten() {
                *names.entry(n).or_insert(0) += c;
            }
        }
        let total: u32 = names.values().sum();
        let (best, count) = names.iter().max_by(|a, b| a.1.cmp(b.1).then_with(|| b.0.cmp(a.0)))?;
        (total >= MIN_SIGHTINGS && f64::from(*count) / f64::from(total) >= MIN_SHARE).then(|| (*best).clone())
    }
}

fn state() -> &'static Mutex<Names> {
    static STATE: OnceLock<Mutex<Names>> = OnceLock::new();
    STATE.get_or_init(|| Mutex::new(Names::default()))
}

/// Counts the declarations of `source` under `key`, replacing what the same key contributed before.
pub(crate) fn add_file(key: &str, source: &str) {
    state().lock().unwrap_or_else(|p| p.into_inner()).add_file(key, source);
}

pub(crate) fn remove_file(key: &str) {
    state().lock().unwrap_or_else(|p| p.into_inner()).remove_file(key);
}

fn lock() -> std::sync::MutexGuard<'static, Names> {
    state().lock().unwrap_or_else(|p| p.into_inner())
}

pub(crate) fn snapshot(include: &dyn Fn(&str) -> bool) -> Vec<(String, u64, Vec<(String, String)>)> {
    lock().snapshot(include)
}

pub(crate) fn restore_file(key: &str, hash: u64, pairs: Vec<(String, String)>) {
    lock().restore_file(key, hash, pairs);
}

pub(crate) fn totals(include: &dyn Fn(&str) -> bool) -> NameTotals {
    lock().totals(include)
}

pub(crate) fn set_prior(totals: &NameTotals) {
    lock().set_prior(totals);
}

pub(crate) fn clear() {
    lock().clear();
}

pub(crate) fn sizes() -> crate::flc_memory::Sizes {
    use crate::flc_memory::{text, two_level, Sizes};
    let h = lock();
    let mut s = Sizes::default();
    for (k, v) in &h.by_file {
        s.add_file(k, v.iter().map(|(a, b)| text(a) + text(b)).sum());
    }
    s.shared = two_level(&h.table, |k| text(k));
    s.shared_entries = h.table.values().map(|r| r.len()).sum();
    s.carried = two_level(&h.prior, |k| text(k));
    s
}

pub(crate) fn files() -> Vec<String> {
    lock().files()
}

/// The text a name must not appear in to be free at `offset`: the method or constructor around it (its parameters
/// included), else the whole file when the caret is in a class body.
fn scope_text(source: &str, offset: usize) -> &str {
    let Some(tree) = bennu_java::prelude::parse_java(source) else { return source };
    let mut node = tree.root_node().descendant_for_byte_range(offset.min(source.len()), offset.min(source.len()));
    while let Some(n) = node {
        if matches!(n.kind(), "method_declaration" | "constructor_declaration") {
            return source.get(n.start_byte()..n.end_byte()).unwrap_or(source);
        }
        node = n.parent();
    }
    source
}

/// Whether `name` already appears as a word in the scope around `offset` — declared or used, so a new variable of that
/// name would collide with it or shadow it.
pub(crate) fn is_taken(source: &str, offset: usize, name: &str) -> bool {
    bennu_flc::prelude::words_of(scope_text(source, offset)).contains(name)
}

/// The variable name the project uses for `type_text` (as `bare_type` reads it off the line), when it is clear and free
/// at `offset` of `source`; `None` otherwise, and the caller keeps the name the type spells.
pub(crate) fn name_for(_file: &str, source: &str, offset: usize, type_text: &str) -> Option<String> {
    let name = state().lock().unwrap_or_else(|p| p.into_inner()).habit(type_text)?;
    (!is_taken(source, offset, &name)).then_some(name)
}

/// All the distinct type keys, for tests and the status line.
#[cfg(test)]
fn types(names: &Names) -> HashSet<&str> {
    names.table.keys().map(String::as_str).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(decls: &str) -> String {
        format!("class C {{\n{decls}\n    void run(Order order) {{ }}\n}}\n")
    }

    #[test]
    fn declarations_cover_fields_locals_parameters_and_for_variables() {
        let src = "class C {\n    private OrderService orderSvc;\n    void f(Order o, int n) {\n        List<Item> rows = load();\n        for (Item it : rows) { }\n        var skipped = 1;\n        java.util.Map<A,B> qualified = null;\n    }\n}\n";
        let got: HashSet<(String, String)> = declarations(src).into_iter().collect();
        for want in [("OrderService", "orderSvc"), ("Order", "o"), ("List<Item>", "rows"), ("Item", "it")] {
            assert!(got.contains(&(want.0.to_string(), want.1.to_string())), "{want:?} missing from {got:?}");
        }
        assert!(!got.iter().any(|(t, _)| t == "int" || t == "var" || t.contains('.')), "{got:?}");
    }

    #[test]
    fn a_clear_habit_wins_and_a_vague_one_does_not() {
        let mut n = Names::default();
        for k in ["A", "B", "C"] {
            n.add_file(k, &file("    private OrderService orderSvc;"));
        }
        assert_eq!(n.habit("OrderService").as_deref(), Some("orderSvc"));
        // below the sightings: three in all are needed
        let mut few = Names::default();
        few.add_file("A", &file("    private OrderService orderSvc;"));
        few.add_file("B", &file("    private OrderService orderSvc;"));
        assert_eq!(few.habit("OrderService"), None);
        // three names, none holding half
        let mut split = Names::default();
        for (k, name) in [("A", "svc"), ("B", "service"), ("C", "orderService"), ("D", "os")] {
            split.add_file(k, &file(&format!("    private OrderService {name};")));
        }
        assert_eq!(split.habit("OrderService"), None);
    }

    #[test]
    fn a_resaved_file_replaces_its_own_counts() {
        let mut n = Names::default();
        for k in ["A", "B", "C"] {
            n.add_file(k, &file("    private OrderService orderSvc;"));
        }
        n.add_file("C", &file("    private Other other;"));
        assert_eq!(n.habit("OrderService"), None, "two sightings are not enough any more");
        n.remove_file("A");
        n.remove_file("B");
        n.remove_file("C");
        assert!(types(&n).is_empty());
    }

    #[test]
    fn generic_types_are_their_own_key() {
        let mut n = Names::default();
        for k in ["A", "B", "C"] {
            n.add_file(k, &file("    private List<Item> rows;"));
        }
        assert_eq!(n.habit("List<Item>").as_deref(), Some("rows"));
        assert_eq!(n.habit("List< Item >").as_deref(), Some("rows"));
        assert_eq!(n.habit("List<Order>"), None);
    }

    #[test]
    fn a_name_already_in_the_method_is_taken() {
        let src = "class C {\n    private int order;\n    void f(Order existing) {\n        Order\n    }\n    void g() { int free; }\n}\n";
        let at = src.find("Order\n    }").unwrap() + 5;
        assert!(is_taken(src, at, "existing"));
        assert!(!is_taken(src, at, "order"), "a field of the class is not in the method");
        assert!(!is_taken(src, at, "free"), "another method's names do not count");
    }
}
