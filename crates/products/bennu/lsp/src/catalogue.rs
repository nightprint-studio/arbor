//! The servers Bennu knows how to run, and the two questions that decide whether one
//! *should* run for a given file: which extensions it serves, and which manifest marks a
//! workspace it can analyse.
//!
//! The root markers are the real gate, and they are why registering a server for `.js`
//! does not mean starting a TypeScript server inside a Struts project: without a
//! `package.json` above the file there is no workspace to open, so nothing starts. The
//! same rule keeps a stray `.py` in a Java repo from spawning a Python server.
//!
//! Adding a server is one entry here. Adding one *without* touching this file is what the
//! user's own `[[lsp.servers]]` config is for — same fields, read at runtime — so a
//! language nobody anticipated needs no release.

use std::path::{Path, PathBuf};

/// A language server Bennu can start.
#[derive(Debug, Clone, Copy)]
pub struct ServerSpec {
    /// Stable id — the config key for a path override or a disable.
    pub id: &'static str,
    /// Display name.
    pub name: &'static str,
    /// The LSP `languageId` sent in `didOpen`, and Bennu's own key for the language.
    pub language: &'static str,
    /// The executable to look for.
    pub cmd: &'static str,
    /// Arguments that put the server in stdio mode. Several servers default to a socket
    /// and need telling.
    pub args: &'static [&'static str],
    /// Lowercase file extensions, no dot.
    pub extensions: &'static [&'static str],
    /// Files whose presence marks a workspace root this server can analyse. Searched from
    /// the file upwards.
    pub root_markers: &'static [&'static str],
    /// What to tell the user when the binary isn't there. A bare "not found" leaves them
    /// with no next step, which is the whole reason this field exists.
    ///
    /// It says **where the server comes from**, not what to type: when [`install`] is
    /// non-empty the command is shown from that, verbatim and copy-pasteable, beside the
    /// button that runs it. Spelling it here too would print it twice on the same row. The
    /// servers with no install command are the exception — for those this carries the whole
    /// instruction, because nothing else does.
    ///
    /// [`install`]: ServerSpec::install
    pub install_hint: &'static str,
    /// The command that installs it, argv-style, or empty when there is none to run.
    ///
    /// A *command*, not a download. Every server here ships through a package manager its
    /// own ecosystem already has — `rustup`, `cargo`, `go`, `npm` — and running that is both
    /// far shorter than a downloader (no release-asset naming, no archive formats, no
    /// signature story, no upgrade path to invent) and the thing the user would have run.
    /// It also means the binary lands where the rest of their toolchain is, so it keeps
    /// working after Arbor is updated or removed.
    ///
    /// Empty for the ones whose install is a system package (clangd is LLVM, lua-language-
    /// server is Homebrew): Bennu will not run a package manager that manages the machine.
    pub install: &'static [&'static str],
}

/// Every built-in server, in the order the settings panel lists them.
pub const BUILTIN_SERVERS: &[ServerSpec] = &[
    ServerSpec {
        id: "rust-analyzer",
        name: "rust-analyzer",
        language: "rust",
        cmd: "rust-analyzer",
        args: &[],
        extensions: &["rs"],
        root_markers: &["Cargo.toml"],
        install_hint: "It ships with the Rust toolchain.",
        install: &["rustup", "component", "add", "rust-analyzer"],
    },
    ServerSpec {
        id: "gopls",
        name: "gopls",
        language: "go",
        cmd: "gopls",
        args: &[],
        extensions: &["go"],
        root_markers: &["go.mod", "go.work"],
        install_hint: "It is installed with the Go toolchain.",
        install: &["go", "install", "golang.org/x/tools/gopls@latest"],
    },
    ServerSpec {
        id: "pyright",
        name: "Pyright",
        language: "python",
        cmd: "pyright-langserver",
        args: &["--stdio"],
        extensions: &["py", "pyi"],
        root_markers: &["pyproject.toml", "setup.py", "setup.cfg", "requirements.txt"],
        install_hint: "It is distributed on npm.",
        install: &["npm", "install", "-g", "pyright"],
    },
    ServerSpec {
        id: "clangd",
        name: "clangd",
        language: "cpp",
        cmd: "clangd",
        args: &["--background-index"],
        extensions: &["c", "cc", "cpp", "cxx", "h", "hh", "hpp", "hxx", "m", "mm"],
        root_markers: &["compile_commands.json", "compile_flags.txt", "CMakeLists.txt"],
        install_hint: "Install the `clangd` package (LLVM), or Xcode's command-line tools.",
        // A system package. Bennu installs language servers, not toolchains.
        install: &[],
    },
    ServerSpec {
        id: "typescript",
        name: "TypeScript / JavaScript",
        language: "typescript",
        cmd: "typescript-language-server",
        args: &["--stdio"],
        extensions: &["ts", "tsx", "mts", "cts", "js", "jsx", "mjs", "cjs"],
        root_markers: &["tsconfig.json", "jsconfig.json", "package.json"],
        install_hint: "It is distributed on npm.",
        install: &["npm", "install", "-g", "typescript-language-server", "typescript"],
    },
    ServerSpec {
        id: "lua",
        name: "Lua",
        language: "lua",
        cmd: "lua-language-server",
        args: &[],
        extensions: &["lua"],
        root_markers: &[".luarc.json", "plugin.toml", ".luacheckrc"],
        install_hint: "Install the `lua-language-server` package \
                       (Homebrew: `brew install lua-language-server`).",
        // Homebrew / the distro. Same rule as clangd.
        install: &[],
    },
    ServerSpec {
        id: "wgsl-analyzer",
        name: "wgsl-analyzer",
        language: "wgsl",
        cmd: "wgsl-analyzer",
        args: &[],
        extensions: &["wgsl"],
        // A shader lives in a Cargo project (Bevy) or beside the code that loads it. Both
        // markers, because a `.wgsl` in an `assets/` folder of a non-Rust project is still a
        // shader — and without a marker above it nothing would start.
        root_markers: &["Cargo.toml", ".git"],
        install_hint: "It is not published on crates.io, so it is built from its repository \
                       — a few minutes the first time.",
        // Not on crates.io, so the git form. The package is `wgsl-analyzer` with a HYPHEN —
        // the repository is a workspace of twenty crates and only this one has a binary, and
        // naming it with an underscore (as some install instructions do) fails with
        // "could not find `wgsl_analyzer` … with version `*`", which reads like a version
        // problem and is a spelling one.
        install: &[
            "cargo",
            "install",
            "--git",
            "https://github.com/wgsl-analyzer/wgsl-analyzer",
            // Build against the lockfile the repository ships. Without it cargo re-resolves
            // every dependency to its newest compatible version, and a language server built
            // from source is exactly the kind of large dependency tree where that turns a
            // working install into a compile error in somebody else's crate.
            "--locked",
            "wgsl-analyzer",
        ],
    },
];

