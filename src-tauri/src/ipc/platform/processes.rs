//! `processes` domain — what Arbor's own processes are costing this machine.
//!
//! ## Why the shell, and only the shell
//!
//! Arbor is not one process. It is this shell, its WebView, one backend per product, whatever
//! those backends started (language servers, a JVM under test, a `cargo` build) and whatever
//! *those* started. Every one of them is a **descendant of this process**, and nothing else in the
//! system is — so the shell is the only place from which the whole picture can be taken, once,
//! without every product growing a reporting API of its own.
//!
//! That is also why the walk is by parent-child rather than by a registry of pids the shell keeps:
//! a registry would list the processes the shell knows it started and miss exactly the ones worth
//! finding — the language server a backend spawned, the `cargo check` that server is running.
//!
//! ## What it can and cannot do
//!
//! It **measures**. It does not cap: there is no portable way to hold a native process under a
//! memory ceiling — Windows has job objects, Linux has cgroups, macOS has nothing equivalent — and
//! a switch that worked on one platform and quietly did nothing on the others would be worse than
//! no switch. What a screen can honestly do with this is show the cost, say when a process has
//! grown past a line the user drew, and offer the one action that is real: restart it.
//!
//! ## The sampler is kept between calls
//!
//! CPU usage is a *delta*: it is the work done since the previous sample divided by the time
//! between them. A fresh `System` per call has no previous sample, so every row would read 0.0%
//! forever. The sampler is therefore process-wide and long-lived, and the first call after it is
//! created reports no CPU at all — which the frontend renders as "—" rather than as zero, because
//! those are different claims.

use std::collections::{HashMap, HashSet};
use std::sync::{LazyLock, Mutex};

use serde::Serialize;
use sysinfo::{Pid, ProcessRefreshKind, RefreshKind, System};

use crate::error::AppError;
use crate::ipc::platform;
use crate::AppState;

/// The sampler, kept for the life of the process — see the module doc.
static SAMPLER: LazyLock<Mutex<System>> = LazyLock::new(|| {
    Mutex::new(System::new_with_specifics(
        RefreshKind::new().with_processes(ProcessRefreshKind::everything()),
    ))
});

/// One process of Arbor's, as a screen needs it.
#[derive(Debug, Clone, Serialize)]
pub struct ProcessRow {
    pub pid: u32,
    /// The process that started it, within Arbor's own tree. `None` only for the shell.
    pub parent: Option<u32>,
    /// The executable's file stem — `bennu-be`, `rust-analyzer`, `java`.
    pub name: String,
    /// What this process **is to Arbor**, which is a different question from what it is called:
    /// `"shell"`, `"backend"`, `"helper"` (a WebView process) — those three are Arbor itself —
    /// `"language-server"`, or `"child"`: anything else Arbor started, from a run to a terminal's
    /// shell.
    pub kind: String,
    /// The product it belongs to — `"bennu"`, `"corvus"` — inherited from the nearest backend
    /// ancestor, so a language server carries the icon of the product that started it. `None` for
    /// the shell and its own helpers.
    pub product: Option<String>,
    /// Percent of **one** core. A four-core machine can legitimately show 400 across the rows, and
    /// the screen says so rather than dividing — "rust-analyzer is using two cores" is the useful
    /// sentence, and it is the one a per-core number gives.
    pub cpu: f32,
    /// What the process costs in memory, in bytes — see [`memory_of`] for which figure that is.
    pub memory: u64,
    /// How long it has been running, in seconds.
    pub uptime_s: u64,
}

/// The whole picture, in one round-trip.
#[derive(Debug, Clone, Serialize)]
pub struct ProcessReport {
    pub rows: Vec<ProcessRow>,
    /// The sum of every row's CPU, in the same per-core units.
    pub total_cpu: f32,
    /// The sum of every row's memory, in bytes.
    pub total_memory: u64,
    /// How many cores this machine has — what turns a per-core CPU figure into a fraction of the
    /// machine, on the screen rather than here.
    pub cores: usize,
    /// Total physical memory, in bytes.
    pub machine_memory: u64,
    /// Whether CPU figures are real yet. The first sample after startup has nothing to compare
    /// against, and a screen that drew those zeros would be reporting an idle machine that is not.
    pub cpu_sampled: bool,
}

