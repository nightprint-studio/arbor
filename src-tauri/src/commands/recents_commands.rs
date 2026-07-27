//! Cross-product "recently opened" history + the open-with-a-project intent.
//!
//! **Why the shell owns the history.** Each product already remembers what it
//! opened, but in its own place and with its own lifetime: Corvus in
//! `recent_repos`, Bennu only in memory, Merula in its own config behind
//! `merula-be`. Canopy would have to start three backends just to draw a list.
//! So every product reports what it opens to [`record_recent_project`], and the
//! launcher reads one list that is always available.
//!
//! **Why an intent.** Opening a recent means "start that product ON this
//! project", but a window opener takes no arguments and a product shell boots
//! long after the click. The launcher parks the path here, opens the product the
//! usual way, and the product's shell pulls it as it mounts — the same
//! pull-flag shape as Tyto's snip intent and the container's product intent.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use tauri::State;

use crate::config::app_config::{self, RecentProject};
use crate::error::AppError;
use crate::AppState;

/// Cap on the stored history. Long enough to cover "what was I doing last
/// week", short enough that the config file stays readable.
const MAX_RECENTS: usize = 40;

/// Parked "open this project" requests, keyed by product id. Consumed once.
static OPEN_INTENT: Mutex<Option<HashMap<String, String>>> = Mutex::new(None);

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Normalise a path for identity comparison: trailing separators dropped, case
/// folded (Windows paths differ only in case all the time).
fn norm(path: &str) -> String {
    path.trim_end_matches(['/', '\\']).to_lowercase()
}

/// Record that `product` opened `path`. Called by each product as it opens a
/// project — including re-opens, which just move the entry to the top.
#[tauri::command]
pub fn record_recent_project(
    state: State<'_, AppState>,
    product: String,
    path: String,
    name: String,
) -> Result<(), AppError> {
    if path.trim().is_empty() {
        return Ok(());
    }
    let key = norm(&path);
    let mut cfg = state.lock_config()?;
    // One entry per (product, path): re-opening moves it up rather than piling
    // duplicates that would push everything else out of the list.
    cfg.recents
        .retain(|r| !(r.product == product && norm(&r.path) == key));
    cfg.recents.insert(
        0,
        RecentProject { product, path, name, opened_at: now_secs() },
    );
    cfg.recents.truncate(MAX_RECENTS);
    let cfg_clone = cfg.clone();
    drop(cfg);
    app_config::save(&cfg_clone).map_err(|e| AppError::Other(e.to_string()))?;
    Ok(())
}

/// The cross-product history, newest first.
#[tauri::command]
pub fn list_recent_projects(state: State<'_, AppState>) -> Result<Vec<RecentProject>, AppError> {
    let mut list = state.lock_config()?.recents.clone();
    list.sort_by(|a, b| b.opened_at.cmp(&a.opened_at));
    Ok(list)
}

/// Drop one entry — the launcher's "remove from recents".
#[tauri::command]
pub fn forget_recent_project(
    state: State<'_, AppState>,
    product: String,
    path: String,
) -> Result<(), AppError> {
    let key = norm(&path);
    let mut cfg = state.lock_config()?;
    let before = cfg.recents.len();
    cfg.recents
        .retain(|r| !(r.product == product && norm(&r.path) == key));
    if cfg.recents.len() == before {
        return Ok(());
    }
    let cfg_clone = cfg.clone();
    drop(cfg);
    app_config::save(&cfg_clone).map_err(|e| AppError::Other(e.to_string()))?;
    Ok(())
}

/// Event announcing a parked open-intent, for products that are ALREADY up.
///
/// A shell only pulls the intent as it mounts, so without this a recent clicked
/// while its product is open would just focus the window and do nothing.
pub const OPEN_INTENT_EVENT: &str = "arbor://open-intent";

#[derive(Clone, serde::Serialize)]
struct OpenIntent<'a> {
    product: &'a str,
    path: &'a str,
}

/// Park "open this project" for `product`, to be pulled by its shell on mount.
/// Overwrites any pending request for the same product — the latest click wins.
#[tauri::command]
pub fn set_open_intent(app: tauri::AppHandle, product: String, path: String) {
    if let Ok(mut g) = OPEN_INTENT.lock() {
        g.get_or_insert_with(HashMap::new)
            .insert(product.clone(), path.clone());
    }
    // Broadcast for the already-running case; a shell that reacts consumes the
    // parked entry through `take_open_intent`, so the two paths can't both fire.
    use tauri::Emitter;
    let _ = app.emit(
        OPEN_INTENT_EVENT,
        OpenIntent { product: &product, path: &path },
    );
}

/// Pull the parked project path for `product`, if any. Returns `None` on every
/// call after the first — a product must not re-open the same project each time
/// its shell remounts.
#[tauri::command]
pub fn take_open_intent(product: String) -> Option<String> {
    OPEN_INTENT
        .lock()
        .ok()
        .and_then(|mut g| g.as_mut().and_then(|m| m.remove(&product)))
}
