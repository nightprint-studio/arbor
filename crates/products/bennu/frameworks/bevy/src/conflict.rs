//! Which pairs of systems an access conflict serialises.
//!
//! ## Why this claim survives a partial scan, and the ordering graph would not
//!
//! This crate reads the project's own sources and nothing else — not the engine's plugins, not a
//! dependency's. So the set of systems it knows is a **subset** of the schedule's. That makes some
//! claims safe and others worthless, and the difference is monotonicity:
//!
//! * *"these two can never run at the same time, because both want `Score`"* stays true however
//!   many systems are added later. A conflict cannot be un-conflicted by a system nobody has read.
//! * *"these two run in parallel"* is unprovable from a subset: one unseen system writing
//!   `Transform` would refute it. So it is never claimed here.
//!
//! Everything below is therefore a **negative** claim, and the report is short of pairs rather than
//! full of invented ones.
//!
//! ## Ordering is a note, not a verdict
//!
//! Two conflicting systems that are explicitly ordered are fine — that is what `.before` / `.after`
//! / `.chain` are for. Two that are *not* run in whichever order the schedule happens to pick,
//! which is the latent bug: a frame-order dependency nobody wrote down. This module records which
//! of the two a pair is, from the ordering it could see, and says `unordered` only in the sense of
//! "no ordering appears in this project's own `add_systems` / `configure_sets` calls". The latter
//! orders *sets*, and is expanded to the systems in them: `configure_sets(Update, (A, B).chain())`
//! orders every system `.in_set(A)` before every one `.in_set(B)`.

use std::collections::{HashMap, HashSet};

use crate::model::{Access, AccessKind, Filter, SystemDecl};

/// Above this many systems in one schedule, the pairwise walk stops being worth its cost — and a
/// report of tens of thousands of pairs is not one anybody reads. The panel says so rather than
/// silently truncating.
pub const MAX_PAIRWISE: usize = 400;

/// What the project's own registrations say about the order of a conflicting pair.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ordering {
    /// A `.before` / `.after` names one from the other, or a `.chain()` covers both.
    Explicit,
    /// Both are in the same set — which may or may not be ordered against itself, so this is
    /// neither a clean bill of health nor an accusation.
    SameSet,
    /// Nothing in this project orders them.
    Unordered,
}

impl Ordering {
    pub fn label(self) -> &'static str {
        match self {
            Ordering::Explicit => "ordered",
            Ordering::SameSet => "same set",
            Ordering::Unordered => "unordered",
        }
    }
}

/// One reason a pair cannot run in parallel.
#[derive(Debug, Clone)]
pub struct Reason {
    /// The contended type — or `World` for an exclusive system.
    pub target: String,
    /// `write/write` or `read/write`.
    pub kind: &'static str,
    /// The two parameters it was read from: the evidence, so a wrong row can be argued with.
    pub a_param: String,
    pub b_param: String,
}

/// A pair of systems that the engine must serialise.
#[derive(Debug, Clone)]
pub struct Conflict {
    /// Indices into [`crate::model::BevyModel::systems`].
    pub a: usize,
    pub b: usize,
    pub schedule: String,
    pub reasons: Vec<Reason>,
    pub ordering: Ordering,
}

/// What the project says about system order.
///
/// Two kinds of fact, kept apart because they close differently:
///
/// * **Directed edges** (`.before` / `.after` / `.chain()`): `a → b` means `a` runs first. These
///   are transitive — `a → b` and `b → c` order `a` against `c` even though nothing names the pair
///   — so [`contains`](Self::contains) follows paths. Direction matters: `a → b` and `c → b` leave
///   `a` and `c` unordered, and an undirected closure would have merged them.
/// * **Undirected pairs** (`.ambiguous_with`, [`add`](Self::add)): a statement about exactly that
///   pair, which says nothing about anyone else.
///
/// ## Sets
///
/// A node is a system **or a set** — `configure_sets(Update, (A, B).chain())` is an edge `A → B`
/// exactly as `a.before(b)` is `a → b`, and the graph does not care which kind of name it holds.
/// What links the two kinds is [`add_member`](Self::add_member): a system in `A` carries `A`'s
/// edges, and a set in `P` carries `P`'s. [`contains`](Self::contains) therefore asks whether any
/// of `a`'s *positions* (itself, its sets, their parents) leads to any of `b`'s — which expands a
/// set-level edge to its member systems without ever materialising the cross product.
///
/// Sets that an external plugin configures simply have no edges here: a pair that only they could
/// order is reported as not proven, which is the cautious answer (see [`warnable`]).
#[derive(Debug, Default)]
pub struct OrderIndex {
    pairs: HashSet<(String, String)>,
    next: HashMap<String, Vec<String>>,
    /// `member → sets it was put in with `.in_set(…)``, for systems and sets alike.
    parents: HashMap<String, Vec<String>>,
}