/// Executable stems that are language servers, so a row can say what it is rather than only what
/// it is called.
///
/// A list rather than "anything under a backend", because the other things under a backend — a
/// `java` under test, a `cargo`, an `mvn` — are runs, and a run being expensive is not the same
/// news as a server being expensive: one of them ends on its own.
const LANGUAGE_SERVERS: &[&str] = &[
    "rust-analyzer",
    "typescript-language-server",
    "tsserver",
    "vscode-html-language-server",
    "vscode-css-language-server",
    "vscode-json-language-server",
    "yaml-language-server",
    "lua-language-server",
    "pylsp",
    "pyright-langserver",
    "gopls",
    "jdtls",
    "ngserver",
    "wgsl-analyzer",
    "nd-dig-lsp",
    "taplo",
];

/// Executable stems of the WebView's own processes, per platform — Arbor's interface, as much a
/// part of the application as the shell that hosts it.
///
/// By name, and deliberately not as "anything under the shell that is not under a backend". That
/// rule was the first version, and it filed the integrated terminal's `zsh` and every `git` the
/// shell spawns as part of Arbor itself — so the table that was meant to answer "what is Arbor
/// costing" answered with whatever you happened to be running. On macOS the WebKit processes are
/// XPC services launched outside the app's tree and do not appear at all; that is a fact about the
/// platform, not a row that is missing.
const WEBVIEW_PROCESSES: &[&str] = &[
    // Windows — WebView2.
    "msedgewebview2",
    // Linux — WebKitGTK.
    "WebKitWebProcess",
    "WebKitNetworkProcess",
    "WebKitGPUProcess",
];

/// What a process costs in memory, as the system itself counts it.
///
/// **Not resident size**, which is what `sysinfo` reports and what this screen first showed. On
/// macOS resident size counts every page of the executable and its libraries that happens to be in
/// memory — pages backed by the file on disk, which the system drops whenever it likes and which
/// are shared with every other process mapping the same library. For a backend that doubled the
/// figure: 173 MB resident against a 131 MB footprint. So each platform gets the number its own
/// activity monitor leads with:
///
/// - **macOS**: the physical footprint (`ri_phys_footprint`) — what Activity Monitor's Memory column is.
/// - **Windows**: private bytes — memory committed to this process alone, which is what Task
///   Manager's memory column approximates; the working set shares the same flaw as resident size.
/// - **elsewhere**: resident size, the best figure `sysinfo` has.
#[cfg(target_os = "macos")]
fn memory_of(pid: Pid, proc: &sysinfo::Process) -> u64 {
    // SAFETY: `rusage_info_v2` is plain data, so a zeroed one is valid, and `proc_pid_rusage` with
    // `RUSAGE_INFO_V2` writes exactly one of them through the pointer — which the C API types as
    // `rusage_info_t *` and callers pass the struct's own address as, hence the cast.
    let mut info: libc::rusage_info_v2 = unsafe { std::mem::zeroed() };
    let rc = unsafe {
        libc::proc_pid_rusage(
            pid.as_u32() as libc::c_int,
            libc::RUSAGE_INFO_V2,
            &mut info as *mut libc::rusage_info_v2 as *mut libc::rusage_info_t,
        )
    };
    // A process that exited between the table refresh and this call has no usage to read; its
    // resident size from the refresh is still the last true thing known about it.
    if rc == 0 { info.ri_phys_footprint } else { proc.memory() }
}

#[cfg(windows)]
fn memory_of(_pid: Pid, proc: &sysinfo::Process) -> u64 {
    // `sysinfo` reports `PrivateUsage` as the virtual-memory figure on Windows.
    proc.virtual_memory()
}

#[cfg(not(any(target_os = "macos", windows)))]
fn memory_of(_pid: Pid, proc: &sysinfo::Process) -> u64 {
    proc.memory()
}

/// Whether `name` is one of Arbor's product backends, and which product it serves.
///
/// By the `-be` suffix, which is the naming convention every backend binary follows — so a backend
/// added tomorrow appears on this screen with no line here.
fn backend_product(name: &str) -> Option<&str> {
    name.strip_suffix("-be").filter(|p| !p.is_empty())
}

