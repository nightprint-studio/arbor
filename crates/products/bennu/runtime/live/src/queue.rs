//! The build queue: numbered tickets, served strictly in order, one run per target at a time.
//!
//! Several conversations (and the user's own Build button) can ask to build or test the same project at
//! once. Two compilers on one `target/` fight over its lock, and refusing the second one made the waiting
//! agent poll — and the agent that polled most starved the rest. This is the broker that replaces the
//! refusal. Everything here is pure plumbing over plain data (a mutex, a condvar), so the rules are tested
//! without a toolchain:
//!
//! * **Tickets, fair FIFO.** Every request takes a ticket with [`Queue::submit`]; requests of one *target*
//!   (a canonical build output directory or project root — the key is the caller's) are served in the order
//!   they were submitted. One that just finished has to submit again and goes to the back.
//! * **Coalescing.** A request whose *class* equals that of a group still **waiting** (not started) joins the
//!   group instead of queueing again: one run serves them all, and every member gets the same result. A
//!   group that has started takes no more riders — a request that arrives then is about a tree that may have
//!   changed since, so it queues behind. A request can opt out (`coalesce: false`).
//! * **Whoever is first runs it.** The thread of the first member that finds its group at the head of its
//!   target becomes the [`Lead`] and does the work; the others wait on the result. A member that is cancelled
//!   or times out simply leaves, so the group never depends on any one caller staying.
//! * **No worker is parked on anything but this queue.** [`Queue::wait`] blocks the calling thread on a
//!   condvar, polling the caller's cancel flag; callers run it off the async runtime's workers.
//!
//! The queue knows nothing about what a run produces: the result is an opaque [`Payload`] the caller
//! downcasts.

use std::any::Any;
use std::collections::HashSet;
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use serde::Serialize;

/// What a run produced, shared by every ticket the run served.
pub type Payload = Arc<dyn Any + Send + Sync>;

/// How often a waiting caller looks at its cancel flag and its deadline. A state change wakes it sooner.
const POLL: Duration = Duration::from_millis(100);

/// Who asked: a conversation of the assistant, or the user (empty conversation).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize)]
pub struct Owner {
    pub conversation: String,
    pub title: String,
}

impl Owner {
    pub fn user() -> Self {
        Self { conversation: String::new(), title: "You".into() }
    }

    pub fn is_user(&self) -> bool {
        self.conversation.is_empty()
    }
}

/// What kind of work a ticket is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Build,
    Test,
    /// A whole-project validation or any other exclusive use of the tree.
    Check,
}

impl Kind {
    pub fn word(self) -> &'static str {
        match self {
            Kind::Build => "build",
            Kind::Test => "test run",
            Kind::Check => "check",
        }
    }
}

/// One request for a turn on a target.
#[derive(Debug, Clone)]
pub struct Request {
    /// The tree being built: requests with the same target are serialised.
    pub target: String,
    /// Requests of one class may share a run (same command, same module, same test selection).
    pub class: String,
    pub kind: Kind,
    /// What the user reads in the queue: `cargo check -p core`.
    pub label: String,
    pub owner: Owner,
    /// May this request ride another's run (and others ride its)? False runs on its own.
    pub coalesce: bool,
}

/// The ticket a request was given.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Ticket {
    pub id: u64,
    /// Joined a group that had not started: no run of its own will be made for it.
    pub joined: bool,
}

/// How a wait ended.
pub enum Wait<'q> {
    /// It is this ticket's turn to run the work; report it with [`Lead::finish`].
    Lead(Lead<'q>),
    /// Another ticket's run served this one.
    Shared(Shared),
    /// The caller cancelled (or the ticket was cancelled from the panel).
    Cancelled,
    /// The deadline passed before the turn came.
    TimedOut,
}

/// A run made for several tickets, as one of the riders receives it.
pub struct Shared {
    pub result: Result<Payload, String>,
    /// How many tickets that run served, the runner's included.
    pub served: usize,
    /// How long this ticket waited.
    pub waited: Duration,
}

#[derive(Debug, Clone)]
struct Member {
    ticket: u64,
    owner: Owner,
    label: String,
    at: Instant,
}

enum Phase {
    Waiting,
    Running { by: u64 },
    Done { result: Result<Payload, String>, served: usize },
}

