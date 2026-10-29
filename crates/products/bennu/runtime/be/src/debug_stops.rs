//! More than one stop at a time, and what happens around a step.
//!
//! A child of `debug`, because all of it works on the session's private state.
//!
//! ## Stops that wait their turn
//!
//! The session shows **one** stop (`Mutable::paused`). A second thread can still reach a breakpoint while the
//! first is being looked at — at the same instant, or, with a breakpoint that suspends only its own thread, at
//! any time. That stop is not lost and does not replace the one on screen: it is *parked*, marked `residual`,
//! and becomes the current one when the current one is resumed or when it is picked (`select_stop`). Before this
//! the second stop silently overwrote the first, and the program stayed frozen by the suspension nobody had
//! released.
//!
//! The VM keeps a count: every event that suspends every thread adds one, every `resume_vm` removes one. So a
//! stop is let go with the call that matches how it suspended (`release`), and `suspends` follows the count.
//! While stops are queued behind each other under that policy the program is still suspended as a whole and a
//! step cannot run; resuming moves on to the next one.
//!
//! Removing a breakpoint lets go of the threads parked on it — never the one being looked at.
//!
//! ## Around a step
//!
//! * a step *into* that lands in a record's accessor steps out of it and in again, as it already does for
//!   libraries: nobody wants to stop in `id() { return id; }`;
//! * a step asked to read the return value watches the method exits of its thread and reports the last one.

use bennu_jdwp::prelude::{method_bytecodes, request_method_exit};
use bennu_proto::prelude::{ReturnedValue, StoppedThread};

use super::*;

/// What a stop carries from the event that caused it.
pub(super) struct StopCtx {
    pub policy: SuspendPolicy,
    pub bp: Option<(String, u32)>,
    pub condition: String,
    pub condition_values: String,
}

impl StopCtx {
    pub(super) fn of(policy: SuspendPolicy) -> Self {
        StopCtx { policy, bp: None, condition: String::new(), condition_values: String::new() }
    }
}

/// The answer to "does this hit stop the program".
pub(super) struct Verdict {
    pub stop: bool,
    pub bp: Option<(String, u32)>,
    pub condition: String,
    pub condition_values: String,
}

impl Verdict {
    pub(super) fn skip() -> Self {
        Verdict { stop: false, bp: None, condition: String::new(), condition_values: String::new() }
    }

    pub(super) fn into_ctx(self, policy: SuspendPolicy) -> StopCtx {
        StopCtx { policy, bp: self.bp, condition: self.condition, condition_values: self.condition_values }
    }
}

/// A step watching the methods that return on its thread.
pub(super) struct ReturnProbe {
    request: i32,
    thread: Id,
    last: Option<(Location, Value)>,
}

/// How far up the inheritance chain a class is followed looking for `java.lang.Record`.
const MAX_SUPERS: usize = 20;
/// Parked stops are bounded: a hot breakpoint in a thread pool must not queue forever.
const MAX_PARKED: usize = 32;
/// Requests of removed breakpoints remembered.
const MAX_REMOVED: usize = 64;

impl Session {
    // ── letting go ─────────────────────────────────────────────────────────────

    /// An event that suspended every thread was received: the VM's count went up.
    pub(super) fn note_suspended(&self, policy: SuspendPolicy) {
        if policy == SuspendPolicy::All {
            self.state.lock().unwrap_or_else(|p| p.into_inner()).suspends += 1;
        }
    }

    /// Undo what an event did to the program, the way it was done.
    pub(super) fn release(&self, thread: Id, policy: SuspendPolicy) -> Result<(), String> {
        match policy {
            SuspendPolicy::All => {
                let mut state = self.state.lock().unwrap_or_else(|p| p.into_inner());
                state.suspends = state.suspends.saturating_sub(1);
                drop(state);
                resume_vm(&self.client).map_err(|e| e.to_string())
            }
            SuspendPolicy::EventThread => resume_thread(&self.client, thread).map_err(|e| e.to_string()),
            SuspendPolicy::None => Ok(()),
        }
    }

