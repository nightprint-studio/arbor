//! Finding a tool's executable on this machine.
//!
//! ## Why this is a platform concern and not each caller's
//!
//! A windowed app does not inherit the shell's `PATH`. On macOS, launchd hands a GUI process
//! `/usr/bin:/bin:/usr/sbin:/sbin` and nothing else — so `~/.cargo/bin` (rustup), `~/go/bin`
//! (`go install`), `/opt/homebrew/bin` (Apple-silicon Homebrew) and npm's global prefix are all
//! invisible to a `PATH` lookup, even though the user's terminal finds them instantly. A caller that
//! only consults `PATH` reports "not installed" to somebody who installed it months ago.
//!
//! Several parts of Arbor learned that the hard way and each of them worked it out again. It lives
//! here now because the knowledge is about **this platform**, not about what is being looked for: the
//! language server client and the debug adapter client search the same directories in the same order
//! and differ only in what they are looking for and where else it might be.
//!
//! ## The order, and why
//!
//! 1. an **absolute path override** — the caller's user said exactly where it is;
//! 2. the caller's **preferred** directories, which beat `PATH`;
//! 3. **`PATH`**, scanned in-process (no `which` subprocess: it would be a spawn per lookup, and on
//!    Windows there is no `which` to spawn);
//! 4. the **generic** places a binary of any kind lands on this platform;
//! 5. the caller's **extra** directories.
//!
//! Step 2 exists for a failure that is invisible without it. `~/.cargo/bin/rust-analyzer` is normally
//! a **rustup proxy** rather than a binary: it is present whether or not the component is installed,
//! and `~/.cargo/bin` is very often on `PATH`. Resolving it looks like success — the file exists, the
//! spawn works — and then the process dies immediately with `Unknown binary … in official toolchain`.
//! Letting a real binary be looked for *before* `PATH` is what makes an installed component win from
//! wherever it is. The same shape applies to a debug adapter shipped inside a VS Code extension.

use std::path::{Path, PathBuf};

pub fn locate_executable(
    cmd: &str,
    override_path: Option<&str>,
    preferred: &[PathBuf],
    extra: &[PathBuf],
) -> Option<String> {
    // 1. An explicit override wins outright — including over a copy on `PATH`. The user
    //    pointing at a specific build is a decision, not a hint.
    if let Some(p) = override_path.map(str::trim).filter(|p| !p.is_empty()) {
        let path = Path::new(p);
        if path.is_absolute() {
            return path.is_file().then(|| p.to_string());
        }
        // A relative override is treated as a command name to look up, which is what
        // somebody typing `rust-analyzer-nightly` into the box means.
        return locate_executable(p, None, preferred, extra);
    }

    // An absolute command from a catalogue or a config needs no searching.
    if Path::new(cmd).is_absolute() {
        return Path::new(cmd).is_file().then(|| cmd.to_string());
    }

    // 2. The caller's own preferred locations, ahead of PATH — see the module docs.
    if let Some(hit) = preferred.iter().find_map(|dir| executable_in(dir, cmd)) {
        return Some(hit);
    }

    // 3. PATH.
    if let Some(hit) = std::env::var_os("PATH")
        .into_iter()
        .flat_map(|paths| std::env::split_paths(&paths).collect::<Vec<_>>())
        .find_map(|dir| executable_in(&dir, cmd))
    {
        return Some(hit);
    }

    // 4 + 5. The other places the binaries actually are.
    generic_bin_dirs()
        .into_iter()
        .chain(extra.iter().cloned())
        .find_map(|dir| executable_in(&dir, cmd))
}

/// `dir/cmd` when it exists and looks runnable.
///
/// On Windows a command name carries no extension, so each of `PATHEXT`'s usual suspects is
/// tried — a tool installed by npm is a `.cmd` shim and nothing else, and looking only for the
/// bare name finds nothing.
pub fn executable_in(dir: &Path, cmd: &str) -> Option<String> {
    let direct = dir.join(cmd);
    if direct.is_file() {
        return Some(direct.to_string_lossy().to_string());
    }
    #[cfg(target_os = "windows")]
    {
        for ext in ["exe", "cmd", "bat", "com", "ps1"] {
            let candidate = dir.join(format!("{cmd}.{ext}"));
            if candidate.is_file() {
                return Some(candidate.to_string_lossy().to_string());
            }
        }
    }
    None
}

/// Directories any tool might be installed into on this platform.
///
/// Ordered by how likely a *deliberate* install is to be there: a Homebrew binary before a
/// distro one, the user's own `~/.local/bin` before a snap.
pub fn generic_bin_dirs() -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = Vec::new();

    #[cfg(not(target_os = "windows"))]
    {
        for p in [
            "/opt/homebrew/bin", // Homebrew on Apple silicon — not on a GUI app's PATH
            "/usr/local/bin",    // Homebrew on Intel, and the conventional manual install
            "/opt/local/bin",    // MacPorts
            "/usr/bin",
            "/snap/bin",
        ] {
            dirs.push(PathBuf::from(p));
        }
    }

    if let Some(home) = arbor_core::prelude::user_home() {
        dirs.push(home.join(".local").join("bin"));
        dirs.push(home.join("bin"));
        // npm's default global prefix on Windows, and a common one elsewhere.
        dirs.push(home.join(".npm-global").join("bin"));
        #[cfg(target_os = "windows")]
        {
            dirs.push(home.join("AppData").join("Roaming").join("npm"));
            dirs.push(home.join("scoop").join("shims"));
        }
    }
    #[cfg(target_os = "windows")]
    {
        if let Some(pf) = std::env::var_os("ProgramFiles") {
            dirs.push(PathBuf::from(pf).join("LLVM").join("bin"));
        }
        if let Some(data) = std::env::var_os("ChocolateyInstall") {
            dirs.push(PathBuf::from(data).join("bin"));
        }
    }
    dirs.retain(|d| d.is_dir());
    dirs
}


