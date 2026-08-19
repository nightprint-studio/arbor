//! `cargo` domain — the Rust tool window's backend.
//!
//! Five handlers, each answering one question the panel or the run-configuration editor asks:
//!
//! | Handler | Question |
//! |---|---|
//! | `bennu_cargo_workspace` | which crates are there, what does each build, what features does it have |
//! | `bennu_cargo_commands` | which cargo subcommands can I offer, and what does each accept |
//! | `bennu_cargo_toolchain` | which cargo is this, and is clippy/rustfmt actually installed |
//! | `bennu_cargo_preview` | what command line would this configuration run |
//! | `bennu_cargo_run` | run one, streaming into the Run console |
//!
//! ## Why the catalogue comes from here
//!
//! `bennu_cargo_commands` returns a table the backend already has ([`bennu_cargo::commands`]), and
//! the frontend could perfectly well hard-code the same list. It does not, because the flags each
//! command accepts are what [`argv`] uses to build the command line — so a panel with its own copy
//! would eventually offer `--release` on a `cargo fmt` row that the backend then drops, and the
//! button would silently do something other than what it says.
//!
//! ## The toolchain probe, and why it is worth two processes
//!
//! `cargo clippy` on a toolchain without the component fails with "no such subcommand", which reads
//! as a broken button rather than a missing install. So the components are read once per session and
//! the panel can say *install it* instead. Same reasoning as the language-server discovery: the
//! answer to "why did nothing happen" has to be on screen.
//!
//! Threading: the serve loop dispatches each request on its own thread, so the short-lived probes
//! below never stall the IPC read loop. `bennu_cargo_run` returns immediately — the child is owned by
//! a background thread, exactly like `bennu_run` (they share [`spawn_streamed`]).

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Mutex, OnceLock};

use arbor_process_ext::prelude::NoWindowExt;
use bennu_cargo::prelude::{
    argv, display_command, read_workspace, CargoWorkspace, CommandDef, Invocation, COMMANDS,
};
use bennu_core::prelude::BennuState;
use bennu_proto::prelude::RunHandle;
use serde::{Deserialize, Serialize};

use crate::build::spawn_streamed;

/// Resolve a rustup-installed tool to a path, falling back to its bare name on `PATH`.
///
/// `PATH` alone is not enough, and the failure only shows up in the **shipped** build: an app
/// started from Finder inherits `/usr/bin:/bin:…` and none of the login shell's additions, so
/// `~/.cargo/bin` is missing and every cargo-backed tool reports "not on PATH" — while the same
/// binary run from a terminal works, because it inherited the terminal's environment. A bug that
/// exists only in the configuration users actually get is the worst place for one to hide.
///
/// Looking in `$CARGO_HOME/bin` is not guessing among alternatives: it is the one location rustup
/// defines, and [`bennu_cargo::prelude::cargo_home`] already resolves it for the registry readers —
/// whose own doc note says a windowed app inheriting almost no environment is the ordinary case.
/// Falling back to the bare name keeps a PATH-only install (a distro package, a corporate image)
/// working exactly as before.
fn rustup_bin(name: &str) -> std::ffi::OsString {
    let exe = format!("{name}{}", std::env::consts::EXE_SUFFIX);
    if let Some(home) = bennu_cargo::prelude::cargo_home() {
        let candidate = home.join("bin").join(&exe);
        if candidate.is_file() {
            return candidate.into_os_string();
        }
    }
    std::ffi::OsString::from(exe)
}

/// Where to launch cargo from. The one spelling every cargo spawn in this backend uses.
pub fn cargo_launcher() -> std::ffi::OsString {
    rustup_bin("cargo")
}

// ── bennu_cargo_workspace ──────────────────────────────────────────────────────

/// Args for the handlers that take a project root.
#[derive(Deserialize, schemars::JsonSchema)]
pub struct RootArgs {
    /// Absolute path to the workspace root (the dir holding the root `Cargo.toml`).
    pub root: String,
}

/// The crate graph: members, targets, features, dependency counts.
///
/// Reads manifests and the filesystem — never `cargo metadata`, which on a cold workspace costs
/// seconds and wants the network. Never errors: a root with no manifest is an empty workspace, which
/// is what an editor opened on the wrong directory should show.
#[arbor_rpc::handler(mcp(
    name = "bennu_cargo_workspace",
    title = "Describe a Cargo workspace and its dependencies",
    safety = read,
    description = "Describe a Rust project's Cargo workspace: its member crates, what \
each depends on, and the third-party crates it pulls in with the versions actually \
resolved. Use it to see the shape of an unfamiliar workspace, to work out what a change \
to one crate would rebuild, and to spot dependencies that have moved on. Reads the \
manifests and the lockfile — it does not run cargo, so it answers on a project that has \
never been built.",
))]
fn bennu_cargo_workspace(_ctx: &BennuState, args: RootArgs) -> Result<CargoWorkspace, String> {
    Ok(read_workspace(Path::new(&args.root)))
}

