//! `package.json`, and the one question that needs the network.
//!
//! | Handler | Question |
//! |---|---|
//! | `bennu_npm_manifest` | what does this manifest declare — scripts, dependencies, and where |
//! | `bennu_npm_version_hints` | which dependencies are behind |
//! | `bennu_npm_run_script` | run this script |
//!
//! Everything but the middle one is read off the buffer or the disk. The middle one is a request
//! per package, which makes the same three things load-bearing here as for crates.io — a switch, a
//! cache with a TTL, and stale beating absent — and they are deliberately **the same settings**:
//! somebody who turned registry lookups off did not mean "off for Rust".
//!
//! The parsing, the cache layout and the range test live in [`bennu_npm`], which opens no sockets.
//! This is the half that does.

use std::path::{Path, PathBuf};
use std::time::Duration;

use arbor_process_ext::prelude::NoWindowExt;
use bennu_core::prelude::{BennuState, CargoConfig};
use bennu_npm::prelude::{
    cache_path, is_fresh, latest_url, package_manager_for, parse, range_admits, read_cache,
    write_cache, REGISTRY,
};
use bennu_proto::prelude::RunHandle;
use serde::{Deserialize, Serialize};

/// Where cached registry answers live — Bennu's own data dir, never npm's cache. Writing into
/// another tool's cache is how two tools start corrupting each other's state.
fn cache_dir() -> PathBuf {
    arbor_core::prelude::bennu_data_dir().join("npm-registry")
}

/// The freshness window, shared with the crates.io side.
///
/// One setting for both registries, on purpose. "Look packages up on the network" is a decision
/// about the machine and about the person's connection, not about which language they happen to
/// have open — and a second knob meaning the same thing is a second knob to find and get wrong.
fn ttl(cfg: &CargoConfig) -> Duration {
    let hours = if cfg.index_ttl_hours == 0 { 24 } else { cfg.index_ttl_hours };
    Duration::from_secs(u64::from(hours) * 3600)
}

/// How many packages one hints request may fetch. A cold `package.json` with eighty dependencies
/// would otherwise be eighty requests before the first hint appeared; the rest arrive on the next
/// pass, from cache, in one go.
const MAX_FETCHES_PER_REQUEST: usize = 24;

// ── The wire ─────────────────────────────────────────────────────────────────

/// A dependency with a newer release. Same shape as the crates.io hint — deliberately, because the
/// editor draws one control for both and a second shape would mean a second control.
#[derive(Debug, Clone, Serialize)]
pub struct NpmVersionHint {
    pub name: String,
    /// Byte offset of the dependency's name — where the hint is drawn.
    pub offset: usize,
    /// 1-based line of the dependency.
    pub line: u32,
    /// Byte span of the version string's **contents**, quotes excluded — what an update replaces.
    pub start: usize,
    pub end: usize,
    /// The range as written.
    pub current: String,
    /// The registry's `latest` dist-tag.
    pub latest: String,
}

/// One `scripts` entry.
#[derive(Debug, Clone, Serialize)]
pub struct NpmScript {
    pub name: String,
    pub command: String,
    pub offset: usize,
    pub line: u32,
}

/// What a manifest declares, and what would run it.
#[derive(Debug, Clone, Serialize)]
pub struct NpmManifest {
    pub name: Option<String>,
    pub version: Option<String>,
    pub scripts: Vec<NpmScript>,
    /// `npm` / `yarn` / `pnpm` / `bun`, from the lockfile beside the manifest — what the run
    /// controls will actually invoke, and worth showing rather than assuming.
    pub package_manager: String,
}

#[derive(Deserialize)]
pub struct ManifestArgs {
    /// The manifest's path. Load-bearing: the package manager is read off the lockfile beside it.
    pub file: String,
    /// The buffer, which is regularly ahead of the file on disk.
    pub source: String,
}

/// What this `package.json` declares.
///
/// Answered from the **buffer**, not the file: a script you just added should get its run control
/// before you save, and a manifest mid-edit still has everything above the broken line.
#[arbor_rpc::handler]
fn bennu_npm_manifest(_ctx: &BennuState, args: ManifestArgs) -> Result<NpmManifest, String> {
    let parsed = parse(&args.source);
    let dir = Path::new(&args.file).parent().unwrap_or(Path::new(".")).to_path_buf();
    Ok(NpmManifest {
        name: parsed.name,
        version: parsed.version,
        scripts: parsed
            .scripts
            .into_iter()
            .map(|s| NpmScript {
                name: s.name,
                command: s.command,
                offset: s.offset,
                line: s.line as u32,
            })
            .collect(),
        package_manager: package_manager_for(&dir).program().to_string(),
    })
}

/// Which dependencies in the buffer have a newer release.
///
/// Only what can be judged: see `bennu_npm::registry::range_admits`, which errs towards silence for
/// every form whose meaning is not unambiguous — a comparator range, a dist-tag, a `workspace:` or
/// a `git+` dependency. A wrong "update available" on a deliberate pin is worse than a missing one.
#[arbor_rpc::handler]
async fn bennu_npm_version_hints(
    _ctx: &BennuState,
    args: ManifestArgs,
) -> Result<Vec<NpmVersionHint>, String> {
    let cfg = bennu_core::config::load().cargo;
    if !cfg.crates_io {
        return Ok(Vec::new());
    }
    let manifest = parse(&args.source);
    let mut budget = MAX_FETCHES_PER_REQUEST;
    let mut out = Vec::new();

    for dep in manifest.dependencies {
        // Cheap first: a range this cannot judge never becomes a request. Most of a lockfile's
        // `workspace:` and `file:` entries drop out here, before any network is considered.
        if dep.range.trim().is_empty() || !judgeable(&dep.range) {
            continue;
        }
        let cached_only = budget == 0;
        let path = cache_path(&cache_dir(), &dep.name);
        if !is_fresh(&path, ttl(&cfg)) && !cached_only {
            budget -= 1;
        }
        let Some(latest) = latest_of(&dep.name, &cfg, cached_only).await else { continue };
        if range_admits(&dep.range, &latest) {
            continue;
        }
        out.push(NpmVersionHint {
            name: dep.name,
            offset: dep.offset,
            line: dep.line as u32,
            start: dep.range_start,
            end: dep.range_end,
            current: dep.range,
            latest,
        });
    }
    Ok(out)
}

