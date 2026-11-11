//! A debug session as a [`LiveTarget`].
//!
//! `crate::debug_backend::DebugBackend` is the debugger's *operation* trait (resume, step, variables, watch…)
//! and stays exactly that. This wrapper adds the other half the listing needs — who it is and what it can do
//! — in the vocabulary a game link also speaks, so `bevy_game_status` and Imber's `live_targets` can put a JVM
//! and a game in one answer.
//!
//! The capabilities are the debugger-only ones (`Pause`, `Stack`, `Eval`, `Output`): the rule of the
//! vocabulary is that only here may a thing be paused.

use std::sync::Arc;

use bennu_link::prelude::{Capability, LiveTarget, TargetKind};

use crate::debug_backend::DebugBackend;

pub(crate) struct DebuggerTarget {
    backend: Arc<dyn DebugBackend>,
}

impl DebuggerTarget {
    pub(crate) fn new(backend: Arc<dyn DebugBackend>) -> Self {
        Self { backend }
    }

    /// Every live debug session, as `(run id, target)`.
    pub(crate) fn all() -> Vec<(String, DebuggerTarget)> {
        let mut sessions: Vec<(String, DebuggerTarget)> = crate::debug_backend::live()
            .into_iter()
            .filter_map(|(id, _)| crate::debug_backend::get(&id).ok().map(|b| (id, DebuggerTarget::new(b))))
            .collect();
        sessions.sort_by(|a, b| a.0.cmp(&b.0));
        sessions
    }
}

impl LiveTarget for DebuggerTarget {
    fn kind(&self) -> TargetKind {
        TargetKind::Debugger
    }

    /// `jdwp` or `dap`.
    fn protocol(&self) -> &str {
        self.backend.kind()
    }

    fn label(&self) -> String {
        self.backend.describe()
    }

    fn capabilities(&self) -> Vec<Capability> {
        vec![Capability::Pause, Capability::Stack, Capability::Eval, Capability::Output]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_debugger_declares_only_what_a_debugger_has() {
        let declared = [Capability::Pause, Capability::Stack, Capability::Eval, Capability::Output];
        assert!(declared.iter().all(|c| c.belongs_to(TargetKind::Debugger)));
    }
}
