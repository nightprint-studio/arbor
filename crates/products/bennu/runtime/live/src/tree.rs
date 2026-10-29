//! Making a spawned child the root of something killable, and killing all of it.
//!
//! A service started through a shell is never the process that matters: `cmd /C mvn spring-boot:run`
//! or `sh -c "npm run dev"` is a launcher whose descendants hold the port. Stop has to take the whole
//! tree, on every OS: `taskkill /T` on Windows, a process group on Unix (the child leads its own, so
//! one `killpg` reaches every descendant that stayed in it).
//!
//! This is the canonical implementation; `bennu-be`'s `child.rs` delegates here so a Run console, a
//! test run and a managed process stop the same way.

use std::process::{Child, Command};

#[cfg(windows)]
use arbor_process_ext::prelude::NoWindowExt;

/// Put a child at the head of its own process group (Unix) so [`kill_tree`] and [`terminate_tree`]
/// can reach its descendants. Call before `spawn`. A no-op on Windows, where `taskkill /T` walks the
/// real parent-child tree.
pub fn own_group(cmd: &mut Command) {
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        // 0 = "a new group, led by the child": its pgid is its own pid, which `kill_tree` checks.
        cmd.process_group(0);
    }
    #[cfg(not(unix))]
    let _ = cmd;
}

/// Ask a child and its tree to end, politely. SIGTERM to the group on Unix; on Windows `taskkill /T`
/// without `/F`, which only reaches processes that answer a close request. The caller waits a grace
/// period and then calls [`kill_tree`].
pub fn terminate_tree(child: &Child) {
    #[cfg(windows)]
    {
        let mut tk = Command::new("taskkill");
        tk.arg("/PID").arg(child.id().to_string()).arg("/T");
        tk.no_window();
        let _ = tk.output();
    }
    #[cfg(unix)]
    {
        let pid = child.id() as libc::pid_t;
        if pid > 0 && unsafe { libc::getpgid(pid) } == pid {
            unsafe { libc::killpg(pid, libc::SIGTERM) };
        } else if pid > 0 {
            unsafe { libc::kill(pid, libc::SIGTERM) };
        }
    }
}

/// After a child that led its own group has gone, kill what is left of the group (Unix). A service
/// that obeyed SIGTERM at the top can still have left a worker behind that did not. Only meant to be
/// called right after our own [`terminate_tree`], when the pid cannot have been recycled. No-op on
/// Windows, where a dead parent no longer links its descendants.
pub fn kill_group_remnants(pid: u32) {
    #[cfg(unix)]
    {
        if pid > 0 {
            unsafe { libc::killpg(pid as libc::pid_t, libc::SIGKILL) };
        }
    }
    #[cfg(not(unix))]
    let _ = pid;
}

/// Kill `child` and its whole process tree, then reap the handle.
///
/// On Unix only when the child really leads a group of its own (`getpgid(pid) == pid`): a child
/// spawned without [`own_group`] shares OUR group, and signalling that would kill unrelated work.
pub fn kill_tree(child: &mut Child) {
    #[cfg(windows)]
    {
        let mut tk = Command::new("taskkill");
        tk.arg("/PID").arg(child.id().to_string()).arg("/T").arg("/F");
        tk.no_window();
        let _ = tk.output();
    }
    #[cfg(unix)]
    {
        let pid = child.id() as libc::pid_t;
        if pid > 0 && unsafe { libc::getpgid(pid) } == pid {
            unsafe { libc::killpg(pid, libc::SIGKILL) };
        }
    }
    let _ = child.kill();
}
