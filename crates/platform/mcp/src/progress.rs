//! Narration while a tool call is still open.
//!
//! Most tools answer in milliseconds and this is inert for all of them. It exists for the
//! ones that cannot: a test run, a build — work measured in minutes, where a client that
//! hears nothing has no way to tell a long run from a hung one, and where the user watching
//! the client deserves to see what it is waiting on.
//!
//! MCP carries this as `notifications/progress`, and only when the caller asked for it: a
//! request that includes `_meta.progressToken` is a request that will correlate the
//! notifications, and one that does not **must not** be sent any. So a [`Progress`] with no
//! token is a real, working sink that discards — the host calls it identically either way,
//! and the transport decides whether anyone is listening.

use std::sync::atomic::{AtomicU64, Ordering};

use arbor_http::prelude::SseEvent;
use serde_json::{json, Value};
use tokio::sync::mpsc;

/// Where a tool's narration goes.
///
/// Cheap to clone-by-reference and safe to hold across a long call; sending on a closed
/// stream is a no-op, because a client that hung up mid-run is not an error for the run.
pub struct Progress {
    live: Option<Live>,
}

struct Live {
    /// The client's own token, echoed on every notification so it can correlate them with
    /// the call it made. Opaque: a string or a number, per the spec.
    token: Value,
    tx: mpsc::Sender<SseEvent>,
    /// `progress` must increase on every notification. Most work does not know its own
    /// length, so a counter is the honest default.
    seq: AtomicU64,
}

impl Progress {
    /// A sink nobody is listening to. The normal case.
    pub fn none() -> Self {
        Self { live: None }
    }

    /// A sink writing `notifications/progress` for `token` onto `tx`.
    pub fn to(token: Value, tx: mpsc::Sender<SseEvent>) -> Self {
        Self { live: Some(Live { token, tx, seq: AtomicU64::new(0) }) }
    }

    /// Whether anything is actually being sent — for a host that would otherwise do work
    /// purely to produce messages nobody reads.
    pub fn is_live(&self) -> bool {
        self.live.is_some()
    }

    /// Report one step. `done` / `total` are used only **together**: a fraction with no
    /// denominator is worse than a plain counter, since a client draws a bar from it.
    pub async fn send(&self, message: &str, done: Option<u64>, total: Option<u64>) {
        let Some(live) = &self.live else { return };

        let mut params = json!({ "progressToken": live.token, "message": message });
        match (done, total) {
            (Some(done), Some(total)) => {
                params["progress"] = json!(done);
                params["total"] = json!(total);
            }
            _ => {
                let n = live.seq.fetch_add(1, Ordering::Relaxed) + 1;
                params["progress"] = json!(n);
            }
        }

        let notification = json!({
            "jsonrpc": "2.0",
            "method": "notifications/progress",
            "params": params,
        });
        // A full channel means the client is not draining; dropping a progress line is
        // always better than parking the run that produced it.
        let _ = live.tx.try_send(SseEvent::data(notification.to_string()));
    }
}

/// The client's progress token for this request, when it asked for one.
///
/// `_meta.progressToken` — absent for the overwhelming majority of calls, and its absence
/// is the instruction not to send notifications rather than an omission to work around.
pub fn token_of(params: &Value) -> Option<Value> {
    let token = params.get("_meta")?.get("progressToken")?;
    match token {
        Value::String(_) | Value::Number(_) => Some(token.clone()),
        // The spec allows only a string or an integer. Anything else is a client bug, and
        // echoing it back would make our notifications unparseable too.
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn a_sink_with_no_token_discards_without_complaining() {
        let p = Progress::none();
        assert!(!p.is_live());
        p.send("anyone there?", None, None).await;
    }

    #[tokio::test]
    async fn each_message_carries_the_clients_token_and_an_increasing_count() {
        let (tx, mut rx) = mpsc::channel(8);
        let p = Progress::to(json!("abc"), tx);
        p.send("first", None, None).await;
        p.send("second", None, None).await;

        let parse = |e: SseEvent| serde_json::from_str::<Value>(&e.data).unwrap();
        let first = parse(rx.recv().await.unwrap());
        assert_eq!(first["method"], "notifications/progress");
        assert_eq!(first["params"]["progressToken"], "abc");
        assert_eq!(first["params"]["progress"], 1);
        assert_eq!(first["params"]["message"], "first");
        assert_eq!(parse(rx.recv().await.unwrap())["params"]["progress"], 2);
    }

    #[tokio::test]
    async fn a_fraction_is_sent_only_when_it_has_a_denominator() {
        let (tx, mut rx) = mpsc::channel(8);
        let p = Progress::to(json!(7), tx);
        p.send("half", Some(5), Some(10)).await;
        p.send("no idea", Some(5), None).await;

        let parse = |e: SseEvent| serde_json::from_str::<Value>(&e.data).unwrap();
        let with = parse(rx.recv().await.unwrap());
        assert_eq!(with["params"]["progress"], 5);
        assert_eq!(with["params"]["total"], 10);

        let without = parse(rx.recv().await.unwrap());
        assert_eq!(without["params"]["progress"], 1, "falls back to the counter");
        assert!(without["params"].get("total").is_none());
    }

    #[test]
    fn a_token_is_read_only_from_where_the_spec_puts_it() {
        assert_eq!(token_of(&json!({ "_meta": { "progressToken": 4 } })), Some(json!(4)));
        assert_eq!(token_of(&json!({ "_meta": { "progressToken": "x" } })), Some(json!("x")));
        assert_eq!(token_of(&json!({ "progressToken": 4 })), None);
        assert_eq!(token_of(&json!({})), None);
        // A token we cannot echo verbatim is no token at all.
        assert_eq!(token_of(&json!({ "_meta": { "progressToken": { "a": 1 } } })), None);
    }
}
