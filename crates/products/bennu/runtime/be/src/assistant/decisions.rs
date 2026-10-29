//! The two things the user can do with a proposal besides reading it: decide it, and — when the files
//! moved on underneath it — have it made again on what they are now.

use bennu_core::prelude::BennuState;
use serde::Serialize;
use serde_json::{json, Value};

use super::apply::{self, CallDisk, Verdict};
use super::store;

/// How a decision came out. `outdated` is not an error: the proposal is still there, `stale` names the
/// files (as the card calls them) that are no longer what it was made against.
#[derive(Serialize)]
pub struct DecideResult {
    pub status: &'static str,
    pub files: Vec<String>,
    pub stale: Vec<String>,
}

/// A proposal after being made again. `status` is `pending` (approve it now) or `rejected` when every
/// change turned out to be in the files already.
#[derive(Serialize)]
pub struct RemakeResult {
    pub status: &'static str,
    pub files: Vec<Value>,
    pub already_there: Vec<String>,
}

fn disk(ctx: &BennuState) -> CallDisk<impl Fn(&str, Value) -> Result<Value, String> + '_> {
    CallDisk(move |method: &str, params: Value| {
        let call = arbor_rpc::registry_for("").get(method).copied().ok_or_else(|| format!("unknown method `{method}`"))?;
        call(ctx as &dyn std::any::Any, params)
    })
}

/// Change the proposal's block in the saved conversation, so the card is right after a restart.
fn amend_saved(conversation_id: &str, proposal_id: &str, change: impl Fn(&mut Value)) -> Result<(), String> {
    let dir = store::dir();
    let Ok(mut conv) = store::load_in(&dir, conversation_id) else { return Ok(()) };
    for m in &mut conv.messages {
        let Some(blocks) = m.get_mut("blocks").and_then(Value::as_array_mut) else { continue };
        for b in blocks {
            if b.pointer("/proposal/id").and_then(Value::as_str) == Some(proposal_id) {
                change(&mut b["proposal"]);
            }
        }
    }
    store::save_in(&dir, &conv)
}

/// Apply or reject the edits a run proposed. Written through the same door as a save, so Local History
/// keeps what the files were. All of the files or none: if any changed since the proposal was made,
/// nothing is written and the proposal stays, outdated.
#[arbor_rpc::handler]
fn assistant_decide(ctx: &BennuState, conversation_id: String, proposal_id: String, apply: bool) -> Result<DecideResult, String> {
    let (status, files, stale) = match apply::decide(&proposal_id, apply, &disk(ctx))? {
        Verdict::Applied(files) => ("applied", files, Vec::new()),
        Verdict::Rejected => ("rejected", Vec::new(), Vec::new()),
        Verdict::Outdated(stale) => ("outdated", Vec::new(), stale),
    };
    amend_saved(&conversation_id, &proposal_id, |p| {
        p["status"] = json!(status);
        if let Some(entries) = p["files"].as_array_mut() {
            for f in entries {
                f["stale"] = json!(stale.iter().any(|s| f["path"].as_str() == Some(s.as_str())));
            }
        }
    })?;
    Ok(DecideResult { status, files, stale })
}

/// Make an outdated proposal again on the files as they are now, for the user to approve. Refused, with
/// the reason, when a change no longer applies cleanly.
#[arbor_rpc::handler]
fn assistant_remake_proposal(ctx: &BennuState, conversation_id: String, proposal_id: String) -> Result<RemakeResult, String> {
    let made = apply::remake(&proposal_id, &disk(ctx))?;
    let status = if made.files.is_empty() { "rejected" } else { "pending" };
    amend_saved(&conversation_id, &proposal_id, |p| {
        p["status"] = json!(status);
        p["files"] = json!(made.files);
    })?;
    Ok(RemakeResult { status, files: made.files, already_there: made.already_there })
}

/// How an undo came out. `outdated` is not an error: the files are untouched, `stale` names the ones that are
/// no longer as the apply left them.
#[derive(Serialize)]
pub struct UndoResult {
    pub status: &'static str,
    pub files: Vec<String>,
    pub stale: Vec<String>,
}

/// Put back the files an applied proposal wrote, exactly as they were — all of them or none. Refused (the files
/// stay as they are) when any was changed since the apply. Written through the same door as a save, so Local
/// History records the undo too.
#[arbor_rpc::handler]
fn assistant_undo_proposal(ctx: &BennuState, conversation_id: String, proposal_id: String) -> Result<UndoResult, String> {
    let (status, files, stale) = match super::undo::undo(&proposal_id, &disk(ctx))? {
        super::undo::Undone::Done(files) => ("undone", files, Vec::new()),
        super::undo::Undone::Outdated(stale) => ("applied", Vec::new(), stale),
    };
    if status == "undone" {
        amend_saved(&conversation_id, &proposal_id, |p| p["status"] = json!("undone"))?;
    }
    Ok(UndoResult { status, files, stale })
}

/// Whether an applied proposal can still be undone from here (it is remembered in memory, for a while).
#[arbor_rpc::handler]
fn assistant_can_undo(_ctx: &BennuState, proposal_id: String) -> Result<bool, String> {
    Ok(super::undo::can_undo(&proposal_id))
}