// ── bennu_cargo_commands ───────────────────────────────────────────────────────

/// The cargo subcommands Bennu offers, with what each accepts. See the module doc for why this is
/// not a frontend constant.
#[arbor_rpc::handler]
fn bennu_cargo_commands(_ctx: &BennuState) -> Result<Vec<CommandDef>, String> {
    Ok(COMMANDS.to_vec())
}

// ── bennu_cargo_toolchain ──────────────────────────────────────────────────────

/// What `cargo` is available, and what it can do.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Toolchain {
    /// `cargo --version`, verbatim. Empty when `cargo` could not be run at all.
    pub version: String,
    /// The rustup components installed for the active toolchain (`clippy`, `rustfmt`, …).
    ///
    /// Empty when `rustup` is not there — which is not the same as "no components": a Rust installed
    /// from a distribution package has clippy without rustup knowing anything. So an empty list means
    /// *unknown*, and the panel offers the command rather than greying it out.
    pub components: Vec<String>,
    /// Whether `rustup` answered at all — the difference between "clippy is missing" and "we cannot
    /// tell".
    pub components_known: bool,
    /// The active toolchain's name (`stable-aarch64-apple-darwin`), empty when unknown.
    pub toolchain: String,
}

impl Toolchain {
    /// Whether `component` is installed. `true` when nothing is known, because refusing a command on
    /// a guess is worse than letting it run and report for itself.
    pub fn has(&self, component: &str) -> bool {
        !self.components_known || component.is_empty() || self.components.iter().any(|c| c == component)
    }
}

/// The active toolchain, probed once per session.
///
/// Cached because it costs two short-lived children and does not change while the editor is open —
/// and because the panel asks for it on every open. `bennu_cargo_toolchain` with `refresh` is the way
/// back out after installing something, which is the same escape hatch the language-server settings
/// page has.
#[derive(Deserialize)]
pub struct ToolchainArgs {
    /// Re-probe instead of answering from the cache. What the panel sends after telling the user to
    /// run `rustup component add clippy`.
    #[serde(default)]
    pub refresh: bool,
}

#[arbor_rpc::handler]
fn bennu_cargo_toolchain(_ctx: &BennuState, args: ToolchainArgs) -> Result<Toolchain, String> {
    if args.refresh {
        if let Ok(mut slot) = toolchain_cache().lock() {
            *slot = None;
        }
    }
    Ok(toolchain())
}

fn toolchain_cache() -> &'static Mutex<Option<Toolchain>> {
    static CACHE: OnceLock<Mutex<Option<Toolchain>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(None))
}

/// The active toolchain, from the cache or probed.
pub(crate) fn toolchain() -> Toolchain {
    if let Ok(slot) = toolchain_cache().lock() {
        if let Some(hit) = slot.as_ref() {
            return hit.clone();
        }
    }
    let probed = probe_toolchain();
    if let Ok(mut slot) = toolchain_cache().lock() {
        *slot = Some(probed.clone());
    }
    probed
}

fn probe_toolchain() -> Toolchain {
    let mut out = Toolchain::default();
    if let Some(text) = capture(cargo_launcher(), &["--version"]) {
        out.version = text.trim().to_string();
    }
    // `rustup component list --installed` prints one component per line. Absent rustup is a normal
    // state, not a failure — see `Toolchain::components`.
    if let Some(text) = capture(rustup_bin("rustup"), &["component", "list", "--installed"]) {
        out.components_known = true;
        out.components = text
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty())
            // The lines carry the target triple (`clippy-aarch64-apple-darwin`); the component is
            // the part before it, and matching on the whole line would never find `clippy`.
            .map(component_name)
            .collect();
    }
    if let Some(text) = capture(rustup_bin("rustup"), &["show", "active-toolchain"]) {
        // `stable-aarch64-apple-darwin (default)` — the name is the first token.
        out.toolchain = text.split_whitespace().next().unwrap_or("").to_string();
    }
    out
}

/// `clippy-aarch64-apple-darwin` → `clippy`.
///
/// The component name is everything before the target triple, and a triple always begins with an
/// architecture — so the split is at the first `-` followed by something that is not a letter-only
/// word we recognise as part of a component name. In practice component names are single words
/// (`clippy`, `rustfmt`, `rust-src`, `rust-analyzer`), so the safe rule is: keep the known prefixes
/// whole, otherwise take the first segment.
fn component_name(line: &str) -> String {
    for known in ["rust-analyzer", "rust-src", "rust-std", "rust-docs", "llvm-tools"] {
        if line == known || line.starts_with(&format!("{known}-")) {
            return known.to_string();
        }
    }
    line.split('-').next().unwrap_or(line).to_string()
}

