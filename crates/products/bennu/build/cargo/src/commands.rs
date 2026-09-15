//! The cargo subcommands Bennu offers, and how one becomes an argv.
//!
//! ## Why a catalogue and not free text
//!
//! The tool window's whole value is that the commands are *there* — you click `clippy` on a crate
//! rather than remembering `cargo clippy -p bennu-cargo --all-targets`. That needs each command's
//! capabilities written down, because they genuinely differ: `cargo fmt` takes no `--release` and no
//! `--features`, `cargo run` needs a target selector when a crate has several binaries, and
//! `cargo clippy` is a rustup component that may not be installed.
//!
//! So [`COMMANDS`] is the table, and [`argv`] is the one place an [`Invocation`] becomes a command
//! line. Every caller — the panel, a run configuration, the command palette — goes through it,
//! which is what stops three slightly different spellings of `cargo test` from appearing.
//!
//! ## What is deliberately not here
//!
//! Running it. This module produces an argument vector and nothing else, so the whole of it is
//! testable without a process — which matters, because "the flags were in the wrong order" is the
//! failure mode of building command lines and it is invisible until something refuses to start.

use serde::{Deserialize, Serialize};

/// One cargo subcommand.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommandDef {
    /// Stable id — what a run configuration persists and what [`argv`] looks up.
    pub id: &'static str,
    /// What the panel shows.
    pub label: &'static str,
    /// One line, shown as the row's hint and the palette's description.
    pub doc: &'static str,
    /// Whether `-p <crate>` / `--workspace` mean anything for it.
    pub scoped: bool,
    /// Whether it accepts `--release`.
    pub profiled: bool,
    /// Whether it accepts `--features` / `--all-features` / `--no-default-features`.
    pub featured: bool,
    /// Whether it accepts a target selector (`--bin`, `--example`, `--lib`, …).
    pub targeted: bool,
    /// Whether arguments after `--` reach a program (as opposed to reaching cargo itself).
    pub passes_args: bool,
    /// The rustup component it needs, empty when it is built into cargo.
    ///
    /// The reason a "clippy did nothing" report has an answer: `cargo clippy` on a toolchain
    /// without the component fails with a message about an unknown subcommand, which reads as a
    /// broken button rather than a missing install.
    pub component: &'static str,
    /// `true` for the handful worth putting in front of the user by default; the rest are one
    /// click further in. Keeps the panel a tool rather than a reference card.
    pub common: bool,
}

/// Every command Bennu offers, in the order the panel lists them.
///
/// Ordered by how often it is what you want, not alphabetically: `check` first because it is the
/// inner loop, `clean` and `update` last because they are occasional and slow.
pub const COMMANDS: &[CommandDef] = &[
    CommandDef {
        id: "check",
        label: "check",
        doc: "Type-check without producing binaries — the fast inner loop.",
        scoped: true, profiled: true, featured: true, targeted: true, passes_args: false,
        component: "", common: true,
    },
    CommandDef {
        id: "build",
        label: "build",
        doc: "Compile and link.",
        scoped: true, profiled: true, featured: true, targeted: true, passes_args: false,
        component: "", common: true,
    },
    CommandDef {
        id: "test",
        label: "test",
        doc: "Build and run the tests.",
        scoped: true, profiled: true, featured: true, targeted: true, passes_args: true,
        component: "", common: true,
    },
    CommandDef {
        id: "run",
        label: "run",
        doc: "Build and launch a binary.",
        scoped: true, profiled: true, featured: true, targeted: true, passes_args: true,
        component: "", common: true,
    },
    CommandDef {
        id: "clippy",
        label: "clippy",
        doc: "The lints rustc does not ship with.",
        scoped: true, profiled: true, featured: true, targeted: true, passes_args: true,
        component: "clippy", common: true,
    },
    CommandDef {
        id: "fmt",
        label: "fmt",
        doc: "Reformat the source.",
        // `cargo fmt -p x` works; nothing else on this list does.
        scoped: true, profiled: false, featured: false, targeted: false, passes_args: true,
        component: "rustfmt", common: true,
    },
    CommandDef {
        id: "doc",
        label: "doc",
        doc: "Build the API documentation.",
        scoped: true, profiled: true, featured: true, targeted: false, passes_args: false,
        component: "", common: true,
    },
    CommandDef {
        id: "bench",
        label: "bench",
        doc: "Build and run the benchmarks.",
        scoped: true, profiled: false, featured: true, targeted: true, passes_args: true,
        component: "", common: false,
    },
    CommandDef {
        id: "tree",
        label: "tree",
        doc: "Print the dependency graph.",
        scoped: true, profiled: false, featured: true, targeted: false, passes_args: false,
        component: "", common: false,
    },
    CommandDef {
        id: "update",
        label: "update",
        doc: "Re-resolve Cargo.lock to the newest allowed versions.",
        // `cargo update -p x` updates one dependency, which is the useful narrow form.
        scoped: true, profiled: false, featured: false, targeted: false, passes_args: false,
        component: "", common: false,
    },
    CommandDef {
        id: "clean",
        label: "clean",
        doc: "Delete target/.",
        scoped: true, profiled: true, featured: false, targeted: false, passes_args: false,
        component: "", common: false,
    },
];

