//! Which debug adapter to run, and where it is.
//!
//! The **search** is `arbor_process_ext::locate` — a windowed app does not inherit the shell's
//! `PATH`, and that lesson belongs to the platform. What is here is the part that is a debug
//! adapter's: which adapters can debug a Rust binary, in what order of preference, what each one
//! needs on its command line, and the places each one is actually installed.
//!
//! ## Why three, and why in this order
//!
//! Nothing ships a debug adapter with the Rust toolchain, so a machine has whichever one its owner
//! installed for some other reason. All three below drive the same native debugger underneath and
//! differ in what they add on top:
//!
//! 1. **`codelldb`** — the VS Code Rust extension's adapter. Preferred because it is the one with
//!    Rust *data formatters*: a `Vec<T>` shows its elements, an `Option` shows `Some(3)` rather than
//!    a discriminant and a union, a `String` shows its text. Without those a debugger is technically
//!    working and practically unusable on real Rust values.
//! 2. **`lldb-dap`** — LLVM's own, shipped with recent LLVM and with Xcode's toolchain. It ships no
//!    Rust formatters, but the *toolchain* does and they can be imported into it at launch, which is
//!    what [`crate::rendering`] exists for. Present on most macOS machines without installing
//!    anything, so in practice it is the one that runs.
//! 3. **`gdb`** — in DAP mode (`--interpreter=dap`, GDB 14+). The fallback for Linux machines with no
//!    LLVM. GDB's Rust support is real but its DAP mode is the newest of the three.
//!
//! The order is preference, not capability: each is tried and the first one **present** wins. A
//! project can pin one, which then wins outright — see [`resolve`].
//!
//! ## Why `codelldb` is looked for in the extension directory
//!
//! It is distributed as a VS Code extension and is almost never on `PATH`. The binary lives at
//! `~/.vscode/extensions/vadimcn.vscode-lldb-<version>/adapter/codelldb`, where `<version>` varies —
//! so the directory has to be *scanned*, not guessed, which is the one thing this module does that
//! the platform search cannot do for it.

use std::path::PathBuf;

use arbor_process_ext::prelude::locate_executable;

/// The debugger underneath the adapter.
///
/// Not cosmetic: it decides how Rust's own types come to be rendered. An LLDB can have the
/// toolchain's formatters imported into it; a GDB auto-loads the printers the binary names and has no
/// hook in its DAP mode to import anything. See [`crate::rendering`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Engine {
    Lldb,
    Gdb,
}

/// One adapter Bennu knows how to drive.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AdapterSpec {
    /// Stable id — what a project pins, and what the frontend shows.
    pub id: &'static str,
    /// What a user should see. Not the id: `lldb-dap` is a file name, "LLDB (lldb-dap)" is a name.
    pub label: &'static str,
    /// The executable to look for.
    pub cmd: &'static str,
    /// Arguments that put it in DAP-over-stdio mode. Empty when that is its only mode.
    pub args: &'static [&'static str],
    /// Whether it ships Rust's own formatters — a `Vec` as its elements rather than as a pointer and
    /// a length. The single biggest difference between these adapters in practice, so it is carried
    /// rather than implied. What a machine actually gets is [`crate::rendering::rendering`], which
    /// also accounts for the toolchain's formatters being importable into an adapter that ships none.
    pub rust_formatters: bool,
    /// The debugger underneath — what decides how Rust rendering can be fixed when it is missing.
    pub engine: Engine,
}

/// Every adapter, in order of preference. See the module docs for why this order.
pub const ADAPTERS: &[AdapterSpec] = &[
    AdapterSpec {
        id: "codelldb",
        label: "CodeLLDB",
        cmd: "codelldb",
        // It speaks DAP on stdio by default; `--port` is the alternative and we do not use it.
        args: &[],
        rust_formatters: true,
        engine: Engine::Lldb,
    },
    AdapterSpec {
        id: "lldb-dap",
        label: "LLDB (lldb-dap)",
        cmd: "lldb-dap",
        args: &[],
        rust_formatters: false,
        engine: Engine::Lldb,
    },
    AdapterSpec {
        id: "gdb",
        label: "GDB (DAP mode)",
        cmd: "gdb",
        // GDB 14 and later. On an older one this exits immediately with a usage error, which is
        // reported as the adapter failing to start rather than being silently absent.
        args: &["--interpreter=dap"],
        rust_formatters: false,
        engine: Engine::Gdb,
    },
];

/// The adapter with this id, if it is one we know.
pub fn spec_by_id(id: &str) -> Option<&'static AdapterSpec> {
    ADAPTERS.iter().find(|a| a.id == id)
}