/// Run `program args…` and return its stdout, or `None` when it could not be run.
fn capture(program: impl AsRef<std::ffi::OsStr>, args: &[&str]) -> Option<String> {
    let mut cmd = Command::new(program);
    cmd.args(args);
    cmd.no_window();
    let out = cmd.output().ok()?;
    out.status.success().then(|| String::from_utf8_lossy(&out.stdout).to_string())
}

// ── bennu_cargo_preview ────────────────────────────────────────────────────────

/// Args for [`bennu_cargo_preview`].
#[derive(Deserialize)]
pub struct PreviewArgs {
    pub invocation: Invocation,
}

/// The command line an invocation would run, as one display string.
///
/// A round-trip for a preview looks extravagant until you consider the alternative: the
/// run-configuration editor would assemble a *second* command line to show, and the two would drift
/// the first time a flag was added to one of them. A preview that disagrees with what runs is worse
/// than no preview, so it comes from the one function that builds the real thing.
#[arbor_rpc::handler]
fn bennu_cargo_preview(_ctx: &BennuState, args: PreviewArgs) -> Result<String, String> {
    Ok(display_command(&args.invocation))
}

// ── bennu_cargo_run ────────────────────────────────────────────────────────────

/// Args for [`bennu_cargo_run`].
#[derive(Deserialize)]
pub struct CargoRunArgs {
    /// Absolute path to the workspace root — where cargo is invoked unless `working_dir` says
    /// otherwise.
    pub root: String,
    /// What to run. The whole command line is derived from it by [`argv`], so the frontend never
    /// assembles cargo flags itself.
    pub invocation: Invocation,
    /// Working directory for the child. Empty / absent = the workspace root.
    ///
    /// Almost always the root: `-p <crate>` is how a crate is targeted, and running *in* a member's
    /// directory changes which manifest a bare command applies to. It is here for the program being
    /// run, which may expect its own `./config`.
    #[serde(default)]
    pub working_dir: Option<String>,
    /// Extra environment variables, merged over the inherited environment.
    #[serde(default)]
    pub env: Option<std::collections::HashMap<String, String>>,
}

/// Launch a cargo subcommand, streaming its output to the Run console.
///
/// Returns immediately with the [`RunHandle`] the console correlates the stream by; the child runs on
/// a background thread. Cancellation (`bennu_cancel_run`) and input (`bennu_run_input`) work on it
/// unchanged, because it goes through the same [`spawn_streamed`] a JVM launch does.
///
/// Two environment variables are set unless the caller overrides them, and both are about the console
/// rather than about cargo: `--color=never` is not passed (cargo respects `CARGO_TERM_COLOR`), and
/// `CARGO_TERM_PROGRESS_WHEN=never` stops the progress bar, which is a carriage-return animation that
/// renders as hundreds of near-identical lines in a log that has no cursor to move.
#[arbor_rpc::handler]
fn bennu_cargo_run(ctx: &BennuState, args: CargoRunArgs) -> Result<RunHandle, String> {
    let root = PathBuf::from(&args.root);
    let cwd = match args.working_dir.as_deref() {
        Some(d) if !d.trim().is_empty() => PathBuf::from(d),
        _ => root.clone(),
    };

    let argv = argv(&args.invocation);
    let mut cmd = Command::new(cargo_launcher());
    cmd.current_dir(&cwd);
    for a in &argv {
        cmd.arg(a);
    }
    // A console shows text, and it has no cursor for cargo to move around.
    cmd.env("CARGO_TERM_COLOR", "never");
    cmd.env("CARGO_TERM_PROGRESS_WHEN", "never");
    if let Some(env) = &args.env {
        for (k, v) in env {
            cmd.env(k, v);
        }
    }
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped()).stdin(Stdio::piped());
    cmd.no_window();

    // A command that could change what is installed has taught the machine about new crates, so the
    // completion catalogue for this root is stale. Dropped before the run rather than after: the
    // child outlives this handler, and the next completion request will rebuild from whatever the
    // lockfile says by then.
    if matches!(args.invocation.command.as_str(), "update" | "build" | "check" | "test" | "clippy") {
        crate::cargo_intel::forget_catalog(&args.root);
    }

    let label = command_label(&args.invocation);
    spawn_streamed(
        cmd,
        label,
        display_command(&args.invocation),
        cwd.display().to_string(),
        &args.root,
        ctx.event_sink(),
        |_| {},
    )
    .map_err(|e| {
        format!(
            "spawn cargo ({}): {e}{}",
            cargo_launcher().to_string_lossy(),
            install_hint(&args.invocation)
        )
    })
}

