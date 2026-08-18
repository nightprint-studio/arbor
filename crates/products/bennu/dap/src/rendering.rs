//! What an adapter can **show** you, and how it reads an expression.
//!
//! [`crate::discovery`] answers *which* adapter; this answers what happens once one is running, and
//! it is the difference between a debugger that works and one that is worth using.
//!
//! ## The problem: a `Vec` is a pointer and a length
//!
//! Neither LLDB nor GDB knows anything about Rust's types out of the box. Stopped on a
//! `Vec<Order>`, an unconfigured LLDB shows you `buf` → `inner` → `ptr` → `pointer` → a raw
//! address, and the elements are nowhere: five clicks down a chain of implementation details to
//! reach nothing. Same for `String` (a byte buffer), `HashMap` (a swisstable), `Option` (a
//! discriminant and a union), `Rc` (a control block).
//!
//! Three different things fix that, one per adapter, and knowing which is which is this module's
//! whole job:
//!
//! * **CodeLLDB** ships its own Rust formatters. Nothing to do, and it is why it is preferred.
//! * **`lldb-dap`** ships none — but **the Rust toolchain does**. `rust-lldb` is a shell script whose
//!   entire content is loading `lldb_lookup.py` and `lldb_commands` out of
//!   `$(rustc --print sysroot)/lib/rustlib/etc` into a plain LLDB. Those two files are exactly what
//!   `lldb-dap` is missing, and it takes `initCommands` — so the same import that `rust-lldb` does on
//!   the command line is done here at launch. This is the fix that matters most in practice: on macOS
//!   `lldb-dap` is the adapter most machines have, because Xcode's command-line tools ship it.
//! * **GDB** reads the printers named in the binary's own `.debug_gdb_scripts` section, which `rustc`
//!   emits, and it understands Rust as a language natively. Its DAP mode offers no hook to force the
//!   import, so what it manages is what it manages — a documented gap rather than a silent one.
//!
//! ## The other half: a struct with no summary
//!
//! Formatters cover the standard library. A `struct Order` of your own has no formatter anywhere and
//! LLDB's default is to print **nothing at all** on the parent row — the fields are underneath, but
//! the row that names the variable is blank, which reads as "empty". `lldb-dap` can synthesise
//! `{id:7, total:19.9}` for those (`enableAutoVariableSummaries`), off by default because it costs a
//! formatting pass per row. Bennu turns it on: the cost is bounded by what is on screen, since a
//! child row is only fetched when it is expanded.
//!
//! ## Expression dialects
//!
//! CodeLLDB does not have *an* expression evaluator, it has three, chosen by a prefix — and the
//! default is not the native one:
//!
//! * **`/se`** — its own simple-expression language. Reads memory and follows the formatters; runs
//!   nothing in the debuggee. For looking at data this is both the safest and the most accurate of the
//!   three, and it is the default.
//! * **`/nat`** — the debugger's native parser. LLDB's is a **C++** parser, which is why asking it
//!   about a Rust value produces C++ prose about Rust type names.
//! * **`/py`** — Python, with `$name` interpolated from the frame.
//!
//! `lldb-dap` and GDB have only their native parser. So the same expression typed into the same box
//! means three different things depending on which adapter resolved, and a client that forwards the
//! string blind cannot say why. What is here is enough for a caller to route: which dialects exist,
//! how they are written, and how to peel one off the front of what a user typed.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

use arbor_process_ext::prelude::{locate_executable, NoWindowExt};

use crate::discovery::{AdapterSpec, Engine};

/// How an adapter comes to render Rust's own types **on this machine**.
///
/// Per-machine, not per-adapter: `lldb-dap` renders a `Vec` properly when a Rust toolchain is
/// installed to borrow the formatters from, and does not when there is none. A caller that wants to
/// warn the user needs the answer for the machine in front of them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RustRendering {
    /// The adapter ships Rust formatters itself. CodeLLDB.
    Builtin,
    /// A plain LLDB, with the toolchain's own formatters to import — the directory holding
    /// `lldb_lookup.py` and `lldb_commands`.
    Toolchain(PathBuf),
    /// GDB, which auto-loads the pretty-printers the binary names and knows Rust as a language.
    Embedded,
    /// A plain LLDB and no toolchain to borrow from. A `Vec` is a pointer and a length here, and
    /// there is nothing this side can do about it.
    Raw,
}