impl AdapterSpec {
    /// Directories to search **before** `PATH`.
    ///
    /// For `codelldb` this is where the search actually succeeds: it is a VS Code extension, its
    /// directory carries the version in its name, and it is not on `PATH`. The newest version wins,
    /// because the extension directory accumulates them and an old one is a debugger with old
    /// formatters.
    pub fn preferred_dirs(&self) -> Vec<PathBuf> {
        if self.id != "codelldb" {
            return Vec::new();
        }
        let Some(home) = arbor_core::prelude::user_home() else { return Vec::new() };
        let mut dirs: Vec<PathBuf> = Vec::new();
        // Both the stable and the Insiders extension roots, plus the VSCodium fork — the extension
        // is the same one and people run all three.
        for root in [".vscode", ".vscode-insiders", ".vscode-oss", ".vscodium"] {
            let extensions = home.join(root).join("extensions");
            let Ok(entries) = std::fs::read_dir(&extensions) else { continue };
            let mut versions: Vec<PathBuf> = entries
                .flatten()
                .map(|e| e.path())
                .filter(|p| {
                    p.file_name()
                        .and_then(|n| n.to_str())
                        .is_some_and(|n| n.starts_with("vadimcn.vscode-lldb"))
                })
                .map(|p| p.join("adapter"))
                .filter(|p| p.is_dir())
                .collect();
            // Lexicographic, which orders these correctly in practice: the suffix is a semver whose
            // parts are zero-padded to the same width by the marketplace.
            versions.sort();
            versions.reverse();
            dirs.extend(versions);
        }
        dirs
    }

    /// Directories to search **after** `PATH` and the platform's generic ones.
    pub fn extra_dirs(&self) -> Vec<PathBuf> {
        let mut dirs: Vec<PathBuf> = Vec::new();
        if self.id == "lldb-dap" {
            // Xcode's toolchain, which is on no GUI app's PATH. Homebrew's versioned LLVM kegs are
            // not symlinked into `/opt/homebrew/bin` either, so each is named.
            for p in [
                "/Applications/Xcode.app/Contents/Developer/usr/bin",
                "/Library/Developer/CommandLineTools/usr/bin",
            ] {
                dirs.push(PathBuf::from(p));
            }
            for major in (15..=21).rev() {
                dirs.push(PathBuf::from(format!("/opt/homebrew/opt/llvm@{major}/bin")));
                dirs.push(PathBuf::from(format!("/usr/local/opt/llvm@{major}/bin")));
                dirs.push(PathBuf::from(format!("/usr/lib/llvm-{major}/bin")));
            }
            dirs.push(PathBuf::from("/opt/homebrew/opt/llvm/bin"));
            dirs.push(PathBuf::from("/usr/local/opt/llvm/bin"));
        }
        dirs.retain(|d| d.is_dir());
        dirs
    }

    /// Where this adapter's executable is, or `None`.
    pub fn locate(&self, override_path: Option<&str>) -> Option<String> {
        locate_executable(self.cmd, override_path, &self.preferred_dirs(), &self.extra_dirs())
    }
}

/// An adapter that is actually on this machine, ready to spawn.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Adapter {
    pub spec: &'static AdapterSpec,
    /// Absolute path to the executable.
    pub path: String,
}

impl Adapter {
    /// The command line: the executable plus whatever puts it in DAP mode.
    pub fn command(&self) -> (String, Vec<String>) {
        (self.path.clone(), self.spec.args.iter().map(|s| s.to_string()).collect())
    }

    /// How this adapter will render Rust values **on this machine** — see
    /// [`crate::rendering::RustRendering`], and its `caveat` for the sentence to show when they will
    /// read badly.
    pub fn rendering(&self) -> crate::rendering::RustRendering {
        crate::rendering::rendering(self.spec)
    }
}

/// Pick an adapter: the pinned one if it is present, else the first one that is.
///
/// `pinned` is an adapter id from the project's config, and `override_path` an explicit executable
/// for it. A pinned adapter that is **not** found resolves to `None` rather than falling through to
/// another one: the user chose it, and silently debugging with a different adapter — with different
/// value rendering and different stepping behaviour — is worse than saying it is missing.
pub fn resolve(pinned: Option<&str>, override_path: Option<&str>) -> Option<Adapter> {
    if let Some(id) = pinned.map(str::trim).filter(|s| !s.is_empty()) {
        let spec = spec_by_id(id)?;
        let path = spec.locate(override_path)?;
        return Some(Adapter { spec, path });
    }
    // Unpinned: preference order, first one present. The override is deliberately not applied here —
    // an executable path means nothing without knowing which adapter it is.
    ADAPTERS.iter().find_map(|spec| spec.locate(None).map(|path| Adapter { spec, path }))
}

