//! `bennu_wait_ready` — wait until the language server for a file can answer.
//!
//! A server that is still loading answers an outline, a symbol or a references question with nothing, and the
//! tools already say so ("not an answer yet, try again in a few seconds"). What a caller lacked was a way to
//! *wait*: asking the same question in a loop costs a request each time and still guesses when to stop. This
//! waits on the registry's own state — starting the server when nobody has — and comes back the moment it is
//! ready, or with what it is doing when the time runs out.

use std::time::{Duration, Instant};

use arbor_ipc::prelude::EventSink;
use bennu_core::prelude::BennuState;
use serde::{Deserialize, Serialize};

use crate::lsp_registry::{LspRegistry, ServerReadiness};

/// Args for [`bennu_wait_ready`].
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct WaitReadyArgs {
    /// Absolute path to the project root.
    pub root: String,
    /// A file of the language to wait for (absolute, or relative to the root): its server is the one waited on.
    pub file: String,
    /// The longest to wait, in seconds. Default 60, at most 300.
    #[serde(default)]
    pub timeout_seconds: Option<u64>,
}

/// How the wait ended.
#[derive(Debug, Serialize)]
pub struct WaitReadyReport {
    /// `ready` · `loading` (still, when the time ran out) · `failed` · `none` (no server serves this file).
    pub state: String,
    /// The server, when one serves the file.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub server: Option<String>,
    pub waited_ms: u64,
    /// What to do next, in a sentence.
    pub note: String,
}

const DEFAULT_WAIT: u64 = 60;
const MAX_WAIT: u64 = 300;
const POLL: Duration = Duration::from_millis(500);

/// Wait until the language server for `file` can answer — before an outline, a symbol lookup or references on a
/// project whose server is still loading, which answer with nothing until then. Starts the server when nobody
/// has. Comes back as soon as it is ready, or after `timeout_seconds` (60 by default) saying what it is doing.
/// A file no language server serves (Java, Patina, …) is answered by Bennu's own engines, which need no wait.
#[arbor_rpc::handler(mcp(
    title = "Wait until the language server can answer",
    safety = read,
))]
fn bennu_wait_ready(ctx: &BennuState, args: WaitReadyArgs) -> Result<WaitReadyReport, String> {
    let file = absolute(&args.root, &args.file);
    let limit = Duration::from_secs(args.timeout_seconds.unwrap_or(DEFAULT_WAIT).clamp(1, MAX_WAIT));
    let registry = LspRegistry::global();
    let started = Instant::now();
    let sink = ctx.event_sink();
    loop {
        let waited_ms = started.elapsed().as_millis() as u64;
        let report = |state: &str, server: Option<String>, note: String| -> Result<WaitReadyReport, String> { Ok(WaitReadyReport { state: state.into(), server, waited_ms, note }) };
        match registry.readiness_for(&file) {
            ServerReadiness::Ready { name } => return report("ready", Some(name), "It answers now: ask again.".into()),
            ServerReadiness::Absent => {
                return report("none", None, "No language server serves this file: Bennu's own engines answer it, with no wait.".into())
            }
            ServerReadiness::Failed { name, message } => {
                return report("failed", Some(name), format!("It could not run ({message}); waiting will not change that."))
            }
            ServerReadiness::Idle { name } | ServerReadiness::Warming { name, .. } if started.elapsed() >= limit => {
                return report(
                    "loading",
                    Some(name),
                    format!("Still loading after {}s: wait again, or work from the files meanwhile.", limit.as_secs()),
                );
            }
            ServerReadiness::Idle { .. } => {
                // Nobody has asked it anything yet: asking is what starts it.
                let _ = registry.ensure(&file);
            }
            ServerReadiness::Warming { name, detail } => {
                let doing = if detail.is_empty() { "starting".to_string() } else { detail };
                sink.progress(&format!("{name}: {doing}"), None, None);
            }
        }
        std::thread::sleep(POLL);
    }
}

fn absolute(root: &str, file: &str) -> String {
    let path = std::path::Path::new(file);
    if path.is_absolute() {
        file.to_string()
    } else {
        std::path::Path::new(root).join(path).to_string_lossy().into_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_relative_file_is_taken_in_the_root() {
        assert_eq!(absolute("/p/voriz", "src/main.rs"), "/p/voriz/src/main.rs");
        assert_eq!(absolute("/p/voriz", "/elsewhere/a.rs"), "/elsewhere/a.rs");
    }
}