impl RustRendering {
    /// The one sentence to put in front of the user when values are going to read badly, or `None`
    /// when they are not.
    ///
    /// Returned as prose rather than as a bool because the useful part is *what to do about it*, and
    /// because a debugger that shows raw struct internals without saying why looks broken rather than
    /// unconfigured — which is the report that produced this module.
    pub fn caveat(&self) -> Option<&'static str> {
        match self {
            RustRendering::Raw => Some(
                "This debug adapter cannot render Rust's own types: a Vec shows as a pointer and a \
                 length, a String as a byte buffer. Install a Rust toolchain (Bennu borrows its \
                 LLDB formatters), or install CodeLLDB, which ships its own.",
            ),
            _ => None,
        }
    }

    /// Whether Rust's containers will read as containers.
    pub fn renders_rust(&self) -> bool {
        !matches!(self, RustRendering::Raw)
    }
}

/// The directory of the active toolchain's debugger scripts, when there is one.
///
/// `$(rustc --print sysroot)/lib/rustlib/etc`, holding `lldb_lookup.py`, `lldb_providers.py` and
/// `lldb_commands` — the files `rust-lldb` loads. Probed once: it costs a short-lived child and it
/// does not change while the editor is open.
///
/// `rustc` is *located*, not spawned by name: a windowed app's `PATH` does not include
/// `~/.cargo/bin`, so a bare `Command::new("rustc")` reports "no toolchain" on a machine that has
/// had one for years. See [`arbor_process_ext::locate`].
pub fn toolchain_etc() -> Option<&'static Path> {
    static ETC: OnceLock<Option<PathBuf>> = OnceLock::new();
    ETC.get_or_init(probe_toolchain_etc).as_deref()
}

fn probe_toolchain_etc() -> Option<PathBuf> {
    let rustc = locate_executable("rustc", None, &rustup_dirs(), &[])?;
    let output = Command::new(rustc).args(["--print", "sysroot"]).no_window().output().ok()?;
    if !output.status.success() {
        return None;
    }
    let sysroot = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if sysroot.is_empty() {
        return None;
    }
    let etc = Path::new(&sysroot).join("lib").join("rustlib").join("etc");
    // Both, because they are two halves of one thing: the module does the work and the command file
    // installs it. Half of it present means an install that is mid-something, and importing the
    // module alone changes nothing.
    let complete = etc.join("lldb_lookup.py").is_file() && etc.join("lldb_commands").is_file();
    complete.then_some(etc)
}

/// Where rustup puts its proxies. Ahead of `PATH` because that is where `rustc` actually is on a
/// machine whose `PATH` a window manager decided.
fn rustup_dirs() -> Vec<PathBuf> {
    let Some(home) = arbor_core::prelude::user_home() else { return Vec::new() };
    let mut dirs = vec![home.join(".cargo").join("bin")];
    if let Some(cargo_home) = std::env::var_os("CARGO_HOME") {
        dirs.push(PathBuf::from(cargo_home).join("bin"));
    }
    dirs.retain(|d| d.is_dir());
    dirs
}

/// How this adapter will render Rust values on this machine.
pub fn rendering(spec: &AdapterSpec) -> RustRendering {
    if spec.rust_formatters {
        return RustRendering::Builtin;
    }
    match spec.engine {
        Engine::Gdb => RustRendering::Embedded,
        Engine::Lldb => match toolchain_etc() {
            Some(etc) => RustRendering::Toolchain(etc.to_path_buf()),
            None => RustRendering::Raw,
        },
    }
}

/// The LLDB commands that teach a plain LLDB about Rust, or nothing when they are not needed.
///
/// This is `rust-lldb`'s two lines, verbatim in effect: import the lookup module, then source the
/// file of `type summary add` / `type synthetic add` commands that points every Rust type at it.
///
/// `command source -s 0` — do not stop on the first error. `lldb_commands` targets several LLDB
/// versions at once and a line the running one does not understand must not abandon the rest of the
/// file, which would leave Rust half-formatted in a way that is very hard to diagnose from the
/// outside.
pub fn init_commands(spec: &AdapterSpec) -> Vec<String> {
    match rendering(spec) {
        RustRendering::Toolchain(etc) => vec![
            format!("command script import \"{}\"", etc.join("lldb_lookup.py").display()),
            format!("command source -s 0 \"{}\"", etc.join("lldb_commands").display()),
        ],
        _ => Vec::new(),
    }
}