impl OrderIndex {
    /// Record that `member` (a system, or a set) was put in `set` with `.in_set(…)`.
    pub fn add_member(&mut self, member: &str, set: &str) {
        if member != set {
            let sets = self.parents.entry(member.to_string()).or_default();
            if !sets.iter().any(|s| s == set) {
                sets.push(set.to_string());
            }
        }
    }

    /// `name` and every set it sits in, directly or through nested sets.
    pub fn positions(&self, name: &str) -> Vec<String> {
        let mut out = vec![name.to_string()];
        let mut i = 0;
        while i < out.len() {
            for p in self.parents.get(&out[i]).into_iter().flatten() {
                if !out.contains(p) {
                    out.push(p.clone());
                }
            }
            i += 1;
        }
        out
    }

    /// Record that `a` and `b` are ordered (or declared not to matter), direction unknown.
    pub fn add(&mut self, a: &str, b: &str) {
        if a != b {
            self.pairs.insert(key(a, b));
        }
    }

    /// Record that `first` runs before `later`.
    pub fn add_edge(&mut self, first: &str, later: &str) {
        if first != later {
            self.next.entry(first.to_string()).or_default().push(later.to_string());
        }
    }

    pub fn contains(&self, a: &str, b: &str) -> bool {
        let from = self.positions(a);
        let to = self.positions(b);
        let paired = from.iter().any(|x| to.iter().any(|y| x != y && self.pairs.contains(&key(x, y))));
        paired || self.reaches(&from, &to) || self.reaches(&to, &from)
    }

    /// Whether a chain of `before` edges leads from any of `from` to any of `to`.
    fn reaches(&self, from: &[String], to: &[String]) -> bool {
        let mut seen: HashSet<&str> = HashSet::new();
        let mut stack: Vec<&str> = from.iter().map(String::as_str).collect();
        while let Some(at) = stack.pop() {
            for n in self.next.get(at).into_iter().flatten() {
                if to.iter().any(|t| t == n) {
                    return true;
                }
                if seen.insert(n.as_str()) {
                    stack.push(n.as_str());
                }
            }
        }
        false
    }
}

fn key(a: &str, b: &str) -> (String, String) {
    if a <= b {
        (a.to_string(), b.to_string())
    } else {
        (b.to_string(), a.to_string())
    }
}

/// Every conflicting pair, schedule by schedule.
///
/// Two systems are only compared when they share a schedule: a `Update` system and a `Startup` one
/// are never candidates for the same frame, and pairing them would fill the panel with rows that
/// are true of nothing.
pub fn detect(systems: &[SystemDecl], order: &OrderIndex) -> Vec<Conflict> {
    let mut schedules: Vec<&str> =
        systems.iter().flat_map(|s| s.schedules.iter()).map(String::as_str).collect();
    schedules.sort_unstable();
    schedules.dedup();

    let mut out = Vec::new();
    for schedule in schedules {
        let members: Vec<usize> = systems
            .iter()
            .enumerate()
            .filter(|(_, s)| s.schedules.iter().any(|x| x == schedule))
            .map(|(i, _)| i)
            .collect();
        if members.len() > MAX_PAIRWISE {
            continue;
        }
        for (n, &i) in members.iter().enumerate() {
            for &j in &members[n + 1..] {
                let reasons = reasons_between(&systems[i], &systems[j]);
                if reasons.is_empty() {
                    continue;
                }
                out.push(Conflict {
                    a: i,
                    b: j,
                    schedule: schedule.to_string(),
                    reasons,
                    ordering: ordering_of(&systems[i], &systems[j], order),
                });
            }
        }
    }
    out
}