impl ServerSpec {
    /// Whether this server serves `file`, by extension.
    pub fn serves(&self, file: &str) -> bool {
        let ext = extension_of(file);
        !ext.is_empty() && self.extensions.contains(&ext.as_str())
    }

    /// Directories to search **before** `PATH`.
    ///
    /// Exists for one situation, and it is not hypothetical: `~/.cargo/bin/rust-analyzer` is
    /// normally a **rustup proxy**, present whether or not the component is installed, and
    /// `~/.cargo/bin` is very often on `PATH`. Searching `PATH` first therefore finds the proxy
    /// and the spawn "succeeds" — then the process dies with `Unknown binary 'rust-analyzer' in
    /// official toolchain`. The toolchain's real binary has to be able to win from wherever it
    /// is, so it is looked for before `PATH` rather than after.
    pub fn preferred_dirs(&self) -> Vec<PathBuf> {
        match self.id {
            "rust-analyzer" => rustup_toolchain_bins(),
            _ => Vec::new(),
        }
    }

    /// Directories to search **after** `PATH` and the generic ones — where this particular
    /// server also installs itself.
    ///
    /// Keyed on `id` rather than carried as a function pointer per entry: the list is
    /// short, the special cases are genuinely per-server, and a `match` keeps the
    /// catalogue above readable as data.
    pub fn extra_dirs(&self) -> Vec<PathBuf> {
        match self.id {
            "rust-analyzer" => rust_analyzer_dirs(),
            "gopls" => go_dirs(),
            _ => Vec::new(),
        }
    }

    /// Whether a resolved candidate can actually run.
    ///
    /// Exists because "the file is there" and "it works" come apart in one specific, common way:
    /// `~/.cargo/bin/rust-analyzer` is normally a **rustup proxy**, present whether or not the
    /// component is installed. Accepting it produces a green path in the settings panel, a
    /// successful spawn, and a server that dies immediately with
    /// `Unknown binary 'rust-analyzer' in official toolchain`. Rejecting it instead reports
    /// *not installed* — which is the truth, and which comes with the one command that fixes it.
    pub fn accepts(&self, path: &Path) -> bool {
        match self.id {
            "rust-analyzer" => !is_dead_rustup_proxy(path, "rust-analyzer"),
            _ => true,
        }
    }

    /// The server-specific `initializationOptions`.
    ///
    /// Only rust-analyzer gets any, and only the settings whose defaults are wrong for an editor
    /// that is not VS Code — see [`rust_analyzer_init_options`].
    ///
    /// `check_command` is passed in rather than read from a file: this crate is a leaf that knows
    /// nothing about where Bennu keeps its settings, and the day a second host uses it that has to
    /// stay true. An empty string means the default.
    pub fn init_options(&self, check_command: &str) -> Option<serde_json::Value> {
        match self.id {
            "rust-analyzer" => Some(rust_analyzer_init_options(check_command)),
            _ => None,
        }
    }
}

