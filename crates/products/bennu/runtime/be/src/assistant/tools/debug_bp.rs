//! The assistant's breakpoints, as edits to the project's persisted debug set.
//!
//! They live in the same `[debug]` section as the user's — that is what makes them appear in the
//! gutter and in the Breakpoints list, and what lets the user delete or disable them with the tools
//! they already have. A flag on each (`by_assistant`) records who set it. The rules kept here:
//!
//! * the assistant never changes or removes a breakpoint the user set;
//! * there is a cap on how many it may hold, so a confused loop cannot litter the project;
//! * a breakpoint is identified by file and line, compared the way the panel compares them
//!   (separators normalised, case folded).

use bennu_proto::prelude::{Breakpoint, BreakpointStatus, DebugConfig, ExceptionBreakpoint};

/// The most breakpoints the assistant may have set at once.
pub(super) const MAX_ASSISTANT_BREAKPOINTS: usize = 20;
/// The longest logpoint message accepted.
pub(super) const MAX_LOG_MESSAGE_CHARS: usize = 300;

/// A file path in the one spelling breakpoints are compared and stored in.
pub(super) fn canon(path: &str) -> String {
    path.replace('\\', "/")
}

fn same_place(b: &Breakpoint, file: &str, line: u32) -> bool {
    b.line == line && canon(&b.file).eq_ignore_ascii_case(&canon(file))
}

/// What setting a breakpoint did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Set {
    Added,
    /// The assistant already had one there; its condition and message were replaced.
    Updated,
    /// The user has a breakpoint on that line. Left exactly as it is.
    UserOwns,
}

/// Set (or update) the assistant's breakpoint at `file:line`.
pub(super) fn set(cfg: &mut DebugConfig, file: &str, line: u32, condition: &str, log_message: &str, capture: &[String]) -> Result<Set, String> {
    set_with(cfg, file, line, condition, log_message, capture, false)
}

/// [`set`], saying whether a stop suspends only the thread that hit the breakpoint.
pub(super) fn set_with(cfg: &mut DebugConfig, file: &str, line: u32, condition: &str, log_message: &str, capture: &[String], thread_only: bool) -> Result<Set, String> {
    if let Some(existing) = cfg.breakpoints.iter_mut().find(|b| same_place(b, file, line)) {
        if !existing.by_assistant {
            return Ok(Set::UserOwns);
        }
        existing.condition = condition.trim().to_string();
        existing.log_message = log_message.trim().to_string();
        existing.capture = capture.to_vec();
        existing.suspend_thread_only = thread_only;
        existing.enabled = true;
        return Ok(Set::Updated);
    }
    if cfg.breakpoints.iter().filter(|b| b.by_assistant).count() >= MAX_ASSISTANT_BREAKPOINTS {
        return Err(format!("you already have {MAX_ASSISTANT_BREAKPOINTS} breakpoints set: remove some with debug_remove_breakpoint first"));
    }
    cfg.breakpoints.push(Breakpoint {
        file: canon(file),
        line,
        condition: condition.trim().to_string(),
        log_message: log_message.trim().to_string(),
        capture: capture.to_vec(),
        by_assistant: true,
        suspend_thread_only: thread_only,
        ..Breakpoint::default()
    });
    Ok(Set::Added)
}

/// What removing a breakpoint did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Removal {
    Removed,
    NotFound,
    /// It is the user's: not the assistant's to remove.
    UserOwns,
}

pub(super) fn remove(cfg: &mut DebugConfig, file: &str, line: u32) -> Removal {
    match cfg.breakpoints.iter().position(|b| same_place(b, file, line)) {
        None => Removal::NotFound,
        Some(i) if !cfg.breakpoints[i].by_assistant => Removal::UserOwns,
        Some(i) => {
            cfg.breakpoints.remove(i);
            Removal::Removed
        }
    }
}

/// Drop every breakpoint the assistant set; returns how many.
pub(super) fn remove_all_assistant(cfg: &mut DebugConfig) -> usize {
    let before = cfg.breakpoints.len() + cfg.exceptions.len();
    cfg.breakpoints.retain(|b| !b.by_assistant);
    cfg.exceptions.retain(|e| !e.by_assistant);
    before - cfg.breakpoints.len() - cfg.exceptions.len()
}