struct Group {
    target: String,
    class: String,
    kind: Kind,
    coalesce: bool,
    members: Vec<Member>,
    phase: Phase,
}

#[derive(Default)]
struct State {
    next_ticket: u64,
    /// In submission order: this is the FIFO.
    groups: Vec<Group>,
    cancelled: HashSet<u64>,
}

type Observer = Arc<dyn Fn() + Send + Sync>;

/// The queue. One per process is the usual shape (a `static`), but nothing here is global.
#[derive(Default)]
pub struct Queue {
    state: Mutex<State>,
    wake: Condvar,
    observer: Mutex<Option<Observer>>,
}

/// The right to run the work of a group. Dropping it without [`Lead::finish`] reports a failure to the
/// riders, so a run that panicked cannot leave the target blocked.
pub struct Lead<'q> {
    queue: &'q Queue,
    ticket: u64,
    finished: bool,
}

impl Lead<'_> {
    /// How many other tickets this run is serving right now.
    pub fn riders(&self) -> usize {
        let st = self.queue.lock();
        st.groups.iter().find(|g| matches!(g.phase, Phase::Running { by, .. } if by == self.ticket)).map_or(0, |g| g.members.len().saturating_sub(1))
    }

    /// Report the run's result to every ticket it served, and free the target for the next group.
    pub fn finish(mut self, result: Result<Payload, String>) {
        self.finished = true;
        self.queue.finish(self.ticket, result);
    }
}

impl Drop for Lead<'_> {
    fn drop(&mut self) {
        if !self.finished {
            self.queue.finish(self.ticket, Err("the run ended without reporting a result".into()));
        }
    }
}

/// Where a row of the queue is in its life.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum RowState {
    /// Its run is in progress and it is the one running it.
    Running,
    /// Waiting for its turn, and it will run the work.
    Waiting,
    /// Waiting, and another ticket's run will serve it too.
    Riding,
}

/// Who is ahead of a waiting ticket.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Behind {
    pub owner: Owner,
    pub kind: Kind,
    pub label: String,
}

/// One ticket as the panel shows it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Row {
    pub ticket: u64,
    pub target: String,
    pub kind: Kind,
    pub label: String,
    pub owner: Owner,
    pub state: RowState,
    /// 1 = the run in progress (or the one this ticket rides), 2 = the next, and so on, per target.
    pub position: usize,
    /// The run ahead of it, for a ticket that is not at the front.
    pub behind: Option<Behind>,
    pub waited_ms: u64,
}

impl Queue {
    pub fn new() -> Self {
        Self::default()
    }

