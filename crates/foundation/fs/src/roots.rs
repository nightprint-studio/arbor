//! Quick-access roots (user dirs + drives + WSL distros) and per-drive storage
//! usage for the Overview dashboard.

use crate::entry::{DriveUsage, FsRoot, OverviewStats};

#[cfg(target_os = "windows")]
fn enumerate_drives() -> Vec<FsRoot> {
    use windows_sys::Win32::Storage::FileSystem::GetLogicalDrives;

    let mut drives = Vec::new();
    // GetLogicalDrives returns a bitmask: bit 0 = A:, bit 1 = B:, …, bit 25 = Z:
    // It's a single fast Win32 call that reads from the system without probing
    // each drive — replacing the old A..Z + Path::exists() loop which blocked
    // for several seconds per unavailable removable/CD drive.
    let mask = unsafe { GetLogicalDrives() };
    if mask == 0 { return drives; }

    for i in 0..26 {
        if mask & (1u32 << i) != 0 {
            let letter = (b'A' + i as u8) as char;
            drives.push(FsRoot {
                name: format!("{letter}:"),
                path: format!("{letter}:\\"),
                kind: "drive".to_string(),
            });
        }
    }
    drives
}

fn list_fs_roots_blocking() -> Vec<FsRoot> {
    let mut roots: Vec<FsRoot> = Vec::new();

    // ── Common user directories ───────────────────────────────────────────
    let common = [
        (dirs::home_dir(),      "Home",      "home"),
        (dirs::desktop_dir(),   "Desktop",   "desktop"),
        (dirs::document_dir(),  "Documents", "documents"),
        (dirs::download_dir(),  "Downloads", "downloads"),
    ];

    for (opt, name, kind) in common {
        if let Some(p) = opt {
            if p.exists() {
                roots.push(FsRoot {
                    name: name.to_string(),
                    path: p.to_string_lossy().to_string(),
                    kind: kind.to_string(),
                });
            }
        }
    }

    // ── Platform-specific drives / root ───────────────────────────────────
    #[cfg(target_os = "windows")]
    {
        roots.extend(enumerate_drives());
    }

    #[cfg(not(target_os = "windows"))]
    {
        roots.push(FsRoot {
            name: "File System".to_string(),
            path: "/".to_string(),
            kind: "drive".to_string(),
        });
    }

    roots
}

/// Filesystem quick-access roots: common user dirs followed by available drive
/// letters (Windows) or `/` (other platforms).
pub fn list_roots() -> Vec<FsRoot> {
    list_fs_roots_blocking()
}

// ── WSL distributions (Windows) — mounted under \\wsl.localhost\<distro> ─────

/// Enumerate installed WSL distributions via `wsl.exe --list --quiet` and map
/// each to its `\\wsl.localhost\<distro>` UNC root (browsable like any other
/// path). `wsl.exe` prints UTF-16LE, so we decode accordingly. Returns empty
/// when WSL isn't installed (the command fails) or off-Windows.
#[cfg(windows)]
fn enumerate_wsl() -> Vec<FsRoot> {
    use arbor_process_ext::prelude::NoWindowExt;
    let Ok(out) = std::process::Command::new("wsl.exe")
        .args(["--list", "--quiet"])
        .no_window()
        .output()
    else {
        return Vec::new();
    };
    if !out.status.success() {
        return Vec::new();
    }
    // Decode UTF-16LE (skip a leading BOM if present).
    let u16s: Vec<u16> = out.stdout.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
    let text = String::from_utf16_lossy(&u16s);
    text.lines()
        .map(|l| l.trim().trim_matches('\u{0}').trim_matches('\u{feff}').trim())
        .filter(|n| !n.is_empty())
        .map(|name| FsRoot {
            name: name.to_string(),
            path: format!(r"\\wsl.localhost\{name}"),
            kind: "wsl".to_string(),
        })
        .collect()
}

#[cfg(not(windows))]
fn enumerate_wsl() -> Vec<FsRoot> {
    Vec::new()
}

/// Installed WSL distributions as navigable roots. The shell loads this once
/// (not on the removable-media poll) since spawning `wsl.exe` is wasteful.
pub fn list_wsl_distros() -> Vec<FsRoot> {
    enumerate_wsl()
}

// ── Overview dashboard — real storage stats per drive ───────────────────────

#[cfg(windows)]
fn disk_free_total(path: &str) -> Option<(u64, u64)> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;
    let wide: Vec<u16> = std::ffi::OsStr::new(path)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    let mut free_avail: u64 = 0;
    let mut total: u64 = 0;
    let mut total_free: u64 = 0;
    let ok = unsafe { GetDiskFreeSpaceExW(wide.as_ptr(), &mut free_avail, &mut total, &mut total_free) };
    if ok != 0 { Some((free_avail, total)) } else { None }
}

#[cfg(not(windows))]
fn disk_free_total(path: &str) -> Option<(u64, u64)> {
    // No std API for free space, so shell out to `df` (present on both Linux and
    // macOS) rather than pull in a new crate. `-P` forces single-line POSIX
    // output, `-k` reports 1024-byte blocks. Columns:
    //   Filesystem  1024-blocks  Used  Available  Capacity  Mounted-on
    let out = std::process::Command::new("df")
        .args(["-k", "-P", path])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&out.stdout);
    let line = text.lines().nth(1)?; // skip the header row
    let cols: Vec<&str> = line.split_whitespace().collect();
    if cols.len() < 4 {
        return None;
    }
    let total_kb: u64 = cols[1].parse().ok()?;
    let avail_kb: u64 = cols[3].parse().ok()?;
    Some((avail_kb.saturating_mul(1024), total_kb.saturating_mul(1024)))
}

/// Real Overview dashboard stats: capacity / free space per drive (`None` on
/// platforms without a std API). The frontend renders the per-drive usage bars
/// and the aggregate capacity / free / used figures from this.
pub fn overview_stats() -> OverviewStats {
    let drives = list_fs_roots_blocking()
        .into_iter()
        .filter(|r| r.kind == "drive")
        .map(|r| {
            let (free, total) = match disk_free_total(&r.path) {
                Some((f, t)) => (Some(f), Some(t)),
                None => (None, None),
            };
            DriveUsage { name: r.name, path: r.path, total, free }
        })
        .collect::<Vec<_>>();
    let total_capacity = drives.iter().filter_map(|d| d.total).sum();
    let total_free     = drives.iter().filter_map(|d| d.free).sum();
    OverviewStats { drives, total_capacity, total_free }
}