/// The console tab's title: the command, plus what it was aimed at.
///
/// `cargo test` and `cargo test -p bennu-cargo` are different runs and a tab strip that called both
/// "test" would be unreadable after the second one.
fn command_label(inv: &Invocation) -> String {
    let command = if inv.command.trim().is_empty() { "check" } else { inv.command.trim() };
    let target = match (inv.package.trim(), inv.target.name.trim()) {
        ("", "") if inv.workspace => "workspace",
        ("", "") => "",
        ("", name) => name,
        (package, "") => package,
        (package, name) => return format!("{command} {package}/{name}"),
    };
    if target.is_empty() { command.to_string() } else { format!("{command} {target}") }
}

/// What to add to a spawn failure, when the command needed a component that is not installed.
///
/// The failure it explains is confusing on its own: `cargo clippy` without the component reports an
/// unknown subcommand, which sounds like a broken Bennu rather than a missing install.
fn install_hint(inv: &Invocation) -> String {
    let Some(def) = bennu_cargo::prelude::command(&inv.command) else { return String::new() };
    if def.component.is_empty() {
        return String::new();
    }
    let tc = toolchain();
    if tc.has(def.component) {
        return String::new();
    }
    format!(
        " — `{}` needs the {} component: run `rustup component add {}`",
        def.id, def.component, def.component
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use bennu_cargo::prelude::TargetSelector;

    #[test]
    fn a_component_line_yields_the_component_not_the_triple() {
        assert_eq!(component_name("clippy-aarch64-apple-darwin"), "clippy");
        assert_eq!(component_name("rustfmt-x86_64-unknown-linux-gnu"), "rustfmt");
        // The hyphenated names have to survive whole, or `rust-src` reads as `rust`.
        assert_eq!(component_name("rust-src"), "rust-src");
        assert_eq!(component_name("rust-analyzer-aarch64-apple-darwin"), "rust-analyzer");
        assert_eq!(component_name("rust-std-wasm32-unknown-unknown"), "rust-std");
        assert_eq!(component_name("cargo"), "cargo");
    }

    /// Not knowing must never withhold a command: an unrunnable button the user can see fail is
    /// better than one that is greyed out for a reason we guessed.
    #[test]
    fn an_unknown_component_set_permits_everything() {
        let unknown = Toolchain::default();
        assert!(unknown.has("clippy"));
        let known = Toolchain {
            components: vec!["rustfmt".into()],
            components_known: true,
            ..Toolchain::default()
        };
        assert!(known.has("rustfmt"));
        assert!(!known.has("clippy"));
        assert!(known.has(""), "a command that needs no component always runs");
    }

    #[test]
    fn the_tab_label_names_what_was_run() {
        let label = |inv: Invocation| command_label(&inv);
        assert_eq!(label(Invocation { command: "check".into(), ..Invocation::default() }), "check");
        assert_eq!(
            label(Invocation { command: "check".into(), workspace: true, ..Invocation::default() }),
            "check workspace"
        );
        assert_eq!(
            label(Invocation {
                command: "test".into(),
                package: "bennu-cargo".into(),
                ..Invocation::default()
            }),
            "test bennu-cargo"
        );
        assert_eq!(
            label(Invocation {
                command: "run".into(),
                package: "app".into(),
                target: TargetSelector { kind: "bin".into(), name: "tool".into() },
                ..Invocation::default()
            }),
            "run app/tool"
        );
        // An empty command is the same fallback `argv` uses, so the tab and the command line agree.
        assert_eq!(label(Invocation::default()), "check");
    }

    #[test]
    fn a_command_needing_a_missing_component_says_how_to_install_it() {
        // The probe is cached, so seed it rather than depending on the machine.
        if let Ok(mut slot) = toolchain_cache().lock() {
            *slot = Some(Toolchain {
                components: vec!["rustfmt".into()],
                components_known: true,
                ..Toolchain::default()
            });
        }
        let hint = install_hint(&Invocation { command: "clippy".into(), ..Invocation::default() });
        assert!(hint.contains("rustup component add clippy"), "{hint}");
        // One that IS installed, and one that needs nothing, say nothing.
        assert!(install_hint(&Invocation { command: "fmt".into(), ..Invocation::default() }).is_empty());
        assert!(install_hint(&Invocation { command: "build".into(), ..Invocation::default() }).is_empty());
        // Leave the cache empty so a later test in this process probes for itself.
        if let Ok(mut slot) = toolchain_cache().lock() {
            *slot = None;
        }
    }
}
