//! crates.io — the three questions that cannot be answered from the machine.
//!
//! | Handler | Question |
//! |---|---|
//! | `bennu_cargo_versions` | what versions of this crate exist |
//! | `bennu_cargo_version_hints` | which dependencies in this manifest are behind |
//! | `bennu_cargo_add` | add this crate to that manifest |
//!
//! Everything else Bennu knows about a Rust project is read off the disk. These three need the
//! network, which makes three things load-bearing:
//!
//! 1. **A switch.** [`CargoConfig::crates_io`] is on by default and turning it off makes Bennu local
//!    again — the hints disappear and the add dialog stops offering a version list, rather than
//!    silently timing out.
//! 2. **A cache with a TTL.** A version list is answered from disk until it is older than
//!    [`CargoConfig::index_ttl_hours`]. Crates publish weekly at most; asking the index on every
//!    manifest open to learn that would be indefensible.
//! 3. **Stale beats absent.** A failed fetch falls back to whatever is cached, however old. On a
//!    train, the right answer is last week's version list.
//!
//! The URL layout, the parsing and the cache live in [`bennu_cargo::registry`], which never opens a
//! socket; this module is the part that does, and the reason the split exists.
//!
//! ## `cargo add` runs the real thing
//!
//! Rather than editing the manifest here. `cargo add` resolves the version requirement the way Cargo
//! would write it, honours `[workspace.dependencies]` inheritance, validates the features against the
//! crate it just resolved, and formats the entry in the file's own style. Reimplementing that would
//! be reimplementing Cargo's opinion about a file Cargo owns.

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use arbor_process_ext::prelude::NoWindowExt;
use bennu_cargo::prelude::{
    declared, index_cache_path, index_is_fresh, index_url, is_release, latest_release, parse_index,
    read_index_cache, requirement_admits, write_index_cache, IndexVersion, Manifest,
    CRATES_IO_INDEX,
};
use bennu_core::prelude::{BennuState, CargoConfig};
use serde::{Deserialize, Serialize};

/// Where cached index files live. Bennu's own data dir, never `$CARGO_HOME` — that belongs to cargo,
/// and writing into another tool's cache is how two tools start corrupting each other's state.
fn cache_dir() -> PathBuf {
    arbor_core::prelude::bennu_data_dir().join("crates-index")
}

/// The configured freshness window. `0` reads as the default rather than as "always refetch": a TTL
/// of zero would mean one request per dependency per manifest open, which is what the cache exists to
/// prevent.
fn ttl(cfg: &CargoConfig) -> Duration {
    let hours = if cfg.index_ttl_hours == 0 { 24 } else { cfg.index_ttl_hours };
    Duration::from_secs(u64::from(hours) * 3600)
}

/// How many crates one hints request may fetch.
///
/// A manifest with sixty dependencies and a cold cache would otherwise be sixty requests before it
/// could answer anything. The ones it does not reach stay uncached, so the next call — a keystroke
/// later, or the next time the file is opened — picks up where this one stopped, and a manifest
/// converges over a few passes instead of blocking once.
const MAX_FETCHES_PER_REQUEST: usize = 16;

/// One published version, as the add dialog needs it.
#[derive(Debug, Clone, Serialize)]
pub struct CrateRelease {
    pub version: String,
    /// Withdrawn by its author — offered, but marked, because a lockfile may still pin one.
    pub yanked: bool,
    /// A pre-release (`1.0.0-rc.1`). Computed here: deciding it in the frontend would mean a semver
    /// parser there, and the one that matters is the one the version ordering already uses.
    pub prerelease: bool,
    /// The features this version declares.
    ///
    /// Per version, not per crate, because they change between them — offering `serde`'s current
    /// feature list while adding an old version would offer features that release does not have. The
    /// index carries them on the same line as the version, so this costs nothing extra.
    pub features: Vec<String>,
}

/// "This dependency is behind", located in the manifest.
#[derive(Debug, Clone, Serialize)]
pub struct VersionHint {
    /// The crate, by its real name (`package = "…"` resolved).
    pub name: String,
    /// Byte offset of the dependency's name — where the hint is drawn.
    pub offset: usize,
    /// 1-based line of the dependency.
    pub line: u32,
    /// Byte span of the version value as written, quotes included — what an update replaces.
    pub start: usize,
    pub end: usize,
    /// The requirement in the file.
    pub current: String,
    /// The newest release on crates.io.
    pub latest: String,
}