/// Whether `file` lives in a **read-only dependency source** — somewhere a package manager
/// unpacked code that belongs to a project rather than being one.
///
/// This exists because of a specific, expensive mistake. `find_root` looks for the highest
/// `Cargo.toml` above a file, and an unpacked crate in `~/.cargo/registry/src` has one of its own —
/// so a go-to-definition that lands in a dependency's source resolves that dependency's directory
/// as a *workspace root* and starts a second language server for it. One per dependency you look
/// into, each indexing a crate from scratch, none of them able to answer anything about your code.
///
/// The right answer is always the session that already has the file: your workspace's server has
/// every dependency's source in its VFS, because that is what "resolved the crate graph" means.
///
/// The locations, per ecosystem, and all of them relative to a package manager's home rather than
/// matched on a path fragment — a directory of yours called `checkouts` is not vendored code:
///
/// | Ecosystem | Where |
/// |---|---|
/// | Cargo | `$CARGO_HOME/registry/src`, `$CARGO_HOME/git/checkouts` |
/// | Rust std | `$RUSTUP_HOME/toolchains/*/lib/rustlib/src` — what `Ctrl+B` on `Vec` opens |
/// | Go | `$GOPATH/pkg/mod`, `$GOMODCACHE` |
///
/// `node_modules` is deliberately **not** here: an npm dependency's `package.json` makes it a root
/// by the same accident, but a TypeScript server started for one is at least about code the
/// project imports directly, and the directory is inside the project rather than in a shared cache —
/// so stopping it would need a different rule than "not mine". Left for when it is a real report.
pub fn is_dependency_source(file: &Path) -> bool {
    is_under_any(file, &dependency_source_dirs())
}

/// Whether `file` is inside any of `dirs`.
///
/// [`Path::starts_with`] and not a string prefix, because it matches whole **components**: a
/// project of yours at `~/.cargo/registry/srcgen` is not inside `~/.cargo/registry/src`, and a
/// string comparison would say it was.
fn is_under_any(file: &Path, dirs: &[PathBuf]) -> bool {
    dirs.iter().any(|d| file.starts_with(d))
}

/// The directories [`is_dependency_source`] tests against.
fn dependency_source_dirs() -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Some(cargo) = env_dir("CARGO_HOME").or_else(|| home_join(".cargo")) {
        out.push(cargo.join("registry").join("src"));
        out.push(cargo.join("git").join("checkouts"));
    }
    if let Some(rustup) = env_dir("RUSTUP_HOME").or_else(|| home_join(".rustup")) {
        for toolchain in child_dirs(&rustup.join("toolchains")) {
            out.push(toolchain.join("lib").join("rustlib").join("src"));
        }
    }
    if let Some(cache) = env_dir("GOMODCACHE") {
        out.push(cache);
    }
    if let Some(gopath) = env_dir("GOPATH").or_else(|| home_join("go")) {
        out.push(gopath.join("pkg").join("mod"));
    }
    out
}

/// The built-in spec with this id.
pub fn spec_by_id(id: &str) -> Option<&'static ServerSpec> {
    BUILTIN_SERVERS.iter().find(|s| s.id == id)
}

/// The lowercase extension of `file`, without the dot. Empty when it has none.
pub fn extension_of(file: &str) -> String {
    let name = file.rsplit(['/', '\\']).next().unwrap_or(file);
    match name.rsplit_once('.') {
        Some((_, ext)) if !ext.is_empty() => ext.to_ascii_lowercase(),
        _ => String::new(),
    }
}

/// Walk up from `file` looking for any of `markers`; return the directory holding one.
///
/// The **highest** match wins, not the nearest: in a Cargo workspace every member crate
/// has its own `Cargo.toml`, and starting a server per member would run four
/// rust-analyzers over the same code, each blind to the others' crates. One server at the
/// workspace root sees the whole graph — which is also the only way cross-crate go-to
/// works.
pub fn find_root(file: &Path, markers: &[&str]) -> Option<PathBuf> {
    let mut highest = None;
    let mut dir = if file.is_dir() { Some(file) } else { file.parent() };
    while let Some(d) = dir {
        if markers.iter().any(|m| d.join(m).is_file()) {
            highest = Some(d.to_path_buf());
        }
        dir = d.parent();
    }
    highest
}

/// `rustup`'s toolchain dirs and the other places a rust-analyzer binary lives.
///
/// The list is longer than it looks like it should be for the reason the JDK and IDE
/// discovery already learned the hard way: a windowed app on macOS inherits launchd's
/// minimal `PATH` (`/usr/bin:/bin:/usr/sbin:/sbin`), not the shell's — so `~/.cargo/bin`,
/// which is where `rustup` puts everything, is invisible unless it is named here.
/// The `bin` directory of every installed rustup toolchain, where the `rust-analyzer`
/// **component's real binary** lives.
///
/// Scanned rather than resolved through `rustup which`, which would mean spawning a process
/// every time the settings panel lists the servers.
fn rustup_toolchain_bins() -> Vec<PathBuf> {
    let Some(rustup) = env_dir("RUSTUP_HOME").or_else(|| home_join(".rustup")) else {
        return Vec::new();
    };
    child_dirs(&rustup.join("toolchains")).into_iter().map(|t| t.join("bin")).collect()
}

