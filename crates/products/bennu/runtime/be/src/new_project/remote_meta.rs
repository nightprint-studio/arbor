//! The service metadata the forms are built from, cached.
//!
//! Fetched at most once an hour per service, held in memory, and written to Bennu's data dir so a
//! later session that is offline still opens a populated form (marked stale, so the wizard can say
//! so). Creating the project still needs the network; that failure has its own clear message — the
//! point of the stale copy is that the user can see and fill the form on a train.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use serde_json::Value;

use super::http::get_json;

const TTL: Duration = Duration::from_secs(3600);

/// What a lookup answered with.
pub struct Fetched {
    pub value: Value,
    /// Read from the on-disk copy because the service could not be reached.
    pub stale: bool,
}

fn memory() -> &'static Mutex<HashMap<String, (Instant, Value)>> {
    static M: OnceLock<Mutex<HashMap<String, (Instant, Value)>>> = OnceLock::new();
    M.get_or_init(Default::default)
}

fn disk_path(key: &str) -> PathBuf {
    arbor_core::prelude::bennu_data_dir().join("new-project").join(format!("{key}.json"))
}

/// Memory, then the network, then the stale disk copy. `key` is a fixed identifier (not user input).
pub async fn cached_json(key: &str, url: &str, accept: &str, refresh: bool) -> Result<Fetched, String> {
    if !refresh {
        let hit = memory().lock().ok().and_then(|m| m.get(key).filter(|(at, _)| at.elapsed() < TTL).map(|(_, v)| v.clone()));
        if let Some(value) = hit {
            return Ok(Fetched { value, stale: false });
        }
    }
    match get_json(url, accept).await {
        Ok(value) => {
            if let Ok(mut m) = memory().lock() {
                m.insert(key.to_string(), (Instant::now(), value.clone()));
            }
            let path = disk_path(key);
            if let Some(dir) = path.parent() {
                let _ = std::fs::create_dir_all(dir);
            }
            let _ = serde_json::to_vec(&value).map(|bytes| std::fs::write(&path, bytes));
            Ok(Fetched { value, stale: false })
        }
        Err(e) => match std::fs::read(disk_path(key)).ok().and_then(|b| serde_json::from_slice::<Value>(&b).ok()) {
            Some(value) => Ok(Fetched { value, stale: true }),
            None => Err(e),
        },
    }
}