/// Args for [`bennu_cargo_versions`].
#[derive(Deserialize)]
pub struct VersionsArgs {
    /// The crate name. The index has no search, so this is a name the user typed or one already in
    /// the manifest.
    pub name: String,
    /// Ignore the cache for this one lookup — the "I published a minute ago" case.
    #[serde(default)]
    pub refresh: bool,
}

/// Every published version of a crate, newest first.
///
/// Empty when the crate does not exist, when crates.io is unreachable with nothing cached, or when
/// the user has turned the index off. All three are states rather than errors: the add dialog can
/// still add a crate by name without a version list.
#[arbor_rpc::handler]
async fn bennu_cargo_versions(
    _ctx: &BennuState,
    args: VersionsArgs,
) -> Result<Vec<CrateRelease>, String> {
    let cfg = bennu_core::config::load().cargo;
    if !cfg.crates_io || args.name.trim().is_empty() {
        return Ok(Vec::new());
    }
    let mut versions = versions_of(&args.name, &cfg, args.refresh).await;
    // Newest first, which is the order a picker wants: the answer to "which version" is almost
    // always the first row.
    versions.reverse();
    Ok(versions
        .into_iter()
        .map(|v| CrateRelease {
            prerelease: !is_release(&v.version),
            version: v.version,
            yanked: v.yanked,
            features: sorted_features(v.features),
        })
        .collect())
}

/// Args for [`bennu_cargo_version_hints`].
#[derive(Deserialize)]
pub struct HintsArgs {
    /// The manifest's path — only used to decide nothing today, but it keeps the call shaped like
    /// every other buffer request and lets a future per-registry lookup key off the project.
    pub file: String,
    /// The live buffer. The hints are drawn in the editor, so they must be spans in *this* text.
    pub source: String,
}

/// Which dependencies in the buffer have a newer release.
///
/// Only crates.io dependencies with a readable requirement are considered — a `path`, a `git` or a
/// workspace-inherited dependency has no version here to be behind, and a deliberate pin (`=1.2.3`)
/// or a range is left alone. See [`requirement_admits`] for why the test errs towards silence: a
/// wrong "update available" on a pin is worse than a missing one.
#[arbor_rpc::handler]
async fn bennu_cargo_version_hints(
    _ctx: &BennuState,
    args: HintsArgs,
) -> Result<Vec<VersionHint>, String> {
    let cfg = bennu_core::config::load().cargo;
    if !cfg.crates_io {
        return Ok(Vec::new());
    }
    let _ = &args.file;
    let manifest = Manifest::parse(&args.source);
    let candidates: Vec<_> = declared(&manifest)
        .into_iter()
        .filter(|d| {
            d.source() == "crates.io"
                && !d.req.is_empty()
                && d.req_end > d.req_start
                && d.complete
        })
        .collect();

    let mut out = Vec::new();
    let mut fetched = 0usize;
    for dep in candidates {
        // The budget covers *fetches*, not lookups: a cached crate is free, so a manifest that has
        // been open before answers in full.
        let cached_only = fetched >= MAX_FETCHES_PER_REQUEST;
        let path = index_cache_path(&cache_dir(), &dep.package);
        if !cached_only && !index_is_fresh(&path, ttl(&cfg)) {
            fetched += 1;
        }
        let versions = if cached_only {
            read_index_cache(&cache_dir(), &dep.package).map(|b| parse_index(&b)).unwrap_or_default()
        } else {
            versions_of(&dep.package, &cfg, false).await
        };
        let Some(latest) = latest_release(&versions) else { continue };
        if requirement_admits(&dep.req, &latest.version) {
            continue;
        }
        out.push(VersionHint {
            name: dep.package.clone(),
            offset: dep.offset,
            line: dep.line,
            start: dep.req_start,
            end: dep.req_end,
            current: dep.req.clone(),
            latest: latest.version.clone(),
        });
    }
    Ok(out)
}