/// The schedules whose system count put them past [`MAX_PAIRWISE`] — what the panel says instead
/// of letting an empty result read as "no conflicts here".
pub fn skipped_schedules(systems: &[SystemDecl]) -> Vec<String> {
    let mut counts: Vec<(String, usize)> = Vec::new();
    for s in systems {
        for sched in &s.schedules {
            match counts.iter_mut().find(|(name, _)| name == sched) {
                Some((_, n)) => *n += 1,
                None => counts.push((sched.clone(), 1)),
            }
        }
    }
    counts.into_iter().filter(|(_, n)| *n > MAX_PAIRWISE).map(|(name, _)| name).collect()
}

/// Whether an unordered pair is worth a **warning in the editor**, as opposed to a row in the
/// panel.
///
/// Narrower than "the ordering says unordered", because a warning has to be right about a negative:
///
/// * A system in a **set** is never accused. The project's own `configure_sets` calls are read and
///   do count towards [`Ordering::Explicit`], but a set can equally be ordered by a plugin this
///   crate never sees (the engine's, a dependency's) — so "not proven ordered" is not "unordered"
///   for a set member, and a squiggle under working code is the price of getting that wrong.
/// * An **exclusive** system is never accused. It contends with everything in its schedule by
///   construction; that is what taking `&mut World` means, not an oversight to report on every peer.
///
/// * A system behind a **run condition** is never accused. `run_if(in_state(A))` against
///   `run_if(in_state(B))` is the ordinary way to make two writers exclusive in time, and this crate
///   cannot evaluate a condition — so it takes the cautious side for any of them.
/// * A pair that contends **only through a message buffer**, one side reading and the other
///   writing, is not accused: a reader running a frame late is how messages are meant to flow, and
///   it is the pattern of nearly every event in a project.
///
/// What is left is the case the tooling exists for: two ordinary systems, in one schedule, wanting
/// the same data, with nothing anywhere in the project saying which goes first.
pub fn warnable(c: &Conflict, systems: &[SystemDecl]) -> bool {
    if c.ordering != Ordering::Unordered {
        return false;
    }
    let (Some(a), Some(b)) = (systems.get(c.a), systems.get(c.b)) else { return false };
    if a.exclusive || b.exclusive || !a.sets.is_empty() || !b.sets.is_empty() {
        return false;
    }
    if a.conditional || b.conditional {
        return false;
    }
    !c.reasons.iter().all(is_message_handoff)
}

/// A reader meeting a writer of the same `Messages<T>` buffer.
fn is_message_handoff(r: &Reason) -> bool {
    r.target.starts_with("Messages<") && r.kind == "read/write"
}

fn ordering_of(a: &SystemDecl, b: &SystemDecl, order: &OrderIndex) -> Ordering {
    if order.contains(&a.name, &b.name) {
        return Ordering::Explicit;
    }
    if a.sets.iter().any(|s| b.sets.contains(s)) {
        return Ordering::SameSet;
    }
    Ordering::Unordered
}

fn reasons_between(a: &SystemDecl, b: &SystemDecl) -> Vec<Reason> {
    // An exclusive system takes the whole world: it is serialised against everything in its
    // schedule, and listing every component it might touch would say less than this one row.
    if a.exclusive || b.exclusive {
        return vec![Reason {
            target: "World".to_string(),
            kind: "exclusive",
            a_param: if a.exclusive { "&mut World".into() } else { a.access_summary() },
            b_param: if b.exclusive { "&mut World".into() } else { b.access_summary() },
        }];
    }
    let mut out: Vec<Reason> = Vec::new();
    for x in &a.accesses {
        for y in &b.accesses {
            if !contend(x, y) {
                continue;
            }
            if out.iter().any(|r| r.target == x.target) {
                continue;
            }
            out.push(Reason {
                target: x.target.clone(),
                kind: if x.kind.writes() && y.kind.writes() { "write/write" } else { "read/write" },
                a_param: x.param.clone(),
                b_param: y.param.clone(),
            });
        }
    }
    out
}