/// Every adapter with whether it was found — what a settings page lists.
///
/// Returned whole rather than filtered, because "CodeLLDB: not installed" beside "LLDB: found" is the
/// answer to *why* a session used the one it did, and a list of only what is present cannot say it.
pub fn survey() -> Vec<(&'static AdapterSpec, Option<String>)> {
    ADAPTERS.iter().map(|spec| (spec, spec.locate(None))).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_preference_order_puts_the_adapter_with_rust_formatters_first() {
        // Not cosmetic: without Rust formatters a `Vec<T>` shows as a pointer and a length, which is
        // a debugger that technically works and practically does not.
        assert_eq!(ADAPTERS[0].id, "codelldb");
        assert!(ADAPTERS[0].rust_formatters);
        assert!(ADAPTERS.iter().skip(1).all(|a| !a.rust_formatters));
        // …and every LLDB-backed adapter that ships none can have the toolchain's imported into it,
        // which is what `rendering` is for.
        assert!(ADAPTERS.iter().any(|a| a.engine == Engine::Lldb && !a.rust_formatters));
    }

    #[test]
    fn every_adapter_has_a_distinct_id_and_a_label_that_is_not_the_id() {
        let mut ids: Vec<&str> = ADAPTERS.iter().map(|a| a.id).collect();
        let count = ids.len();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), count, "ids are what a project pins — they must be unique");
        for a in ADAPTERS {
            assert!(!a.label.is_empty());
            assert_ne!(a.label, a.id, "a file name is not a name a user should be shown");
            assert!(!a.cmd.is_empty());
        }
    }

    #[test]
    fn gdb_is_asked_for_dap_mode_and_the_others_need_no_argument() {
        assert_eq!(spec_by_id("gdb").unwrap().args, &["--interpreter=dap"]);
        assert!(spec_by_id("codelldb").unwrap().args.is_empty());
        assert!(spec_by_id("lldb-dap").unwrap().args.is_empty());
    }

    #[test]
    fn an_unknown_id_is_not_an_adapter() {
        assert!(spec_by_id("nope").is_none());
        assert!(spec_by_id("").is_none());
    }

    /// The whole point of pinning: a session must not silently use a different debugger, because the
    /// value rendering and the stepping behaviour differ between them.
    #[test]
    fn a_pinned_adapter_that_is_missing_does_not_fall_through_to_another() {
        // A pinned id with an override pointing nowhere: `None`, even on a machine that has lldb-dap.
        assert_eq!(resolve(Some("codelldb"), Some("/nonexistent/codelldb")), None);
        // An unknown pinned id is likewise not silently ignored.
        assert_eq!(resolve(Some("not-an-adapter"), None), None);
    }

    #[test]
    fn a_blank_pin_means_unpinned() {
        // A settings field starts empty and round-trips through TOML as `""`; treating that as an
        // adapter id named "" would resolve to nothing on every machine.
        assert_eq!(resolve(Some(""), None), resolve(None, None));
        assert_eq!(resolve(Some("  "), None), resolve(None, None));
    }

    #[test]
    fn the_survey_lists_every_adapter_whether_or_not_it_is_here() {
        let rows = survey();
        assert_eq!(rows.len(), ADAPTERS.len(), "a settings page has to say what is NOT installed");
        // Whatever this machine has, a found path is absolute and a real file.
        for (_, found) in &rows {
            if let Some(path) = found {
                assert!(std::path::Path::new(path).is_absolute(), "{path}");
                assert!(std::path::Path::new(path).is_file(), "{path}");
            }
        }
    }

    #[test]
    fn the_command_line_carries_the_mode_argument() {
        let gdb = Adapter { spec: spec_by_id("gdb").unwrap(), path: "/usr/bin/gdb".into() };
        let (exe, args) = gdb.command();
        assert_eq!(exe, "/usr/bin/gdb");
        assert_eq!(args, vec!["--interpreter=dap".to_string()]);

        let lldb =
            Adapter { spec: spec_by_id("lldb-dap").unwrap(), path: "/usr/bin/lldb-dap".into() };
        assert!(lldb.command().1.is_empty());
    }

    /// The directories are searched, not guessed: the extension folder carries a version in its name.
    #[test]
    fn only_codelldb_scans_the_extension_directories() {
        assert!(spec_by_id("lldb-dap").unwrap().preferred_dirs().is_empty());
        assert!(spec_by_id("gdb").unwrap().preferred_dirs().is_empty());
        // Whatever is found on this machine, every entry is a real directory named `adapter`.
        for dir in spec_by_id("codelldb").unwrap().preferred_dirs() {
            assert!(dir.is_dir(), "{dir:?}");
            assert!(dir.ends_with("adapter"), "{dir:?}");
        }
    }

    #[test]
    fn the_extra_dirs_returned_all_exist() {
        for spec in ADAPTERS {
            for dir in spec.extra_dirs() {
                assert!(dir.is_dir(), "{spec:?} offered {dir:?}, which is not a directory");
            }
        }
    }
}
