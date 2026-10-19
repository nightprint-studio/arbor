//! Headless repos — a repo named by **path**, with no UI tab behind it.
//!
//! Every handler resolves a `tab_id` to a repo through [`crate::repo::repo_path`]. A tab is
//! normally registered by the shell when the user opens a repo; an editor (Bennu's blame
//! gutter) or an MCP client has no tab to offer, only a path. Rather than give 179 handlers a
//! second way to name a repo, a path is made to *look like* a tab: `path:<dir>`, where `<dir>`
//! is any directory inside the repo. The first call registers it lazily, every later call
//! reuses it and pushes its idle deadline back, and [`start_sweeper`] drops it once nobody has
//! asked about it for `cache.headless_repo_ttl_secs`.
//!
//! Registered headless repos live apart from the shell's open-tab set
//! ([`CorvusState::ephemeral_tabs`]), so graph refresh, the worktree orchestrator and the
//! memory report never mistake a repo that was merely queried for one the user has open.
//!
//! Callers should pass the id [`repo_tab_for_path`] hands back (the normalised workdir): any
//! other spelling still works, but is registered as an alias of its own until it expires.

use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use corvus_core::prelude::{CorvusState, EPHEMERAL_TAB_PREFIX};
use git2::Repository;

/// How often idle headless repos are looked for. Coarse on purpose: the TTL is minutes, and a
/// repo outliving its deadline by up to one tick costs nothing.
const SWEEP_INTERVAL: Duration = Duration::from_secs(30);

/// Floor for the configured TTL, so a typo can't make every call re-discover its repo.
const MIN_TTL_SECS: u64 = 30;

/// `path:` + `workdir`, normalised so one repo has one id however its path was spelled.
fn canonical_tab_id(workdir: &str) -> String {
    let key = workdir.to_string();
    // Windows paths are case-insensitive; the id must not split `C:/Repo` from `c:/repo`.
    #[cfg(windows)]
    let key = key.to_lowercase();
    format!("{EPHEMERAL_TAB_PREFIX}{key}")
}

