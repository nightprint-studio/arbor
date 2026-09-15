//! What a live debug session must be able to do, whichever protocol it speaks.
//!
//! ## Why a trait, here, now
//!
//! Bennu debugs a JVM over JDWP and a native binary over DAP, and the two protocols have almost
//! nothing in common: JDWP is a binary protocol spoken by the thing being debugged, DAP is JSON
//! spoken by an adapter process that drives a real debugger. What they have in common is the eight
//! things a debugger *panel* asks for — resume, step, detach, mute, the variables of a frame, what is
//! inside a value, a watch, and the breakpoints to install.
//!
//! Which is the case CLAUDE.md calls a justified trait: two implementations that genuinely converge,
//! and a seam that already exists whether or not it is named. The ten `bennu_debug_*` handlers were
//! already written against exactly these operations — they just happened to be inherent methods on the
//! JDWP session. Naming the seam means the frontend, the IPC surface and the handlers do not know
//! there are two protocols, which is the whole point: **which debugger is in use is not a question the
//! panel should be able to ask.**
//!
//! ## What is deliberately not on it
//!
//! Nothing about *starting* a session. The two are started by completely different means — JDWP by
//! listening on a socket and waiting for a JVM launched with an agent argument, DAP by spawning an
//! adapter and handing it a binary — and a common `start` would be a signature that fits neither.
//! Each protocol's module owns its own launch, and registers what it produced.

use std::sync::Arc;

use bennu_proto::prelude::DebugValue;

/// A step's direction, in the vocabulary Bennu's own debug contract uses.
///
/// Its own type rather than each protocol's: JDWP counts a depth (`INTO` / `OVER` / `OUT` as
/// integers), DAP names three commands (`stepIn` / `next` / `stepOut`), and the wire word the
/// frontend sends is neither. Converting once at the seam beats converting at each call site.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    Over,
    Into,
    Out,
}

impl Step {
    /// Parse the frontend's word. Anything unrecognised is `Over`, which is the safe default: a
    /// mistyped depth stepping one line is recoverable, and refusing the request leaves a user
    /// pressing a key that does nothing.
    pub fn parse(depth: &str) -> Step {
        match depth {
            "into" | "in" => Step::Into,
            "out" => Step::Out,
            _ => Step::Over,
        }
    }
}

/// One live session, whichever protocol is behind it.
///
/// Every method returns `Result<_, String>` because the error crosses the RPC seam as a `Display` —
/// see CLAUDE.md on the error contract — and because a debugger's failures are things a user reads:
/// "there is no variable called `foo`", not a code.
pub trait DebugBackend: Send + Sync {
    /// Which protocol this is: `jdwp` or `dap`. For the status the frontend shows, and for the one or
    /// two places a panel legitimately differs (a JDWP frame has a class, a DAP frame has a name).
    fn kind(&self) -> &'static str;

    /// What is being debugged, for the status line — a JVM version, or the adapter's name.
    fn describe(&self) -> String;

    /// The project root this session is debugging.
    ///
    /// On the trait because editing the breakpoints pushes the new set to **every live session of that
    /// root** — a push that silently reached one of two would be worse than one that reached neither.
    fn root(&self) -> String;

    /// Let the program run on.
    fn resume(&self) -> Result<(), String>;

    /// One step, at line granularity.
    fn step(&self, step: Step) -> Result<(), String>;

    /// Mute or unmute the breakpoints: still set, still listed, not installed.
    ///
    /// Both protocols can honour this, by different means — JDWP clears its event requests, DAP sends
    /// an empty breakpoint list per file — which is why it is on the trait rather than being a JDWP
    /// peculiarity the panel has to know about.
    fn set_muted(&self, muted: bool) -> Result<(), String>;

    /// Let go: the program keeps running, with no debugger attached.
    ///
    /// Deliberately not a kill on either protocol. Stopping the *program* is the Run console's Stop.
    /// DAP's `disconnect` carries `terminateDebuggee`, and this passes `false`.
    fn detach(&self) -> Result<(), String>;

    /// The variables in scope at a frame of the stopped thread.
    fn variables(&self, frame: usize) -> Result<Vec<DebugValue>, String>;

    /// What is inside a value — fields, or elements. `handle` is a [`DebugValue::object`].
    fn expand(&self, handle: &str) -> Result<Vec<DebugValue>, String>;

    /// Evaluate a watch against a frame.
    fn watch(&self, frame: usize, expression: &str) -> Result<DebugValue, String>;

    /// Install a replacement set of breakpoints, after the user edited them.
    ///
    /// Takes the whole configured set rather than a delta: what the panel edits is the set, a delta
    /// would have to be computed from a copy of the previous one, and two copies of the same set is
    /// how they start disagreeing.
    fn reinstall(&self, config: &bennu_proto::prelude::DebugConfig) -> Result<(), String>;
}

