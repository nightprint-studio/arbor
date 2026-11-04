//! The assistant's message as the panel draws it, built up while a run goes on.
//!
//! The builder itself is product-neutral and lives in `imber-core` (`View`), so Bennu and Corvus stream
//! the very same block shapes to the same panel; this module only re-exports it under the names the run
//! code here already uses.

pub(super) use bennu_assistant::prelude::{detail_of, View};