/// Whether a range is one `range_admits` will have an opinion about.
///
/// A pre-filter and not a second implementation of the rule: it exists so a manifest full of
/// `workspace:*` entries costs nothing, and it is deliberately *more* permissive than the real
/// test — anything it lets through is judged properly there.
fn judgeable(range: &str) -> bool {
    let r = range.trim();
    matches!(r.as_bytes().first(), Some(b'^' | b'~' | b'v') | Some(b'0'..=b'9'))
        && !r.contains([' ', '|', '*'])
        && !r.contains('x')
}

/// The `latest` dist-tag for one package, from cache or from the registry.
async fn latest_of(name: &str, cfg: &CargoConfig, cached_only: bool) -> Option<String> {
    let dir = cache_dir();
    if cached_only || is_fresh(&cache_path(&dir, name), ttl(cfg)) {
        if let Some(body) = read_cache(&dir, name) {
            return version_of(&body);
        }
        if cached_only {
            return None;
        }
    }
    match fetch_latest(name).await {
        Ok(body) => {
            let version = version_of(&body);
            // Only a body that parsed is worth keeping: caching a 404's HTML would answer nothing
            // for a day and look like a package that has no versions.
            if version.is_some() {
                write_cache(&dir, name, &body);
            }
            version
        }
        // Offline, blocked, or a package that does not exist. An old copy beats none.
        Err(_) => read_cache(&dir, name).and_then(|b| version_of(&b)),
    }
}

/// The `"version"` out of a `/latest` document.
///
/// A targeted read rather than a full deserialize: the document has one field this needs and forty
/// it does not, several of which are objects that would need types written for them.
fn version_of(body: &str) -> Option<String> {
    let value: serde_json::Value = serde_json::from_str(body).ok()?;
    value.get("version")?.as_str().map(str::to_string)
}

/// GET one package's latest release.
///
/// Through the workspace client, so the request carries Arbor's user-agent and its bounded
/// timeout — an unbounded one here is a hint that never resolves.
async fn fetch_latest(name: &str) -> Result<String, String> {
    let resp = arbor_core::prelude::client()
        .get(&latest_url(REGISTRY, name))
        .send()
        .await
        .map_err(|e| format!("request: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("status {}", resp.status()));
    }
    resp.text().await.map_err(|e| format!("body: {e}"))
}

// ── Running a script ─────────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct RunScriptArgs {
    /// The project root — where the Run console files this run.
    pub root: String,
    /// The manifest whose script this is. The command runs in **its** directory, which is what
    /// makes a script in a workspace member run as that member rather than as the root.
    pub file: String,
    pub script: String,
}

/// Run one script, streaming into a Run console tab.
///
/// The same console `cargo run` uses. A script is a run, and giving it a second surface would mean
/// two places to look for output depending on which language the project happened to be.
#[arbor_rpc::handler]
fn bennu_npm_run_script(ctx: &BennuState, args: RunScriptArgs) -> Result<RunHandle, String> {
    if args.script.trim().is_empty() {
        return Err("no script name".to_string());
    }
    let manifest = PathBuf::from(&args.file);
    let cwd = manifest.parent().unwrap_or(Path::new(".")).to_path_buf();
    let pm = package_manager_for(&cwd);

    let mut cmd = std::process::Command::new(pm.program());
    cmd.current_dir(&cwd);
    for a in pm.run_args(&args.script) {
        cmd.arg(a);
    }
    // A console shows text and has no cursor for a progress bar to move around. `FORCE_COLOR=0`
    // is the one every Node tool reads; `NO_COLOR` is the one the rest of the world reads.
    cmd.env("FORCE_COLOR", "0");
    cmd.env("NO_COLOR", "1");
    cmd.stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .stdin(std::process::Stdio::piped());
    cmd.no_window();

    let program = pm.program().to_string();
    crate::build::spawn_streamed(
        cmd,
        format!("{program} {}", args.script),
        format!("{program} {}", pm.run_args(&args.script).join(" ")),
        cwd.display().to_string(),
        &args.root,
        ctx.event_sink(),
        |_| {},
    )
    .map_err(|e| format!("spawn {program}: {e} — is it installed and on your PATH?"))
}

#[cfg(test)]
mod tests {
    use super::judgeable;

    #[test]
    fn the_prefilter_lets_through_exactly_what_the_real_test_can_judge() {
        for range in ["^1.2.3", "~1.2.3", "1.2.3", "v1.2.3", "^0.2.0"] {
            assert!(judgeable(range), "`{range}` is judgeable");
        }
        // Everything here would cost a network request to learn nothing.
        for range in [
            ">=1.0.0 <2.0.0", "1.x", "*", "1.2.x", "workspace:*", "file:../a", "latest",
            "github:o/r", "npm:other@^1", "1.x || 2.x",
        ] {
            assert!(!judgeable(range), "`{range}` must not cost a request");
        }
    }
}