#[cfg(test)]
mod tests {
    use super::*;

    /// A directory holding one fake executable, cleaned up on drop.
    struct Fixture {
        dir: PathBuf,
    }

    impl Fixture {
        fn new(tag: &str, name: &str) -> Self {
            let dir =
                std::env::temp_dir().join(format!("arbor-locate-{tag}-{}", std::process::id()));
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(dir.join(name), b"#!/bin/sh\n").unwrap();
            Self { dir }
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }

    #[test]
    fn a_directory_scan_finds_the_binary_in_it() {
        // The unit under every step of the search. Deliberately NOT tested by setting `PATH`: cargo
        // runs tests as threads in one process, so mutating a process-global would leak into
        // whichever other test read it at the wrong moment.
        let f = Fixture::new("dir", "fake-tool");
        assert!(executable_in(&f.dir, "fake-tool").is_some());
        assert!(executable_in(&f.dir, "not-there").is_none());
        assert!(executable_in(Path::new("/definitely/not/a/dir"), "fake-tool").is_none());
    }

    #[test]
    fn an_absolute_override_is_used_when_it_exists() {
        let f = Fixture::new("abs", "my-tool");
        let path = f.dir.join("my-tool");
        let found = locate_executable("whatever", Some(&path.to_string_lossy()), &[], &[]);
        assert_eq!(found.as_deref(), Some(path.to_string_lossy().as_ref()));
    }

    #[test]
    fn an_absolute_override_that_does_not_exist_resolves_to_nothing() {
        // Rather than silently falling back to a copy on PATH: the user named a specific build, and
        // quietly running a different one is worse than reporting it missing.
        assert_eq!(locate_executable("ls", Some("/nonexistent/my-tool"), &[], &[]), None);
    }

    #[test]
    fn a_blank_override_is_ignored_rather_than_treated_as_a_path() {
        // A settings field starts empty and round-trips through TOML as `""`.
        let f = Fixture::new("blank", "fake-tool");
        let dirs = [f.dir.clone()];
        let baseline = locate_executable("fake-tool", None, &dirs, &[]);
        assert!(baseline.is_some());
        assert_eq!(locate_executable("fake-tool", Some(""), &dirs, &[]), baseline);
        assert_eq!(locate_executable("fake-tool", Some("   "), &dirs, &[]), baseline);
    }

    #[test]
    fn a_relative_override_is_looked_up_as_a_command_name() {
        // What somebody typing `rust-analyzer-nightly` into a settings box means: a name to find,
        // not a path relative to whatever the working directory happens to be.
        let f = Fixture::new("rel", "nightly-tool");
        let found = locate_executable("fake-tool", Some("nightly-tool"), &[f.dir.clone()], &[]);
        assert!(found.is_some_and(|p| p.ends_with("nightly-tool")));
    }

    #[test]
    fn a_missing_command_resolves_to_nothing_rather_than_a_bare_name() {
        // Returning the bare name would make the tool look "available" and then fail at spawn — the
        // exact failure the IDE detection had on macOS.
        assert_eq!(locate_executable("definitely-not-a-tool-xyz", None, &[], &[]), None);
    }

    #[test]
    fn a_preferred_dir_wins_over_everything_but_an_override() {
        // The rustup-proxy case: `~/.cargo/bin/rust-analyzer` exists whether or not the component is
        // installed and is often on PATH, so a PATH-first search resolves a file that spawns and then
        // dies with "Unknown binary". The real binary has to win.
        let preferred = Fixture::new("pref", "fake-tool");
        let other = Fixture::new("other", "fake-tool");
        let found =
            locate_executable("fake-tool", None, &[preferred.dir.clone()], &[other.dir.clone()])
                .expect("found");
        assert!(
            found.starts_with(preferred.dir.to_string_lossy().as_ref()),
            "expected the preferred dir, got {found}"
        );

        // …but an explicit override still beats it: the user naming a build is a decision.
        let override_path = other.dir.join("fake-tool");
        let found = locate_executable(
            "fake-tool",
            Some(&override_path.to_string_lossy()),
            &[preferred.dir.clone()],
            &[],
        );
        assert_eq!(found.as_deref(), Some(override_path.to_string_lossy().as_ref()));
    }

    #[test]
    fn an_absolute_command_needs_no_searching() {
        let f = Fixture::new("cmd", "fake-tool");
        let abs = f.dir.join("fake-tool");
        assert_eq!(
            locate_executable(&abs.to_string_lossy(), None, &[], &[]).as_deref(),
            Some(abs.to_string_lossy().as_ref()),
        );
        assert_eq!(locate_executable("/nope/fake-tool", None, &[], &[]), None);
    }

    #[test]
    fn the_generic_dirs_are_all_absolute_and_exist() {
        for d in generic_bin_dirs() {
            assert!(d.is_absolute(), "{d:?} is not absolute");
            assert!(d.is_dir(), "{d:?} was returned but is not a directory");
        }
    }
}