    /// Be told, after every change, that the queue looks different now. Called without any lock held, so
    /// the callback may take a [`Queue::snapshot`].
    pub fn set_observer(&self, observer: impl Fn() + Send + Sync + 'static) {
        *self.observer.lock().unwrap_or_else(|p| p.into_inner()) = Some(Arc::new(observer));
    }

    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|p| p.into_inner())
    }

    fn notify_changed(&self) {
        self.wake.notify_all();
        let observer = self.observer.lock().unwrap_or_else(|p| p.into_inner()).clone();
        if let Some(o) = observer {
            o();
        }
    }

    /// Take a ticket. Never blocks: the turn is awaited with [`Queue::wait`].
    pub fn submit(&self, req: Request, now: Instant) -> Ticket {
        let ticket = {
            let mut st = self.lock();
            st.next_ticket += 1;
            let id = st.next_ticket;
            let member = Member { ticket: id, owner: req.owner.clone(), label: req.label.clone(), at: now };
            let joinable = if req.coalesce {
                st.groups.iter().position(|g| g.coalesce && matches!(g.phase, Phase::Waiting) && g.target == req.target && g.class == req.class)
            } else {
                None
            };
            match joinable {
                Some(i) => {
                    st.groups[i].members.push(member);
                    Ticket { id, joined: true }
                }
                None => {
                    st.groups.push(Group { target: req.target, class: req.class, kind: req.kind, coalesce: req.coalesce, members: vec![member], phase: Phase::Waiting });
                    Ticket { id, joined: false }
                }
            }
        };
        self.notify_changed();
        ticket
    }

    /// Block until the ticket's turn comes, another ticket's run serves it, `cancelled` says stop, or
    /// `timeout` passes. `cancelled` is polled while holding the queue's lock: it must be cheap (an atomic
    /// load) and must not call back into the queue.
    pub fn wait(&self, ticket: u64, cancelled: &dyn Fn() -> bool, timeout: Option<Duration>) -> Wait<'_> {
        let started = Instant::now();
        let mut st = self.lock();
        loop {
            let outcome = if st.cancelled.remove(&ticket) || cancelled() {
                Some(Self::leave(&mut st, ticket, Wait::Cancelled))
            } else {
                match Self::step(&mut st, ticket, self) {
                    Step::Ready(w) => Some(w),
                    Step::Pending if timeout.is_some_and(|t| started.elapsed() >= t) => Some(Self::leave(&mut st, ticket, Wait::TimedOut)),
                    Step::Pending => None,
                }
            };
            if let Some(w) = outcome {
                drop(st);
                self.notify_changed();
                return w;
            }
            st = self.wake.wait_timeout(st, POLL).unwrap_or_else(|p| p.into_inner()).0;
        }
    }

    /// One look at the ticket without waiting: `None` while it is still queued.
    pub fn try_take(&self, ticket: u64) -> Option<Wait<'_>> {
        let mut st = self.lock();
        let ready = if st.cancelled.remove(&ticket) { Some(Self::leave(&mut st, ticket, Wait::Cancelled)) } else { Self::step(&mut st, ticket, self).ready() };
        drop(st);
        if ready.is_some() {
            self.notify_changed();
        }
        ready
    }

    /// Withdraw a ticket from the queue (the panel's Cancel). False when it is the run in progress — that
    /// one is stopped through the tool that runs it — or is not in the queue.
    pub fn cancel(&self, ticket: u64) -> bool {
        let ok = {
            let mut st = self.lock();
            let runs = st.groups.iter().any(|g| matches!(g.phase, Phase::Running { by, .. } if by == ticket));
            let queued = st.groups.iter().any(|g| g.members.iter().any(|m| m.ticket == ticket));
            let ok = queued && !runs;
            if ok {
                st.cancelled.insert(ticket);
            }
            ok
        };
        if ok {
            self.notify_changed();
        }
        ok
    }

    /// Whether any ticket is queued or running on `target`.
    pub fn busy(&self, target: &str) -> bool {
        self.lock().groups.iter().any(|g| g.target == target && !matches!(g.phase, Phase::Done { .. }))
    }

    /// Every ticket in the queue, per target in order: the run in progress first, then the waiting ones.
    pub fn snapshot(&self) -> Vec<Row> {
        let st = self.lock();
        let mut rows = Vec::new();
        let mut targets: Vec<&str> = Vec::new();
        for g in &st.groups {
            if !targets.contains(&g.target.as_str()) {
                targets.push(&g.target);
            }
        }
        for target in targets {
            // The running group is always the front, whatever its place in the vector.
            let mut line: Vec<&Group> = st.groups.iter().filter(|g| g.target == target && !matches!(g.phase, Phase::Done { .. })).collect();
            line.sort_by_key(|g| !matches!(g.phase, Phase::Running { .. }));
            for (i, group) in line.iter().enumerate() {
                let ahead = i.checked_sub(1).map(|j| line[j]).and_then(|g| g.members.first().map(|m| Behind { owner: m.owner.clone(), kind: g.kind, label: m.label.clone() }));
                let runner = match group.phase {
                    Phase::Running { by, .. } => Some(by),
                    _ => None,
                };
                for (n, m) in group.members.iter().enumerate() {
                    let state = match (runner, n) {
                        (Some(by), _) if by == m.ticket => RowState::Running,
                        (None, 0) => RowState::Waiting,
                        _ => RowState::Riding,
                    };
                    rows.push(Row {
                        ticket: m.ticket,
                        target: group.target.clone(),
                        kind: group.kind,
                        label: m.label.clone(),
                        owner: m.owner.clone(),
                        state,
                        position: i + 1,
                        behind: ahead.clone(),
                        waited_ms: m.at.elapsed().as_millis() as u64,
                    });
                }
            }
        }
        rows
    }

    // ── internals ────────────────────────────────────────────────────────────────────────────────

    fn finish(&self, ticket: u64, result: Result<Payload, String>) {
        {
            let mut st = self.lock();
            if let Some(i) = st.groups.iter().position(|g| matches!(g.phase, Phase::Running { by, .. } if by == ticket)) {
                let g = &mut st.groups[i];
                let served = g.members.len();
                g.members.retain(|m| m.ticket != ticket);
                if g.members.is_empty() {
                    st.groups.remove(i);
                } else {
                    g.phase = Phase::Done { result, served };
                }
            }
        }
        self.notify_changed();
    }

    /// Take `ticket` out of the queue; the group goes with its last member, unless it is mid-run (the runner
    /// is committed, and riders that stay still need its result).
    fn leave<'q>(st: &mut State, ticket: u64, outcome: Wait<'q>) -> Wait<'q> {
        if let Some(i) = st.groups.iter().position(|g| g.members.iter().any(|m| m.ticket == ticket)) {
            let is_runner = matches!(st.groups[i].phase, Phase::Running { by, .. } if by == ticket);
            if !is_runner {
                st.groups[i].members.retain(|m| m.ticket != ticket);
                if st.groups[i].members.is_empty() && !matches!(st.groups[i].phase, Phase::Running { .. }) {
                    st.groups.remove(i);
                }
            }
        }
        outcome
    }

    fn step<'q>(st: &mut State, ticket: u64, queue: &'q Queue) -> Step<'q> {
        let Some(i) = st.groups.iter().position(|g| g.members.iter().any(|m| m.ticket == ticket)) else {
            // Not in the queue (withdrawn, or already served): nothing will ever come for it.
            return Step::Ready(Wait::Cancelled);
        };
        let tag = match st.groups[i].phase {
            Phase::Done { .. } => 0,
            Phase::Running { .. } => 1,
            Phase::Waiting => 2,
        };
        match tag {
            0 => {
                let waited = st.groups[i].members.iter().find(|m| m.ticket == ticket).map(|m| m.at.elapsed()).unwrap_or_default();
                let (result, served) = match &st.groups[i].phase {
                    Phase::Done { result, served } => (result.clone(), *served),
                    _ => unreachable!("checked above"),
                };
                st.groups[i].members.retain(|m| m.ticket != ticket);
                if st.groups[i].members.is_empty() {
                    st.groups.remove(i);
                }
                Step::Ready(Wait::Shared(Shared { result, served, waited }))
            }
            1 => Step::Pending,
            _ => {
                let target = st.groups[i].target.clone();
                let ahead = st.groups[..i].iter().any(|g| g.target == target && !matches!(g.phase, Phase::Done { .. }));
                let running = st.groups.iter().any(|g| g.target == target && matches!(g.phase, Phase::Running { .. }));
                if ahead || running {
                    return Step::Pending;
                }
                st.groups[i].phase = Phase::Running { by: ticket };
                Step::Ready(Wait::Lead(Lead { queue, ticket, finished: false }))
            }
        }
    }
}