/// The features of one version, in an order a list can be drawn in.
///
/// Sorted with `default` first, because that is the one whose absence changes what you get, and the
/// rest alphabetically — the index's own order is however the crate's author wrote the table.
fn sorted_features(mut features: Vec<String>) -> Vec<String> {
    features.sort_by(|a, b| match (a.as_str(), b.as_str()) {
        ("default", "default") => std::cmp::Ordering::Equal,
        ("default", _) => std::cmp::Ordering::Less,
        (_, "default") => std::cmp::Ordering::Greater,
        _ => a.cmp(b),
    });
    features
}

/// A crate's versions: cache first, then the index, then whatever is cached however old.
async fn versions_of(name: &str, cfg: &CargoConfig, refresh: bool) -> Vec<IndexVersion> {
    let dir = cache_dir();
    let path = index_cache_path(&dir, name);
    if !refresh && index_is_fresh(&path, ttl(cfg)) {
        if let Some(body) = read_index_cache(&dir, name) {
            return parse_index(&body);
        }
    }
    match fetch_index(name).await {
        Ok(body) => {
            write_index_cache(&dir, name, &body);
            parse_index(&body)
        }
        // Offline, blocked, or a crate that does not exist. An old copy is a better answer than
        // none, and for a crate that never existed there is none to fall back to anyway.
        Err(_) => read_index_cache(&dir, name).map(|b| parse_index(&b)).unwrap_or_default(),
    }
}

