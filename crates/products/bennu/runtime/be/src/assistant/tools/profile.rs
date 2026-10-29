//! The profiler as the assistant's tools: record a JVM that is already running, then read what it recorded.
//!
//! Reading is the catalogue's (`bennu_profile_recordings`, `bennu_profile_summary`, `bennu_profile_diff`); this
//! file is the part that *does* something: `profile_start` attaches Flight Recorder to a JVM through `jcmd`, and
//! `profile_stop` ends the recording, waits until it has been read and saved, and returns its summary.
//!
//! It asks first, every time: starting a recording changes a running program (it samples it, with a small cost)
//! even though nothing in the program is altered. Sub-agents never have it. A recording is started in one message
//! and stopped in a later one, so what is recording lives per conversation, not per run.

use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant};

use super::*;
use crate::profiler;

pub(super) const START: &str = "profile_start";
pub(super) const STOP: &str = "profile_stop";

/// How long `profile_stop` waits for the recording to be read, and the most it may be asked to.
const DEFAULT_WAIT_S: u64 = 120;
const MAX_WAIT_S: u64 = 300;
/// Rows of each list in the summary that comes back with the stop.
const SUMMARY_ROWS: usize = 10;
const POLL: Duration = Duration::from_millis(250);

pub(super) fn is_profile_tool(name: &str) -> bool {
    matches!(name, START | STOP)
}

pub(super) fn specs() -> Vec<ToolSpec> {
    vec![
        spec(
            START,
            "Start recording a JVM that is already running on this machine with Flight Recorder (through jcmd), to find where its time and memory go. The user approves every start. Name the JVM by `pid` or by part of its main class in `main_class`; with neither, you get the list of JVMs to choose from. One recording per conversation. Then have the program do the work to measure (an http_request, a test run, the user using it) and call profile_stop. The recording keeps sampling until you stop it, so do not leave it running.",
            json!({"type":"object","properties":{
                "pid":{"type":"integer","description":"The JVM's process id."},
                "main_class":{"type":"string","description":"Part of the JVM's main class or jar name, when you do not know the pid."},
                "label":{"type":"string","description":"A few words for the recording in the Profiler window."}
            }}),
        ),
        spec(
            STOP,
            "Stop the recording this conversation started, wait until it has been read and saved (it can take a while for a long one), and return its summary: the hottest methods with their source lines, the biggest allocation sites, lock contention and GC pauses. The recording stays in the Profiler window; bennu_profile_recordings lists it, bennu_profile_diff compares it with another.",
            json!({"type":"object","properties":{"wait_s":{"type":"integer","description":"How long to wait for it to be read. Default 120, at most 300."}}}),
        ),
    ]
}

/// A recording a conversation has going.
struct Active {
    session_id: String,
    pid: u32,
    name: String,
}

static ACTIVE: LazyLock<Mutex<HashMap<String, Active>>> = LazyLock::new(Default::default);

fn active() -> std::sync::MutexGuard<'static, HashMap<String, Active>> {
    ACTIVE.lock().unwrap_or_else(|p| p.into_inner())
}

/// Which JVM a call means: the `pid`, or the only one whose name contains `main_class`. `Err` carries the list to
/// choose from when it cannot tell.
fn pick_jvm(jvms: &[(u32, String)], pid: Option<u64>, main_class: &str) -> Result<(u32, String), String> {
    let listing = || jvms.iter().map(|(p, n)| format!("  pid {p}: {n}")).collect::<Vec<_>>().join("\n");
    if jvms.is_empty() {
        return Err("no JVM is running that jcmd can see. Start the program first (debug_launch is for debugging; for a plain run the user starts it), then try again.".into());
    }
    if let Some(pid) = pid {
        return jvms.iter().find(|(p, _)| u64::from(*p) == pid).cloned().ok_or_else(|| format!("no JVM has pid {pid}. The ones running:\n{}", listing()));
    }
    let wanted = main_class.trim().to_lowercase();
    if wanted.is_empty() {
        return Err(format!("which JVM? Give `pid` or `main_class`. The ones running:\n{}", listing()));
    }
    let hits: Vec<&(u32, String)> = jvms.iter().filter(|(_, n)| n.to_lowercase().contains(&wanted)).collect();
    match hits.as_slice() {
        [one] => Ok((*one).clone()),
        [] => Err(format!("no running JVM matches `{main_class}`. The ones running:\n{}", listing())),
        many => Err(format!("`{main_class}` matches several JVMs, give the `pid`:\n{}", many.iter().map(|(p, n)| format!("  pid {p}: {n}")).collect::<Vec<_>>().join("\n"))),
    }
}