/// The set as the model reads it: who set each, whether it is on, and what the running VM made of it.
/// `states` are the debugger's answers, matched by place; empty when nothing is running.
pub(super) fn list_text(cfg: &DebugConfig, states: &[BreakpointStatus]) -> String {
    if cfg.breakpoints.is_empty() {
        return "no breakpoints are set in this project.".into();
    }
    let lines: Vec<String> = cfg
        .breakpoints
        .iter()
        .map(|b| {
            let who = if b.by_assistant { "yours" } else { "the user's" };
            let name = canon(&b.file).rsplit('/').next().unwrap_or("").to_string();
            let mut line = format!("{name}:{} ({who}{})", b.line, if b.enabled { "" } else { ", disabled" });
            if !b.condition.is_empty() {
                line.push_str(&format!(" when {}", b.condition));
            }
            if !b.log_message.is_empty() {
                line.push_str(&format!(" logs \"{}\"", b.log_message));
            }
            if !b.capture.is_empty() {
                line.push_str(&format!(" captures {}", b.capture.join(", ")));
            }
            if let Some(s) = states.iter().find(|s| s.line == b.line && canon(&s.file).eq_ignore_ascii_case(&canon(&b.file))) {
                line.push_str(&format!(" — {}", state_word(s)));
            }
            line
        })
        .collect();
    format!("{} breakpoint(s):\n{}", lines.len(), lines.join("\n"))
}

/// What the VM made of one breakpoint, in a few words.
pub(super) fn state_word(s: &BreakpointStatus) -> String {
    let mut parts = vec![if s.verified { "installed".to_string() } else { "not installed yet".to_string() }];
    if !s.message.is_empty() {
        parts.push(s.message.clone());
    }
    if !s.condition_error.is_empty() {
        parts.push(s.condition_error.clone());
    }
    if s.hits > 0 {
        parts.push(format!("hit {}×", s.hits));
    }
    parts.join("; ")
}

/// Whether any breakpoint (the user's or the assistant's, enabled or not) is already on `file:line`.
pub(super) fn has_breakpoint(cfg: &DebugConfig, file: &str, line: u32) -> bool {
    cfg.breakpoints.iter().any(|b| same_place(b, file, line))
}

/// The most paths one breakpoint may capture, and how long each may be.
pub(super) const MAX_CAPTURE_PATHS: usize = bennu_assistant::prelude::MAX_CAPTURES;

// ── Exception breakpoints ────────────────────────────────────────────────────────────────────────

/// The most exception breakpoints the assistant may hold at once.
pub(super) const MAX_ASSISTANT_EXCEPTIONS: usize = 5;

/// Whether `name` can be a fully-qualified class name (letters, digits, `_`, `$`, dots).
pub(super) fn is_class_name(name: &str) -> bool {
    !name.is_empty()
        && name.split('.').all(|seg| {
            let mut chars = seg.chars();
            chars.next().is_some_and(|c| c.is_alphabetic() || c == '_' || c == '$') && chars.all(|c| c.is_alphanumeric() || c == '_' || c == '$')
        })
}

/// What setting an exception breakpoint did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ExceptionSet {
    Added,
    Updated,
    /// The user has one for that class; left as it is.
    UserOwns,
}

/// Stop when `class` (empty: any throwable) is thrown — `caught` and/or `uncaught`.
pub(super) fn set_exception(cfg: &mut DebugConfig, class: &str, caught: bool, uncaught: bool) -> Result<ExceptionSet, String> {
    let class = class.trim();
    if !class.is_empty() && !is_class_name(class) {
        return Err(format!("`{class}` is not a fully-qualified class name"));
    }
    if !caught && !uncaught {
        return Err("choose `caught`, `uncaught` or both".into());
    }
    if let Some(existing) = cfg.exceptions.iter_mut().find(|e| e.class == class) {
        if !existing.by_assistant {
            return Ok(ExceptionSet::UserOwns);
        }
        existing.caught = caught;
        existing.uncaught = uncaught;
        existing.enabled = true;
        return Ok(ExceptionSet::Updated);
    }
    if cfg.exceptions.iter().filter(|e| e.by_assistant).count() >= MAX_ASSISTANT_EXCEPTIONS {
        return Err(format!("you already stop on {MAX_ASSISTANT_EXCEPTIONS} exceptions: remove some with `remove: true` first"));
    }
    cfg.exceptions.push(ExceptionBreakpoint { class: class.to_string(), caught, uncaught, enabled: true, by_assistant: true });
    Ok(ExceptionSet::Added)
}