    /// Let the current stop go. `next` moves on to a parked one when there is one (a resume); a step does not,
    /// because the thread stepping is the one the user chose.
    pub(super) fn advance(&self, next: bool) -> Result<(), String> {
        let (was, promoted) = {
            let mut state = self.state.lock().unwrap_or_else(|p| p.into_inner());
            let Some(was) = state.paused.take() else { return Ok(()) };
            if next && !state.parked.is_empty() {
                state.paused = Some(state.parked.remove(0));
            }
            refresh_also_stopped(&mut state);
            (was, state.paused.as_ref().map(|p| (p.thread, p.pause.clone())))
        };
        self.release(was.thread, was.policy)?;
        match promoted {
            Some((thread, pause)) => {
                self.announce(&pause);
                self.notify_assistant(thread, &pause.reason, &[]);
            }
            None => emit_status(&self.sink, &self.id, "running", "", ""),
        }
        Ok(())
    }

    /// Step the current stop's thread.
    pub(super) fn step_current(&self, depth: StepDepth) -> Result<(), String> {
        let (thread, policy, queued) = {
            let state = self.state.lock().unwrap_or_else(|p| p.into_inner());
            let Some(p) = state.paused.as_ref() else { return Err("the program is not stopped".to_string()) };
            (p.thread, p.policy, state.parked.len())
        };
        if policy == SuspendPolicy::All && queued > 0 {
            return Err(format!(
                "{queued} more stop(s) are waiting behind this one and the whole program is suspended, so it cannot step: resume to move on to the next one, or give the breakpoint \"suspend only its thread\""
            ));
        }
        self.clear_step();
        let want_return = std::mem::take(&mut self.state.lock().unwrap_or_else(|p| p.into_inner()).arm_return);
        let request = request_step(&self.client, thread, depth, &self.exclude_refs(), policy).map_err(|e| e.to_string())?;
        let probe = want_return.then(|| request_method_exit(&self.client, thread, &self.exclude_refs()));
        {
            let mut state = self.state.lock().unwrap_or_else(|p| p.into_inner());
            state.step = Some(request);
            state.step_depth = Some(depth);
            state.probe = None;
            state.probe_note.clear();
            match probe {
                Some(Ok(request)) => state.probe = Some(ReturnProbe { request, thread, last: None }),
                Some(Err(why)) => state.probe_note = format!("the VM would not report return values ({why})"),
                None => {}
            }
        }
        self.advance(false)
    }

    /// The next step also reads what the method it leaves returned.
    pub(super) fn arm_return_value(&self) {
        self.state.lock().unwrap_or_else(|p| p.into_inner()).arm_return = true;
    }

    // ── what a step does on the way ────────────────────────────────────────────

    /// Whether the step that just landed at `at` has arrived nowhere worth stopping, in which case it was
    /// sent on and the caller returns.
    pub(super) fn continue_past(&self, thread: Id, at: Location, policy: SuspendPolicy) -> bool {
        // Back from a getter: from the caller, step in again.
        let coming_back = std::mem::take(&mut self.state.lock().unwrap_or_else(|p| p.into_inner()).pending_into);
        if coming_back {
            return self.step_on(thread, policy, StepDepth::Into, false);
        }
        if self.is_opaque(at) {
            return self.step_on(thread, policy, StepDepth::Into, false);
        }
        let into = self.state.lock().unwrap_or_else(|p| p.into_inner()).step_depth == Some(StepDepth::Into);
        if into && self.is_record_accessor(at) {
            return self.step_on(thread, policy, StepDepth::Out, true);
        }
        false
    }

    /// Send a step that arrived nowhere on its way. `false` when the budget is spent: the caller then stops where
    /// it is, because a debugger that never comes back is worse than one that stops somewhere unhelpful.
    fn step_on(&self, thread: Id, policy: SuspendPolicy, depth: StepDepth, then_into: bool) -> bool {
        {
            let mut state = self.state.lock().unwrap_or_else(|p| p.into_inner());
            if state.skips >= MAX_STEP_SKIPS {
                state.skips = 0;
                return false;
            }
            state.skips += 1;
        }
        let Ok(request) = request_step(&self.client, thread, depth, &self.exclude_refs(), policy) else { return false };
        {
            let mut state = self.state.lock().unwrap_or_else(|p| p.into_inner());
            state.step = Some(request);
            state.pending_into = then_into;
        }
        self.release(thread, policy).is_ok()
    }

