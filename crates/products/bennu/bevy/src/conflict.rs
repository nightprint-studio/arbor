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
//! "no ordering appears in this project's own `add_systems` calls".

use std::collections::HashSet;

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

/// Pairs known to be ordered, keyed by the two system names in sorted order.
#[derive(Debug, Default)]
pub struct OrderIndex(HashSet<(String, String)>);

impl OrderIndex {
    pub fn add(&mut self, a: &str, b: &str) {
        if a != b {
            self.0.insert(key(a, b));
        }
    }

    pub fn contains(&self, a: &str, b: &str) -> bool {
        self.0.contains(&key(a, b))
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
/// * A system in a **set** is never accused. A set's ordering is declared by `configure_sets`,
///   which this crate does not read — so two systems in sets may well be ordered by something it
///   cannot see, and saying otherwise would be a squiggle under working code.
/// * An **exclusive** system is never accused. It contends with everything in its schedule by
///   construction; that is what taking `&mut World` means, not an oversight to report on every peer.
///
/// What is left is the case the tooling exists for: two ordinary systems, in one schedule, wanting
/// the same data, with nothing anywhere in the project saying which goes first.
pub fn warnable(c: &Conflict, systems: &[SystemDecl]) -> bool {
    if c.ordering != Ordering::Unordered {
        return false;
    }
    let (Some(a), Some(b)) = (systems.get(c.a), systems.get(c.b)) else { return false };
    !a.exclusive && !b.exclusive && a.sets.is_empty() && b.sets.is_empty()
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

    use super::{detect, disjoint, OrderIndex, Ordering};
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
