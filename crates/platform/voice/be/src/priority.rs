//! Lower the calling thread's scheduling priority so a transcription never competes with the
//! editor for the CPU. Best effort: a failure just leaves the thread at normal priority.

#[cfg(windows)]
pub fn lower() {
    extern "system" {
        fn GetCurrentThread() -> isize;
        fn SetThreadPriority(thread: isize, priority: i32) -> i32;
    }
    const THREAD_PRIORITY_BELOW_NORMAL: i32 = -1;
    // SAFETY: both are plain kernel32 calls on the current thread's pseudo-handle.
    unsafe {
        SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_BELOW_NORMAL);
    }
}

#[cfg(all(unix, not(target_os = "macos")))]
pub fn lower() {
    extern "C" {
        fn nice(inc: i32) -> i32;
    }
    // SAFETY: `nice` only adjusts the calling thread's niceness on Linux.
    unsafe {
        nice(5);
    }
}

// macOS: `nice` there is process-wide, which would also slow the capture thread; leave it.
#[cfg(target_os = "macos")]
pub fn lower() {}
