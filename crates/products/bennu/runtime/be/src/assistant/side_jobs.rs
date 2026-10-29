//! The work that runs beside the conversation, on a cheaper model: compacting, and — at the end of a run,
//! when the conversation has grown past a share of its window — compacting at a natural boundary instead of
//! under pressure.
//!
//! Which model is `bennu_assistant::sidejobs`; this is the part with the settings, the price list and the
//! network. A side job's money is counted in the run like any other (the spending limits see it), at the
//! side model's own prices — which is why it is kept apart from the main model's requests.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use bennu_assistant::prelude::{due_at_boundary, find_model, image_tokens, resolve_side_model, Message, ProviderError, SideJob, Usage};

use super::compact::{self, tokens};
use super::models;
use super::transport::HttpTransport;

/// The model for a side job: the user's setting, else the default Haiku-class one, else `main`.
pub(super) fn side_model(job: SideJob, main: &str) -> String {
    let (table, _) = models::load();
    resolve_side_model(job, &bennu_core::config::load().assistant.side_model, &table, main)
}

fn spent() -> &'static Mutex<HashMap<String, f64>> {
    static SPENT: OnceLock<Mutex<HashMap<String, f64>>> = OnceLock::new();
    SPENT.get_or_init(Default::default)
}

/// Count what a side request cost, in dollars at `model`'s prices, against run `run_id`.
pub(super) fn record(run_id: &str, model: &str, usage: &Usage) {
    let (table, _) = models::load();
    let usd = find_model(&table, model).map_or(0.0, |m| m.cost(usage));
    *spent().lock().unwrap_or_else(|p| p.into_inner()).entry(run_id.to_string()).or_default() += usd;
}

/// What side jobs have cost run `run_id` so far, in dollars.
pub(super) fn cost_of(run_id: &str) -> f64 {
    spent().lock().unwrap_or_else(|p| p.into_inner()).get(run_id).copied().unwrap_or(0.0)
}

/// Take the figure out when the run is over.
pub(super) fn take_cost(run_id: &str) -> f64 {
    spent().lock().unwrap_or_else(|p| p.into_inner()).remove(run_id).unwrap_or(0.0)
}

/// What a boundary compaction did.
pub(super) struct Folded {
    /// What to tell the user was dropped.
    pub report: String,
}

/// At the end of a run: if the conversation is past its share of the window, fold the early part into a note
/// now, while nothing is waiting on the answer — rather than mid-run when it is nearly full, with every request
/// until then carrying the bulk. `overhead` is the standing part of a request (instructions and tools), in tokens.
/// Changes `transcript` only when it folded something.
/// `Err` carries why the note could not be written; the transcript is then as it was.
pub(super) fn compact_at_boundary(run_id: &str, transcript: &mut Vec<Message>, overhead: u64, max_context: u32, key: &str, main_model: &str) -> Result<Option<Folded>, ProviderError> {
    let size = |m: &[Message]| overhead + tokens(&serde_json::to_string(m).unwrap_or_default()) + image_tokens(m);
    if !due_at_boundary(size(transcript), u64::from(max_context)) {
        return Ok(None);
    }
    let model = side_model(SideJob::Compaction, main_model);
    let transport = HttpTransport::new().map_err(ProviderError::local)?;
    let done = compact::compact(transcript, &transport, key, &model, None, true, &size, 0)?;
    if !done.summarized {
        return Ok(None);
    }
    record(run_id, &model, &done.usage);
    *transcript = done.transcript;
    Ok(Some(Folded { report: done.dropped }))
}
