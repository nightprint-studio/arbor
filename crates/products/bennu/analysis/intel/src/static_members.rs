//! [`StaticMemberIndex`] — a bare static member name → the classes that declare it.
//!
//! Powers the **Import static** intention: `PUBLIC_FIELD` or `assertThat(...)` written with no class in
//! front of it is unresolved until `import static pkg.Owner.NAME;` is added, and the editor can only
//! offer that if it knows which classes have a public static member called that. The type-name index
//! ([`ClassNameIndex`](crate::class_names::ClassNameIndex)) cannot say: it knows classes, not what is
//! inside them.
//!
//! ## Built in the background, and not through the memo
//!
//! Answering needs a decode of every class that could hold a static member — the project's own types,
//! the JDK's public packages and every dependency jar. That is far too much to do on the first
//! Alt+Enter, so the sweep runs on its own thread the first time anybody asks, project types first and
//! the JDK next, and results are visible as they arrive: an early question gets a partial answer, not
//! a wait. The classpath half reads through
//! [`ClasspathIndex::members_unmemoized`](bennu_query::prelude::ClasspathIndex::members_unmemoized),
//! because the memoizing path would flush an ever-growing file to disk and keep every decoded class
//! in memory for the sake of a lookup table that needs only names.
//!
//! The index describes the **last built** project: a static member added in an unsaved buffer shows up
//! after the next index build, which is also when the provider that owns this table is replaced.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock, Weak};
use std::time::Duration;

use bennu_index::prelude::SymbolKind;
use bennu_java::prelude::TypeResolver;
use bennu_query::prelude::{ClasspathIndex, IndexResolver};

/// Whether a static member is looked up as a field (a constant, an enum constant) or a method.
///
/// Two tables rather than one, because the diagnostic that asks knows which it is: `LIMIT` is an
/// unresolved *symbol*, `limit()` an unresolved *call*, and offering a class that has a method of the
/// first name to a use of the second would be an import that fixes nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum StaticKind {
    Field,
    Method,
}

/// How many classes are read between two merges into the table (and so between two chances for a
/// question to see the new ones, and for the thread to notice its index was dropped).
const BATCH: usize = 64;

/// A breath between batches. The sweep is a courtesy to the next Alt+Enter, not work anyone is waiting
/// on, so it gives the cores back instead of pegging one for the seconds a large classpath takes.
const PAUSE: Duration = Duration::from_millis(2);

/// Packages nobody static-imports from on purpose, and which are most of what a JDK image holds.
/// Skipped before they are decoded, not just ranked low: they are thousands of classes.
const INTERNAL_PREFIXES: &[&str] = &["sun.", "com.sun.", "jdk.", "apple.", "com.apple."];

#[derive(Default)]
struct Tables {
    fields: HashMap<String, Vec<String>>,
    methods: HashMap<String, Vec<String>>,
}

impl Tables {
    fn of(&self, kind: StaticKind) -> &HashMap<String, Vec<String>> {
        match kind {
            StaticKind::Field => &self.fields,
            StaticKind::Method => &self.methods,
        }
    }

    fn of_mut(&mut self, kind: StaticKind) -> &mut HashMap<String, Vec<String>> {
        match kind {
            StaticKind::Field => &mut self.fields,
            StaticKind::Method => &mut self.methods,
        }
    }
}

/// The public static members of one class, as `(kind, name)` — see [`surface`].
#[derive(Default)]
struct Surface {
    statics: Vec<(StaticKind, String)>,
}

/// Static member name → the dotted FQNs of the classes that declare it as `public static`.
#[derive(Default)]
pub struct StaticMemberIndex {
    tables: RwLock<Tables>,
    started: AtomicBool,
    complete: AtomicBool,
}

impl StaticMemberIndex {
    /// The classes declaring a public static `name` of `kind` (dotted FQNs), in the order they were
    /// found. Empty for an unknown name — and, while the sweep is still running, for one it has not
    /// reached yet; see [`is_complete`](Self::is_complete).
    pub fn owners(&self, kind: StaticKind, name: &str) -> Vec<String> {
        let tables = self.tables.read().unwrap_or_else(|p| p.into_inner());
        tables.of(kind).get(name).cloned().unwrap_or_default()
    }

    /// Whether the sweep has covered every class — `false` until then, so "no owner found" can be told
    /// from "not looked at yet".
    pub fn is_complete(&self) -> bool {
        self.complete.load(Ordering::Acquire)
    }

    /// Whether the sweep was ever started. Checked before assembling its input, which is large.
    pub fn is_started(&self) -> bool {
        self.started.load(Ordering::Acquire)
    }