/// GET one crate's index file.
///
/// Through the workspace client, so the request carries Arbor's user-agent and its bounded timeout —
/// crates.io asks that a client identify itself, and an unbounded request here would be a hint that
/// never resolves.
async fn fetch_index(name: &str) -> Result<String, String> {
    let url = index_url(CRATES_IO_INDEX, name);
    let resp = arbor_core::prelude::client()
        .get(&url)
        .send()
        .await
        .map_err(|e| format!("request: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("status {}", resp.status()));
    }
    resp.text().await.map_err(|e| format!("body: {e}"))
}

/// Args for [`bennu_cargo_add`].
#[derive(Deserialize)]
pub struct AddArgs {
    /// Absolute path to the workspace root — where cargo runs.
    pub root: String,
    /// The crate to add.
    pub name: String,
    /// The version requirement, without an operator (`1.0.219`). Empty lets cargo pick the newest,
    /// which is what a bare `cargo add serde` does.
    #[serde(default)]
    pub version: String,
    /// Features to enable.
    #[serde(default)]
    pub features: Vec<String>,
    /// `--no-default-features`.
    #[serde(default)]
    pub no_default_features: bool,
    /// Add it as a dev-dependency (`--dev`) or a build-dependency (`--build`); empty for a normal
    /// one. A string rather than two bools because the three are exclusive.
    #[serde(default)]
    pub kind: String,
    /// `--optional`.
    #[serde(default)]
    pub optional: bool,
    /// Which workspace member to add it to (`-p`). Empty for the root manifest.
    #[serde(default)]
    pub package: String,
}

/// What `cargo add` did.
#[derive(Debug, Clone, Serialize)]
pub struct AddResult {
    pub ok: bool,
    /// The command line that ran, so a failure can be repeated in a terminal.
    pub command: String,
    /// Cargo's own report — the "Adding serde v1.0.219 to dependencies" block, or the error. Shown
    /// rather than summarised: cargo says which features it enabled, and that is the part you want
    /// to read.
    pub output: String,
}

/// Add a dependency by running `cargo add`.
///
/// Captured rather than streamed into the Run console, unlike every other cargo command here, and for
/// a reason about what it *is*: a build is something you watch, this is a one-line edit you want a
/// verdict on. The caller gets `ok` plus cargo's own report and can then reload the manifest.
///
/// Runs on the blocking pool: it is a child process that talks to the network, and holding an async
/// worker for the seconds a cold index update takes is how a runtime with a reverse channel deadlocks
/// (see `docs/reverse-channel.md`).
#[arbor_rpc::handler]
async fn bennu_cargo_add(_ctx: &BennuState, args: AddArgs) -> Result<AddResult, String> {
    if args.name.trim().is_empty() {
        return Err("no crate name".to_string());
    }
    let argv = add_argv(&args);
    let command = format!("cargo {}", argv.join(" "));
    let root = args.root.clone();

    let output = tokio::task::spawn_blocking(move || {
        let mut cmd = std::process::Command::new("cargo");
        cmd.current_dir(Path::new(&root));
        for a in &argv {
            cmd.arg(a);
        }
        cmd.env("CARGO_TERM_COLOR", "never");
        cmd.env("CARGO_TERM_PROGRESS_WHEN", "never");
        cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
        cmd.no_window();
        cmd.output()
    })
    .await
    .map_err(|e| format!("join: {e}"))?
    .map_err(|e| format!("spawn cargo: {e}"))?;

    // cargo writes its report to stderr and nothing to stdout, but both are joined rather than
    // guessed at — a future cargo that moves the report would silently produce an empty dialog.
    let mut text = String::from_utf8_lossy(&output.stderr).to_string();
    let stdout = String::from_utf8_lossy(&output.stdout);
    if !stdout.trim().is_empty() {
        text.push_str(&stdout);
    }

    if output.status.success() {
        // The manifest just changed, so the completion catalogue built from it (and from the lockfile
        // cargo has just rewritten) describes a project that no longer exists.
        crate::cargo_intel::forget_catalog(&args.root);
    }
    Ok(AddResult { ok: output.status.success(), command, output: text.trim_end().to_string() })
}

/// The argv for one `cargo add`.
///
/// Its own function so it can be tested without running cargo — the flags are the part that is easy
/// to get wrong, and the failure mode of a wrong one is a dependency added to the wrong table.
fn add_argv(args: &AddArgs) -> Vec<String> {
    let spec = if args.version.trim().is_empty() {
        args.name.trim().to_string()
    } else {
        format!("{}@{}", args.name.trim(), args.version.trim())
    };
    let mut argv = vec!["add".to_string(), spec];
    match args.kind.as_str() {
        "dev" => argv.push("--dev".to_string()),
        "build" => argv.push("--build".to_string()),
        _ => {}
    }
    if !args.package.trim().is_empty() {
        argv.push("-p".to_string());
        argv.push(args.package.trim().to_string());
    }
    let features: Vec<&str> =
        args.features.iter().map(|f| f.trim()).filter(|f| !f.is_empty()).collect();
    if !features.is_empty() {
        argv.push("--features".to_string());
        argv.push(features.join(","));
    }
    if args.no_default_features {
        argv.push("--no-default-features".to_string());
    }
    if args.optional {
        argv.push("--optional".to_string());
    }
    argv
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(name: &str) -> AddArgs {
        AddArgs {
            root: "/p".to_string(),
            name: name.to_string(),
            version: String::new(),
            features: Vec::new(),
            no_default_features: false,
            kind: String::new(),
            optional: false,
            package: String::new(),
        }
    }

    #[test]
    fn a_bare_add_is_just_the_name() {
        assert_eq!(add_argv(&args("serde")), vec!["add", "serde"]);
    }

    #[test]
    fn a_version_becomes_the_at_spec_cargo_expects() {
        let mut a = args("serde");
        a.version = "1.0.219".to_string();
        assert_eq!(add_argv(&a), vec!["add", "serde@1.0.219"]);
    }

    #[test]
    fn every_flag_lands_in_the_form_cargo_reads() {
        let mut a = args("tokio");
        a.version = "1".to_string();
        a.features = vec!["rt".to_string(), "  ".to_string(), "macros".to_string()];
        a.no_default_features = true;
        a.optional = true;
        a.kind = "dev".to_string();
        a.package = "my-crate".to_string();
        assert_eq!(
            add_argv(&a),
            vec![
                "add",
                "tokio@1",
                "--dev",
                "-p",
                "my-crate",
                // Comma-joined, and the blank one dropped: `--features ""` makes cargo complain
                // about an empty feature name.
                "--features",
                "rt,macros",
                "--no-default-features",
                "--optional",
            ]
        );
    }

    #[test]
    fn the_kind_is_exclusive_and_an_unknown_one_means_a_normal_dependency() {
        let mut a = args("cc");
        a.kind = "build".to_string();
        assert!(add_argv(&a).contains(&"--build".to_string()));
        a.kind = "nonsense".to_string();
        let argv = add_argv(&a);
        assert!(!argv.iter().any(|f| f == "--dev" || f == "--build"), "{argv:?}");
    }
}