    /// Whether `at` is the first line of a record's accessor: `aload_0; getfield; return`, nothing else. Read
    /// once per method.
    fn is_record_accessor(&self, at: Location) -> bool {
        let key = (at.class, at.method);
        if let Some(known) = self.cache.lock().unwrap_or_else(|p| p.into_inner()).accessors.get(&key) {
            return *known;
        }
        let takes_nothing = self.methods_of(at.class).iter().find(|m| m.id == at.method).is_some_and(|m| m.signature.starts_with("()"));
        let accessor = takes_nothing
            && self.extends_record(at.class)
            && method_bytecodes(&self.client, at.class, at.method).is_ok_and(|code| is_getter_code(&code));
        self.cache.lock().unwrap_or_else(|p| p.into_inner()).accessors.insert(key, accessor);
        accessor
    }

    fn extends_record(&self, class: Id) -> bool {
        let mut current = class;
        for _ in 0..MAX_SUPERS {
            current = superclass(&self.client, current).unwrap_or(0);
            if current == 0 {
                return false;
            }
            if self.class_name_of(current) == "java.lang.Record" {
                return true;
            }
        }
        false
    }

    // ── return values ──────────────────────────────────────────────────────────

    /// A method returned on the thread a step is watching.
    pub(super) fn note_return(&self, request: i32, thread: Id, at: Location, value: Value) {
        let mut state = self.state.lock().unwrap_or_else(|p| p.into_inner());
        if let Some(probe) = state.probe.as_mut().filter(|p| p.request == request && p.thread == thread) {
            probe.last = Some((at, value));
        }
    }

    /// Stop watching and say what the last method returned — or why nothing came back.
    pub(super) fn finish_return_probe(&self) -> Option<ReturnedValue> {
        let (probe, note) = {
            let mut state = self.state.lock().unwrap_or_else(|p| p.into_inner());
            (state.probe.take(), std::mem::take(&mut state.probe_note))
        };
        let nothing = |why: String| Some(ReturnedValue { method: String::new(), type_name: String::new(), value: why });
        let Some(probe) = probe else { return if note.is_empty() { None } else { nothing(note) } };
        let _ = clear_event(&self.client, kind::METHOD_EXIT_WITH_RETURN_VALUE, probe.request);
        let Some((at, value)) = probe.last else { return nothing("no method returned during the step".to_string()) };
        let method = format!(
            "{}.{}",
            self.class_name_of(at.class),
            self.methods_of(at.class).iter().find(|m| m.id == at.method).map(|m| m.name.as_str()).unwrap_or("?")
        );
        if value == Value::Void {
            return Some(ReturnedValue { method, type_name: "void".to_string(), value: "(returns nothing)".to_string() });
        }
        let row = crate::debug_value::returned_row(self, value);
        Some(ReturnedValue { method, type_name: row.type_name, value: row.value })
    }

    // ── several stops ──────────────────────────────────────────────────────────

    /// Show a stop: tell the panel, and say the program is paused.
    pub(super) fn announce(&self, pause: &DebugPause) {
        self.sink.emit(EVT_DEBUG_PAUSED, serde_json::to_value(pause).unwrap_or(json!({})));
        emit_status(&self.sink, &self.id, "paused", "", "");
    }

    /// Put a new stop in its place: the current one if nothing is being looked at, else parked behind it.
    /// Returns the stop to announce — `None` when it was parked, in which case the one on screen is
    /// announced again with the list of the others updated.
    pub(super) fn place_stop(&self, mut stop: Paused) -> Placed {
        let mut state = self.state.lock().unwrap_or_else(|p| p.into_inner());
        let busy = state.paused.is_some();
        stop.pause.detail.origin = if stop.pause.reason == "step" { "step" } else if busy { "residual" } else { "fresh" }.to_string();
        if !busy {
            let pause = stop.pause.clone();
            state.paused = Some(stop);
            refresh_also_stopped(&mut state);
            let shown = state.paused.as_ref().map(|p| p.pause.clone()).unwrap_or(pause);
            return Placed::Current(shown);
        }
        if state.parked.len() >= MAX_PARKED {
            return Placed::Dropped(stop);
        }
        state.parked.push(stop);
        refresh_also_stopped(&mut state);
        Placed::Parked(state.paused.as_ref().map(|p| p.pause.clone()))
    }

