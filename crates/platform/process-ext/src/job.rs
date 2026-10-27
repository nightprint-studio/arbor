//! Tie the lifetime of every descendant process to the current one.
//!
//! A backend that launches long-lived children — a Spring service, a test JVM, a language server —
//! is responsible for them dying with it. Doing that by hand (kill each child on shutdown) covers the
//! orderly exit and nothing else: a crash, a `TerminateProcess` from the shell, a window closed
//! while a build was mid-flight all skip the cleanup, and the children go on, holding their ports
//! and their `target/` directories with nobody left to press Stop.
//!
//! On Windows the operating system has the right primitive for this: a **job object** with
//! `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`. Put the current process in one, and every process it starts
//! afterwards — grandchildren included, which `taskkill /T` loses the moment a middle launcher such
//! as `mvn.cmd` exits — belongs to it too. When the last handle to the job closes, which is when the
//! owning process dies *for any reason*, the system kills the whole set.
//!
//! `BREAKAWAY_OK` is set so a caller that deliberately wants something to outlive us (an editor
//! launched with `CREATE_BREAKAWAY_FROM_JOB`) still can.
//!
//! The Win32 calls are declared by hand rather than pulled in from a crate: it is four functions and
//! two structs, and this crate has no other reason to depend on one. Elsewhere this is a no-op — Unix
//! children are started as process-group leaders and killed by group, which survives the same cases.

/// Make every process started from now on die when this one does. Idempotent; best-effort.
///
/// - **Windows**: a kill-on-close job object; nothing more to do per child.
/// - **Unix**: a *watchdog* — a tiny helper process (this same executable, re-run with
///   `--arbor-watchdog`) that outlives us and kills the registered process groups when the pipe we
///   hold to it closes, which is when we die. Children are only covered once they are announced with
///   [`track_child_group`], because on Unix there is no "everything I start" handle: they are
///   deliberately group leaders of their own (so Stop can `killpg` them), which is also why they
///   would otherwise survive us. The caller must also call [`run_watchdog_if_requested`] first thing
///   in `main`.
///
/// Returns whether the guarantee is in place. `false` is not an error to act on: the caller keeps the
/// manual cleanup it already has.
pub fn kill_descendants_on_exit() -> bool {
    imp::install()
}

/// Announce a child that leads its own process group (see `own_group` in the backend), so the
/// watchdog kills it if we die. No-op on Windows, where the job object already covers it.
pub fn track_child_group(pid: u32) {
    imp::track(pid, true);
}

/// The child has been reaped: take it off the watchdog's list, so a recycled pid is never signalled.
pub fn untrack_child_group(pid: u32) {
    imp::track(pid, false);
}

/// If this process was started as the watchdog, run it (it never returns to the caller's `main`
/// logic) and say so. Call as the **first** thing in `main`, before any other setup:
///
/// ```ignore
/// if arbor_process_ext::prelude::run_watchdog_if_requested() { return; }
/// ```
pub fn run_watchdog_if_requested() -> bool {
    if std::env::args().nth(1).as_deref() != Some("--arbor-watchdog") {
        return false;
    }
    imp::watchdog_main();
    true
}

#[cfg(windows)]
mod imp {
    use std::ffi::c_void;
    use std::sync::OnceLock;

    type Handle = *mut c_void;

    const JOB_OBJECT_EXTENDED_LIMIT_INFORMATION_CLASS: i32 = 9;
    const LIMIT_BREAKAWAY_OK: u32 = 0x0000_0800;
    const LIMIT_KILL_ON_JOB_CLOSE: u32 = 0x0000_2000;

    #[repr(C)]
    struct BasicLimitInformation {
        per_process_user_time_limit: i64,
        per_job_user_time_limit: i64,
        limit_flags: u32,
        minimum_working_set_size: usize,
        maximum_working_set_size: usize,
        active_process_limit: u32,
        affinity: usize,
        priority_class: u32,
        scheduling_class: u32,
    }

    #[repr(C)]
    struct IoCounters {
        counts: [u64; 6],
    }

    #[repr(C)]
    struct ExtendedLimitInformation {
        basic: BasicLimitInformation,
        io: IoCounters,
        process_memory_limit: usize,
        job_memory_limit: usize,
        peak_process_memory_used: usize,
        peak_job_memory_used: usize,
    }

