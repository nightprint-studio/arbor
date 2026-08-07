//! Finding a language server's executable on this machine.
//!
//! The same lesson three other parts of Arbor learned the hard way, applied up front: a
//! windowed app does not inherit the shell's `PATH`. On macOS, launchd hands a GUI process
//! `/usr/bin:/bin:/usr/sbin:/sbin` and nothing else — so `~/.cargo/bin` (rustup), `~/go/bin`
//! (`go install`), `/opt/homebrew/bin` (Apple-silicon Homebrew) and npm's global prefix are
//! all invisible to a `PATH` lookup, even though the user's terminal finds them instantly.
//! A client that only consults `PATH` reports "rust-analyzer is not installed" to somebody
//! who installed it months ago.
//!
//! So the search is five steps, in order of authority:
//!
//! 1. an **absolute path override** from the config — the user said exactly where it is;
//! 2. the server's **preferred** directories ([`ServerSpec::preferred_dirs`]), which beat
//!    `PATH` — see below for the one case that needs this;
//! 3. **`PATH`**, scanned in-process (no `which` subprocess: it would be one spawn per
//!    server per detection pass, and on Windows there is no `which` to spawn);
//! 4. the **generic** places a binary of any kind lands on this platform;
//! 5. the **server-specific** places, from [`ServerSpec::extra_dirs`].
//!
//! Step 2 exists for a failure that is invisible without it. `~/.cargo/bin/rust-analyzer` is
//! normally a **rustup proxy** rather than a binary: it is present whether or not the component
//! is installed, and `~/.cargo/bin` is very often on `PATH`. Resolving it looks like success —
//! the file exists, the spawn works — and then the process dies immediately with
//! `Unknown binary 'rust-analyzer' in official toolchain`. Letting the toolchain's real binary
//! be looked for *before* `PATH` is what makes an installed component win from wherever it is.

use std::path::{Path, PathBuf};

use crate::catalogue::ServerSpec;

/// Resolve `spec`'s executable, honouring an explicit `override_path`.
///
/// Returns the absolute path as a string (forward slashes untouched — this is handed to
/// `Command::new`, not to the wire).
pub fn locate(spec: &ServerSpec, override_path: Option<&str>) -> Option<String> {
    let found = locate_command(spec.cmd, override_path, &spec.preferred_dirs(), &spec.extra_dirs())?;
    // A resolved file is not necessarily a runnable server — see [`ServerSpec::accepts`], which
    // rejects rustup's proxy for a component nobody installed. Reporting "not found" there is the
    // truth, and it arrives with the command that fixes it instead of as a dead process.
    //
    // An explicit override is exempt: the user naming a binary is a decision, and second-guessing
    // it would make a deliberate choice un-selectable.
    if override_path.map(str::trim).is_some_and(|p| !p.is_empty()) {
        return Some(found);
    }
    spec.accepts(Path::new(&found)).then_some(found)
}

/// [`locate`] for a command that has no catalogue entry — a user-defined server from the
/// `[[lsp.servers]]` config, which supplies its own command and nothing else.
pub fn locate_custom(cmd: &str, override_path: Option<&str>) -> Option<String> {
    locate_command(cmd, override_path, &[], &[])
}