    /// Make another stopped thread the one being looked at (and stepped, and resumed). `which` is a thread id or
    /// a name, or a part of one.
    pub(super) fn select_stop(&self, which: &str) -> Result<DebugPause, String> {
        let chosen = {
            let mut state = self.state.lock().unwrap_or_else(|p| p.into_inner());
            let Some(current) = state.paused.as_ref().map(|p| (p.pause.thread.clone(), p.pause.thread_name.clone())) else {
                return Err("the program is not stopped".to_string());
            };
            let others: Vec<(String, String)> = state.parked.iter().map(|p| (p.pause.thread.clone(), p.pause.thread_name.clone())).collect();
            if pick(which, std::slice::from_ref(&current)).is_ok() {
                return state.paused.as_ref().map(|p| p.pause.clone()).ok_or_else(|| "the program is not stopped".to_string());
            }
            let index = pick(which, &others).map_err(|why| {
                let listed: Vec<String> = std::iter::once(&current).chain(others.iter()).map(|(id, name)| format!("`{name}` ({id})")).collect();
                format!("{why}; the stopped threads are {}", listed.join(", "))
            })?;
            let next = state.parked.remove(index);
            if let Some(previous) = state.paused.replace(next) {
                state.parked.push(previous);
            }
            refresh_also_stopped(&mut state);
            state.paused.as_ref().map(|p| p.pause.clone())
        };
        let pause = chosen.ok_or_else(|| "the program is not stopped".to_string())?;
        self.announce(&pause);
        Ok(pause)
    }

    /// A breakpoint set was replaced: let go of the threads parked on a breakpoint that is gone, and remember
    /// its requests so an event still in flight for it is let go too.
    pub(super) fn forget_removed(&self, old: &[Bp]) {
        let doomed: Vec<(String, u32)> = {
            let state = self.state.lock().unwrap_or_else(|p| p.into_inner());
            let wanted: Vec<(String, u32)> =
                state.breakpoints.iter().filter(|b| b.at.enabled).map(|b| (normalize(&b.at.file), b.at.line)).collect();
            old.iter()
                .map(|b| (normalize(&b.at.file), b.at.line))
                .filter(|key| !wanted.contains(key))
                .collect()
        };
        if doomed.is_empty() {
            return;
        }
        let gone: Vec<Paused> = {
            let mut state = self.state.lock().unwrap_or_else(|p| p.into_inner());
            for bp in old.iter().filter(|b| doomed.contains(&(normalize(&b.at.file), b.at.line))) {
                state.removed.extend(bp.requests.iter().filter(|(k, _)| *k == kind::BREAKPOINT).map(|(_, r)| *r));
            }
            let excess = state.removed.len().saturating_sub(MAX_REMOVED);
            state.removed.drain(..excess);
            let (gone, keep): (Vec<Paused>, Vec<Paused>) = std::mem::take(&mut state.parked)
                .into_iter()
                .partition(|p| p.bp.as_ref().is_some_and(|(file, line)| doomed.contains(&(normalize(file), *line))));
            state.parked = keep;
            state.released += gone.len() as u32;
            refresh_also_stopped(&mut state);
            gone
        };
        for stop in &gone {
            let _ = self.release(stop.thread, stop.policy);
        }
        if !gone.is_empty() {
            if let Some(pause) = self.paused_pause() {
                self.sink.emit(EVT_DEBUG_PAUSED, serde_json::to_value(pause).unwrap_or(json!({})));
            }
        }
    }

    /// The threads let go by removing breakpoints since the last time this was asked.
    pub(super) fn take_released(&self) -> u32 {
        std::mem::take(&mut self.state.lock().unwrap_or_else(|p| p.into_inner()).released)
    }

    pub(super) fn paused_pause(&self) -> Option<DebugPause> {
        self.state.lock().unwrap_or_else(|p| p.into_inner()).paused.as_ref().map(|p| p.pause.clone())
    }