impl ToolHost {
    pub(super) fn run_profile(&self, tool: &str, input: &Value) -> Result<String, String> {
        match tool {
            START => self.profile_start(input),
            STOP => self.profile_stop(input),
            other => Err(format!("`{other}` is not a profiler tool")),
        }
    }

    fn profile_start(&self, input: &Value) -> Result<String, String> {
        let conversation = self.session.conversation_id.clone();
        if let Some(a) = active().get(&conversation) {
            return Err(format!("this conversation is already recording `{}` (pid {}): call profile_stop first", a.name, a.pid));
        }
        let root = self.session.root.clone();
        let jvms = profiler::running_jvms(&root)?;
        let (pid, name) = pick_jvm(&jvms, input.get("pid").and_then(Value::as_u64), input.get("main_class").and_then(Value::as_str).unwrap_or(""))?;

        // Asked after the JVM is known, so the question says which program it is about.
        self.confirm(
            "Let Imber record a running JVM?",
            &format!("Flight Recorder will be started inside “{name}” (pid {pid}) with jcmd, sampling it until Imber stops it. Nothing in the program is changed; it runs a little slower while it is recorded."),
            false,
        )?;

        let label = input.get("label").and_then(Value::as_str).map(str::trim).filter(|l| !l.is_empty()).map(str::to_string).unwrap_or_else(|| format!("Imber: {}", name.chars().take(40).collect::<String>()));
        let session_id = profiler::start_recording(&self.session.ctx, &root, pid, &label)?;
        active().insert(conversation, Active { session_id, pid, name: name.clone() });
        Ok(format!("recording “{name}” (pid {pid}). Make the program do the work you want measured, then call profile_stop."))
    }

    fn profile_stop(&self, input: &Value) -> Result<String, String> {
        let conversation = &self.session.conversation_id;
        let Active { session_id, name, .. } = active().remove(conversation).ok_or("this conversation has no recording going: start one with profile_start")?;
        profiler::stop_recording(&self.session.ctx, &session_id)?;

        let wait = Duration::from_secs(input.get("wait_s").and_then(Value::as_u64).unwrap_or(DEFAULT_WAIT_S).clamp(1, MAX_WAIT_S));
        let deadline = Instant::now() + wait;
        loop {
            match profiler::outcome_of(&session_id) {
                Some(Ok(())) => {
                    let summary = profiler::recording_summary(&self.session.root, &session_id, SUMMARY_ROWS)?;
                    return Ok(format!("stopped “{name}”; the recording `{session_id}` is saved.\n{summary}"));
                }
                Some(Err(why)) => return Err(format!("the recording of “{name}” could not be read: {why}")),
                None => {}
            }
            if self.session.cancelled() {
                return Err(format!("cancelled while the recording was being read; it is saved as `{session_id}` if it finishes"));
            }
            if Instant::now() >= deadline {
                return Ok(format!("stopped “{name}”, but the recording is still being read after {} s. It will appear in bennu_profile_recordings as `{session_id}`; read it with bennu_profile_summary then.", wait.as_secs()));
            }
            std::thread::sleep(POLL);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn jvms() -> Vec<(u32, String)> {
        vec![(101, "com.acme.OrderApp --server".into()), (202, "org.gradle.launcher.daemon".into()), (303, "com.acme.BatchJob".into())]
    }

    #[test]
    fn a_jvm_is_found_by_pid_or_by_a_unique_part_of_its_name() {
        assert_eq!(pick_jvm(&jvms(), Some(202), "").unwrap().0, 202);
        assert_eq!(pick_jvm(&jvms(), None, "orderapp").unwrap().0, 101);
        assert_eq!(pick_jvm(&jvms(), Some(101), "ignored").unwrap().0, 101, "a pid wins over a name");
    }

    #[test]
    fn when_it_cannot_tell_it_lists_the_choices() {
        assert!(pick_jvm(&jvms(), None, "").unwrap_err().contains("pid 303"));
        assert!(pick_jvm(&jvms(), None, "acme").unwrap_err().contains("several"), "two matches");
        assert!(pick_jvm(&jvms(), None, "nothing").unwrap_err().contains("no running JVM matches"));
        assert!(pick_jvm(&jvms(), Some(9), "").unwrap_err().contains("no JVM has pid 9"));
        assert!(pick_jvm(&[], None, "x").unwrap_err().contains("no JVM is running"));
    }

    #[test]
    fn the_tools_are_recognised_and_start_is_never_offered_to_an_agent() {
        assert!(is_profile_tool(START) && is_profile_tool(STOP) && !is_profile_tool("bennu_profile_summary"));
        assert_eq!(specs().len(), 2);
        assert_eq!(super::super::families::family_of(START), Some(super::super::families::Family::Profile));
        assert!(super::super::spawn::child_off_ids(bennu_assistant::prelude::ChildTools::ReadEdit).contains(&"profile"));
    }
}