/// The launch/attach arguments that are about **this adapter** rather than about the program.
///
/// Merged into the request by [`crate::session::Launch`]. Each adapter ignores what it does not
/// know, so the set is additive rather than branching — but it is built per adapter anyway, because
/// `initCommands` on CodeLLDB would import formatters it already has and `sourceLanguages` on
/// `lldb-dap` is dead weight.
pub fn launch_extras(spec: &AdapterSpec) -> Vec<(&'static str, serde_json::Value)> {
    let mut extras: Vec<(&'static str, serde_json::Value)> = Vec::new();

    let commands = init_commands(spec);
    if !commands.is_empty() {
        extras.push(("initCommands", serde_json::json!(commands)));
    }

    if spec.rust_formatters {
        // CodeLLDB gates part of its Rust handling on being told the language, and pins its default
        // expression dialect on request. `simple` is already its default; saying so is what stops a
        // user's own `settings.json` from changing what an expression means inside Bennu.
        extras.push(("sourceLanguages", serde_json::json!(["rust"])));
        extras.push(("expressions", serde_json::json!(Evaluator::Simple.setting())));
    }

    if spec.engine == Engine::Lldb && !spec.rust_formatters {
        // See the module docs: without this, a struct of your own shows a blank value on the row that
        // names it, which reads as an empty object rather than as "expand me".
        extras.push(("enableAutoVariableSummaries", serde_json::json!(true)));
    }

    extras
}

// ── expression dialects ─────────────────────────────────────────────────────────

/// One of the adapter's own expression evaluators.
///
/// Bennu's *own* path evaluator is not in here: it does not belong to an adapter, it is built on the
/// `variables` request and works the same on all three. This is only the list of what an adapter can
/// be asked to do with a string.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Evaluator {
    /// CodeLLDB's simple-expression language: reads memory, follows formatters, runs nothing in the
    /// debuggee.
    Simple,
    /// The debugger's native expression parser. LLDB's is a C++ parser; GDB's understands Rust.
    Native,
    /// CodeLLDB's Python, with `$name` interpolated from the frame.
    Python,
}

impl Evaluator {
    /// The prefix a user types to select it — CodeLLDB's own spelling, because that is what a user
    /// who knows CodeLLDB will reach for and there is no reason to invent a second one.
    pub fn prefix(self) -> &'static str {
        match self {
            Evaluator::Simple => "/se",
            Evaluator::Native => "/nat",
            Evaluator::Python => "/py",
        }
    }

    /// How CodeLLDB names it in the `expressions` launch setting.
    pub fn setting(self) -> &'static str {
        match self {
            Evaluator::Simple => "simple",
            Evaluator::Native => "native",
            Evaluator::Python => "python",
        }
    }

    /// What to say about it in one line.
    pub fn describe(self) -> &'static str {
        match self {
            Evaluator::Simple => "reads memory through the formatters, runs nothing",
            Evaluator::Native => "the debugger's own parser",
            Evaluator::Python => "Python, with $name from the frame",
        }
    }

    fn from_prefix(prefix: &str) -> Option<Evaluator> {
        [Evaluator::Simple, Evaluator::Native, Evaluator::Python]
            .into_iter()
            .find(|e| e.prefix() == prefix)
    }
}

/// The dialects this adapter honours, in the order a UI should offer them.
///
/// One entry for the two adapters that have a single parser — which is the point: it is what lets a
/// caller refuse `/py` **by name** on `lldb-dap` instead of sending it and having the debugger
/// complain about a syntax error at a `/`.
pub fn evaluators(spec: &AdapterSpec) -> &'static [Evaluator] {
    if spec.rust_formatters {
        &[Evaluator::Simple, Evaluator::Native, Evaluator::Python]
    } else {
        &[Evaluator::Native]
    }
}

