//! Backend events, for listeners **inside** the shell.
//!
//! Every backend event already has one destination: the frontend, via `AppHandle::emit`.
//! That is the right destination for all of them and the only possible one for most — a
//! panel is what an event is usually for.
//!
//! It is not the only *listener*, though. The MCP endpoint runs in this process and can be
//! waiting on a tool call that takes minutes; the events that call produces are exactly
//! what it needs to tell its client what is happening. Emitting to a webview does not help
//! it, and reaching into a backend's internals to find out would be worse.
//!
//! So this is a second, in-process destination for the same events: a subscriber names the
//! program it cares about and reads what that backend emits for as long as it holds the
//! subscription. Costs nothing when nobody is listening, which is almost always — the
//! publish returns before it clones anything.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};

use serde_json::Value;
use tokio::sync::mpsc;

/// One event, with the backend it came from.
///
/// The program is the shell's knowledge, not the backend's: an event arrives on that
/// backend's own channel, so nothing has to be stamped at the source.
#[derive(Debug, Clone)]
pub struct BackendEvent {
    pub program: &'static str,
    pub topic: String,
    pub payload: Value,
}

/// The process-wide tap.
pub fn tap() -> &'static EventTap {
    static TAP: OnceLock<EventTap> = OnceLock::new();
    TAP.get_or_init(EventTap::default)
}

#[derive(Default)]
pub struct EventTap {
    subscribers: Mutex<Vec<Subscriber>>,
    next_id: AtomicU64,
}

struct Subscriber {
    id: u64,
    program: &'static str,
    topic: &'static str,
    tx: mpsc::UnboundedSender<BackendEvent>,
}

impl EventTap {
    /// Listen to one topic of one program, until the returned [`Subscription`] is dropped.
    ///
    /// The topic is part of the subscription rather than something the listener filters on
    /// afterwards, and that is not tidiness: a Maven run emits a line of console output per
    /// line Maven prints — ten thousand of them — and a subscriber interested only in the
    /// progress topic would otherwise pay for a clone of every one.
    ///
    /// Unbounded, deliberately: the alternative is a publish that can block, and the
    /// publisher here is a backend's event reader thread — the one thing in the system that
    /// must never wait on a consumer.
    pub fn subscribe(&self, program: &'static str, topic: &'static str) -> Subscription {
        let (tx, rx) = mpsc::unbounded_channel();
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        if let Ok(mut subscribers) = self.subscribers.lock() {
            subscribers.push(Subscriber { id, program, topic, tx });
        }
        Subscription { id, rx }
    }

    /// Offer one event to whoever is listening to `program`.
    pub fn publish(&self, program: &'static str, topic: &str, payload: &Value) {
        let Ok(mut subscribers) = self.subscribers.lock() else { return };
        if subscribers.is_empty() {
            return;
        }
        // A closed receiver means a subscription that was dropped between the lock and the
        // send — reap it here rather than letting the list grow for the process's lifetime.
        subscribers.retain(|s| {
            s.program != program
                || s.topic != topic
                || s.tx
                    .send(BackendEvent {
                        program,
                        topic: topic.to_string(),
                        payload: payload.clone(),
                    })
                    .is_ok()
        });
    }

    fn unsubscribe(&self, id: u64) {
        if let Ok(mut subscribers) = self.subscribers.lock() {
            subscribers.retain(|s| s.id != id);
        }
    }
}

/// A live subscription. Dropping it stops the delivery.
pub struct Subscription {
    id: u64,
    rx: mpsc::UnboundedReceiver<BackendEvent>,
}

impl Subscription {
    /// The next event, or `None` once the tap has dropped this subscription.
    pub async fn recv(&mut self) -> Option<BackendEvent> {
        self.rx.recv().await
    }
}

impl Drop for Subscription {
    fn drop(&mut self) {
        tap().unsubscribe(self.id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use arbor_ipc::prelude::PROGRESS_TOPIC;
    use serde_json::json;

    // The tap is process-wide, and `cargo test` runs these on threads at once — so every
    // test names its OWN program. Sharing one made them interfere: a publish from the
    // drop test landed in the filter test's queue and failed it on whichever ran first.
    // The keys are the isolation the type already offers; the alternative is a mutex that
    // serialises tests to work around a static they need not have shared.

    #[tokio::test]
    async fn a_subscriber_hears_its_own_program_and_topic_and_nothing_else() {
        let mut sub = tap().subscribe("tap-filter", PROGRESS_TOPIC);
        tap().publish("tap-filter-other", PROGRESS_TOPIC, &json!({ "message": "another product" }));
        tap().publish("tap-filter", "arbor://bennu/test-output", &json!({ "text": "a console line" }));
        tap().publish("tap-filter", PROGRESS_TOPIC, &json!({ "message": "Running" }));

        let event = sub.recv().await.unwrap();
        assert_eq!(event.program, "tap-filter");
        assert_eq!(event.payload["message"], "Running", "the other two must not be queued");
    }

    #[tokio::test]
    async fn publishing_with_nobody_listening_is_a_no_op() {
        // The normal state of the system: every backend event goes through here.
        tap().publish("tap-unheard", PROGRESS_TOPIC, &json!({ "message": "unheard" }));
    }

    #[tokio::test]
    async fn a_dropped_subscription_stops_receiving() {
        let sub = tap().subscribe("tap-dropped", PROGRESS_TOPIC);
        let id = sub.id;
        drop(sub);
        tap().publish("tap-dropped", PROGRESS_TOPIC, &json!({}));
        let held = tap().subscribers.lock().unwrap();
        assert!(!held.iter().any(|s| s.id == id));
    }
}
