//! [`SchemaCache`] — what each connection last said about itself.
//!
//! Reading a schema is a round trip to a database, and there are two things that
//! want it constantly: the object tree in the sidebar, and the SQL abbreviation
//! expander, which is asked again on **every keystroke**. Going to the server for
//! that would turn a typing aid into a reason to close the panel.
//!
//! So the snapshot is held, keyed by connection id, and — exactly like
//! [`crate::scripts::ScriptCache`] — **nothing here expires on its own**. A schema
//! that quietly refreshed itself would make "why does this say the column is gone"
//! depend on how long the user had been looking at it. It is dropped when the
//! connection reads its schema again, when the connection closes, and at no other
//! time.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard};

// Through `picus-db-api`, which already re-exports the shared vocabulary: this
// crate has no need to name `picus-types` directly, and the README of that crate
// says so.
use picus_db_api::prelude::SchemaSnapshot;

/// Schemas read so far, by connection id.
#[derive(Default)]
pub struct SchemaCache {
    by_connection: Mutex<HashMap<String, Arc<SchemaSnapshot>>>,
}

impl std::fmt::Debug for SchemaCache {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let held = self.by_connection.lock().map(|g| g.len()).unwrap_or(0);
        f.debug_struct("SchemaCache").field("connections", &held).finish()
    }
}

impl SchemaCache {
    pub fn new() -> Self {
        Self::default()
    }

    /// The schema a connection last reported, if it has reported one.
    pub fn get(&self, connection: &str) -> Option<Arc<SchemaSnapshot>> {
        self.lock().get(connection).cloned()
    }

    /// Store a freshly read schema, replacing any previous one.
    pub fn put(&self, connection: &str, schema: Arc<SchemaSnapshot>) {
        self.lock().insert(connection.to_string(), schema);
    }

    /// Forget one connection's schema — what a re-read and a disconnect do.
    pub fn invalidate(&self, connection: &str) {
        self.lock().remove(connection);
    }

    pub fn clear(&self) {
        self.lock().clear();
    }

    pub fn len(&self) -> usize {
        self.lock().len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// A poisoned lock is recovered from rather than propagated: the map holds
    /// nothing a panic could have left half-written, and refusing to answer would
    /// take the whole panel down for a cache.
    fn lock(&self) -> MutexGuard<'_, HashMap<String, Arc<SchemaSnapshot>>> {
        self.by_connection.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn schema() -> Arc<SchemaSnapshot> {
        Arc::new(SchemaSnapshot {
            tables: Vec::new(),
            views: Vec::new(),
            sequences: Vec::new(),
            triggers: Vec::new(),
        })
    }

    #[test]
    fn a_schema_is_held_until_something_drops_it() {
        let cache = SchemaCache::new();
        assert!(cache.get("conn-1").is_none());

        cache.put("conn-1", schema());
        assert!(cache.get("conn-1").is_some());
        assert!(cache.get("conn-2").is_none(), "connections do not share");

        cache.invalidate("conn-1");
        assert!(cache.get("conn-1").is_none());
    }

    #[test]
    fn re_reading_replaces_rather_than_accumulating() {
        let cache = SchemaCache::new();
        cache.put("conn-1", schema());
        cache.put("conn-1", schema());
        assert_eq!(cache.len(), 1);
        cache.clear();
        assert!(cache.is_empty());
    }
}