/// Peel a leading dialect prefix off what the user typed.
///
/// Returns the dialect it names and the rest of the expression. A prefix must be followed by
/// whitespace: `/se x` selects the simple evaluator, `/self.len` is a path that happens to start
/// with a slash and is left entirely alone.
pub fn split_dialect(expression: &str) -> (Option<Evaluator>, &str) {
    let trimmed = expression.trim();
    if !trimmed.starts_with('/') {
        return (None, trimmed);
    }
    let (head, rest) = match trimmed.find(char::is_whitespace) {
        Some(at) => (&trimmed[..at], trimmed[at..].trim_start()),
        // A bare `/nat` with nothing after it: still recognised, so the caller can say "and then
        // what?" rather than "`/nat` is not a variable".
        None => (trimmed, ""),
    };
    match Evaluator::from_prefix(head) {
        Some(dialect) => (Some(dialect), rest),
        None => (None, trimmed),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::discovery::spec_by_id;

    fn codelldb() -> &'static AdapterSpec {
        spec_by_id("codelldb").unwrap()
    }
    fn lldb_dap() -> &'static AdapterSpec {
        spec_by_id("lldb-dap").unwrap()
    }
    fn gdb() -> &'static AdapterSpec {
        spec_by_id("gdb").unwrap()
    }

    #[test]
    fn codelldb_needs_no_formatters_imported_and_is_told_the_language() {
        assert_eq!(rendering(codelldb()), RustRendering::Builtin);
        assert!(init_commands(codelldb()).is_empty(), "it ships its own");

        let extras = launch_extras(codelldb());
        let keys: Vec<&str> = extras.iter().map(|(k, _)| *k).collect();
        assert!(keys.contains(&"sourceLanguages"));
        assert!(keys.contains(&"expressions"));
        assert!(!keys.contains(&"initCommands"));
        // The default dialect is pinned so a user's own VS Code settings cannot change what an
        // expression means inside Bennu.
        let (_, expressions) = extras.iter().find(|(k, _)| *k == "expressions").unwrap();
        assert_eq!(expressions, "simple");
    }

    /// The fix that matters on macOS: `lldb-dap` is what most machines have, and it knows nothing
    /// about Rust until the toolchain's own formatters are imported into it.
    #[test]
    fn a_plain_lldb_imports_the_toolchains_formatters_when_there_is_a_toolchain() {
        match rendering(lldb_dap()) {
            RustRendering::Toolchain(etc) => {
                let commands = init_commands(lldb_dap());
                assert_eq!(commands.len(), 2, "import the module, then source the type commands");
                assert!(commands[0].starts_with("command script import "));
                assert!(commands[0].contains("lldb_lookup.py"));
                // Not stop-on-error: `lldb_commands` targets several LLDB versions and one line the
                // running one rejects must not abandon the rest.
                assert!(commands[1].starts_with("command source -s 0 "));
                assert!(commands[1].contains("lldb_commands"));
                assert!(etc.join("lldb_providers.py").is_file(), "the module imports this sibling");

                let keys: Vec<&str> = launch_extras(lldb_dap()).iter().map(|(k, _)| *k).collect();
                assert!(keys.contains(&"initCommands"));
                // A struct of your own has no formatter anywhere; without this its row is blank.
                assert!(keys.contains(&"enableAutoVariableSummaries"));
            }
            // A machine with no Rust installed. Then there is nothing to import, and the honest
            // answer is the caveat rather than a broken import.
            RustRendering::Raw => {
                assert!(init_commands(lldb_dap()).is_empty());
                assert!(rendering(lldb_dap()).caveat().is_some());
            }
            other => panic!("a plain LLDB is Toolchain or Raw, never {other:?}"),
        }
    }

    #[test]
    fn gdb_is_left_to_its_own_auto_loading() {
        assert_eq!(rendering(gdb()), RustRendering::Embedded);
        assert!(init_commands(gdb()).is_empty(), "its DAP mode has no hook to import through");
        assert!(rendering(gdb()).caveat().is_none());
    }

    /// The caveat exists for exactly one situation, and it says what to do rather than what is wrong.
    #[test]
    fn only_a_bare_lldb_carries_a_caveat() {
        assert!(RustRendering::Raw.caveat().is_some());
        assert!(!RustRendering::Raw.renders_rust());
        for good in [
            RustRendering::Builtin,
            RustRendering::Embedded,
            RustRendering::Toolchain(PathBuf::from("/x")),
        ] {
            assert!(good.caveat().is_none(), "{good:?}");
            assert!(good.renders_rust(), "{good:?}");
        }
    }

    #[test]
    fn only_codelldb_offers_more_than_one_dialect() {
        assert_eq!(evaluators(codelldb()).len(), 3);
        assert_eq!(evaluators(lldb_dap()), &[Evaluator::Native]);
        assert_eq!(evaluators(gdb()), &[Evaluator::Native]);
    }

    #[test]
    fn a_dialect_prefix_is_peeled_off_and_needs_whitespace_after_it() {
        assert_eq!(split_dialect("/nat v.size()"), (Some(Evaluator::Native), "v.size()"));
        assert_eq!(split_dialect("/py len($v)"), (Some(Evaluator::Python), "len($v)"));
        assert_eq!(split_dialect("  /se  order.total "), (Some(Evaluator::Simple), "order.total"));
        // A bare prefix is recognised, so the caller can say "and then what?".
        assert_eq!(split_dialect("/nat"), (Some(Evaluator::Native), ""));
        // Not a dialect: an expression that merely begins with a slash. `/self.len` would be a very
        // confusing thing to reinterpret.
        assert_eq!(split_dialect("/self.len"), (None, "/self.len"));
        assert_eq!(split_dialect("/nope x"), (None, "/nope x"));
        assert_eq!(split_dialect("order.total"), (None, "order.total"));
        assert_eq!(split_dialect(""), (None, ""));
    }

    #[test]
    fn every_dialect_has_a_distinct_prefix_and_setting_name() {
        let all = [Evaluator::Simple, Evaluator::Native, Evaluator::Python];
        for a in all {
            assert!(a.prefix().starts_with('/'));
            assert!(!a.describe().is_empty());
            assert_eq!(Evaluator::from_prefix(a.prefix()), Some(a));
            for b in all {
                if a != b {
                    assert_ne!(a.prefix(), b.prefix());
                    assert_ne!(a.setting(), b.setting());
                }
            }
        }
        assert_eq!(Evaluator::from_prefix("/x"), None);
    }
}
