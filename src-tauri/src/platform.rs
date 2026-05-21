/// Platform-specific process management helpers.
///
/// Public entry point: [`set_efficiency_mode`] — enables/disables OS-level
/// power-throttling when Arbor moves to the background.
///
/// On Windows the function sets EcoQoS on **both** the main process and all
/// its direct/indirect child processes (WebView2 renderers) so that Task
/// Manager shows the green leaf icon at the app-group level.

// ---------------------------------------------------------------------------
// Windows
// ---------------------------------------------------------------------------

#[cfg(target_os = "windows")]
mod imp {
    use std::collections::VecDeque;
    use windows_sys::Win32::Foundation::BOOL;
    use windows_sys::Win32::System::Threading::{
        GetCurrentProcess, GetCurrentProcessId, OpenProcess, SetPriorityClass,
        SetProcessInformation, IDLE_PRIORITY_CLASS, NORMAL_PRIORITY_CLASS,
        PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SET_INFORMATION,
    };
    use windows_sys::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, Process32First, Process32Next, PROCESSENTRY32,
        TH32CS_SNAPPROCESS,
    };
    use windows_sys::Win32::Foundation::{CloseHandle, HANDLE, INVALID_HANDLE_VALUE};

    const PROCESS_POWER_THROTTLING_CLASS: i32 = 4;
    const POWER_THROTTLING_VERSION:       u32 = 1;
    const THROTTLE_EXECUTION_SPEED:       u32 = 0x1;

    #[repr(C)]
    struct PowerThrottlingState {
        version:      u32,
        control_mask: u32,
        state_mask:   u32,
    }

    /// Apply EcoQoS throttling + priority to a single process handle.
    /// Returns true if both calls succeeded.
    unsafe fn apply_to_handle(handle: HANDLE, enabled: bool) -> bool {
        let throttling = PowerThrottlingState {
            version:      POWER_THROTTLING_VERSION,
            control_mask: THROTTLE_EXECUTION_SPEED,
            state_mask:   if enabled { THROTTLE_EXECUTION_SPEED } else { 0 },
        };
        let ok1: BOOL = SetProcessInformation(
            handle,
            PROCESS_POWER_THROTTLING_CLASS,
            &throttling as *const PowerThrottlingState as *const _,
            std::mem::size_of::<PowerThrottlingState>() as u32,
        );
        // IDLE_PRIORITY_CLASS (not BELOW_NORMAL) is the canonical combination
        // that makes Windows 11 Task Manager surface the green "leaf" icon
        // alongside EcoQoS.  With BELOW_NORMAL the throttling still works
        // internally but the Task Manager indicator is often suppressed,
        // leading users to believe efficiency mode is inactive.
        let priority = if enabled { IDLE_PRIORITY_CLASS } else { NORMAL_PRIORITY_CLASS };
        let ok2: BOOL = SetPriorityClass(handle, priority);
        ok1 != 0 && ok2 != 0
    }

    /// Read the image file name from a `PROCESSENTRY32` (ANSI buffer terminated
    /// by NUL). Returned as a Rust `String` for case-insensitive comparison.
    /// Defensive against non-UTF-8 bytes via `from_utf8_lossy`.
    unsafe fn exe_name_from_entry(entry: &PROCESSENTRY32) -> String {
        let bytes: Vec<u8> = entry.szExeFile.iter()
            .take_while(|&&c| c != 0)
            .map(|&c| c as u8)
            .collect();
        String::from_utf8_lossy(&bytes).into_owned()
    }

    /// Collect descendants of `root_pid` as `(pid, exe_name)` pairs so the
    /// caller can filter by image name. Built via a single Toolhelp snapshot
    /// + BFS over a parent→children map.
    fn collect_descendant_processes(root_pid: u32) -> Vec<(u32, String)> {
        let mut result  = Vec::new();
        let mut pending = VecDeque::new();
        pending.push_back(root_pid);

        unsafe {
            let snap = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
            if snap == INVALID_HANDLE_VALUE {
                return result;
            }

            let mut entry: PROCESSENTRY32 = std::mem::zeroed();
            entry.dwSize = std::mem::size_of::<PROCESSENTRY32>() as u32;

            // Two side-tables built from the snapshot in one walk:
            //   parent_pid → [child pid …]   (for BFS)
            //   pid        → image name      (for filtering)
            let mut children: std::collections::HashMap<u32, Vec<u32>> =
                std::collections::HashMap::new();
            let mut names: std::collections::HashMap<u32, String> =
                std::collections::HashMap::new();

            if Process32First(snap, &mut entry) != 0 {
                loop {
                    children
                        .entry(entry.th32ParentProcessID)
                        .or_default()
                        .push(entry.th32ProcessID);
                    names.insert(entry.th32ProcessID, exe_name_from_entry(&entry));
                    if Process32Next(snap, &mut entry) == 0 {
                        break;
                    }
                }
            }
            CloseHandle(snap);

            // BFS over the tree, attaching the image name as we emit each pid.
            while let Some(pid) = pending.pop_front() {
                if pid != root_pid {
                    let name = names.get(&pid).cloned().unwrap_or_default();
                    result.push((pid, name));
                }
                if let Some(kids) = children.get(&pid) {
                    pending.extend(kids.iter().copied());
                }
            }
        }
        result
    }

    /// Image names of processes we want to include in the throttling sweep.
    /// The list is intentionally tight: only the WebView2 renderer / GPU /
    /// network helpers, which Arbor would not have spawned without the
    /// embedded webview, qualify. Anything else under our subtree is
    /// considered a *user-launched* process (compile builds, Tomcat, terminal
    /// shells from the integrated terminal, plugin jobs, …) and must keep
    /// running at full speed regardless of Arbor's focus state.
    /// Compared case-insensitively.
    const THROTTLEABLE_DESCENDANTS: &[&str] = &[
        "msedgewebview2.exe",
    ];

    fn is_throttleable(name: &str) -> bool {
        THROTTLEABLE_DESCENDANTS.iter().any(|w| w.eq_ignore_ascii_case(name))
    }

    pub fn set_efficiency_mode(enabled: bool) {
        let t_total = std::time::Instant::now();
        unsafe {
            // ── Main process ──────────────────────────────────────────────
            let main_handle = GetCurrentProcess();
            let t_main = std::time::Instant::now();
            if !apply_to_handle(main_handle, enabled) {
                tracing::warn!(
                    "[efficiency] main process FAILED: {}",
                    std::io::Error::last_os_error()
                );
            }
            let main_ms = t_main.elapsed().as_millis();

            // ── WebView2 helpers only ─────────────────────────────────────
            // We deliberately filter the descendant list: only WebView2
            // processes inherit Arbor's efficiency state. User-launched
            // children (cmd.exe / java.exe / mvn / tomcat / integrated
            // terminal shells / …) keep NORMAL_PRIORITY_CLASS and full CPU
            // throttle off — running a Tomcat from a pipeline must not be
            // demoted to IDLE just because the user Alt-Tabbed away from
            // the Arbor window for a moment.
            let my_pid       = GetCurrentProcessId();
            let t_scan       = std::time::Instant::now();
            let descendants  = collect_descendant_processes(my_pid);
            let scan_ms      = t_scan.elapsed().as_millis();
            let access       = PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_SET_INFORMATION;

            let t_apply = std::time::Instant::now();
            let mut applied = 0usize;
            for (pid, name) in &descendants {
                if !is_throttleable(name) { continue; }
                let handle = OpenProcess(access, 0, *pid);
                if handle == 0 {
                    tracing::debug!("[efficiency] OpenProcess({pid} {name}) failed (may be exited)");
                    continue;
                }
                if !apply_to_handle(handle, enabled) {
                    tracing::debug!(
                        "[efficiency] apply to pid {pid} ({name}) failed: {}",
                        std::io::Error::last_os_error()
                    );
                } else {
                    applied += 1;
                }
                CloseHandle(handle);
            }
            tracing::info!(
                target: "arbor::focus",
                "set_efficiency_mode(enabled={enabled}) main={main_ms}ms scan={scan_ms}ms descendants={} applied={} apply={}ms total={}ms",
                descendants.len(),
                applied,
                t_apply.elapsed().as_millis(),
                t_total.elapsed().as_millis(),
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Linux
// ---------------------------------------------------------------------------

#[cfg(target_os = "linux")]
mod imp {
    pub fn set_efficiency_mode(enabled: bool) {
        tracing::info!("[efficiency] set_efficiency_mode(enabled={enabled}) [linux/sched]");
        #[allow(unsafe_code)]
        unsafe {
            let policy = if enabled { libc::SCHED_IDLE } else { libc::SCHED_OTHER };
            let param  = libc::sched_param { sched_priority: 0 };
            let ret    = libc::sched_setscheduler(0, policy, &param);
            if ret != 0 {
                tracing::warn!(
                    "[efficiency] sched_setscheduler({policy}) failed: {}",
                    std::io::Error::last_os_error()
                );
            } else {
                tracing::info!("[efficiency] sched_setscheduler({policy}) OK");
            }
        }
    }
}

// ---------------------------------------------------------------------------
// macOS
// ---------------------------------------------------------------------------

#[cfg(target_os = "macos")]
mod imp {
    pub fn set_efficiency_mode(enabled: bool) {
        tracing::info!("[efficiency] set_efficiency_mode(enabled={enabled}) [macos/nice]");
        #[allow(unsafe_code)]
        unsafe {
            let nice: libc::c_int = if enabled { 10 } else { 0 };
            let ret = libc::setpriority(libc::PRIO_PROCESS, 0, nice);
            if ret != 0 {
                tracing::warn!(
                    "[efficiency] setpriority({nice}) failed: {}",
                    std::io::Error::last_os_error()
                );
            } else {
                tracing::info!("[efficiency] setpriority({nice}) OK");
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Fallback
// ---------------------------------------------------------------------------

#[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "macos")))]
mod imp {
    #[inline(always)]
    pub fn set_efficiency_mode(_enabled: bool) {}
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Enable or disable OS-level power-saving / Efficiency Mode for Arbor and
/// the WebView2 helper processes it owns. **User-launched children** (any
/// process spawned through `arbor.job.spawn`, the pipeline runner, the
/// integrated terminal, plugin VMs, …) are excluded from the throttle set
/// — they always run at NORMAL/no-throttle regardless of Arbor's focus
/// state.  See `is_throttleable` / `THROTTLEABLE_DESCENDANTS` in the
/// Windows impl for the exact filter; on Linux/macOS we only touch the
/// current process so the question doesn't arise.
pub fn set_efficiency_mode(enabled: bool) {
    imp::set_efficiency_mode(enabled);
}