    /// Start the background sweep over `resolver`'s project types and `classpath_classes` (dotted
    /// FQNs). Once only: later calls do nothing.
    ///
    /// The thread holds the index weakly, so replacing the provider that owns it — which happens on
    /// every index build — stops the sweep at its next batch instead of letting it finish a table
    /// nobody will read.
    pub fn start(self: &Arc<Self>, resolver: Arc<IndexResolver<ClasspathIndex>>, classpath_classes: Vec<String>) {
        if self.started.swap(true, Ordering::AcqRel) {
            return;
        }
        let weak = Arc::downgrade(self);
        let spawned = std::thread::Builder::new().name("bennu-static-members".to_string()).spawn(move || {
            if sweep(&weak, &resolver, classpath_classes) {
                if let Some(index) = weak.upgrade() {
                    index.complete.store(true, Ordering::Release);
                }
            }
        });
        if spawned.is_err() {
            // No thread, no sweep — and a later question may find one.
            self.started.store(false, Ordering::Release);
        }
    }

    fn merge(&self, found: Vec<(String, Surface)>) {
        let mut tables = self.tables.write().unwrap_or_else(|p| p.into_inner());
        for (owner, surface) in found {
            for (kind, name) in surface.statics {
                let owners = tables.of_mut(kind).entry(name).or_default();
                if !owners.contains(&owner) {
                    owners.push(owner.clone());
                }
            }
        }
    }
}

/// Read every class and merge what it exports. `false` when the index was dropped before the end.
fn sweep(
    weak: &Weak<StaticMemberIndex>,
    resolver: &IndexResolver<ClasspathIndex>,
    mut classpath_classes: Vec<String>,
) -> bool {
    // The project's own types first: they are what the user is writing against, they are cheap (a
    // members blob each, no bytecode) and a partial answer is most likely to be one of them.
    let project: Vec<String> = resolver
        .project()
        .prefix("")
        .into_iter()
        .filter(|s| {
            matches!(s.kind, SymbolKind::Class | SymbolKind::Interface | SymbolKind::Enum | SymbolKind::Record)
        })
        .map(|s| s.fqn)
        .filter(|fqn| !fqn.is_empty())
        .collect();
    let read_project = |binary: &str| {
        let members = resolver.members_of(binary)?;
        Some(surface(
            members.flags.is_enum,
            members
                .fields
                .iter()
                .map(|m| (StaticKind::Field, m.name.as_str(), m.is_static && is_public(&m.visibility)))
                .chain(
                    members
                        .methods
                        .iter()
                        .map(|m| (StaticKind::Method, m.name.as_str(), m.is_static && is_public(&m.visibility))),
                ),
        ))
    };
    if !merge_batches(weak, &project, read_project) {
        return false;
    }

    classpath_classes.retain(|fqn| !INTERNAL_PREFIXES.iter().any(|p| fqn.starts_with(p)));
    // The JDK before the libraries: `Collectors`, `TimeUnit`, `StandardCharsets` are the static imports
    // everybody writes, and there are few enough of them to be answerable in the first second.
    classpath_classes.sort_by_key(|fqn| !(fqn.starts_with("java.") || fqn.starts_with("javax.")));
    let binaries: Vec<String> = classpath_classes.iter().map(|fqn| fqn.replace('.', "/")).collect();
    let classpath = resolver.jdk_index();
    merge_batches(weak, &binaries, |binary| {
        let members = classpath.members_unmemoized(binary)?;
        Some(surface(
            members.flags.is_enum,
            members
                .fields
                .iter()
                .map(|m| (StaticKind::Field, m.name.as_str(), m.is_static && is_public_cp(&m.visibility)))
                .chain(
                    members
                        .methods
                        .iter()
                        .map(|m| (StaticKind::Method, m.name.as_str(), m.is_static && is_public_cp(&m.visibility))),
                ),
        ))
    })
}

/// Read `classes` (binary or slash-form names) a batch at a time, merging each batch as it completes.
/// `false` when the index went away mid-way.
fn merge_batches(
    weak: &Weak<StaticMemberIndex>,
    classes: &[String],
    read: impl Fn(&str) -> Option<Surface>,
) -> bool {
    for batch in classes.chunks(BATCH) {
        let Some(index) = weak.upgrade() else { return false };
        let found: Vec<(String, Surface)> = batch
            .iter()
            .filter_map(|binary| {
                let surface = read(binary)?;
                (!surface.statics.is_empty()).then(|| (binary.replace('/', "."), surface))
            })
            .collect();
        index.merge(found);
        drop(index);
        std::thread::sleep(PAUSE);
    }
    true
}

fn is_public(visibility: &bennu_java::prelude::Visibility) -> bool {
    matches!(visibility, bennu_java::prelude::Visibility::Public)
}