    extern "system" {
        fn CreateJobObjectW(attributes: *mut c_void, name: *const u16) -> Handle;
        fn SetInformationJobObject(job: Handle, class: i32, info: *mut c_void, len: u32) -> i32;
        fn AssignProcessToJobObject(job: Handle, process: Handle) -> i32;
        fn GetCurrentProcess() -> Handle;
    }

    /// The job's handle as an address, kept for the life of the process. It is **never closed**:
    /// closing it is what kills the members, and that must happen when we die, not before.
    static JOB: OnceLock<usize> = OnceLock::new();

    pub fn track(_pid: u32, _add: bool) {}

    pub fn watchdog_main() {}

    pub fn install() -> bool {
        *JOB.get_or_init(|| unsafe {
            let job = CreateJobObjectW(std::ptr::null_mut(), std::ptr::null());
            if job.is_null() {
                return 0;
            }
            let mut info: ExtendedLimitInformation = std::mem::zeroed();
            info.basic.limit_flags = LIMIT_KILL_ON_JOB_CLOSE | LIMIT_BREAKAWAY_OK;
            let set = SetInformationJobObject(
                job,
                JOB_OBJECT_EXTENDED_LIMIT_INFORMATION_CLASS,
                (&mut info as *mut ExtendedLimitInformation).cast(),
                std::mem::size_of::<ExtendedLimitInformation>() as u32,
            );
            if set == 0 || AssignProcessToJobObject(job, GetCurrentProcess()) == 0 {
                return 0;
            }
            job as usize
        }) != 0
    }
}

#[cfg(unix)]
mod imp {
    use std::collections::HashSet;
    use std::io::{BufRead, Write};
    use std::os::unix::process::CommandExt;
    use std::process::{ChildStdin, Command, Stdio};
    use std::sync::{Mutex, OnceLock};

    /// The write end of the watchdog's stdin; `None` until started, or if it could not be.
    static PIPE: OnceLock<Mutex<Option<ChildStdin>>> = OnceLock::new();

    pub fn install() -> bool {
        let slot = PIPE.get_or_init(|| Mutex::new(None));
        let mut guard = slot.lock().unwrap_or_else(|p| p.into_inner());
        if guard.is_some() {
            return true;
        }
        let Ok(exe) = std::env::current_exe() else { return false };
        // Its own process group: a Ctrl+C or a group kill aimed at us must not take the watchdog
        // down with us — surviving us is its whole job.
        let spawned = Command::new(exe)
            .arg("--arbor-watchdog")
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .process_group(0)
            .spawn();
        match spawned {
            Ok(mut child) => {
                *guard = child.stdin.take();
                // Not waited on: it ends by itself when the pipe closes.
                std::mem::forget(child);
                guard.is_some()
            }
            Err(_) => false,
        }
    }

    pub fn track(pid: u32, add: bool) {
        let Some(slot) = PIPE.get() else { return };
        let mut guard = slot.lock().unwrap_or_else(|p| p.into_inner());
        if let Some(pipe) = guard.as_mut() {
            let _ = writeln!(pipe, "{}{pid}", if add { '+' } else { '-' });
            let _ = pipe.flush();
        }
    }

    /// The watchdog process: remember which groups are announced, and when stdin closes (the owner
    /// died) kill every one that still leads its own group.
    pub fn watchdog_main() {
        let mut groups: HashSet<i32> = HashSet::new();
        for line in std::io::stdin().lock().lines().map_while(Result::ok) {
            let (op, rest) = line.split_at(line.len().min(1));
            let Ok(pid) = rest.trim().parse::<i32>() else { continue };
            match op {
                "+" => { groups.insert(pid); }
                "-" => { groups.remove(&pid); }
                _ => {}
            }
        }
        for pid in groups {
            // The same test the owner uses before signalling: only a pid that still leads a group
            // of its own is ours. A pid recycled for something else is not a group leader, and
            // killing it would be killing a stranger.
            if pid > 0 && unsafe { libc::getpgid(pid) } == pid {
                unsafe { libc::killpg(pid, libc::SIGKILL) };
            }
        }
    }
}
