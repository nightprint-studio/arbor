//! Finding a language server's executable on this machine.
//!
//! The **search** itself is `arbor_process_ext::locate` — a windowed app does not inherit the shell's
//! `PATH`, and that lesson belongs to the platform rather than to this crate; the debug-adapter client
//! searches the same directories in the same order and differs only in what it looks for.
//!
//! What is left here is the part that is genuinely a language server's: the catalogue entry's own
//! preferred and extra directories, and the one check that a resolved file is a server rather than a
//! rustup proxy for a component nobody installed.

use std::path::Path;

use arbor_process_ext::prelude::locate_executable;

use crate::catalogue::ServerSpec;

/// Resolve `spec`'s executable, honouring an explicit `override_path`.
///
/// Returns the absolute path as a string (forward slashes untouched — this is handed to
/// `Command::new`, not to the wire).
pub fn locate(spec: &ServerSpec, override_path: Option<&str>) -> Option<String> {
    let found =
        locate_executable(spec.cmd, override_path, &spec.preferred_dirs(), &spec.extra_dirs())?;
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
    locate_executable(cmd, override_path, &[], &[])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::spec_by_id;
    use std::path::PathBuf;

    // The directory search and its override rules are tested in `arbor_process_ext::locate`, which
    // owns them. What is tested here is the part this module still decides.

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

    /// An override is exempt from the proxy check: naming a binary is a decision, and
    /// second-guessing it would make a deliberate choice un-selectable.
    #[test]
    fn an_absolute_override_is_used_even_though_it_is_not_a_known_server() {
        let f = Fixture::new("abs", "my-analyzer");
        let path = f.dir.join("my-analyzer");
        let found = locate(spec_by_id("rust-analyzer").unwrap(), Some(&path.to_string_lossy()));
        assert_eq!(found.as_deref(), Some(path.to_string_lossy().as_ref()));
    }

    #[test]
    fn an_absolute_override_that_does_not_exist_resolves_to_nothing() {
        let found =
            locate(spec_by_id("rust-analyzer").unwrap(), Some("/nonexistent/rust-analyzer"));
        assert_eq!(found, None);
    }

    #[test]
    fn a_blank_override_is_ignored_rather_than_treated_as_a_path() {
        let spec = spec_by_id("rust-analyzer").unwrap();
        assert_eq!(locate(spec, Some("")), locate(spec, None));
        assert_eq!(locate(spec, Some("   ")), locate(spec, None));
    }

    #[test]
    fn a_missing_command_resolves_to_nothing_rather_than_a_bare_name() {
        assert_eq!(locate_custom("definitely-not-a-language-server-xyz", None), None);
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
}