/// Every process of Arbor's, with what it is costing.
///
/// Includes the shell itself, so the figures add up to something a user can compare against what
/// their activity monitor says — a report that quietly left out the biggest row would be worse
/// than no report.
#[platform::handler(program = "platform")]
fn process_report(_state: &AppState) -> Result<ProcessReport, AppError> {
    let mut sampler = SAMPLER
        .lock()
        .map_err(|_| AppError::MutexPoisoned("processes".into()))?;

    // Whether this is the first look. Read before the refresh, since the refresh is what makes it
    // no longer true.
    let cpu_sampled = !sampler.processes().is_empty();
    sampler.refresh_processes();

    let Some(own) = sysinfo::get_current_pid().ok() else {
        return Err(AppError::Other("this process has no pid".into()));
    };

    // Arbor's own tree: the shell, then everything descended from it. Walked breadth-first from
    // the shell rather than by testing each process's ancestry, which would be a walk up the whole
    // machine's process table per entry.
    let children: HashMap<Pid, Vec<Pid>> =
        sampler.processes().iter().fold(HashMap::new(), |mut acc, (pid, proc)| {
            if let Some(parent) = proc.parent() {
                acc.entry(parent).or_default().push(*pid);
            }
            acc
        });

    let mut rows: Vec<ProcessRow> = Vec::new();
    let mut seen: HashSet<Pid> = HashSet::new();
    // Each entry carries the product it inherited, so a language server two levels under `bennu-be`
    // still says `bennu`.
    let mut queue: Vec<(Pid, Option<String>)> = vec![(own, None)];

    while let Some((pid, product)) = queue.pop() {
        if !seen.insert(pid) {
            continue; // a cycle cannot happen, but a pid reused mid-walk can
        }
        let Some(proc) = sampler.process(pid) else { continue };
        let name = std::path::Path::new(proc.name())
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| proc.name().to_string());

        let mine = backend_product(&name).map(str::to_string);
        let product = mine.clone().or(product);
        let is_backend = mine.is_some();

        let kind = if pid == own {
            "shell"
        } else if is_backend {
            "backend"
        } else if LANGUAGE_SERVERS.contains(&name.as_str()) {
            "language-server"
        } else if WEBVIEW_PROCESSES.contains(&name.as_str()) {
            "helper"
        } else {
            // Everything else Arbor started, directly or through a backend: a run, a build, a
            // terminal's shell, a `git`. Arbor's to have started, not Arbor's to be.
            "child"
        };

        rows.push(ProcessRow {
            pid: pid.as_u32(),
            parent: (pid != own).then(|| proc.parent().map(|p| p.as_u32())).flatten(),
            name,
            kind: kind.to_string(),
            product: product.clone(),
            cpu: proc.cpu_usage(),
            memory: memory_of(pid, proc),
            uptime_s: proc.run_time(),
        });

        for child in children.get(&pid).into_iter().flatten() {
            queue.push((*child, product.clone()));
        }
    }

    // The shell first, then the backends, then everything else — and alphabetically inside each,
    // so a row does not move under the cursor between two refreshes of a screen that polls.
    rows.sort_by(|a, b| {
        fn rank(kind: &str) -> u8 {
            match kind {
                "shell" => 0,
                "backend" => 1,
                "language-server" => 2,
                "child" => 3,
                _ => 4,
            }
        }
        rank(&a.kind)
            .cmp(&rank(&b.kind))
            .then_with(|| a.product.cmp(&b.product))
            .then_with(|| a.name.cmp(&b.name))
            .then_with(|| a.pid.cmp(&b.pid))
    });

    let total_cpu = rows.iter().map(|r| r.cpu).sum();
    let total_memory = rows.iter().map(|r| r.memory).sum();

    Ok(ProcessReport {
        rows,
        total_cpu,
        total_memory,
        cores: std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1),
        machine_memory: sampler.total_memory(),
        cpu_sampled,
    })
}

/// A backend's own account of what its memory is holding — `None` when it does not measure itself.
///
/// Asked for when somebody opens a backend's row in the monitor, never on the sampling tick: a
/// backend answers by walking its own structures, which is fine on demand and wasteful once a
/// second. Only a backend that advertises `__memory` is asked, so one that has no reporter costs a
/// set lookup and answers `None` — which the screen shows as "does not report", a different thing
/// from an empty breakdown.
///
/// Reached through the `rpc` command, which runs every sync handler on the blocking pool, so the
/// framed round-trip to the backend never holds a runtime worker.
#[platform::handler(program = "platform")]
fn process_memory(
    state: &AppState,
    product: String,
) -> Result<Option<arbor_be::prelude::MemoryReport>, AppError> {
    use arbor_be::prelude::MEMORY_METHOD;
    if !crate::ipc::split_broker::serves(&product, MEMORY_METHOD) {
        return Ok(None);
    }
    let value = crate::ipc::dispatch_rpc(state, &product, MEMORY_METHOD, serde_json::Value::Null)?;
    serde_json::from_value(value)
        .map(Some)
        .map_err(|e| AppError::Other(format!("{product} answered {MEMORY_METHOD} with something unreadable: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_backend_is_recognised_by_its_suffix_and_names_its_product() {
        assert_eq!(backend_product("bennu-be"), Some("bennu"));
        assert_eq!(backend_product("corvus-be"), Some("corvus"));
        // Not a backend, and neither is a bare `-be`.
        assert_eq!(backend_product("rust-analyzer"), None);
        assert_eq!(backend_product("-be"), None);
    }
}