/// Whether `path` is rustup's proxy for a component that is **not installed**.
///
/// Two questions, and both are needed:
///
/// 1. **Is it the proxy at all?** A `cargo install`ed binary lives in the same directory and is
///    perfectly real, so this cannot be decided by location. rustup installs its proxies as
///    symlinks to `rustup` on Unix, and as copies of `rustup.exe` on Windows — so: a symlink, or
///    a file exactly as large as the `rustup` sitting beside it.
/// 2. **Would it work?** The proxy resolves only when a toolchain actually ships the component's
///    binary. If one does, the proxy is fine and is left alone.
///
/// Getting (1) wrong in the permissive direction is the safe failure — a real binary stays
/// accepted — which is why the identification is required rather than assumed from the path.
fn is_dead_rustup_proxy(path: &Path, component: &str) -> bool {
    let Some(parent) = path.parent() else { return false };
    let Some(cargo_bin) =
        env_dir("CARGO_HOME").or_else(|| home_join(".cargo")).map(|c| c.join("bin"))
    else {
        return false;
    };
    if parent != cargo_bin {
        return false; // outside cargo's bin dir there is no proxy
    }
    if !looks_like_rustup_proxy(path, parent) {
        return false; // a real, self-installed binary
    }
    // A proxy is only dead when no toolchain carries the component.
    !rustup_toolchain_bins().iter().any(|bin| {
        bin.join(component).is_file() || bin.join(format!("{component}.exe")).is_file()
    })
}

/// Whether `path` has the shape rustup gives its proxies.
fn looks_like_rustup_proxy(path: &Path, dir: &Path) -> bool {
    // Unix: a symlink to `rustup`.
    if std::fs::symlink_metadata(path).map(|m| m.file_type().is_symlink()).unwrap_or(false) {
        return true;
    }
    // Windows (and a hardlinked Unix install): byte-identical in size to the `rustup` beside it.
    let rustup = ["rustup", "rustup.exe"].iter().map(|n| dir.join(n)).find(|p| p.is_file());
    let Some(rustup) = rustup else { return false };
    match (std::fs::metadata(path), std::fs::metadata(&rustup)) {
        (Ok(a), Ok(b)) => a.len() == b.len() && a.len() > 0,
        _ => false,
    }
}

/// The other places a rust-analyzer binary lives — searched after `PATH`.
fn rust_analyzer_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();

    // `~/.cargo/bin`: the rustup proxy (which fails when the component is not installed — see
    // `preferred_dirs`), but also where a `cargo install`ed one lands, which is a real binary
    // and has to keep working.
    if let Some(cargo) = env_dir("CARGO_HOME").or_else(|| home_join(".cargo")) {
        dirs.push(cargo.join("bin"));
    }
    // The VS Code extension ships its own, which is often the only copy on a machine
    // whose owner never ran `rustup component add`.
    for editor in [".vscode", ".vscode-insiders", ".vscode-oss", ".cursor"] {
        let Some(exts) = home_join(editor).map(|d| d.join("extensions")) else { continue };
        for dir in child_dirs(&exts) {
            let name = dir.file_name().and_then(|n| n.to_str()).unwrap_or_default();
            if name.starts_with("rust-lang.rust-analyzer-") {
                dirs.push(dir.join("server"));
            }
        }
    }
    dirs
}

/// Where `go install` puts binaries.
fn go_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Some(gobin) = env_dir("GOBIN") {
        dirs.push(gobin);
    }
    if let Some(gopath) = env_dir("GOPATH") {
        dirs.push(gopath.join("bin"));
    }
    if let Some(home) = home_join("go") {
        dirs.push(home.join("bin"));
    }
    dirs
}