/// The definition of `id`, or `None` for one this build does not know.
pub fn command(id: &str) -> Option<&'static CommandDef> {
    COMMANDS.iter().find(|c| c.id == id)
}

/// What a target selector selects. `Lib` and the four named kinds are Cargo's own flags.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct TargetSelector {
    /// `lib` · `bin` · `example` · `test` · `bench` · `bins` · `tests` · `all-targets`, or empty
    /// for whatever the command defaults to.
    pub kind: String,
    /// The target's name. Unused (and ignored) by the plural kinds.
    pub name: String,
}

impl TargetSelector {
    /// The flags this selector contributes, in order.
    fn flags(&self) -> Vec<String> {
        let kind = self.kind.trim();
        let name = self.name.trim();
        match kind {
            "" => Vec::new(),
            // The plural and whole-crate forms take no name.
            "lib" | "bins" | "tests" | "benches" | "examples" | "all-targets" => {
                vec![format!("--{kind}")]
            }
            // A named target. Without a name the flag would be a cargo error, so it is dropped —
            // "run this crate" is a meaningful request and `--bin` with nothing after it is not.
            "bin" | "example" | "test" | "bench" => {
                if name.is_empty() { Vec::new() } else { vec![format!("--{kind}"), name.to_string()] }
            }
            // Something a newer Bennu wrote. Passed through as a flag rather than dropped: the
            // configuration is the user's, and refusing to run it would be worse than trying.
            _ => {
                let mut out = vec![format!("--{kind}")];
                if !name.is_empty() {
                    out.push(name.to_string());
                }
                out
            }
        }
    }
}

/// One request to run a cargo command.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Invocation {
    /// The [`CommandDef::id`].
    pub command: String,
    /// `-p <name>`. Empty means the manifest in the working directory decides.
    pub package: String,
    /// `--workspace`. Ignored when `package` is set — asking for both is contradictory, and Cargo
    /// resolves it in a way nobody predicts.
    pub workspace: bool,
    pub target: TargetSelector,
    /// `--release`.
    pub release: bool,
    /// A non-default profile by name (`--profile <name>`). Wins over [`Invocation::release`], which
    /// is itself just `--profile release` spelled the short way.
    pub profile: String,
    /// `--features a,b`.
    pub features: Vec<String>,
    /// `--all-features`.
    pub all_features: bool,
    /// `--no-default-features`.
    pub no_default_features: bool,
    /// Extra cargo flags, already split into tokens (`--locked`, `--offline`).
    pub extra: Vec<String>,
    /// Arguments after `--`, for the command that passes them on.
    pub args: Vec<String>,
}

