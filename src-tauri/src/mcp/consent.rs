//! The "ask the user" half of the policy.
//!
//! A pending call parks on a channel while one Arbor window shows a real modal. Four
//! properties matter and each is here for a reason:
//!
//! - **It times out.** A prompt nobody is at (every window in the tray, the user away)
//!   must not hold the tool call — and the model's turn — open indefinitely. The
//!   timeout answers *no*.
//! - **It fails closed.** No window to ask in, a channel that dropped, a poisoned lock:
//!   every one of those denies.
//! - **It is asked in exactly one window, chosen to be seen.** See [`prompt_window`].
//! - **"Allow for this session" is remembered in memory only.** It is a convenience for
//!   a working session, not a setting; a user who grants something at 3pm should not
//!   discover next week that it is still granted.

use std::collections::{HashMap, HashSet};
use std::sync::Mutex;
use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, UserAttentionType, WebviewWindow};
use tokio::sync::oneshot;

/// What the launcher is asked to show.
#[derive(Debug, Clone, Serialize)]
pub struct ConsentRequest {
    /// Correlates the answer back to the waiting call.
    pub id: String,
    /// The tool's name, as the client called it.
    pub tool: String,
    /// Its human title.
    pub title: String,
    /// The backend that would serve it.
    pub program: String,
    /// `read` / `write` / `destructive` — drives how loud the modal is.
    pub safety: String,
    /// What the tool says it does. The user is being asked to approve an action, and
    /// the tool name alone is not enough to judge one.
    pub description: String,
    /// Pretty-printed arguments. The actual thing being approved.
    pub arguments: String,
}

/// In-flight prompts, keyed by request id.
static PENDING: Mutex<Option<HashMap<String, oneshot::Sender<bool>>>> = Mutex::new(None);

/// Tools the user allowed for the rest of this run.
static SESSION_GRANTS: Mutex<Option<HashSet<String>>> = Mutex::new(None);

/// Whether this tool already carries a session-wide grant.
pub fn granted_for_session(tool: &str) -> bool {
    SESSION_GRANTS
        .lock()
        .ok()
        .and_then(|g| g.as_ref().map(|set| set.contains(tool)))
        .unwrap_or(false)
}

/// Drop every session grant — what "revoke" in the UI does, and what a config change
/// should do so a tightened policy takes effect immediately rather than after a restart.
pub fn clear_session_grants() {
    if let Ok(mut g) = SESSION_GRANTS.lock() {
        g.take();
    }
}

/// Which window shows the prompt — and why exactly one.
///
/// Every window mounts the modal (it rides in `GlobalOverlays`, because the endpoint is
/// process-wide and no single window is guaranteed to be open). A broadcast would
/// therefore put the same question in all of them at once: N copies, N-1 of which
/// answer nothing once the first is clicked, and all of which have to be dismissed. So
/// the shell elects one and emits only there.
///
/// The ranking is about *being seen*. The prompt has a timeout already running, and the
/// usual case is that Arbor is not in front at all — the user is in the terminal with
/// the client that triggered this — so a modal drawn in a window behind everything else
/// is a denial with extra steps. Focused first, then merely visible, then a hidden one
/// as the last resort. Overlay surfaces are skipped: they are chromeless windows owned
/// by another one, and they mount no overlays of their own.
fn prompt_window(app: &AppHandle) -> Option<WebviewWindow> {
    let mut best: Option<(u8, WebviewWindow)> = None;
    for (label, window) in app.webview_windows() {
        if !crate::window::surface_kind_for_label(&label).is_switchable() {
            continue;
        }
        // Arbor's own home outranks a product window when neither has focus: it is
        // where the settings this prompt is enforcing actually live.
        let home = crate::window::is_launcher_label(&label)
            || label == crate::window::workspace::WORKSPACE_WINDOW_LABEL;
        let rank = match (
            window.is_focused().unwrap_or(false),
            window.is_visible().unwrap_or(false),
            home,
        ) {
            (true, _, _) => 4,
            (_, true, true) => 3,
            (_, true, false) => 2,
            (_, false, true) => 1,
            (_, false, false) => 0,
        };
        let better = match &best {
            None => true,
            Some((seen, _)) => rank > *seen,
        };
        if better {
            best = Some((rank, window));
        }
    }
    best.map(|(_, window)| window)
}

/// Ask, and wait. `false` on denial, on timeout, and on anything going wrong.
pub async fn ask(app: &AppHandle, request: ConsentRequest, timeout: Duration) -> bool {
    let (tx, rx) = oneshot::channel();
    {
        let Ok(mut pending) = PENDING.lock() else { return false };
        pending.get_or_insert_with(HashMap::new).insert(request.id.clone(), tx);
    }

    // No window at all — every one closed to tray and none restorable. Fail closed
    // rather than hold the call open for a prompt that will never be drawn.
    let Some(window) = prompt_window(app) else {
        forget(&request.id);
        return false;
    };
    // A question nobody can see is a denial on a timer, so a hidden winner is revealed.
    // Attention, though, not focus: the user is most likely typing in the client that
    // triggered this, and yanking the cursor out of it is hostile.
    if !window.is_visible().unwrap_or(false) {
        let _ = window.show();
    }
    let _ = window.request_user_attention(Some(UserAttentionType::Informational));

    if app
        .emit_to(window.label(), "arbor://mcp-consent", request.clone())
        .is_err()
    {
        forget(&request.id);
        return false;
    }

    match tokio::time::timeout(timeout, rx).await {
        Ok(Ok(answer)) => answer,
        // Sender dropped (the window closed) or the timer expired. Both are "no".
        _ => {
            forget(&request.id);
            false
        }
    }
}

/// Deliver the user's answer. Returns whether a call was actually waiting for it — a
/// late click on a prompt that already timed out changes nothing.
pub fn respond(id: &str, allow: bool, remember: bool, tool: &str) -> bool {
    if allow && remember {
        if let Ok(mut g) = SESSION_GRANTS.lock() {
            g.get_or_insert_with(HashSet::new).insert(tool.to_string());
        }
    }
    let sender = PENDING.lock().ok().and_then(|mut p| p.as_mut().and_then(|m| m.remove(id)));
    match sender {
        Some(tx) => tx.send(allow).is_ok(),
        None => false,
    }
}

/// Drop a prompt nobody will answer.
fn forget(id: &str) {
    if let Ok(mut pending) = PENDING.lock() {
        if let Some(map) = pending.as_mut() {
            map.remove(id);
        }
    }
}

/// A fresh request id. Not security-relevant — it only has to be unique among the
/// handful of prompts that can be open at once.
pub fn new_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn responding_to_an_unknown_prompt_is_a_no_op() {
        assert!(!respond("no-such-id", true, false, "x"));
    }

    #[test]
    fn remember_grants_only_the_named_tool() {
        clear_session_grants();
        assert!(!granted_for_session("bennu_read_file"));
        // A grant is recorded even though no call was waiting — the click happened.
        respond("stale", true, true, "bennu_read_file");
        assert!(granted_for_session("bennu_read_file"));
        assert!(!granted_for_session("bennu_write_file"));
        clear_session_grants();
        assert!(!granted_for_session("bennu_read_file"));
    }

    #[tokio::test]
    async fn a_denial_is_the_answer() {
        clear_session_grants();
        let (tx, rx) = oneshot::channel();
        PENDING.lock().unwrap().get_or_insert_with(HashMap::new).insert("id1".into(), tx);
        assert!(respond("id1", false, false, "t"));
        assert_eq!(rx.await.unwrap(), false);
    }
}