fn is_public_cp(visibility: &bennu_classpath::prelude::Visibility) -> bool {
    matches!(visibility, bennu_classpath::prelude::Visibility::Public)
}

/// What a class contributes: the `(kind, name, exported)` triples it has, reduced to the ones a static
/// import can name. The one place the policy lives, shared by the project and the classpath reads —
/// two member models, one rule.
fn surface<'a>(is_enum: bool, members: impl Iterator<Item = (StaticKind, &'a str, bool)>) -> Surface {
    let mut statics: Vec<(StaticKind, String)> = members
        .filter(|(_, name, exported)| *exported && importable(name, is_enum))
        .map(|(kind, name, _)| (kind, name.to_string()))
        .collect();
    // Overloads repeat a name; the owner is what the table records, once.
    statics.sort();
    statics.dedup();
    Surface { statics }
}

/// Whether a member name is one a person would static-import: not a synthetic (`lambda$run$0`,
/// `$VALUES`), not an initializer, not `main`, and not the `values`/`valueOf` every enum carries — a
/// name that would otherwise list every enum on the classpath as its owner.
fn importable(name: &str, is_enum: bool) -> bool {
    !name.is_empty()
        && !name.contains('$')
        && !name.starts_with('<')
        && name != "main"
        && !(is_enum && matches!(name, "values" | "valueOf"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn statics(is_enum: bool, members: &[(StaticKind, &str, bool)]) -> Vec<(StaticKind, String)> {
        surface(is_enum, members.iter().copied()).statics
    }

    #[test]
    fn only_exported_names_are_kept_once_each() {
        let got = statics(
            false,
            &[
                (StaticKind::Field, "MAX", true),
                (StaticKind::Method, "of", true),
                (StaticKind::Method, "of", true), // an overload
                (StaticKind::Field, "hidden", false),
            ],
        );
        assert_eq!(got, [(StaticKind::Field, "MAX".to_string()), (StaticKind::Method, "of".to_string())]);
    }

    #[test]
    fn synthetics_and_initializers_are_not_importable() {
        let got = statics(
            false,
            &[
                (StaticKind::Method, "lambda$run$0", true),
                (StaticKind::Field, "$VALUES", true),
                (StaticKind::Method, "<clinit>", true),
                (StaticKind::Method, "main", true),
                (StaticKind::Method, "help", true),
            ],
        );
        assert_eq!(got, [(StaticKind::Method, "help".to_string())]);
    }

    #[test]
    fn an_enum_does_not_offer_its_implicit_methods_but_its_constants_and_other_statics() {
        let members = [
            (StaticKind::Field, "RED", true),
            (StaticKind::Method, "values", true),
            (StaticKind::Method, "valueOf", true),
            (StaticKind::Method, "parse", true),
        ];
        let names: Vec<_> = statics(true, &members).into_iter().map(|(_, n)| n).collect();
        assert_eq!(names, ["RED", "parse"]);
        // On a class the same two names are ordinary static methods (`Integer.valueOf`).
        let names: Vec<_> = statics(false, &members).into_iter().map(|(_, n)| n).collect();
        assert_eq!(names, ["RED", "parse", "valueOf", "values"]);
    }

    #[test]
    fn a_name_is_looked_up_as_a_field_or_as_a_method_never_as_both() {
        let index = StaticMemberIndex::default();
        index.merge(vec![
            ("a.Limits".to_string(), Surface { statics: vec![(StaticKind::Field, "MAX".to_string())] }),
            ("a.Maths".to_string(), Surface { statics: vec![(StaticKind::Method, "MAX".to_string())] }),
        ]);
        assert_eq!(index.owners(StaticKind::Field, "MAX"), ["a.Limits"]);
        assert_eq!(index.owners(StaticKind::Method, "MAX"), ["a.Maths"]);
        assert!(index.owners(StaticKind::Field, "MIN").is_empty());
    }

    #[test]
    fn a_class_found_twice_is_one_owner() {
        let index = StaticMemberIndex::default();
        for _ in 0..2 {
            index.merge(vec![(
                "a.Limits".to_string(),
                Surface { statics: vec![(StaticKind::Field, "MAX".to_string())] },
            )]);
        }
        index.merge(vec![(
            "b.Limits".to_string(),
            Surface { statics: vec![(StaticKind::Field, "MAX".to_string())] },
        )]);
        assert_eq!(index.owners(StaticKind::Field, "MAX"), ["a.Limits", "b.Limits"]);
    }

    #[test]
    fn a_fresh_index_is_neither_started_nor_complete() {
        let index = StaticMemberIndex::default();
        assert!(!index.is_started());
        assert!(!index.is_complete());
    }
}