/// The full argument vector for `inv` — everything after the word `cargo`.
///
/// Flags a command does not accept are **dropped**, not passed: `cargo fmt --release` is an error,
/// and a run configuration that grew a `--release` while it was a `build` and was then switched to
/// `fmt` must still run. The order is cargo's conventional one — subcommand, scope, target,
/// profile, features, extras, then `--` and the program's own arguments.
pub fn argv(inv: &Invocation) -> Vec<String> {
    let def = command(&inv.command);
    let mut out: Vec<String> = Vec::new();

    // An unknown command is still spelled out: the id came from a configuration, and refusing it
    // would strand somebody's `cargo nextest`.
    let sub = if inv.command.trim().is_empty() { "check" } else { inv.command.trim() };
    out.push(sub.to_string());

    let accepts = |f: fn(&CommandDef) -> bool| def.map(f).unwrap_or(true);

    if accepts(|d| d.scoped) {
        let package = inv.package.trim();
        if !package.is_empty() {
            out.push("-p".to_string());
            out.push(package.to_string());
        } else if inv.workspace {
            out.push("--workspace".to_string());
        }
    }
    if accepts(|d| d.targeted) {
        out.extend(inv.target.flags());
    }
    if accepts(|d| d.profiled) {
        let profile = inv.profile.trim();
        if !profile.is_empty() {
            out.push("--profile".to_string());
            out.push(profile.to_string());
        } else if inv.release {
            out.push("--release".to_string());
        }
    }
    if accepts(|d| d.featured) {
        if inv.all_features {
            out.push("--all-features".to_string());
        }
        if inv.no_default_features {
            out.push("--no-default-features".to_string());
        }
        let features: Vec<&str> =
            inv.features.iter().map(|f| f.trim()).filter(|f| !f.is_empty()).collect();
        if !features.is_empty() {
            out.push("--features".to_string());
            out.push(features.join(","));
        }
    }
    out.extend(inv.extra.iter().filter(|e| !e.trim().is_empty()).cloned());

    let args: Vec<&String> = inv.args.iter().filter(|a| !a.trim().is_empty()).collect();
    if !args.is_empty() && accepts(|d| d.passes_args) {
        out.push("--".to_string());
        out.extend(args.into_iter().cloned());
    }
    out
}