/// Forward slashes, no `\\?\` verbatim prefix, no trailing separator — the shape libgit2's
/// `workdir()` reports and the one the rest of the backend compares paths in.
fn normalise(path: &str) -> String {
    let p = path.strip_prefix(r"\\?\").unwrap_or(path).replace('\\', "/");
    let trimmed = p.trim_end_matches('/');
    // Keep a bare root (`/`, `C:/`) intact.
    if trimmed.is_empty() || trimmed.ends_with(':') {
        p
    } else {
        trimmed.to_string()
    }
}

/// The working directory of the repo containing `dir`, normalised.
fn discover_workdir(dir: &str) -> Result<String, String> {
    let repo = Repository::discover(Path::new(dir))
        .map_err(|e| format!("no git repository at or above '{dir}': {e}"))?;
    let workdir = repo
        .workdir()
        .ok_or_else(|| format!("the repository at '{dir}' is bare and has no working directory"))?;
    let abs = std::fs::canonicalize(workdir).unwrap_or_else(|_| workdir.to_path_buf());
    Ok(normalise(&abs.to_string_lossy()))
}

/// Register the headless repo behind `tab_id` (a `path:<dir>` id) and return its workdir.
///
/// Registers the canonical id and, when `tab_id` is spelled differently, `tab_id` itself too,
/// so the next call with the same spelling is a plain registry hit and its per-tab caches are
/// cleaned up with it.
pub fn adopt(state: &CorvusState, tab_id: &str) -> Result<String, String> {
    let dir = tab_id.strip_prefix(EPHEMERAL_TAB_PREFIX).unwrap_or(tab_id);
    let workdir = discover_workdir(dir)?;
    let canonical = canonical_tab_id(&workdir);
    state.register_ephemeral(canonical.clone(), workdir.clone());
    if canonical != tab_id {
        state.register_ephemeral(tab_id.to_string(), workdir.clone());
    }
    Ok(workdir)
}

/// The tab id for the repo containing `path` — registering it as a headless repo if needed.
///
/// `path` may be the repo root, any directory inside it, or a file inside it. Pass the result
/// as `tab_id` to every other handler (`get_file_blame`, `get_commit_log`, …).
#[arbor_rpc::handler]
fn repo_tab_for_path(state: &CorvusState, path: String) -> Result<String, String> {
    tab_for(state, &path)
}

/// [`repo_tab_for_path`]'s body, for the backend's own handlers (the MCP tools).
pub fn tab_for(state: &CorvusState, path: &str) -> Result<String, String> {
    // A file path is fine: `discover` walks up from it. Normalise first so the same file
    // spelled `C:\x\a.rs` and `C:/x/a.rs` lands on one id.
    let raw = format!("{EPHEMERAL_TAB_PREFIX}{}", normalise(path));
    let workdir = adopt(state, &raw)?;
    Ok(canonical_tab_id(&workdir))
}

/// Start the thread that drops idle headless repos. Called once before the serve loop; the
/// thread lives as long as the process.
pub fn start_sweeper(state: Arc<CorvusState>) {
    let spawned = std::thread::Builder::new().name("corvus-ephemeral-sweeper".into()).spawn(move || loop {
        std::thread::sleep(SWEEP_INTERVAL);
        let ttl = ttl(&state);
        let dropped = state.sweep_ephemeral(ttl, Instant::now());
        if !dropped.is_empty() {
            // stderr: stdout is the protocol channel.
            eprintln!("corvus-be: dropped {} idle headless repo(s)", dropped.len());
        }
    });
    if let Err(e) = spawned {
        eprintln!("corvus-be: failed to start the headless-repo sweeper: {e}");
    }
}

fn ttl(state: &CorvusState) -> Duration {
    let secs = crate::corvus_config::load(state).cache.headless_repo_ttl_secs;
    Duration::from_secs(secs.max(MIN_TTL_SECS))
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use arbor_ipc::prelude::EventSink;
    use serde_json::Value;

    use super::*;

    struct NullSink;
    impl EventSink for NullSink {
        fn emit(&self, _topic: &str, _payload: Value) {}
    }

    fn state() -> CorvusState {
        CorvusState::new(Arc::new(NullSink))
    }

    /// A fresh repo under the OS temp dir, removed on drop.
    struct TempRepo(std::path::PathBuf);
    impl TempRepo {
        fn new(tag: &str) -> Self {
            let nanos = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0);
            let dir = std::env::temp_dir().join(format!("corvus-eph-{tag}-{}-{nanos}", std::process::id()));
            std::fs::create_dir_all(dir.join("sub")).unwrap();
            Repository::init(&dir).unwrap();
            Self(dir)
        }
        fn str(&self) -> String {
            self.0.to_string_lossy().into_owned()
        }
    }
    impl Drop for TempRepo {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn normalise_strips_verbatim_prefix_and_separators() {
        assert_eq!(normalise(r"\\?\C:\work\repo\"), "C:/work/repo");
        assert_eq!(normalise("/home/u/repo/"), "/home/u/repo");
        assert_eq!(normalise("C:/"), "C:/");
        assert_eq!(normalise("/"), "/");
    }

    #[test]
    fn a_subdirectory_resolves_to_the_repo_root_id() {
        let repo = TempRepo::new("root");
        let s = state();
        let root_id = repo_tab_for_path(&s, repo.str()).unwrap();
        let sub_id = repo_tab_for_path(&s, format!("{}/sub", repo.str())).unwrap();
        assert_eq!(root_id, sub_id);
        assert!(root_id.starts_with(EPHEMERAL_TAB_PREFIX));
    }

    #[test]
    fn adopted_repo_is_a_headless_tab_not_an_open_one() {
        let repo = TempRepo::new("tabs");
        let s = state();
        let id = repo_tab_for_path(&s, repo.str()).unwrap();
        assert!(s.repo_path(&id).is_some());
        assert!(s.open_tabs().is_empty());
        assert!(!s.ephemeral_tabs().is_empty());
    }

    #[test]
    fn a_non_repo_path_is_a_clear_error() {
        let dir = std::env::temp_dir().join(format!("corvus-eph-none-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let err = repo_tab_for_path(&state(), dir.to_string_lossy().into_owned());
        let _ = std::fs::remove_dir_all(&dir);
        assert!(err.unwrap_err().contains("no git repository"));
    }
}