/// rust-analyzer's `initializationOptions`.
///
/// Two settings, both because rust-analyzer's defaults assume a client that Bennu is not:
///
/// * **`checkOnSave` stays on**, running whatever `check_command` says — `cargo check` by default,
///   `cargo clippy` when the user asked for it. This is what produces real diagnostics — type
///   errors, borrow errors — as opposed to the syntactic ones the parser alone can see. It costs a
///   build after each save, which is the deal every Rust editor makes.
/// * **`procMacro` and `cargo.buildScripts` on**, because a project that uses derive
///   macros (which in practice is all of them — `serde`, `thiserror`) resolves almost
///   nothing without them: no `Deserialize`, no generated field accessors, and a page of
///   false "unresolved" diagnostics.
/// * **the code lenses Bennu can honour, and only those** — see [`LENS_OPTIONS`].
///
/// Deliberately *not* set: inlay hints (Bennu does not render them yet, so asking for them
/// is work the server does for nobody) and any `rustfmt` override (the project's own
/// `rustfmt.toml` is the authority, and second-guessing it is how a formatter starts
/// fighting the repository).
fn rust_analyzer_init_options(check_command: &str) -> serde_json::Value {
    // Anything other than the two we offer would be passed straight through to a server that then
    // fails on every save with an unknown subcommand — so an unrecognised value reads as the
    // default rather than as an instruction.
    let command = match check_command.trim() {
        "clippy" => "clippy",
        _ => "check",
    };
    serde_json::json!({
        "checkOnSave": true,
        "check": { "command": command },
        "procMacro": { "enable": true },
        "cargo": { "buildScripts": { "enable": true } },
        "completion": { "autoimport": { "enable": true } },
        "lens": lens_options(),
    })
}

/// How large a query cache a background session may keep.
///
/// rust-analyzer's own default is *unbounded* — the cache grows with whatever has been asked, which
/// is the right trade when a person is typing into the answer and the wrong one for a session that
/// answered three questions an hour ago. A bound costs recomputation, never correctness.
const BACKGROUND_LRU_CAPACITY: u32 = 64;

/// How many worker threads a background session may use.
///
/// Its default is every core. Two is enough to answer a request in a reasonable time and leaves the
/// machine to whoever is actually sitting at it — which for a session with no window is everybody
/// else.
const BACKGROUND_THREADS: u32 = 2;

/// The `initializationOptions` a session gets when **no window is showing its project** — one
/// started to answer a request rather than because somebody opened the project.
///
/// Merged over [`ServerSpec::init_options`], shallowly and deliberately: none of these keys appear
/// there, so the merge is an addition rather than an override, and the settings that make a Rust
/// project resolve at all are untouched by construction.
///
/// The three chosen all trade *time* for *resources* and none of them removes an answer:
///
/// * **`cachePriming` off.** On open, rust-analyzer primes its caches for every crate in the
///   workspace — the long "indexing" bar, and on a twenty-crate workspace the bulk of the cost of
///   starting one at all. Off, the work happens when a request needs that crate: a session asked
///   about two files analyses two files. The first question about a cold crate is slower; the
///   ninety that never come cost nothing.
/// * **`lru.capacity` bounded** — see [`BACKGROUND_LRU_CAPACITY`].
/// * **`numThreads` bounded** — see [`BACKGROUND_THREADS`].
///
/// **Deliberately absent, and this is the important half.** `procMacro` and `cargo.buildScripts`
/// are the two settings a naive tuning turns off first, because they are the most expensive — and
/// they are the two that must never be touched. A Bevy project resolves almost nothing without
/// proc macros: every `#[derive(Component)]`, `#[derive(Resource)]`, `#[derive(Bundle)]` and
/// reflect derive becomes unresolved, so the saving buys a project that reads as catastrophically
/// broken while compiling perfectly. `cargo.allTargets` is absent for a smaller version of the same
/// reason: it would halve the graph by dropping tests, on a tool surface that can run them.
///
/// `checkOnSave` is left alone for now. It is the largest recurring cost and the strongest
/// candidate — a caller that can compile on demand does not need an ambient one — but turning it
/// off makes Rust diagnostics silently unavailable to the per-file check, and that has to be said
/// in the same change rather than discovered.
pub fn background_init_options(id: &str) -> Option<serde_json::Value> {
    (id == "rust-analyzer").then(|| {
        serde_json::json!({
            "cachePriming": { "enable": false },
            "lru": { "capacity": BACKGROUND_LRU_CAPACITY },
            "numThreads": BACKGROUND_THREADS,
        })
    })
}

/// Which code lenses to ask rust-analyzer for.
///
/// Every one of these is a client-side command, so the rule for turning one on is "Bennu can do
/// what pressing it promises". That is what each entry is deciding:
///
/// * **implementations** — on. The command carries the locations it counted, so pressing it shows
///   them without a second query.
/// * **references on types and traits** — on, and off for methods and enum variants. The server
///   defaults all four off because each one is a reference query per item, and methods are by far
///   the most numerous items in a file: the two that stay on are the ones where the count answers a
///   question you were going to ask ("is anything using this type"), at a cost proportional to the
///   declarations rather than to the members.
/// * **run and debug** — off, because Bennu has no runnable runner yet. rust-analyzer defaults them
///   on, which would put a ▶ Run above every `fn main` and every `#[test]` that did nothing when
///   pressed, and a control that does nothing teaches that the feature is broken. Turn these on in
///   the same change that teaches the Run console to launch a `runnable`.
fn lens_options() -> serde_json::Value {
    serde_json::json!({
        "enable": true,
        "implementations": { "enable": true },
        "references": {
            "adt": { "enable": true },
            "trait": { "enable": true },
            "method": { "enable": false },
            "enumVariant": { "enable": false },
        },
        "run": { "enable": false },
        "debug": { "enable": false },
    })
}

