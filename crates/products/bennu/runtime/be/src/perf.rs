//! Opt-in phase timing for the interactive paths (completion, ghost text, go-to).
//!
//! Complements the per-request line `arbor-ipc` prints (`wait` / `run` / `write`): that says a request was
//! slow, this says which PHASE of the handler was. Both are switched on by the same variable —
//! `ARBOR_BE_PERF=<ms>` — and both write to **stderr** (stdout is the protocol channel). With it unset a
//! [`phase`] is one cached atomic load and a `None`.
//!
//! ```text
//! let _t = perf::phase("completion", "provider");
//! ```
//!
//! prints `[perf]   completion/provider 12.4ms (Foo.java)` when the guard drops, for any phase of at least
//! one millisecond. Nest guards in scopes to get a breakdown; the request line gives the total.

use std::sync::OnceLock;
use std::time::Instant;

fn enabled() -> bool {
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| {
        std::env::var("ARBOR_BE_PERF").ok().and_then(|v| v.trim().parse::<u64>().ok()).is_some_and(|ms| ms > 0)
    })
}

/// A running phase; reports when dropped.
pub(crate) struct Phase {
    started: Instant,
    domain: &'static str,
    name: &'static str,
    detail: Option<String>,
}

/// Start timing `domain/name`. `None` (and free) unless timing is switched on.
pub(crate) fn phase(domain: &'static str, name: &'static str) -> Option<Phase> {
    enabled().then(|| Phase { started: Instant::now(), domain, name, detail: None })
}

/// Like [`phase`], with a short note shown after the timing (the file, a count…). The note is built
/// only when timing is on.
pub(crate) fn phase_with(domain: &'static str, name: &'static str, detail: impl FnOnce() -> String) -> Option<Phase> {
    enabled().then(|| Phase { started: Instant::now(), domain, name, detail: Some(detail()) })
}

/// One line of facts (counts, cache hits) rather than a duration; the text is built only when timing is on.
pub(crate) fn note(domain: &'static str, name: &'static str, text: impl FnOnce() -> String) {
    if enabled() {
        eprintln!("[perf]   {domain}/{name} {}", text());
    }
}

impl Drop for Phase {
    fn drop(&mut self) {
        let ms = self.started.elapsed().as_secs_f64() * 1000.0;
        if ms < 1.0 {
            return;
        }
        match &self.detail {
            Some(d) => eprintln!("[perf]   {}/{} {ms:.1}ms ({d})", self.domain, self.name),
            None => eprintln!("[perf]   {}/{} {ms:.1}ms", self.domain, self.name),
        }
    }
}