/// Remove the assistant's exception breakpoint for `class`, or all of its own when `class` is empty and `all`
/// is set. Returns how many went. The user's are never touched.
pub(super) fn remove_exceptions(cfg: &mut DebugConfig, class: &str, all: bool) -> usize {
    let before = cfg.exceptions.len();
    cfg.exceptions.retain(|e| !(e.by_assistant && (all || e.class == class.trim())));
    before - cfg.exceptions.len()
}

/// The exception breakpoints as the model reads them.
pub(super) fn exceptions_text(cfg: &DebugConfig) -> String {
    if cfg.exceptions.is_empty() {
        return String::new();
    }
    let lines: Vec<String> = cfg
        .exceptions
        .iter()
        .map(|e| {
            let who = if e.by_assistant { "yours" } else { "the user's" };
            let what = if e.class.is_empty() { "any exception".to_string() } else { e.class.clone() };
            let when = match (e.caught, e.uncaught) {
                (true, true) => "caught or uncaught",
                (true, false) => "caught",
                _ => "uncaught",
            };
            format!("{what} ({when}; {who}{})", if e.enabled { "" } else { ", disabled" })
        })
        .collect();
    format!("\nStops on exceptions:\n{}", lines.join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_exception_breakpoint_is_the_assistants_and_never_touches_the_users() {
        let mut cfg = DebugConfig::default();
        assert_eq!(set_exception(&mut cfg, "java.lang.IllegalStateException", true, false).unwrap(), ExceptionSet::Added);
        assert!(cfg.exceptions[0].by_assistant);
        assert_eq!(set_exception(&mut cfg, "java.lang.IllegalStateException", true, true).unwrap(), ExceptionSet::Updated);
        assert!(cfg.exceptions[0].uncaught);
        cfg.exceptions.push(ExceptionBreakpoint { class: "a.B".into(), caught: false, uncaught: true, enabled: true, by_assistant: false });
        assert_eq!(set_exception(&mut cfg, "a.B", true, true).unwrap(), ExceptionSet::UserOwns);
        assert!(!cfg.exceptions[1].caught);
        assert_eq!(remove_exceptions(&mut cfg, "a.B", false), 0, "the user's stays");
        assert_eq!(remove_exceptions(&mut cfg, "", true), 1);
        assert_eq!(cfg.exceptions.len(), 1);
    }

    #[test]
    fn a_bad_class_or_an_empty_choice_is_refused() {
        let mut cfg = DebugConfig::default();
        assert!(set_exception(&mut cfg, "not a class!", true, true).is_err());
        assert!(set_exception(&mut cfg, "a.B", false, false).is_err());
        assert_eq!(set_exception(&mut cfg, "", false, true).unwrap(), ExceptionSet::Added, "empty = any throwable");
        assert!(is_class_name("com.acme.Outer$Inner") && !is_class_name("1abc") && !is_class_name("a..b"));
    }

    #[test]
    fn the_assistant_cannot_litter_the_exception_list() {
        let mut cfg = DebugConfig::default();
        for i in 0..MAX_ASSISTANT_EXCEPTIONS {
            set_exception(&mut cfg, &format!("a.E{i}"), true, true).unwrap();
        }
        assert!(set_exception(&mut cfg, "a.Extra", true, true).unwrap_err().contains("remove some"));
        assert!(exceptions_text(&cfg).contains("a.E0 (caught or uncaught; yours)"));
        assert_eq!(exceptions_text(&DebugConfig::default()), "");
    }

    #[test]
    fn a_breakpoint_is_found_by_place_and_captures_are_kept() {
        let mut cfg = DebugConfig::default();
        set(&mut cfg, "C:/p/A.java", 5, "", "", &["order.id".to_string()]).unwrap();
        assert!(has_breakpoint(&cfg, "c:\\P\\a.java", 5) && !has_breakpoint(&cfg, "C:/p/A.java", 6));
        assert_eq!(cfg.breakpoints[0].capture, vec!["order.id".to_string()]);
        assert!(list_text(&cfg, &[]).contains("captures order.id"));
    }

    fn user_bp(file: &str, line: u32) -> Breakpoint {
        Breakpoint { file: file.into(), line, ..Breakpoint::default() }
    }

    #[test]
    fn a_new_breakpoint_is_marked_as_the_assistants() {
        let mut cfg = DebugConfig::default();
        assert_eq!(set(&mut cfg, "C:\\p\\src\\Order.java", 42, "  total > 100 ", "", &[]).unwrap(), Set::Added);
        let b = &cfg.breakpoints[0];
        assert!(b.by_assistant && b.enabled);
        assert_eq!(b.file, "C:/p/src/Order.java", "stored in the panel's spelling");
        assert_eq!(b.condition, "total > 100");
    }

    #[test]
    fn setting_it_again_updates_instead_of_duplicating() {
        let mut cfg = DebugConfig::default();
        set(&mut cfg, "C:/p/A.java", 10, "", "", &[]).unwrap();
        cfg.breakpoints[0].enabled = false;
        assert_eq!(set(&mut cfg, "c:/P/a.java", 10, "x > 1", "seen {x}", &[]).unwrap(), Set::Updated);
        assert_eq!(cfg.breakpoints.len(), 1);
        assert!(cfg.breakpoints[0].enabled, "setting it again turns it back on");
        assert_eq!(cfg.breakpoints[0].log_message, "seen {x}");
    }

    #[test]
    fn a_breakpoint_of_the_user_is_never_touched() {
        let mut cfg = DebugConfig { breakpoints: vec![user_bp("C:/p/A.java", 10)], ..DebugConfig::default() };
        assert_eq!(set(&mut cfg, "C:/p/A.java", 10, "x > 1", "", &[]).unwrap(), Set::UserOwns);
        assert!(cfg.breakpoints[0].condition.is_empty() && !cfg.breakpoints[0].by_assistant);
        assert_eq!(remove(&mut cfg, "C:/p/A.java", 10), Removal::UserOwns);
        assert_eq!(cfg.breakpoints.len(), 1);
        assert_eq!(remove_all_assistant(&mut cfg), 0);
        assert_eq!(cfg.breakpoints.len(), 1);
    }

    #[test]
    fn removing_takes_only_what_the_assistant_set() {
        let mut cfg = DebugConfig { breakpoints: vec![user_bp("C:/p/A.java", 1)], ..DebugConfig::default() };
        set(&mut cfg, "C:/p/A.java", 2, "", "", &[]).unwrap();
        set(&mut cfg, "C:/p/B.java", 3, "", "", &[]).unwrap();
        assert_eq!(remove(&mut cfg, "C:/p/A.java", 2), Removal::Removed);
        assert_eq!(remove(&mut cfg, "C:/p/A.java", 2), Removal::NotFound);
        assert_eq!(remove_all_assistant(&mut cfg), 1);
        assert_eq!(cfg.breakpoints.len(), 1);
    }

    #[test]
    fn the_assistant_cannot_litter_the_project() {
        let mut cfg = DebugConfig::default();
        for line in 1..=MAX_ASSISTANT_BREAKPOINTS as u32 {
            set(&mut cfg, "C:/p/A.java", line, "", "", &[]).unwrap();
        }
        assert!(set(&mut cfg, "C:/p/A.java", 999, "", "", &[]).unwrap_err().contains("remove some"));
        // The user's own breakpoints do not count against it.
        let mut cfg = DebugConfig { breakpoints: (1..=30).map(|l| user_bp("C:/p/A.java", l)).collect(), ..DebugConfig::default() };
        assert!(set(&mut cfg, "C:/p/B.java", 1, "", "", &[]).is_ok());
    }

    #[test]
    fn the_list_says_who_set_what_and_what_the_vm_made_of_it() {
        let mut cfg = DebugConfig { breakpoints: vec![user_bp("C:/p/A.java", 1)], ..DebugConfig::default() };
        set(&mut cfg, "C:/p/B.java", 20, "n > 3", "", &[]).unwrap();
        let states = vec![BreakpointStatus {
            file: "C:\\p\\B.java".into(),
            line: 20,
            verified: true,
            message: String::new(),
            condition_error: String::new(),
            hits: 2,
        }];
        let text = list_text(&cfg, &states);
        assert!(text.contains("A.java:1 (the user's)"), "{text}");
        assert!(text.contains("B.java:20 (yours) when n > 3 — installed; hit 2×"), "{text}");
        assert_eq!(list_text(&DebugConfig::default(), &[]), "no breakpoints are set in this project.");
    }

    #[test]
    fn a_breakpoint_that_slid_or_waits_says_so() {
        let waiting = BreakpointStatus { file: String::new(), line: 0, verified: false, message: "waiting for the class to load".into(), condition_error: String::new(), hits: 0 };
        assert_eq!(state_word(&waiting), "not installed yet; waiting for the class to load");
        let slid = BreakpointStatus { verified: true, message: "line 5 has no code — stopping at line 6".into(), ..waiting };
        assert!(state_word(&slid).starts_with("installed; line 5 has no code"));
    }
}