enum Step<'q> {
    Ready(Wait<'q>),
    Pending,
}

impl<'q> Step<'q> {
    fn ready(self) -> Option<Wait<'q>> {
        match self {
            Step::Ready(w) => Some(w),
            Step::Pending => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::thread;

    fn req(target: &str, class: &str, who: &str) -> Request {
        Request {
            target: target.into(),
            class: class.into(),
            kind: Kind::Build,
            label: format!("cargo check {class}"),
            owner: Owner { conversation: who.into(), title: format!("chat {who}") },
            coalesce: true,
        }
    }

    fn solo(target: &str, class: &str, who: &str) -> Request {
        Request { coalesce: false, ..req(target, class, who) }
    }

    fn payload(n: u32) -> Result<Payload, String> {
        Ok(Arc::new(n))
    }

    fn lead(w: Wait<'_>) -> Lead<'_> {
        match w {
            Wait::Lead(l) => l,
            _ => panic!("expected the lead"),
        }
    }

    fn never() -> bool {
        false
    }

    #[test]
    fn an_idle_target_runs_the_first_request_at_once() {
        let q = Queue::new();
        let t = q.submit(req("/p", "all", "a"), Instant::now());
        assert!(!t.joined);
        lead(q.wait(t.id, &never, None)).finish(payload(1));
        assert!(!q.busy("/p"));
    }

    #[test]
    fn requests_of_one_target_are_served_in_the_order_they_came() {
        let q = Queue::new();
        let now = Instant::now();
        let a = q.submit(solo("/p", "x", "a"), now);
        let b = q.submit(solo("/p", "x", "b"), now);
        let c = q.submit(solo("/p", "x", "c"), now);
        let la = lead(q.try_take(a.id).unwrap());
        assert!(q.try_take(b.id).is_none() && q.try_take(c.id).is_none(), "nobody runs beside the one in progress");
        la.finish(payload(1));
        assert!(q.try_take(c.id).is_none(), "c does not jump ahead of b");
        let lb = lead(q.try_take(b.id).unwrap());
        assert!(q.try_take(c.id).is_none());
        lb.finish(payload(2));
        lead(q.try_take(c.id).unwrap()).finish(payload(3));
    }

    #[test]
    fn the_one_that_just_finished_goes_to_the_back() {
        let q = Queue::new();
        let now = Instant::now();
        let a1 = q.submit(solo("/p", "x", "a"), now);
        let b = q.submit(solo("/p", "x", "b"), now);
        let c = q.submit(solo("/p", "x", "c"), now);
        lead(q.try_take(a1.id).unwrap()).finish(payload(1));
        // `a` asks again straight away (the polling agent): it queues behind b and c.
        let a2 = q.submit(solo("/p", "x", "a"), now);
        assert!(q.try_take(a2.id).is_none());
        let order: Vec<u64> = {
            let mut served = Vec::new();
            for t in [b.id, c.id, a2.id] {
                let l = lead(q.try_take(t).expect("its turn has come"));
                served.push(t);
                l.finish(payload(0));
            }
            served
        };
        assert_eq!(order, vec![b.id, c.id, a2.id]);
    }

    #[test]
    fn different_targets_do_not_wait_for_each_other() {
        let q = Queue::new();
        let now = Instant::now();
        let a = q.submit(solo("/one", "x", "a"), now);
        let b = q.submit(solo("/two", "x", "b"), now);
        let la = lead(q.try_take(a.id).unwrap());
        let lb = lead(q.try_take(b.id).unwrap());
        la.finish(payload(1));
        lb.finish(payload(2));
    }

    #[test]
    fn requests_for_the_same_class_before_the_start_share_one_run() {
        let q = Queue::new();
        let now = Instant::now();
        let blocker = q.submit(solo("/p", "other", "z"), now);
        let a = q.submit(req("/p", "ws", "a"), now);
        let b = q.submit(req("/p", "ws", "b"), now);
        let c = q.submit(req("/p", "ws", "c"), now);
        assert!(!a.joined && b.joined && c.joined);
        lead(q.try_take(blocker.id).unwrap()).finish(payload(0));
        let la = lead(q.try_take(a.id).unwrap());
        assert_eq!(la.riders(), 2);
        la.finish(payload(42));
        for t in [b.id, c.id] {
            match q.try_take(t).unwrap() {
                Wait::Shared(s) => {
                    assert_eq!(s.served, 3);
                    assert_eq!(*s.result.unwrap().downcast::<u32>().unwrap(), 42);
                }
                _ => panic!("a rider receives the shared result"),
            }
        }
        assert!(!q.busy("/p"), "the group is gone once everyone collected");
    }

    #[test]
    fn a_request_arriving_after_the_start_queues_behind_instead_of_riding() {
        let q = Queue::new();
        let now = Instant::now();
        let a = q.submit(req("/p", "ws", "a"), now);
        let la = lead(q.try_take(a.id).unwrap());
        let late = q.submit(req("/p", "ws", "b"), now);
        assert!(!late.joined, "the tree may have changed since the run began");
        la.finish(payload(1));
        lead(q.try_take(late.id).unwrap()).finish(payload(2));
    }

    #[test]
    fn different_classes_do_not_coalesce() {
        let q = Queue::new();
        let now = Instant::now();
        let a = q.submit(req("/p", "module-a", "a"), now);
        let b = q.submit(req("/p", "module-b", "b"), now);
        assert!(!a.joined && !b.joined);
        let c = q.submit(req("/q", "module-a", "c"), now);
        assert!(!c.joined, "nor do identical classes on another target");
    }

    #[test]
    fn a_class_that_will_not_coalesce_never_joins_or_is_joined() {
        let q = Queue::new();
        let now = Instant::now();
        let a = q.submit(solo("/p", "t", "a"), now);
        let b = q.submit(req("/p", "t", "b"), now);
        let c = q.submit(solo("/p", "t", "c"), now);
        assert!(!a.joined && !b.joined && !c.joined);
    }

    #[test]
    fn a_coalesced_ticket_does_not_starve_the_ones_between() {
        let q = Queue::new();
        let now = Instant::now();
        let a = q.submit(req("/p", "ws", "a"), now);
        let b = q.submit(solo("/p", "other", "b"), now);
        let c = q.submit(req("/p", "ws", "c"), now);
        assert!(c.joined, "c rides a's run, which was always going to happen");
        let la = lead(q.try_take(a.id).unwrap());
        la.finish(payload(1));
        lead(q.try_take(b.id).unwrap()).finish(payload(2));
        assert!(matches!(q.try_take(c.id).unwrap(), Wait::Shared(_)));
    }

    #[test]
    fn a_cancelled_member_leaves_and_the_rest_still_run() {
        let q = Queue::new();
        let now = Instant::now();
        let blocker = q.submit(solo("/p", "x", "z"), now);
        let a = q.submit(req("/p", "ws", "a"), now);
        let b = q.submit(req("/p", "ws", "b"), now);
        let lz = lead(q.try_take(blocker.id).unwrap());
        assert!(q.cancel(a.id));
        assert!(matches!(q.try_take(a.id), Some(Wait::Cancelled)));
        lz.finish(payload(0));
        // b, now alone, is the one to run.
        lead(q.try_take(b.id).unwrap()).finish(payload(5));
    }

    #[test]
    fn cancelling_the_only_waiting_ticket_removes_its_group() {
        let q = Queue::new();
        let a = q.submit(solo("/p", "x", "a"), Instant::now());
        let b = q.submit(solo("/p", "x", "b"), Instant::now());
        let la = lead(q.try_take(a.id).unwrap());
        assert!(q.cancel(b.id));
        assert!(matches!(q.try_take(b.id), Some(Wait::Cancelled)));
        la.finish(payload(1));
        assert!(!q.busy("/p"));
    }

    #[test]
    fn the_run_in_progress_cannot_be_cancelled_from_the_queue() {
        let q = Queue::new();
        let a = q.submit(solo("/p", "x", "a"), Instant::now());
        let la = lead(q.try_take(a.id).unwrap());
        assert!(!q.cancel(a.id));
        assert!(!q.cancel(999), "an unknown ticket is not cancelled either");
        la.finish(payload(1));
    }

    #[test]
    fn a_run_that_never_reported_fails_its_riders_and_frees_the_target() {
        let q = Queue::new();
        let now = Instant::now();
        let a = q.submit(req("/p", "ws", "a"), now);
        let b = q.submit(req("/p", "ws", "b"), now);
        let next = q.submit(solo("/p", "x", "c"), now);
        drop(lead(q.try_take(a.id).unwrap()));
        match q.try_take(b.id).unwrap() {
            Wait::Shared(s) => assert!(s.result.is_err()),
            _ => panic!("the rider is told the run failed"),
        }
        lead(q.try_take(next.id).unwrap()).finish(payload(1));
    }

    #[test]
    fn waiting_blocks_until_the_turn_and_wakes_on_finish() {
        let q = Arc::new(Queue::new());
        let a = q.submit(solo("/p", "x", "a"), Instant::now());
        let b = q.submit(solo("/p", "x", "b"), Instant::now());
        let la = lead(q.wait(a.id, &never, None));
        let waiter = {
            let q = Arc::clone(&q);
            thread::spawn(move || match q.wait(b.id, &never, Some(Duration::from_secs(10))) {
                Wait::Lead(l) => {
                    l.finish(payload(2));
                    true
                }
                _ => false,
            })
        };
        thread::sleep(Duration::from_millis(150));
        la.finish(payload(1));
        assert!(waiter.join().unwrap(), "b ran once a was done");
    }

    #[test]
    fn a_waiting_caller_stops_when_its_cancel_flag_is_set() {
        let q = Arc::new(Queue::new());
        let a = q.submit(solo("/p", "x", "a"), Instant::now());
        let b = q.submit(solo("/p", "x", "b"), Instant::now());
        let la = lead(q.wait(a.id, &never, None));
        let flag = Arc::new(AtomicBool::new(false));
        let waiter = {
            let (q, flag) = (Arc::clone(&q), Arc::clone(&flag));
            thread::spawn(move || matches!(q.wait(b.id, &|| flag.load(Ordering::SeqCst), None), Wait::Cancelled))
        };
        thread::sleep(Duration::from_millis(120));
        flag.store(true, Ordering::SeqCst);
        assert!(waiter.join().unwrap());
        la.finish(payload(1));
        assert!(!q.busy("/p"), "the cancelled ticket left nothing behind");
    }

    #[test]
    fn a_wait_has_a_deadline() {
        let q = Queue::new();
        let a = q.submit(solo("/p", "x", "a"), Instant::now());
        let b = q.submit(solo("/p", "x", "b"), Instant::now());
        let la = lead(q.wait(a.id, &never, None));
        assert!(matches!(q.wait(b.id, &never, Some(Duration::from_millis(150))), Wait::TimedOut));
        la.finish(payload(1));
        assert!(!q.busy("/p"));
    }

    #[test]
    fn many_threads_are_served_exactly_once_each_in_order() {
        let q = Arc::new(Queue::new());
        let order = Arc::new(Mutex::new(Vec::new()));
        let tickets: Vec<Ticket> = (0..6).map(|i| q.submit(solo("/p", "x", &format!("c{i}")), Instant::now())).collect();
        let handles: Vec<_> = tickets
            .iter()
            .map(|t| {
                let (q, order, id) = (Arc::clone(&q), Arc::clone(&order), t.id);
                thread::spawn(move || {
                    if let Wait::Lead(l) = q.wait(id, &never, Some(Duration::from_secs(20))) {
                        order.lock().unwrap().push(id);
                        thread::sleep(Duration::from_millis(10));
                        l.finish(payload(0));
                    }
                })
            })
            .collect();
        handles.into_iter().for_each(|h| h.join().unwrap());
        let expected: Vec<u64> = tickets.iter().map(|t| t.id).collect();
        assert_eq!(*order.lock().unwrap(), expected);
    }

    #[test]
    fn the_snapshot_numbers_the_line_and_names_who_is_ahead() {
        let q = Queue::new();
        let now = Instant::now();
        let a = q.submit(solo("/p", "x", "a"), now);
        let b = q.submit(solo("/p", "y", "b"), now);
        let c = q.submit(req("/p", "y2", "c"), now);
        let rider = q.submit(req("/p", "y2", "d"), now);
        let _la = lead(q.try_take(a.id).unwrap());
        let rows = q.snapshot();
        let row = |t: u64| rows.iter().find(|r| r.ticket == t).unwrap().clone();
        assert_eq!((row(a.id).state, row(a.id).position), (RowState::Running, 1));
        assert_eq!((row(b.id).state, row(b.id).position), (RowState::Waiting, 2));
        assert_eq!(row(b.id).behind.unwrap().owner.title, "chat a");
        assert_eq!((row(c.id).state, row(c.id).position), (RowState::Waiting, 3));
        assert_eq!(row(c.id).behind.unwrap().owner.title, "chat b");
        assert_eq!((row(rider.id).state, row(rider.id).position), (RowState::Riding, 3));
    }

    #[test]
    fn the_observer_hears_every_change() {
        let q = Queue::new();
        let hits = Arc::new(Mutex::new(0u32));
        let h = Arc::clone(&hits);
        q.set_observer(move || *h.lock().unwrap() += 1);
        let a = q.submit(solo("/p", "x", "a"), Instant::now());
        lead(q.try_take(a.id).unwrap()).finish(payload(1));
        assert!(*hits.lock().unwrap() >= 3, "submit, start and finish");
    }
}
