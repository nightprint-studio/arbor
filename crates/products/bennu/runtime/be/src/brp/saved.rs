//! What the user keeps between sessions: the games they connect to, and the searches they run often.
//!
//! Stored in the profile's own `bennu/brp.toml` (not in `config.toml`, whose struct is the editor's):
//! the file is rewritten whole on every change, and a missing or unreadable one is just "nothing saved
//! yet". The frontend owns the list and sends it back complete, so this file has no add/remove verbs —
//! only a read, and a normalising write.

use bennu_core::prelude::BennuState;
use serde::{Deserialize, Serialize};

/// More than this and the list stops being something you pick from.
const MAX_ENTRIES: usize = 50;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct SavedEndpoint {
    pub name: String,
    pub endpoint: String,
}

/// A search-box string (`goblin has:Health no:Dead`) with a name.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct SavedQuery {
    pub name: String,
    pub search: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Saved {
    pub endpoints: Vec<SavedEndpoint>,
    pub queries: Vec<SavedQuery>,
}

fn path() -> std::path::PathBuf {
    arbor_core::prelude::bennu_config_path("brp.toml")
}

/// Trim, drop blanks, keep the first of any duplicate key, cap the length.
fn dedupe<T>(items: Vec<T>, key: impl Fn(&T) -> String, blank: impl Fn(&T) -> bool) -> Vec<T> {
    let mut seen = std::collections::HashSet::new();
    items
        .into_iter()
        .filter(|i| !blank(i) && seen.insert(key(i)))
        .take(MAX_ENTRIES)
        .collect()
}

fn normalise(saved: Saved) -> Saved {
    let endpoints = saved
        .endpoints
        .into_iter()
        .map(|e| SavedEndpoint { name: e.name.trim().to_string(), endpoint: e.endpoint.trim().to_string() })
        .collect();
    let queries = saved
        .queries
        .into_iter()
        .map(|q| SavedQuery { name: q.name.trim().to_string(), search: q.search.trim().to_string() })
        .collect();
    Saved {
        endpoints: dedupe(endpoints, |e: &SavedEndpoint| e.endpoint.to_lowercase(), |e| e.endpoint.is_empty()),
        queries: dedupe(queries, |q: &SavedQuery| q.name.to_lowercase(), |q| q.name.is_empty() || q.search.is_empty()),
    }
}

fn read() -> Saved {
    std::fs::read_to_string(path())
        .ok()
        .and_then(|text| toml::from_str::<Saved>(&text).ok())
        .map(normalise)
        .unwrap_or_default()
}

fn write(saved: &Saved) -> Result<(), String> {
    let path = path();
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let text = toml::to_string_pretty(saved).map_err(|e| e.to_string())?;
    std::fs::write(&path, text).map_err(|e| e.to_string())
}

/// The saved endpoints and searches.
#[arbor_rpc::handler]
async fn bennu_brp_saved_get(_ctx: &BennuState) -> Result<Saved, String> {
    tokio::task::spawn_blocking(read).await.map_err(|e| e.to_string())
}

/// Replace what is saved; answers with what was actually stored (normalised).
#[arbor_rpc::handler]
async fn bennu_brp_saved_set(_ctx: &BennuState, args: Saved) -> Result<Saved, String> {
    let saved = normalise(args);
    let out = saved.clone();
    tokio::task::spawn_blocking(move || write(&saved)).await.map_err(|e| e.to_string())??;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn endpoint(name: &str, url: &str) -> SavedEndpoint {
        SavedEndpoint { name: name.into(), endpoint: url.into() }
    }

    #[test]
    fn blanks_are_dropped_and_text_is_trimmed() {
        let saved = normalise(Saved {
            endpoints: vec![endpoint(" Game ", " http://127.0.0.1:15702 "), endpoint("x", "  ")],
            queries: vec![SavedQuery { name: "".into(), search: "has:Health".into() }, SavedQuery { name: "Alive".into(), search: " has:Health no:Dead ".into() }],
        });
        assert_eq!(saved.endpoints, vec![endpoint("Game", "http://127.0.0.1:15702")]);
        assert_eq!(saved.queries, vec![SavedQuery { name: "Alive".into(), search: "has:Health no:Dead".into() }]);
    }

    #[test]
    fn the_first_of_a_duplicate_endpoint_wins_ignoring_case() {
        let saved = normalise(Saved {
            endpoints: vec![endpoint("A", "http://Localhost:1"), endpoint("B", "http://localhost:1"), endpoint("C", "http://localhost:2")],
            queries: vec![],
        });
        assert_eq!(saved.endpoints.iter().map(|e| e.name.as_str()).collect::<Vec<_>>(), ["A", "C"]);
    }

    #[test]
    fn the_list_is_capped() {
        let many = (0..MAX_ENTRIES + 10).map(|i| endpoint("n", &format!("http://localhost:{i}"))).collect();
        assert_eq!(normalise(Saved { endpoints: many, queries: vec![] }).endpoints.len(), MAX_ENTRIES);
    }

    #[test]
    fn a_saved_file_round_trips_through_toml() {
        let saved = Saved { endpoints: vec![endpoint("Game", "http://127.0.0.1:15702")], queries: vec![SavedQuery { name: "Alive".into(), search: "has:Health".into() }] };
        let text = toml::to_string_pretty(&saved).unwrap();
        assert_eq!(toml::from_str::<Saved>(&text).unwrap(), saved);
    }
}
