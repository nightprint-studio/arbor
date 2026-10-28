//! `index_stats` domain — `bennu_index_stats` (index inspector).
//!
//! A cheap snapshot of the per-project index for an inspector panel: symbol counts (types
//! / members) from the last full build, the resolved JDK level, the config-graph counts
//! (actions / beans / relations), and whether the build has finished (`ready`).
//!
//! Never errors just because the index isn't built yet — an unbuilt (or unknown) project
//! reports zeros + `ready = false`, so the FE can poll it while the background build runs.

use bennu_core::prelude::BennuState;
use bennu_proto::prelude::IndexStats;
use serde::Deserialize;

use crate::index_service::IndexService;

/// Args for [`bennu_index_stats`].
#[derive(Deserialize, schemars::JsonSchema)]
pub struct IndexStatsArgs {
    /// Absolute path to the project root to report on.
    pub root: String,
}

/// Report whether this project's engine can answer yet, and — on a Java project — how much of
/// the semantic index is built.
///
/// Worth checking when a navigation call comes back empty: an engine that is still coming up
/// means "not yet", not "nothing there".
///
/// **`engine` says which engine that is, and the counters belong to one of them.** `types`,
/// `members`, `actions`, `beans` and `jdk_version` count *Java* things; on a Cargo project they
/// are zero and always will be, and `ready` there reports the **language server** instead. It used
/// to report the Java index, which meant a Rust project answered `ready: false` for ever — a
/// readiness signal that always says no, which is worse than none, because it teaches a reader to
/// ignore it and go and grep a project whose references worked from the first call.
#[arbor_rpc::handler(mcp(
    title = "Check the index state",
    safety = read,
))]
fn bennu_index_stats(_ctx: &BennuState, args: IndexStatsArgs) -> Result<IndexStats, String> {
    let cargo = crate::agent::is_cargo_root(&args.root);
    // The index is keyed by the exact string the project was opened with, so a differently spelled
    // root (`c:\p` vs `C:/p`) used to miss and report zeros — indistinguishable from "still building".
    let opened = opened_spelling(&args.root);
    let mut stats = IndexService::global().index_stats(opened.as_deref().unwrap_or(&args.root));
    let server = cargo.then(|| crate::agent::server_for_root(&args.root)).flatten();
    let (ready, engine) = readiness(cargo, stats.ready, server.as_ref());
    stats.ready = ready;
    stats.engine = engine;
    if !cargo && opened.is_none() {
        stats.engine = NOT_OPEN.to_string();
    }
    Ok(stats)
}

/// Said in full, and distinct from "still building": a root nobody opened will never finish.
const NOT_OPEN: &str = "project not open — no Java index exists for this root; check the path or open the project first";

/// The spelling `root` was opened under, if it is open at all.
fn opened_spelling(root: &str) -> Option<String> {
    let want = normalise_root(root);
    IndexService::global().indexed_roots().into_iter().find(|r| normalise_root(r) == want)
}

/// `\` vs `/`, a trailing separator and (on Windows) drive-letter case do not make a different project.
fn normalise_root(root: &str) -> String {
    let slashed = root.replace('\\', "/");
    let trimmed = slashed.trim_end_matches('/');
    if cfg!(windows) { trimmed.to_lowercase() } else { trimmed.to_string() }
}

/// What `ready` and `engine` should say, given the project kind and who is serving it.
///
/// Separated from the lookup so the decision — which is the whole bug — can be tested without a
/// language server on the machine running the tests.
fn readiness(
    cargo: bool,
    java_ready: bool,
    server: Option<&bennu_proto::prelude::LspStatus>,
) -> (bool, String) {
    if !cargo {
        return (java_ready, "bennu-index".to_string());
    }
    match server {
        Some(s) => (s.state == "ready", format!("{} ({})", s.name, s.state)),
        // Not ready, but for a reason a caller can act on — and the action is to **ask**, not to
        // wait. A server starts on the first question about a source file, so waiting here is
        // waiting for something that nothing will cause.
        None => (false, NO_SERVER.to_string()),
    }
}

/// Said in full rather than paraphrased, because it is the sentence that has to replace "wait".
const NO_SERVER: &str = "no language server is running for this project yet — one starts on the first question about a source file";

#[cfg(test)]
mod tests {
    use super::{normalise_root, readiness};
    use bennu_proto::prelude::LspStatus;

    fn server(state: &str) -> LspStatus {
        LspStatus {
            id: "rust-analyzer".into(),
            name: "rust-analyzer".into(),
            language: "rust".into(),
            root: "/w/geode".into(),
            command: "rust-analyzer".into(),
            version: None,
            state: state.into(),
            message: String::new(),
            progress: String::new(),
            features: Vec::new(),
            log_tail: Vec::new(),
            pid: None,
        }
    }

    #[test]
    fn a_cargo_projects_readiness_is_its_servers_and_not_the_java_indexs() {
        // The bug, in one assertion: the Java index is not ready on a Cargo root and never will
        // be, so reporting it here answered `false` for ever. A readiness signal that always says
        // no is worse than none — it teaches a reader to ignore it, which is exactly what
        // happened: a session read it, concluded "not ready", and went and used `grep` on a
        // project whose references worked from the first call.
        let (ready, engine) = readiness(true, false, Some(&server("ready")));
        assert!(ready, "the server is up, so the project can answer");
        assert!(engine.contains("rust-analyzer"), "{engine}");

        // Still warming is a real "not yet", and it says whose.
        let (ready, engine) = readiness(true, false, Some(&server("starting")));
        assert!(!ready);
        assert!(engine.contains("starting"), "{engine}");
    }

    #[test]
    fn no_server_yet_is_not_ready_and_says_what_would_start_one() {
        let (ready, engine) = readiness(true, false, None);
        assert!(!ready);
        // The instruction has to be "ask", not "wait": nothing starts a server except a question.
        assert!(engine.contains("first question"), "{engine}");
    }

    #[test]
    fn root_spellings_of_one_project_compare_equal() {
        assert_eq!(normalise_root("C:\\p\\app\\"), normalise_root("C:/p/app"));
    }

    #[test]
    fn a_java_project_still_reports_its_own_index() {
        assert_eq!(readiness(false, true, None), (true, "bennu-index".to_string()));
        assert!(!readiness(false, false, None).0);
    }
}