/// Whether two accesses contend — the rule the engine applies, minus what this crate cannot prove.
fn contend(x: &Access, y: &Access) -> bool {
    if x.target != y.target || is_resource(x.kind) != is_resource(y.kind) {
        return false;
    }
    if !x.kind.writes() && !y.kind.writes() {
        return false;
    }
    // A filter expression with an `Or` in it can make two queries disjoint in ways this crate does
    // not model. Rather than claim a conflict it cannot stand behind, it says nothing.
    if x.opaque_filter || y.opaque_filter {
        return false;
    }
    !disjoint(&x.filters, &y.filters)
}

fn is_resource(kind: AccessKind) -> bool {
    matches!(kind, AccessKind::ResourceRead | AccessKind::ResourceWrite)
}

/// Bevy's disjointness rule, which is narrower than it first looks: two queries are disjoint when
/// one **requires** a component the other **excludes**. Two queries filtered on *different*
/// components are not disjoint — an entity may have both.
fn disjoint(a: &[Filter], b: &[Filter]) -> bool {
    a.iter().any(|f| match f {
        Filter::With(t) => b.contains(&Filter::Without(t.clone())),
        Filter::Without(t) => b.contains(&Filter::With(t.clone())),
    })
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::{detect, disjoint, warnable, OrderIndex, Ordering};
    use crate::model::{Access, AccessKind, Filter, SystemDecl};

    fn sys(name: &str, schedule: &str, accesses: Vec<Access>) -> SystemDecl {
        SystemDecl {
            name: name.to_string(),
            file: PathBuf::from("src/main.rs"),
            offset: 0,
            line: 1,
            accesses,
            exclusive: false,
            schedules: vec![schedule.to_string()],
            sets: Vec::new(),
            conditional: false,
        }
    }

    fn access(target: &str, kind: AccessKind, filters: Vec<Filter>) -> Access {
        Access {
            target: target.to_string(),
            kind,
            filters,
            opaque_filter: false,
            param: format!("q: Query<&{target}>"),
        }
    }

    #[test]
    fn a_write_and_a_read_of_the_same_component_contend() {
        let systems = vec![
            sys("move_player", "Update", vec![access("Transform", AccessKind::ComponentWrite, vec![])]),
            sys("draw", "Update", vec![access("Transform", AccessKind::ComponentRead, vec![])]),
        ];
        let found = detect(&systems, &OrderIndex::default());
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].reasons[0].target, "Transform");
        assert_eq!(found[0].ordering, Ordering::Unordered);
    }

    #[test]
    fn two_reads_never_contend() {
        let systems = vec![
            sys("a", "Update", vec![access("Transform", AccessKind::ComponentRead, vec![])]),
            sys("b", "Update", vec![access("Transform", AccessKind::ComponentRead, vec![])]),
        ];
        assert!(detect(&systems, &OrderIndex::default()).is_empty());
    }

    #[test]
    fn a_component_and_a_resource_of_the_same_name_are_different_things() {
        let systems = vec![
            sys("a", "Update", vec![access("Score", AccessKind::ComponentWrite, vec![])]),
            sys("b", "Update", vec![access("Score", AccessKind::ResourceWrite, vec![])]),
        ];
        assert!(detect(&systems, &OrderIndex::default()).is_empty());
    }

    #[test]
    fn different_schedules_are_never_paired() {
        let systems = vec![
            sys("a", "Update", vec![access("Score", AccessKind::ResourceWrite, vec![])]),
            sys("b", "FixedUpdate", vec![access("Score", AccessKind::ResourceWrite, vec![])]),
        ];
        assert!(detect(&systems, &OrderIndex::default()).is_empty());
    }

    #[test]
    fn with_and_without_the_same_component_are_disjoint() {
        assert!(disjoint(
            &[Filter::With("Player".into())],
            &[Filter::Without("Player".into())]
        ));
        // Two different requirements are NOT disjoint: an entity can carry both.
        assert!(!disjoint(&[Filter::With("Player".into())], &[Filter::With("Enemy".into())]));
    }

    #[test]
    fn an_explicit_order_is_recorded_but_the_pair_is_still_serialised() {
        let systems = vec![
            sys("a", "Update", vec![access("Score", AccessKind::ResourceWrite, vec![])]),
            sys("b", "Update", vec![access("Score", AccessKind::ResourceRead, vec![])]),
        ];
        let mut order = OrderIndex::default();
        order.add("a", "b");
        let found = detect(&systems, &order);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].ordering, Ordering::Explicit);
    }

    #[test]
    fn before_edges_are_transitive_but_a_fan_in_orders_nothing() {
        let mut chain = OrderIndex::default();
        chain.add_edge("a", "b");
        chain.add_edge("b", "c");
        assert!(chain.contains("a", "c") && chain.contains("c", "a"));

        let mut fan_in = OrderIndex::default();
        fan_in.add_edge("a", "b");
        fan_in.add_edge("c", "b");
        assert!(!fan_in.contains("a", "c"), "both run before b; nothing says which is first");
    }

    #[test]
    fn a_set_edge_orders_the_systems_in_the_sets() {
        let mut order = OrderIndex::default();
        order.add_edge("Input", "Logic");
        order.add_member("read_keys", "Input");
        order.add_member("move_player", "Logic");
        assert!(order.contains("read_keys", "move_player"));
        assert!(order.contains("move_player", "read_keys"));
        assert!(!order.contains("read_keys", "elsewhere"));
    }

    #[test]
    fn set_edges_follow_nesting_and_pass_through_an_empty_set() {
        let mut order = OrderIndex::default();
        order.add_edge("A", "B");
        order.add_edge("B", "C");
        order.add_member("a_sys", "Inner");
        order.add_member("Inner", "A");
        order.add_member("c_sys", "C");
        // `B` has no member here, as when a plugin fills it: the order still holds through it.
        assert!(order.contains("a_sys", "c_sys"));
    }

    #[test]
    fn a_system_ordered_against_a_set_is_ordered_against_its_members() {
        let mut order = OrderIndex::default();
        order.add_edge("Physics", "draw"); // `draw.after(Physics)`
        order.add_member("step", "Physics");
        assert!(order.contains("step", "draw"));
        assert!(!order.contains("other", "draw"));
    }

    #[test]
    fn two_sets_both_before_a_third_stay_unordered_against_each_other() {
        let mut order = OrderIndex::default();
        order.add_edge("A", "C");
        order.add_edge("B", "C");
        order.add_member("a_sys", "A");
        order.add_member("b_sys", "B");
        assert!(!order.contains("a_sys", "b_sys"));
    }

    #[test]
    fn a_cyclic_ordering_terminates() {
        let mut order = OrderIndex::default();
        order.add_edge("a", "b");
        order.add_edge("b", "a");
        assert!(!order.contains("a", "z"));
    }

    #[test]
    fn a_system_behind_a_run_condition_is_never_warned_about() {
        let mut systems = vec![
            sys("a", "Update", vec![access("Score", AccessKind::ResourceWrite, vec![])]),
            sys("b", "Update", vec![access("Score", AccessKind::ResourceWrite, vec![])]),
        ];
        let found = detect(&systems, &OrderIndex::default());
        assert!(warnable(&found[0], &systems));
        systems[1].conditional = true;
        assert!(!warnable(&found[0], &systems));
    }

    #[test]
    fn a_message_reader_and_writer_are_not_warned_but_two_writers_are() {
        let msg = |kind| access("Messages<Hit>", kind, vec![]);
        let handoff = vec![
            sys("w", "Update", vec![msg(AccessKind::ResourceWrite)]),
            sys("r", "Update", vec![msg(AccessKind::ResourceRead)]),
        ];
        let found = detect(&handoff, &OrderIndex::default());
        assert_eq!(found.len(), 1);
        assert!(!warnable(&found[0], &handoff));

        let writers = vec![
            sys("w1", "Update", vec![msg(AccessKind::ResourceWrite)]),
            sys("w2", "Update", vec![msg(AccessKind::ResourceWrite)]),
        ];
        let found = detect(&writers, &OrderIndex::default());
        assert!(warnable(&found[0], &writers));
    }

    #[test]
    fn an_exclusive_system_contends_with_everything_in_its_schedule() {
        let mut ex = sys("save", "Update", vec![]);
        ex.exclusive = true;
        let systems =
            vec![ex, sys("b", "Update", vec![access("Score", AccessKind::ResourceRead, vec![])])];
        let found = detect(&systems, &OrderIndex::default());
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].reasons[0].kind, "exclusive");
    }
}