/// Every live session, by id. The id is the **run id**, so the console tab and the debugger are the
/// same thing to anything that has to correlate them.
pub fn registry() -> &'static std::sync::Mutex<std::collections::HashMap<String, Arc<dyn DebugBackend>>>
{
    static REG: std::sync::OnceLock<
        std::sync::Mutex<std::collections::HashMap<String, Arc<dyn DebugBackend>>>,
    > = std::sync::OnceLock::new();
    REG.get_or_init(|| std::sync::Mutex::new(std::collections::HashMap::new()))
}

/// Register a session under `id`, replacing whatever was there.
pub fn insert(id: &str, backend: Arc<dyn DebugBackend>) {
    registry().lock().unwrap_or_else(|p| p.into_inner()).insert(id.to_string(), backend);
}

/// Forget a session — its process ended.
pub fn remove(id: &str) {
    registry().lock().unwrap_or_else(|p| p.into_inner()).remove(id);
}

/// The session with this id, or the message the handlers report.
pub fn get(id: &str) -> Result<Arc<dyn DebugBackend>, String> {
    registry()
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .get(id)
        .cloned()
        .ok_or_else(|| "that debug session is no longer live".to_string())
}

/// Every live session's id and kind — what tells a caller whether anything is being debugged at all.
pub fn live() -> Vec<(String, &'static str)> {
    registry()
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .iter()
        .map(|(id, backend)| (id.clone(), backend.kind()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_step_word_from_the_frontend_maps_onto_the_three_directions() {
        assert_eq!(Step::parse("into"), Step::Into);
        assert_eq!(Step::parse("in"), Step::Into);
        assert_eq!(Step::parse("out"), Step::Out);
        assert_eq!(Step::parse("over"), Step::Over);
        // An unrecognised depth steps one line rather than refusing: a key that does nothing is a
        // worse answer than a key that does the ordinary thing.
        assert_eq!(Step::parse("sideways"), Step::Over);
        assert_eq!(Step::parse(""), Step::Over);
    }

    /// A stub, to pin the registry's behaviour without a debugger behind it.
    struct Stub(&'static str);

    impl DebugBackend for Stub {
        fn kind(&self) -> &'static str {
            self.0
        }
        fn describe(&self) -> String {
            self.0.to_string()
        }
        fn root(&self) -> String {
            "/p".to_string()
        }
        fn resume(&self) -> Result<(), String> {
            Ok(())
        }
        fn step(&self, _: Step) -> Result<(), String> {
            Ok(())
        }
        fn set_muted(&self, _: bool) -> Result<(), String> {
            Ok(())
        }
        fn detach(&self) -> Result<(), String> {
            Ok(())
        }
        fn variables(&self, _: usize) -> Result<Vec<DebugValue>, String> {
            Ok(Vec::new())
        }
        fn expand(&self, _: &str) -> Result<Vec<DebugValue>, String> {
            Ok(Vec::new())
        }
        fn watch(&self, _: usize, _: &str) -> Result<DebugValue, String> {
            Err("no".into())
        }
        fn reinstall(&self, _: &bennu_proto::prelude::DebugConfig) -> Result<(), String> {
            Ok(())
        }
    }

    #[test]
    fn a_session_is_found_by_its_id_and_gone_once_removed() {
        // A distinct id, so this does not depend on what another test registered.
        let id = "registry-test-session";
        insert(id, Arc::new(Stub("dap")));

        // `expect` would need `Debug` on the trait object, which a live session is not: bind the
        // outcome instead.
        let Ok(found) = get(id) else { panic!("a registered session must be found") };
        assert_eq!(found.kind(), "dap");
        assert!(live().iter().any(|(i, k)| i == id && *k == "dap"));

        remove(id);
        let Err(err) = get(id) else { panic!("a removed session must be gone") };
        // The message a handler reports, and it has to read as an explanation rather than a code.
        assert!(err.contains("no longer live"), "{err}");
    }

    #[test]
    fn registering_the_same_id_replaces_rather_than_duplicating() {
        let id = "registry-replace-test";
        insert(id, Arc::new(Stub("jdwp")));
        insert(id, Arc::new(Stub("dap")));
        let Ok(found) = get(id) else { panic!("registered") };
        assert_eq!(found.kind(), "dap", "a relaunch under one run id is one session");
        assert_eq!(live().iter().filter(|(i, _)| i == id).count(), 1);
        remove(id);
    }

    #[test]
    fn asking_for_a_session_that_never_existed_is_an_error_and_not_a_panic() {
        assert!(get("no-such-session").is_err());
    }
}
