//! Killing a spawned child **and everything it started**.
//!
//! One function, shared by the two domains that launch long-lived children (`build`'s
//! `java` run and `tests`' `mvn test`), because the Windows half of it is the kind of
//! detail that is right in the place someone remembered it and wrong everywhere else.

use std::process::Child;
#[cfg(windows)]
use std::process::Command;

#[cfg(windows)]
use arbor_process_ext::prelude::NoWindowExt;

/// Kill `child` and its whole process tree.
///
/// `Child::kill` kills exactly the handle we hold, and on Windows that handle is usually a
/// **launcher**: `mvn.cmd` for a test run, and for anything started through a shell the real
/// work is a grandchild. Killing the launcher leaves the JVM running — still holding
/// `target/`, still writing to files, still listening on the port — while the UI says the run
/// has stopped. `taskkill /T` takes the tree.
///
/// `kill` follows in both cases: on Unix it is the whole mechanism, and everywhere it reaps
/// the handle so the waiting thread's `wait()` returns.
pub(crate) fn kill_tree(child: &mut Child) {
    #[cfg(windows)]
    {
        let mut tk = Command::new("taskkill");
        tk.arg("/PID").arg(child.id().to_string()).arg("/T").arg("/F");
        tk.no_window();
        let _ = tk.output();
    }
    let _ = child.kill();
}