/// An existing directory named by an environment variable.
fn env_dir(var: &str) -> Option<PathBuf> {
    let value = std::env::var_os(var)?;
    if value.is_empty() {
        return None;
    }
    Some(PathBuf::from(value))
}

/// `<home>/<child>`, when the home directory resolves.
fn home_join(child: &str) -> Option<PathBuf> {
    arbor_core::prelude::user_home().map(|h| h.join(child))
}

/// The immediate sub-directories of `dir`, sorted by name so discovery is deterministic
/// (two toolchains, or two extension versions, must not resolve differently per run).
fn child_dirs(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(dir) else { return Vec::new() };
    let mut out: Vec<PathBuf> =
        entries.flatten().map(|e| e.path()).filter(|p| p.is_dir()).collect();
    out.sort();
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The containment rule, on paths this test owns — the environment is not mutated, because
    /// cargo runs these as threads in one process and a `set_var` would leak into every other test.
    #[test]
    fn a_file_is_under_a_directory_by_components_not_by_string_prefix() {
        let registry = PathBuf::from("/home/me/.cargo/registry/src");
        let dirs = vec![registry.clone()];

        assert!(is_under_any(Path::new("/home/me/.cargo/registry/src/idx/serde-1.0.0/src/lib.rs"), &dirs));
        assert!(is_under_any(&registry, &dirs), "the directory itself counts");

        // A project of yours that merely starts with the same letters is NOT inside it. This is the
        // whole reason the check is `Path::starts_with` and not `str::starts_with`.
        assert!(!is_under_any(Path::new("/home/me/.cargo/registry/srcgen/Cargo.toml"), &dirs));
        assert!(!is_under_any(Path::new("/home/me/work/project/src/main.rs"), &dirs));
        assert!(!is_under_any(Path::new("/home/me/.cargo/bin/rust-analyzer"), &dirs));
    }

    /// The locations themselves. Asserted against whatever this machine resolves rather than a
    /// fixture, because the point of the list is that it is relative to a package manager's home.
    #[test]
    fn the_dependency_locations_include_cargos_unpacked_sources() {
        let dirs = dependency_source_dirs();
        let cargo = std::env::var_os("CARGO_HOME")
            .map(PathBuf::from)
            .or_else(|| home_join(".cargo"));
        if let Some(cargo) = cargo {
            let src = cargo.join("registry").join("src");
            assert!(dirs.contains(&src), "{src:?} missing from {dirs:?}");
            assert!(dirs.contains(&cargo.join("git").join("checkouts")));
            // …and a file in one of them is recognised through the public entry point.
            assert!(is_dependency_source(&src.join("index.crates.io-1949cf/serde-1.0.219/src/lib.rs")));
        }
        // A path that is nobody's cache is never a dependency source, on any machine.
        assert!(!is_dependency_source(Path::new("/tmp/some-project/src/main.rs")));
    }

    #[test]
    fn the_check_command_reaches_the_init_options_and_only_the_two_we_offer_do() {
        let ra = spec_by_id("rust-analyzer").unwrap();
        let command = |choice: &str| {
            ra.init_options(choice).unwrap()["check"]["command"].as_str().unwrap().to_string()
        };
        assert_eq!(command("check"), "check");
        assert_eq!(command("clippy"), "clippy");
        // Anything else would be passed to a server that then fails on every save with an unknown
        // subcommand, so it reads as the default rather than as an instruction.
        assert_eq!(command(""), "check");
        assert_eq!(command("cranky"), "check");
        assert_eq!(command("  clippy  "), "clippy", "trimmed");
        // And a server that gets no init options still gets none.
        assert!(spec_by_id("gopls").unwrap().init_options("clippy").is_none());
    }

    #[test]
    fn only_the_lenses_bennu_can_honour_are_asked_for() {
        let opts = spec_by_id("rust-analyzer").unwrap().init_options("check").unwrap();
        let lens = &opts["lens"];
        assert_eq!(lens["enable"], serde_json::json!(true));
        assert_eq!(lens["implementations"]["enable"], serde_json::json!(true));
        // The guard that matters: a ▶ Run lens Bennu cannot launch is a control that does nothing,
        // and rust-analyzer turns both of these on by default. They go on in the same change that
        // teaches the Run console to launch a runnable — not before.
        assert_eq!(lens["run"]["enable"], serde_json::json!(false));
        assert_eq!(lens["debug"]["enable"], serde_json::json!(false));
        // Reference counts on declarations, not on members: one query per item, and methods are the
        // most numerous items in a file.
        assert_eq!(lens["references"]["adt"]["enable"], serde_json::json!(true));
        assert_eq!(lens["references"]["method"]["enable"], serde_json::json!(false));
    }

    #[test]
    fn extensions_are_matched_case_insensitively_and_only_at_the_end() {
        let ra = spec_by_id("rust-analyzer").unwrap();
        assert!(ra.serves("/p/src/main.rs"));
        assert!(ra.serves(r"C:\p\src\Main.RS"), "extensions are case-insensitive");
        assert!(!ra.serves("/p/rs"), "a file named `rs` has no extension");
        assert!(!ra.serves("/p/main.rs.bak"));
        assert!(!ra.serves("/p/Cargo.toml"));
    }

    #[test]
    fn extension_of_handles_both_separators_and_the_no_extension_cases() {
        assert_eq!(extension_of("/a/b/c.RS"), "rs");
        assert_eq!(extension_of(r"C:\a\b.TOML"), "toml");
        assert_eq!(extension_of("/a/Makefile"), "");
        assert_eq!(extension_of("/a/.gitignore"), "gitignore");
        assert_eq!(extension_of("/a/b."), "", "a trailing dot is not an extension");
        // A dot in a directory name must not be read as the file's extension.
        assert_eq!(extension_of("/a.b/Makefile"), "");
    }

    #[test]
    fn every_catalogue_entry_is_well_formed() {
        let mut ids = std::collections::HashSet::new();
        for s in BUILTIN_SERVERS {
            assert!(ids.insert(s.id), "duplicate server id {}", s.id);
            assert!(!s.extensions.is_empty(), "{} serves no extension", s.id);
            assert!(!s.root_markers.is_empty(), "{} has no root marker → it would never start", s.id);
            assert!(!s.install_hint.is_empty(), "{} has no install hint", s.id);
            for e in s.extensions {
                assert_eq!(*e, e.to_ascii_lowercase(), "{}: extensions must be lowercase", s.id);
                assert!(!e.starts_with('.'), "{}: extensions carry no dot", s.id);
            }
        }
    }

    #[test]
    fn no_two_builtin_servers_claim_the_same_extension() {
        // Two servers for one extension would make "which server owns this file" a
        // coin flip that depends on catalogue order.
        let mut owner = std::collections::HashMap::new();
        for s in BUILTIN_SERVERS {
            for e in s.extensions {
                if let Some(prev) = owner.insert(*e, s.id) {
                    panic!("both {prev} and {} claim .{e}", s.id);
                }
            }
        }
    }

    #[test]
    fn find_root_picks_the_highest_marker_not_the_nearest() {
        // A Cargo workspace: every member has a `Cargo.toml`, and one server at the top is
        // the only arrangement in which cross-crate go-to works.
        let tmp = std::env::temp_dir().join(format!("bennu-lsp-root-{}", std::process::id()));
        let member = tmp.join("crates").join("leaf").join("src");
        std::fs::create_dir_all(&member).unwrap();
        std::fs::write(tmp.join("Cargo.toml"), "[workspace]\n").unwrap();
        std::fs::write(tmp.join("crates").join("leaf").join("Cargo.toml"), "[package]\n").unwrap();
        let file = member.join("lib.rs");
        std::fs::write(&file, "").unwrap();

        let root = find_root(&file, &["Cargo.toml"]).expect("a root");
        assert_eq!(
            root.canonicalize().unwrap(),
            tmp.canonicalize().unwrap(),
            "the workspace root, not the member crate"
        );

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn find_root_yields_nothing_without_a_marker() {
        // The gate that keeps a stray `.py` in a Java repo from starting a Python server.
        let tmp = std::env::temp_dir().join(format!("bennu-lsp-noroot-{}", std::process::id()));
        std::fs::create_dir_all(&tmp).unwrap();
        let file = tmp.join("script.py");
        std::fs::write(&file, "").unwrap();
        assert!(find_root(&file, &["pyproject.toml", "setup.py"]).is_none());
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn only_rust_analyzer_carries_init_options_and_they_enable_proc_macros() {
        let ra = spec_by_id("rust-analyzer").unwrap();
        let opts = ra.init_options("check").expect("rust-analyzer is configured");
        // Without proc macros a serde project resolves almost nothing and reports a page
        // of false "unresolved" errors.
        assert_eq!(opts["procMacro"]["enable"], serde_json::json!(true));
        assert_eq!(opts["cargo"]["buildScripts"]["enable"], serde_json::json!(true));
        assert_eq!(opts["check"]["command"], serde_json::json!("check"));
        assert!(opts.get("rustfmt").is_none(), "the project's rustfmt.toml is the authority");
        assert!(spec_by_id("gopls").unwrap().init_options("check").is_none());
    }

    #[test]
    fn a_server_that_needs_stdio_is_told_so() {
        // Several servers default to a socket; forgetting the flag is a start that hangs
        // with no output at all.
        for id in ["pyright", "typescript"] {
            let s = spec_by_id(id).unwrap();
            assert!(s.args.contains(&"--stdio"), "{id} must be put in stdio mode");
        }
    }

    /// The rustup-proxy trap, end to end. A symlink to `rustup` in cargo's bin dir, with no
    /// toolchain carrying the component, is exactly what `Unknown binary 'rust-analyzer' in
    /// official toolchain` comes from — and it has to read as "not installed", not as "found".
    #[test]
    fn a_rustup_proxy_is_only_rejected_when_it_is_one_and_would_fail() {
        let tmp = std::env::temp_dir().join(format!("bennu-lsp-proxy-{}", std::process::id()));
        let cargo_bin = tmp.join(".cargo").join("bin");
        std::fs::create_dir_all(&cargo_bin).unwrap();
        std::fs::write(cargo_bin.join("rustup"), b"rustup").unwrap();

        // Outside cargo's bin dir nothing is ever treated as a proxy.
        let elsewhere = tmp.join("usr-local-bin");
        std::fs::create_dir_all(&elsewhere).unwrap();
        let real_elsewhere = elsewhere.join("rust-analyzer");
        std::fs::write(&real_elsewhere, b"#!/bin/sh\n").unwrap();

        // A `cargo install`ed binary lives in cargo's bin dir and is real: a regular file whose
        // size differs from `rustup`'s. It must NOT be rejected.
        let real_in_cargo = cargo_bin.join("rust-analyzer");
        std::fs::write(&real_in_cargo, b"a much longer fake binary body").unwrap();

        // `CARGO_HOME` / `RUSTUP_HOME` are read from the environment, so this test asserts the
        // parts it can reach without mutating process-globals (cargo runs tests as threads).
        assert!(
            !looks_like_rustup_proxy(&real_in_cargo, &cargo_bin),
            "a regular file of a different size is not a proxy"
        );
        assert!(
            !looks_like_rustup_proxy(&real_elsewhere, &elsewhere),
            "with no rustup beside it, nothing is a proxy"
        );

        // A same-size copy of `rustup` is how it looks on Windows.
        let copy = cargo_bin.join("ra-copy");
        std::fs::write(&copy, b"rustup").unwrap();
        assert!(looks_like_rustup_proxy(&copy, &cargo_bin), "a byte-size match reads as a proxy");

        // …and a symlink is how it looks on Unix, which is the real case here.
        #[cfg(unix)]
        {
            let link = cargo_bin.join("ra-link");
            std::os::unix::fs::symlink("rustup", &link).unwrap();
            assert!(looks_like_rustup_proxy(&link, &cargo_bin), "a symlink reads as a proxy");
        }

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn a_real_binary_outside_cargo_bin_is_always_accepted() {
        // The guard against the rejection being over-eager: only a candidate in cargo's bin dir
        // can possibly be the proxy, so everything else passes untouched.
        let spec = spec_by_id("rust-analyzer").unwrap();
        assert!(spec.accepts(Path::new("/opt/ra/rust-analyzer")));
        assert!(spec.accepts(Path::new("/usr/local/bin/rust-analyzer")));
        // Another server never consults this at all.
        assert!(spec_by_id("gopls").unwrap().accepts(Path::new("/anything/gopls")));
    }

    #[test]
    fn rust_analyzer_discovery_looks_beyond_path() {
        // The list must at least reach cargo's bin dir — the single most likely location,
        // and one a macOS GUI app's PATH does not contain.
        let dirs = spec_by_id("rust-analyzer").unwrap().extra_dirs();
        if arbor_core::prelude::user_home().is_some() {
            assert!(
                dirs.iter().any(|d| d.ends_with("bin")),
                "expected a cargo/rustup bin dir among {dirs:?}"
            );
        }
    }
}

#[cfg(test)]
mod background_profile_tests {
    use super::*;

    #[test]
    fn the_lean_profile_never_touches_what_makes_a_project_resolve() {
        // The two settings a naive tuning turns off first, and the two that must never be: without
        // proc macros a Bevy project loses every `#[derive(Component)]` and reads as broken.
        let opts = background_init_options("rust-analyzer").unwrap();
        assert!(opts.get("procMacro").is_none());
        assert!(opts.get("cargo").is_none());
        assert!(opts.get("checkOnSave").is_none());
    }

    #[test]
    fn it_bounds_the_three_costs_it_is_there_to_bound() {
        let opts = background_init_options("rust-analyzer").unwrap();
        assert_eq!(opts["cachePriming"]["enable"], serde_json::json!(false));
        assert_eq!(opts["lru"]["capacity"], serde_json::json!(BACKGROUND_LRU_CAPACITY));
        assert_eq!(opts["numThreads"], serde_json::json!(BACKGROUND_THREADS));
    }

    #[test]
    fn only_the_server_whose_settings_we_know_gets_one() {
        assert!(background_init_options("gopls").is_none());
        assert!(background_init_options("my-custom-server").is_none());
    }
}