    /// Whether a hit for `request` belongs to a breakpoint that was removed.
    pub(super) fn is_removed(&self, request: i32) -> bool {
        self.state.lock().unwrap_or_else(|p| p.into_inner()).removed.contains(&request)
    }
}

/// Where a new stop went.
pub(super) enum Placed {
    /// Nothing was being looked at: this is the stop now.
    Current(DebugPause),
    /// Another stop is current; this waits. Holds the current one, with the list of the others updated.
    Parked(Option<DebugPause>),
    /// The queue is full; the thread is let go by the caller.
    Dropped(Paused),
}

/// Rebuild the list of the other stopped threads carried by the current stop.
fn refresh_also_stopped(state: &mut Mutable) {
    let others: Vec<StoppedThread> = state
        .parked
        .iter()
        .map(|p| StoppedThread {
            thread: p.pause.thread.clone(),
            thread_name: p.pause.thread_name.clone(),
            reason: p.pause.reason.clone(),
            location: p
                .pause
                .frames
                .first()
                .map(|f| match (&f.file, f.line) {
                    (Some(file), Some(line)) => format!("{}:{line}", file.rsplit(['/', '\\']).next().unwrap_or(file)),
                    _ => format!("{}.{}", f.class, f.method),
                })
                .unwrap_or_default(),
        })
        .collect();
    if let Some(current) = state.paused.as_mut() {
        current.pause.detail.also_stopped = others;
    }
}

/// `aload_0; getfield #n; <t>return`: a method that does nothing but hand back a field of `this`.
pub(super) fn is_getter_code(code: &[u8]) -> bool {
    matches!(code, [0x2a, 0xb4, _, _, 0xac..=0xb0])
}

/// Which of `(id, name)` is meant by `which`: the id, else the name, else a name that contains it — if only one
/// does.
pub(super) fn pick(which: &str, threads: &[(String, String)]) -> Result<usize, String> {
    let wanted = which.trim();
    if wanted.is_empty() {
        return Err("name the thread: its id or its name".to_string());
    }
    if let Some(i) = threads.iter().position(|(id, _)| id == wanted) {
        return Ok(i);
    }
    if let Some(i) = threads.iter().position(|(_, name)| name.eq_ignore_ascii_case(wanted)) {
        return Ok(i);
    }
    let lower = wanted.to_lowercase();
    let partial: Vec<usize> = threads.iter().enumerate().filter(|(_, (_, name))| name.to_lowercase().contains(&lower)).map(|(i, _)| i).collect();
    match partial.as_slice() {
        [one] => Ok(*one),
        [] => Err(format!("no stopped thread is called `{wanted}`")),
        _ => Err(format!("`{wanted}` matches several stopped threads")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn threads(list: &[(&str, &str)]) -> Vec<(String, String)> {
        list.iter().map(|(a, b)| (a.to_string(), b.to_string())).collect()
    }

    #[test]
    fn only_a_field_getter_is_skipped() {
        assert!(is_getter_code(&[0x2a, 0xb4, 0x00, 0x07, 0xb0]), "areturn");
        assert!(is_getter_code(&[0x2a, 0xb4, 0x00, 0x02, 0xac]), "ireturn");
        assert!(!is_getter_code(&[0x2a, 0xb4, 0x00, 0x07, 0xb1]), "a void return is not a getter");
        assert!(!is_getter_code(&[0x2a, 0xb4, 0x00, 0x07, 0x04, 0x60, 0xac]), "it adds something: it has logic");
        assert!(!is_getter_code(&[0xb2, 0x00, 0x07, 0xb0]), "a static field is not a field of this");
        assert!(!is_getter_code(&[]));
    }

    #[test]
    fn a_thread_is_found_by_id_then_name_then_part_of_a_name() {
        let t = threads(&[("11", "http-nio-8080-exec-1"), ("12", "http-nio-8080-exec-2"), ("13", "scheduler")]);
        assert_eq!(pick("12", &t), Ok(1));
        assert_eq!(pick("SCHEDULER", &t), Ok(2));
        assert_eq!(pick("sched", &t), Ok(2));
        assert!(pick("exec", &t).unwrap_err().contains("several"));
        assert!(pick("main", &t).unwrap_err().contains("no stopped thread"));
        assert!(pick("  ", &t).is_err());
    }
}
