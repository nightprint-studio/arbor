//! How many files this process may hold open at once.
//!
//! ## The bug this exists for
//!
//! A dependency jar is **held open for the session**: `JarSource` keeps the `ZipArchive` — and its
//! `File` — alive, because the alternative is re-opening the zip on every class it decodes. So a
//! project with 150 dependency jars costs 150 file descriptors that never come back, plus the JDK
//! image, plus every mmapped index file, plus the language servers and the child processes.
//!
//! macOS hands a process a **soft limit of 256**. From a terminal you rarely notice — a login shell
//! usually raises it — but a GUI app inherits launchd's, and that is the one the user runs. The
//! failure that follows is not a message about file descriptors: the dependency index fails to build
//! with `os error 24`, so every library type reads as unresolved and every import in the file goes
//! red; a file opened afterwards comes back empty, because opening it needed a descriptor too. It
//! looks like three unrelated bugs and it is one number.
//!
//! The hard limit is effectively unbounded, so raising the soft one needs no privileges and costs
//! nothing: a limit is a ceiling, not an allocation.
//!
//! Windows has no equivalent knob — the CRT's own cap applies to `open`/`fopen` handles and not to
//! the `HANDLE`s a `File` actually uses — so there is nothing to raise there.

/// What this process asks for. Comfortably above any real project: 150 jars is an ordinary Spring
/// service, a legacy monolith reaches four figures, and the whole point of asking once at start-up
/// is not to have to think about the exact number again.
#[cfg(unix)]
const WANTED: u64 = 16_384;

/// Raise the open-file soft limit towards [`WANTED`], capped at whatever the hard limit allows.
///
/// Best-effort and silent about success: it is start-up plumbing, and a backend that announced its
/// file-descriptor budget on every launch would be noise. A failure is worth a line on stderr,
/// because it turns the confusing symptom above into something searchable.
#[cfg(unix)]
pub fn raise_open_file_limit() {
    // SAFETY: `getrlimit`/`setrlimit` with a valid resource id and an owned struct. No aliasing,
    // no lifetimes, and the call is made once from `main` before any thread is spawned.
    unsafe {
        let mut lim = libc::rlimit { rlim_cur: 0, rlim_max: 0 };
        if libc::getrlimit(libc::RLIMIT_NOFILE, &mut lim) != 0 {
            eprintln!("bennu-be: could not read the open-file limit — leaving it as it is");
            return;
        }
        if (lim.rlim_cur as u64) >= WANTED {
            return; // already roomy — a login shell or a launcher has done it for us
        }
        // Descending, because the ceiling is not one number and not one that can be asked for.
        // The hard limit says `RLIM_INFINITY` on macOS while the kernel still refuses anything
        // above `kern.maxfilesperproc`, and `setrlimit` answers a too-large request by failing
        // outright rather than by clamping — so a single confident ask leaves the limit exactly
        // where it was, which is the state this exists to get out of.
        for target in [WANTED, 8192, 4096, 2048, 1024] {
            if target <= lim.rlim_cur as u64 {
                break;
            }
            let raised = libc::rlimit { rlim_cur: target as libc::rlim_t, rlim_max: lim.rlim_max };
            if libc::setrlimit(libc::RLIMIT_NOFILE, &raised) == 0 {
                return;
            }
        }
        eprintln!(
            "bennu-be: could not raise the open-file limit above {} — a project with many \
             dependency jars may fail to index with `Too many open files`",
            lim.rlim_cur
        );
    }
}

#[cfg(not(unix))]
pub fn raise_open_file_limit() {}

/// The current soft limit, for the tests and for anything that wants to report it.
#[cfg(unix)]
pub fn open_file_limit() -> u64 {
    // SAFETY: as above — a read of an owned struct through a valid resource id.
    unsafe {
        let mut lim = libc::rlimit { rlim_cur: 0, rlim_max: 0 };
        if libc::getrlimit(libc::RLIMIT_NOFILE, &mut lim) != 0 {
            return 0;
        }
        lim.rlim_cur as u64
    }
}

#[cfg(not(unix))]
pub fn open_file_limit() -> u64 {
    u64::MAX
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::fs::File;

    /// The soft limit macOS hands a GUI app. Everything below is written against this number
    /// because it is the one the user's machine actually enforces.
    const LAUNCHD_DEFAULT: u64 = 256;

    /// Put the process back where a bundled app starts, so the test measures the real starting
    /// point rather than whatever the shell running `cargo test` happened to set.
    fn set_soft_limit(to: u64) {
        unsafe {
            let mut lim = libc::rlimit { rlim_cur: 0, rlim_max: 0 };
            assert_eq!(libc::getrlimit(libc::RLIMIT_NOFILE, &mut lim), 0);
            let lowered = libc::rlimit { rlim_cur: to as libc::rlim_t, rlim_max: lim.rlim_max };
            assert_eq!(libc::setrlimit(libc::RLIMIT_NOFILE, &lowered), 0, "lower the soft limit");
        }
    }

    /// The regression, stated as the thing that actually failed: **hold more files open at once
    /// than a Mac app is given by default.**
    ///
    /// 150 dependency jars are held open for the whole session, and the index opens the JDK image
    /// and its own files beside them. Under the 256 the app inherits, that arithmetic runs out —
    /// and it does not fail as "too many open files" anywhere the user can see it: the dependency
    /// index quietly fails to build, so every library import in every file turns red, and the next
    /// file opened comes back empty.
    ///
    /// **One test, both halves.** `RLIMIT_NOFILE` is per-PROCESS, and cargo runs tests in threads
    /// of one process: as two tests, one lowered the limit back to 256 while the other was opening
    /// its four hundredth file, and the failure looked like the fix not working. The negative half
    /// has to be here anyway — without it this test could pass because the machine is roomy rather
    /// than because the call did anything.
    #[test]
    fn four_hundred_files_need_the_raised_limit_and_get_it() {
        let restore = open_file_limit();
        let dir = std::env::temp_dir().join(format!("bennu-fd-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("temp dir");
        let path = dir.join("held.bin");
        std::fs::write(&path, b"x").expect("write");
        let open_400 = || -> Result<Vec<File>, std::io::Error> {
            // Held all at once, exactly as the jar sources are held on the project slot: the
            // descriptors must be alive together, which is the whole shape of the bug.
            (0..400).map(|_| File::open(&path)).collect()
        };

        // Under what a bundled Mac app is handed, this is the failure the user saw.
        set_soft_limit(LAUNCHD_DEFAULT);
        let err = open_400().expect_err("400 opens must not fit under a 256 limit");
        assert_eq!(
            err.raw_os_error(),
            Some(24),
            "and it must be EMFILE — the `os error 24` the notification reported"
        );

        // And after the call `main` makes, it fits.
        raise_open_file_limit();
        assert!(
            open_file_limit() > LAUNCHD_DEFAULT,
            "the point of the call is to leave the default behind"
        );
        let held = open_400().expect("400 files open at once under a raised limit");
        assert_eq!(held.len(), 400);
        drop(held);

        let _ = std::fs::remove_dir_all(&dir);
        set_soft_limit(restore);
    }
}
