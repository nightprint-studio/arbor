//! `activity` domain — what every session on the server is doing, and asking one
//! of them to stop.
//!
//! Two calls, and the asymmetry between them is the point. [`picus_activity`] is a
//! read the interface repeats on a timer; [`picus_stop_session`] interrupts
//! somebody else's work and is fired once, by hand, after a confirmation.
//!
//! Neither is offered by every engine, so both go through the optional half of
//! [`DbSession`](picus_db_api::prelude::DbSession) and the interface reads
//! `capabilities.sessionActivity` rather than calling and catching. A monitor that
//! is *absent* on an engine without the concept is honest; one that is present and
//! errors on every refresh is not.

use picus_core::prelude::PicusState;
use picus_db_api::prelude::{ActivitySnapshot, StopKind};

use crate::connections::require_session;

/// Every backend on the server this connection is open to, with the blocking graph.
///
/// One read, so the sessions and the edges describe the same instant — see
/// [`picus_db_api::prelude::ActivitySnapshot`]. The caller polls this while its
/// panel is on screen and stops when it is not: this is a query against somebody's
/// production server, and a poll nobody is watching is load bought for nothing.
#[arbor_rpc::handler]
async fn picus_activity(state: &PicusState, id: String) -> Result<ActivitySnapshot, String> {
    require_session(state, &id)?.activity().await.map_err(|e| e.to_string())
}

/// Ask one backend to stop — cancel its statement, or close it outright.
///
/// `pid` is the server-side process id from the snapshot, never an index into it:
/// the list is refreshed under the user and a positional handle would eventually
/// terminate the row that moved into place.
///
/// The boolean is the **server's** answer, and it means "there was no such pid" —
/// the session ended between the read and the click, which is ordinary. A refusal
/// for want of privilege comes back as an error carrying the server's own sentence
/// instead, because "you are not allowed to do that" and "it was already gone" are
/// different things to tell somebody who just pressed Terminate.
#[arbor_rpc::handler]
async fn picus_stop_session(
    state: &PicusState,
    id: String,
    pid: i32,
    kind: StopKind,
) -> Result<bool, String> {
    require_session(state, &id)?.stop_session(pid, kind).await.map_err(|e| e.to_string())
}
