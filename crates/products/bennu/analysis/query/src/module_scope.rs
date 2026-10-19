//! The module a file is being resolved **in**, for the resolver to read.
//!
//! Two modules of one reactor can declare the same binary name, and the project index files each
//! module's copy under a per-module alias key (`bennu_index::module`). Which copy a lookup wants is
//! decided by where the *asking file* lives — information the `TypeResolver` seam (a binary name in,
//! members out) has no room for, and which a hundred and fifty call sites would have to thread
//! through. So it is ambient, the same way [`dep_record`](crate::dep_record)'s recorder is: a
//! thread-local, set around one file's validation and restored after, composing with parallel
//! validation because every worker thread has its own.

use std::cell::RefCell;

thread_local! {
    static MODULE: RefCell<Option<String>> = const { RefCell::new(None) };
}

/// Restores the previous module when dropped — see [`enter`].
#[must_use = "the module is only in effect while this guard lives"]
pub struct ModuleScope {
    previous: Option<String>,
}

impl Drop for ModuleScope {
    fn drop(&mut self) {
        let previous = self.previous.take();
        MODULE.with(|m| *m.borrow_mut() = previous);
    }
}

/// Resolve as a file of `module` until the returned guard is dropped. `None` is a file in no module
/// (or a caller that does not know), which reads the plain keys exactly as before.
pub fn enter(module: Option<String>) -> ModuleScope {
    let previous = MODULE.with(|m| m.replace(module));
    ModuleScope { previous }
}

/// Run `f` with the module in effect on this thread, without copying it out — the resolver asks on
/// every `members_of`, and a clone per call would be the whole cost of the feature.
pub(crate) fn with_current<R>(f: impl FnOnce(Option<&str>) -> R) -> R {
    MODULE.with(|m| f(m.borrow().as_deref()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_scope_nests_and_restores() {
        assert_eq!(with_current(|m| m.map(str::to_string)), None);
        let outer = enter(Some("a".into()));
        assert_eq!(with_current(|m| m.map(str::to_string)).as_deref(), Some("a"));
        {
            let _inner = enter(Some("b".into()));
            assert_eq!(with_current(|m| m.map(str::to_string)).as_deref(), Some("b"));
        }
        assert_eq!(with_current(|m| m.map(str::to_string)).as_deref(), Some("a"));
        drop(outer);
        assert_eq!(with_current(|m| m.map(str::to_string)), None);
    }
}