/// `cargo <argv>` as one display line, quoting the tokens that need it.
///
/// The line the console prints, and the line you paste into a terminal when a run misbehaves — so
/// it has to be the truth about what ran rather than a plausible reconstruction.
pub fn display(inv: &Invocation) -> String {
    let mut parts = vec!["cargo".to_string()];
    parts.extend(argv(inv).into_iter().map(|t| {
        if t.contains(char::is_whitespace) { format!("\"{t}\"") } else { t }
    }));
    parts.join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn inv(command: &str) -> Invocation {
        Invocation { command: command.to_string(), ..Invocation::default() }
    }

    #[test]
    fn a_bare_command_is_just_the_subcommand() {
        assert_eq!(argv(&inv("check")), vec!["check"]);
    }

    #[test]
    fn the_flags_come_out_in_cargos_conventional_order() {
        let i = Invocation {
            command: "test".into(),
            package: "bennu-cargo".into(),
            target: TargetSelector { kind: "test".into(), name: "it".into() },
            release: true,
            features: vec!["std".into(), "extra".into()],
            no_default_features: true,
            extra: vec!["--locked".into()],
            args: vec!["--nocapture".into()],
            ..Invocation::default()
        };
        assert_eq!(
            argv(&i),
            vec![
                "test", "-p", "bennu-cargo", "--test", "it", "--release",
                "--no-default-features", "--features", "std,extra", "--locked", "--", "--nocapture",
            ]
        );
    }

    /// The reason [`argv`] consults the table: a flag a command does not accept is an error that
    /// stops the run, and a configuration outlives the command it was created for.
    #[test]
    fn flags_a_command_does_not_accept_are_dropped() {
        let i = Invocation {
            command: "fmt".into(),
            package: "x".into(),
            release: true,
            all_features: true,
            features: vec!["std".into()],
            target: TargetSelector { kind: "bin".into(), name: "app".into() },
            ..Invocation::default()
        };
        // `-p` survives (cargo fmt takes it); the profile, the features and the target do not.
        assert_eq!(argv(&i), vec!["fmt", "-p", "x"]);
    }

    #[test]
    fn a_package_and_the_whole_workspace_are_not_asked_for_together() {
        let i = Invocation { command: "build".into(), package: "x".into(), workspace: true, ..inv("build") };
        assert_eq!(argv(&i), vec!["build", "-p", "x"], "the narrower scope wins");
        let i = Invocation { command: "build".into(), workspace: true, ..inv("build") };
        assert_eq!(argv(&i), vec!["build", "--workspace"]);
    }

    #[test]
    fn a_named_profile_wins_over_the_release_shorthand() {
        let i = Invocation {
            command: "build".into(),
            release: true,
            profile: "release-lto".into(),
            ..inv("build")
        };
        assert_eq!(argv(&i), vec!["build", "--profile", "release-lto"]);
    }

    #[test]
    fn a_target_kind_with_no_name_is_dropped_rather_than_becoming_a_cargo_error() {
        let i = Invocation {
            command: "run".into(),
            target: TargetSelector { kind: "bin".into(), name: "  ".into() },
            ..inv("run")
        };
        // `cargo run --bin` with nothing after it fails; "run this crate" is what was meant.
        assert_eq!(argv(&i), vec!["run"]);
        // The plural forms genuinely take no name.
        let i = Invocation {
            command: "check".into(),
            target: TargetSelector { kind: "all-targets".into(), name: String::new() },
            ..inv("check")
        };
        assert_eq!(argv(&i), vec!["check", "--all-targets"]);
    }

    #[test]
    fn program_arguments_only_appear_for_a_command_that_passes_them_on() {
        let i = Invocation { command: "run".into(), args: vec!["--verbose".into()], ..inv("run") };
        assert_eq!(argv(&i), vec!["run", "--", "--verbose"]);
        // `cargo build -- x` means something quite different (it goes to rustc), so it is not
        // offered.
        let i = Invocation { command: "build".into(), args: vec!["--verbose".into()], ..inv("build") };
        assert_eq!(argv(&i), vec!["build"]);
    }

    /// A configuration written by a newer Bennu names a subcommand this one has never heard of.
    /// It has to run anyway — the alternative is stranding somebody's `cargo nextest`.
    #[test]
    fn an_unknown_subcommand_is_passed_through_with_everything_it_was_given() {
        let i = Invocation {
            command: "nextest".into(),
            package: "x".into(),
            release: true,
            args: vec!["run".into()],
            ..Invocation::default()
        };
        assert_eq!(argv(&i), vec!["nextest", "-p", "x", "--release", "--", "run"]);
    }

    #[test]
    fn an_empty_command_falls_back_to_the_harmless_one() {
        assert_eq!(argv(&Invocation::default()), vec!["check"]);
    }

    #[test]
    fn empty_features_and_extras_do_not_leave_stray_flags() {
        let i = Invocation {
            command: "build".into(),
            features: vec!["  ".into(), String::new()],
            extra: vec!["  ".into()],
            args: vec![String::new()],
            ..inv("build")
        };
        assert_eq!(argv(&i), vec!["build"]);
    }

    #[test]
    fn the_display_line_is_pasteable() {
        let i = Invocation {
            command: "run".into(),
            package: "app".into(),
            args: vec!["--path".into(), "/some dir/file".into()],
            ..inv("run")
        };
        assert_eq!(display(&i), "cargo run -p app -- --path \"/some dir/file\"");
    }

    #[test]
    fn every_catalogue_entry_resolves_and_has_a_doc() {
        for c in COMMANDS {
            assert!(command(c.id).is_some(), "{}", c.id);
            assert!(!c.doc.is_empty(), "{} has no doc", c.id);
            assert!(!c.label.is_empty(), "{} has no label", c.id);
        }
        // No duplicate ids — two rows for one command in the panel.
        let mut ids: Vec<&str> = COMMANDS.iter().map(|c| c.id).collect();
        let count = ids.len();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), count);
    }
}
