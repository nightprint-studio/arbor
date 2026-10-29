//! Live runs in the backend: one [`Live`] per project — its managed processes, its timeline, the last
//! results of its scenarios — shared by the assistant's tools and the Live panel.
//!
//! The assistant and the user drive the very same objects, so what the model started shows in the
//! panel and what the user started can be read back by the model. The pure machinery is the
//! `bennu-live` crate; this module is the process-wide registry (one per project root), the event the
//! panel refreshes on, and the project files.
//!
//! ## Reusable process API (for other Bennu features, e.g. a profiler)
//!
//! ```ignore
//! let live = live::live_for(&root, &sink);
//! let info = live.manager.start(StartSpec { name, command, cwd, env, ports, ready, group: None, lifetime: None, owner: None })?;
//! live.manager.wait_ready(&info.id, None, &|| cancelled())?;   // blocking: call off the request worker
//! let (info, lines) = live.manager.logs(&info.id, &LogQuery::default())?;
//! live.manager.stop(&info.id)?;                                  // polite, then the whole tree
//! ```
//! Everything is cleaned up by [`stop_root`] (project closed) and [`stop_everything`] (backend exit).
//!
//! ## Several conversations at once
//!
//! A project's [`Live`] is shared by every conversation and by the panel, so each process records its
//! **owner** (`StartSpec::owner`: a conversation id, or `None` for the user). The panel shows all of them;
//! a conversation stops only its own, cannot start a name or a port another owner holds (the refusal
//! names the owner, see [`owner_label`]) and may read the others' status, logs and the timeline. A
//! conversation's recipe approvals are its own ([`conversation_key`]).

pub(crate) mod files;
pub(crate) mod handlers;
pub(crate) mod world;

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, LazyLock, Mutex, Weak};
use std::time::Duration;

use arbor_ipc::prelude::EventSink;
use bennu_live::prelude::{
    redact_headers, send, Approvals, EventKind, HttpRequest, HttpResponse, Limits, Manager, ScenarioResult, Timeline,
};
use serde_json::json;

/// The event the Live panel refreshes on; coalesced, so a chatty service is not a message per line.
pub(crate) const TOPIC: &str = "arbor://bennu/live";

/// The most response body kept per request, in bytes.
pub(crate) const MAX_BODY: usize = 64 * 1024;

/// Events kept per project.
const TIMELINE_CAP: usize = 20_000;

pub(crate) struct Live {
    pub root: String,
    pub manager: Manager,
    pub timeline: Arc<Timeline>,
    /// The last result of each scenario, by name.
    pub results: Mutex<HashMap<String, ScenarioResult>>,
    /// What the panel should tell the user: a recipe that failed to start, a scenario still running.
    pub notice: Mutex<Option<String>>,
    /// Set while a scenario runs, so a second run is refused rather than interleaved.
    pub scenario_running: Mutex<Option<String>>,
    dirty: Arc<AtomicBool>,
}

static HUB: LazyLock<Mutex<HashMap<String, Arc<Live>>>> = LazyLock::new(Default::default);
static APPROVALS: LazyLock<Approvals> = LazyLock::new(Default::default);

/// What the user has approved to run, this session.
pub(crate) fn approvals() -> &'static Approvals {
    &APPROVALS
}

/// One spelling of a root for the registry: forward slashes, no trailing slash, case-folded on
/// Windows (where `C:\P` and `c:\p` are the same folder).
pub(crate) fn key_of(root: &str) -> String {
    let k = root.replace('\\', "/").trim_end_matches('/').to_string();
    if cfg!(windows) { k.to_lowercase() } else { k }
}

/// The live state of a project, created on first use.
pub(crate) fn live_for(root: &str, sink: &Arc<dyn EventSink>) -> Arc<Live> {
    let key = key_of(root);
    let mut hub = HUB.lock().unwrap_or_else(|p| p.into_inner());
    if let Some(live) = hub.get(&key) {
        return Arc::clone(live);
    }
    let timeline = Arc::new(Timeline::new(TIMELINE_CAP));
    let dirty = Arc::new(AtomicBool::new(false));
    {
        let dirty = Arc::clone(&dirty);
        timeline.set_observer(Arc::new(move || dirty.store(true, Ordering::Relaxed)));
    }
    let live = Arc::new(Live {
        root: root.to_string(),
        manager: Manager::new(Limits::default(), Arc::clone(&timeline)),
        timeline,
        results: Mutex::new(HashMap::new()),
        notice: Mutex::new(None),
        scenario_running: Mutex::new(None),
        dirty,
    });
    spawn_notifier(Arc::downgrade(&live), Arc::clone(sink));
    hub.insert(key, Arc::clone(&live));
    live
}

/// Tell the panel something changed, at most four times a second. Ends when the project's state is dropped.
fn spawn_notifier(live: Weak<Live>, sink: Arc<dyn EventSink>) {
    std::thread::spawn(move || loop {
        std::thread::sleep(Duration::from_millis(250));
        let Some(live) = live.upgrade() else { return };
        if live.dirty.swap(false, Ordering::Relaxed) {
            sink.emit(TOPIC, json!({ "root": live.root }));
        }
    });
}

impl Live {
    /// Ask for a refresh of the panel (state that is not in the timeline changed).
    pub(crate) fn touch(&self) {
        self.dirty.store(true, Ordering::Relaxed);
    }

    pub(crate) fn set_notice(&self, text: Option<String>) {
        *self.notice.lock().unwrap_or_else(|p| p.into_inner()) = text;
        self.touch();
    }

    pub(crate) fn store_result(&self, result: ScenarioResult) {
        self.results.lock().unwrap_or_else(|p| p.into_inner()).insert(result.name.clone(), result);
        self.touch();
    }

