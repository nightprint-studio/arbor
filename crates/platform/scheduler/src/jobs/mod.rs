//! Scheduled jobs as **data**: a [`Job`] (when, what, how the last run went), the [`Store`] that keeps the
//! list in one atomically-written JSON file, and [`when_to_recurrence`] for the friendly `when` an assistant
//! writes. The calendar arithmetic underneath is [`crate::calendar`].
//!
//! Product-neutral on purpose: Imber's schedules (Corvus, Bennu) and Hirundo's runner schedules are the same
//! idea with a different *action*, so each product brings its own [`JobAction`] and nothing else.

mod job;
mod store;
mod when;

pub use job::{clip, Job, JobAction, JobInput, LastRun, Tick, MAX_DETAIL_CHARS, MAX_NAME_CHARS, MAX_PROMPT_CHARS};
pub use store::{new_id, now_ms, parse, write_atomic, Reader, Store};
pub use when::when_to_recurrence;
