//! The one `dev` session of the process, and the protocol the user chose.
//!
//! Like the inspector's BRP connection, a Bennu window is attached to one game at a time over this wire, so
//! the link is a process-wide slot. The lock is only held to copy an `Arc` in or out — never across an
//! `.await` or a request — so a slow game cannot stall the handlers that merely ask "is there a link?".

use std::sync::{Arc, Mutex, MutexGuard};

use crate::dev::DevChannelLink;
use crate::pick::Protocol;

static DEV: Mutex<Option<Arc<DevChannelLink>>> = Mutex::new(None);
static PREFERENCE: Mutex<Protocol> = Mutex::new(Protocol::Auto);

fn guard<T>(m: &'static Mutex<T>) -> MutexGuard<'static, T> {
    // A poisoned lock means a handler panicked while holding it; the data is a plain slot with no
    // invariant spanning fields, so carrying on beats wedging the link until restart.
    m.lock().unwrap_or_else(|e| e.into_inner())
}

/// The open `dev` link, if there is one.
pub fn dev() -> Option<Arc<DevChannelLink>> {
    guard(&DEV).clone()
}

/// Make `link` the open `dev` link, replacing (and dropping) the one before.
pub fn set_dev(link: Arc<DevChannelLink>) {
    *guard(&DEV) = Some(link);
}

/// Drop the `dev` link. `true` when there was one.
pub fn clear_dev() -> bool {
    guard(&DEV).take().is_some()
}

/// The protocol the game tools route by (set by the last `bevy_game_connect`; `auto` before any).
pub fn preference() -> Protocol {
    *guard(&PREFERENCE)
}

pub fn set_preference(protocol: Protocol) {
    *guard(&PREFERENCE) = protocol;
}