    /// Send a request to a service on this machine, recording the request and the answer on the
    /// timeline under one correlation id. Header secrets are redacted in what is recorded.
    pub(crate) fn http(&self, mut req: HttpRequest) -> Result<(HttpResponse, String), String> {
        let corr = req.ensure_correlation();
        let target = bennu_live::prelude::parse_local_url(&req.url)?;
        let sent_headers = redact_headers(&req.headers, &[]);
        self.timeline.push(
            EventKind::HttpRequest,
            "http",
            None,
            Some(&corr),
            &format!("{} {}", req.method.to_uppercase(), target.path),
            Some(json!({ "url": req.url, "headers": sent_headers, "body": req.body.as_deref().map(preview) })),
        );
        match send(&req, MAX_BODY) {
            Ok(resp) => {
                self.timeline.push(
                    EventKind::HttpResponse,
                    "http",
                    None,
                    Some(&corr),
                    &format!("{} {} in {} ms", resp.status, resp.reason, resp.elapsed_ms),
                    Some(json!({ "status": resp.status, "headers": redact_headers(&resp.headers, &[]), "body": preview(&resp.body), "elapsed_ms": resp.elapsed_ms })),
                );
                Ok((resp, corr))
            }
            Err(e) => {
                self.timeline.push(EventKind::HttpResponse, "http", None, Some(&corr), &format!("failed: {e}"), Some(json!({ "error": e })));
                Err(format!("{e} (correlation id {corr})"))
            }
        }
    }
}

fn preview(text: &str) -> String {
    let t: String = text.chars().take(600).collect();
    if text.chars().count() > 600 { format!("{t}…") } else { t }
}

/// Stop everything a project started and forget its state: the project was closed.
pub(crate) fn stop_root(root: &str) {
    let live = HUB.lock().unwrap_or_else(|p| p.into_inner()).remove(&key_of(root));
    if let Some(live) = live {
        live.manager.stop_all();
    }
    APPROVALS.revoke(&key_of(root));
}

/// The backend is exiting: kill every managed process of every project, with no polite round.
pub(crate) fn stop_everything() {
    let all: Vec<Arc<Live>> = HUB.lock().unwrap_or_else(|p| p.into_inner()).values().cloned().collect();
    for live in all {
        live.manager.kill_all_now();
    }
}

/// The processes running for a project that `owner` started (`None`: the user's), without creating its
/// state. A conversation's closing summary names only its own: another chat's services are not its doing.
pub(crate) fn running_names_of(root: &str, owner: Option<&str>) -> Vec<String> {
    HUB.lock()
        .unwrap_or_else(|p| p.into_inner())
        .get(&key_of(root))
        .map(|l| l.manager.list().into_iter().filter(|p| !p.phase.is_finished() && p.owner.as_deref() == owner).map(|p| p.name).collect())
        .unwrap_or_default()
}

/// Stop what a conversation started in a project (it was deleted, or its isolated copy was dropped).
pub(crate) fn stop_owned(root: &str, owner: &str) -> usize {
    let live = HUB.lock().unwrap_or_else(|p| p.into_inner()).get(&key_of(root)).cloned();
    live.map(|l| l.manager.stop_owned(Some(owner))).unwrap_or(0)
}

// ── Who owns what ────────────────────────────────────────────────────────────────────────────────

static OWNER_TITLES: LazyLock<Mutex<HashMap<String, String>>> = LazyLock::new(Default::default);

/// Remember a conversation's title, so a refusal can say whose process or file it ran into.
pub(crate) fn note_owner(conversation: &str, title: &str) {
    OWNER_TITLES.lock().unwrap_or_else(|p| p.into_inner()).insert(conversation.to_string(), title.to_string());
}

/// `the user (the Live panel)` or `conversation c-1 ("Fix the cart")`.
pub(crate) fn owner_label(owner: Option<&str>) -> String {
    match owner {
        None => "the user (started from the Live panel)".into(),
        Some(id) => match OWNER_TITLES.lock().unwrap_or_else(|p| p.into_inner()).get(id).filter(|t| !t.trim().is_empty()) {
            Some(title) => format!("conversation {id} (\"{}\")", title.trim()),
            None => format!("conversation {id}"),
        },
    }
}

/// The key a conversation's own approvals are recorded under: a yes given in one chat is not a yes in another.
pub(crate) fn conversation_key(root: &str, conversation: &str) -> String {
    format!("{}#conv:{conversation}", key_of(root))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_owner_is_named_with_its_title_and_the_user_by_the_panel() {
        note_owner("c-77", "Fix the cart");
        assert_eq!(owner_label(Some("c-77")), "conversation c-77 (\"Fix the cart\")");
        assert_eq!(owner_label(Some("c-unknown")), "conversation c-unknown");
        assert!(owner_label(None).contains("Live panel"));
    }

    #[test]
    fn approvals_are_per_conversation_and_the_panels_covers_everyone() {
        use super::world::{approve_recipes_for, recipes_approved};
        let root = "C:/approvals-test/app";
        approve_recipes_for(root, Some("c-a"), "h1");
        assert!(recipes_approved(root, Some("c-a"), "h1"));
        assert!(!recipes_approved(root, Some("c-b"), "h1"), "another chat's yes is not mine");
        assert!(!recipes_approved(root, Some("c-a"), "h2"), "an edited file asks again");
        approve_recipes_for(root, None, "h3");
        assert!(recipes_approved(root, Some("c-b"), "h3"), "the user approved it in the panel");
        stop_root(root);
    }

    #[test]
    fn roots_that_name_the_same_folder_share_a_key() {
        assert_eq!(key_of("C:\\Work\\App\\"), key_of("C:/Work/App"));
    }
}