fn locate_command(
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
        return locate_command(p, None, preferred, extra);
    }

    // An absolute command in the catalogue/config needs no searching.
    if Path::new(cmd).is_absolute() {
        return Path::new(cmd).is_file().then(|| cmd.to_string());
    }

    // 2. The server's own preferred locations, ahead of PATH — see the module docs.
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
/// tried — a server installed by npm is a `.cmd` shim and nothing else, and looking only for
/// the bare name finds nothing.
fn executable_in(dir: &Path, cmd: &str) -> Option<String> {
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
fn generic_bin_dirs() -> Vec<PathBuf> {
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
    use crate::catalogue::spec_by_id;

    /// A directory holding one fake executable, cleaned up on drop.
    struct Fixture {
        dir: PathBuf,
    }

    impl Fixture {
        fn new(tag: &str, name: &str) -> Self {
            let dir = std::env::temp_dir()
                .join(format!("bennu-lsp-disco-{tag}-{}", std::process::id()));
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
    fn an_absolute_override_is_used_when_it_exists() {
        let f = Fixture::new("abs", "my-analyzer");
        let path = f.dir.join("my-analyzer");
        let found = locate(
            spec_by_id("rust-analyzer").unwrap(),
            Some(&path.to_string_lossy()),
        );
        assert_eq!(found.as_deref(), Some(path.to_string_lossy().as_ref()));
    }

    #[test]
    fn an_absolute_override_that_does_not_exist_resolves_to_nothing() {
        // Rather than silently falling back to a copy on PATH: the user named a specific
        // build, and quietly running a different one is worse than reporting it missing.
        let found = locate(
            spec_by_id("rust-analyzer").unwrap(),
            Some("/nonexistent/rust-analyzer"),
        );
        assert_eq!(found, None);
    }

    #[test]
    fn a_blank_override_is_ignored_rather_than_treated_as_a_path() {
        // The settings field starts empty and round-trips through TOML as `""`.
        let spec = spec_by_id("rust-analyzer").unwrap();
        assert_eq!(locate(spec, Some("")), locate(spec, None));
        assert_eq!(locate(spec, Some("   ")), locate(spec, None));
    }

    #[test]
    fn a_directory_scan_finds_the_binary_in_it() {
        // The unit under every step of the search. Deliberately NOT tested by setting
        // `PATH`: cargo runs tests as threads in one process, so mutating a process-global
        // would leak into whichever other test read it at the wrong moment.
        let f = Fixture::new("dir", "fake-ls");
        assert!(executable_in(&f.dir, "fake-ls").is_some());
        assert!(executable_in(&f.dir, "not-there").is_none());
        assert!(executable_in(Path::new("/definitely/not/a/dir"), "fake-ls").is_none());
    }

    #[test]
    fn a_missing_command_resolves_to_nothing_rather_than_a_bare_name() {
        // Returning the bare name would make the server look "available" and then fail at
        // spawn — the exact failure the IDE detection had on macOS.
        assert_eq!(locate_custom("definitely-not-a-language-server-xyz", None), None);
    }

    #[test]
    fn a_preferred_dir_wins_over_everything_but_an_override() {
        // The rustup-proxy case: `~/.cargo/bin/rust-analyzer` exists whether or not the component
        // is installed and is often on PATH, so a PATH-first search resolves a file that spawns
        // and then dies with "Unknown binary". The toolchain's real binary has to win.
        let preferred = Fixture::new("pref", "fake-ls");
        let other = Fixture::new("other", "fake-ls");
        let found = locate_command("fake-ls", None, &[preferred.dir.clone()], &[other.dir.clone()])
            .expect("found");
        assert!(
            found.starts_with(preferred.dir.to_string_lossy().as_ref()),
            "expected the preferred dir, got {found}"
        );

        // …but an explicit override still beats it: the user naming a build is a decision.
        let override_path = other.dir.join("fake-ls");
        let found = locate_command(
            "fake-ls",
            Some(&override_path.to_string_lossy()),
            &[preferred.dir.clone()],
            &[],
        );
        assert_eq!(found.as_deref(), Some(override_path.to_string_lossy().as_ref()));
    }

    #[test]
    fn rust_analyzer_prefers_the_toolchain_over_the_cargo_proxy() {
        // Stated as an ordering invariant rather than by resolving a real binary, so it holds on a
        // machine with no Rust toolchain at all.
        let spec = spec_by_id("rust-analyzer").unwrap();
        for dir in spec.preferred_dirs() {
            assert!(
                dir.components().any(|c| c.as_os_str() == "toolchains"),
                "a preferred dir must be a rustup toolchain bin, got {dir:?}"
            );
        }
        assert!(
            spec.extra_dirs().iter().any(|d| d.ends_with("bin")),
            "the cargo bin dir stays reachable — a `cargo install`ed one is a real binary"
        );
    }

    #[test]
    fn the_generic_dirs_are_all_absolute_and_exist() {
        for d in generic_bin_dirs() {
            assert!(d.is_absolute(), "{d:?} is not absolute");
            assert!(d.is_dir(), "{d:?} was returned but is not a directory");
        }
    }
}
