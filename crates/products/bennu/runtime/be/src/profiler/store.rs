//! Recordings on disk: `<project>/.arbor/bennu/profiles/<id>.json` (the whole profile) beside a
//! small `<id>.meta.json` the list reads, so listing never loads a call tree.

use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};

use bennu_profile::prelude::*;
use serde::{Deserialize, Serialize};

/// Recordings kept per project; older ones are deleted when a new one lands.
const KEEP: usize = 20;
/// Profiles held decoded in memory (a tree can be tens of MB).
const CACHE: usize = 3;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfileMeta {
    pub id: String,
    pub lang: Lang,
    pub label: String,
    pub created_ms: u64,
    pub duration_ms: Option<f64>,
    pub samples: u64,
    pub ms_per_unit: f64,
    /// `Class.method` of the hottest self-time method, for the list row.
    pub top_method: String,
}

pub fn dir(root: &str) -> PathBuf {
    PathBuf::from(root).join(".arbor").join("bennu").join("profiles")
}

fn valid_id(id: &str) -> bool {
    !id.is_empty() && id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

fn cache() -> &'static Mutex<Vec<(String, Arc<Profile>)>> {
    static C: OnceLock<Mutex<Vec<(String, Arc<Profile>)>>> = OnceLock::new();
    C.get_or_init(|| Mutex::new(Vec::new()))
}

pub fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

pub fn meta_of(p: &Profile) -> ProfileMeta {
    ProfileMeta {
        id: p.id.clone(),
        lang: p.lang,
        label: p.label.clone(),
        created_ms: p.created_ms,
        duration_ms: p.duration_ms,
        samples: p.tree.total,
        ms_per_unit: p.ms_per_unit,
        top_method: p.methods.first().map(|m| format!("{}.{}", m.class, m.method)).unwrap_or_default(),
    }
}

/// Write a profile (and its meta), then trim the directory to [`KEEP`] recordings.
pub fn save(root: &str, p: &Profile) -> Result<(), String> {
    let d = dir(root);
    std::fs::create_dir_all(&d).map_err(|e| format!("cannot create {}: {e}", d.display()))?;
    let body = serde_json::to_vec(p).map_err(|e| e.to_string())?;
    std::fs::write(d.join(format!("{}.json", p.id)), body).map_err(|e| e.to_string())?;
    let meta = serde_json::to_vec(&meta_of(p)).map_err(|e| e.to_string())?;
    std::fs::write(d.join(format!("{}.meta.json", p.id)), meta).map_err(|e| e.to_string())?;
    let all = list(root);
    for old in all.iter().skip(KEEP) {
        let _ = delete(root, &old.id);
    }
    Ok(())
}

/// Newest first.
pub fn list(root: &str) -> Vec<ProfileMeta> {
    let Ok(rd) = std::fs::read_dir(dir(root)) else { return Vec::new() };
    let mut v: Vec<ProfileMeta> = rd
        .filter_map(Result::ok)
        .filter(|e| e.file_name().to_string_lossy().ends_with(".meta.json"))
        .filter_map(|e| serde_json::from_slice(&std::fs::read(e.path()).ok()?).ok())
        .collect();
    v.sort_by(|a, b| b.created_ms.cmp(&a.created_ms));
    v
}

pub fn load(root: &str, id: &str) -> Result<Arc<Profile>, String> {
    if !valid_id(id) {
        return Err(format!("invalid recording id `{id}`"));
    }
    let key = format!("{root}\u{0}{id}");
    if let Ok(c) = cache().lock() {
        if let Some((_, p)) = c.iter().find(|(k, _)| *k == key) {
            return Ok(p.clone());
        }
    }
    let path = dir(root).join(format!("{id}.json"));
    let bytes = std::fs::read(&path).map_err(|e| format!("recording `{id}` cannot be read: {e}"))?;
    let p: Profile = serde_json::from_slice(&bytes).map_err(|e| format!("recording `{id}` is corrupt: {e}"))?;
    let p = Arc::new(p);
    if let Ok(mut c) = cache().lock() {
        c.retain(|(k, _)| *k != key);
        c.insert(0, (key, p.clone()));
        c.truncate(CACHE);
    }
    Ok(p)
}

/// The newest recording of `lang` (any language when `None`).
pub fn latest(root: &str, lang: Option<Lang>) -> Option<ProfileMeta> {
    list(root).into_iter().find(|m| lang.map_or(true, |l| m.lang == l))
}

pub fn delete(root: &str, id: &str) -> Result<(), String> {
    if !valid_id(id) {
        return Err(format!("invalid recording id `{id}`"));
    }
    let d = dir(root);
    let _ = std::fs::remove_file(d.join(format!("{id}.json")));
    let _ = std::fs::remove_file(d.join(format!("{id}.meta.json")));
    if let Ok(mut c) = cache().lock() {
        c.retain(|(k, _)| !k.ends_with(&format!("\u{0}{id}")));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tiny(id: &str, created: u64) -> Profile {
        let mut b = TreeBuilder::default();
        b.add_stack(Some("t"), &[RawFrame::method("a.B", "m")], 3);
        let tree = b.finish();
        Profile {
            id: id.into(),
            lang: Lang::Java,
            label: "x".into(),
            created_ms: created,
            duration_ms: None,
            ms_per_unit: 10.0,
            methods: tree.method_stats(10),
            threads: tree.thread_stats(),
            tree,
            alloc_sites: vec![],
            alloc_types: vec![],
            locks: vec![],
            gc: vec![],
            notes: vec![],
        }
    }

    #[test]
    fn save_list_load_delete_round_trip_and_ids_are_validated() {
        let root = std::env::temp_dir().join(format!("bennu-profile-store-test-{}", now_ms()));
        let r = root.display().to_string();
        save(&r, &tiny("java-1", 1)).unwrap();
        save(&r, &tiny("java-2", 2)).unwrap();
        let l = list(&r);
        assert_eq!(l.iter().map(|m| m.id.as_str()).collect::<Vec<_>>(), ["java-2", "java-1"]);
        assert_eq!(l[0].top_method, "a.B.m");
        assert_eq!(load(&r, "java-1").unwrap().tree.total, 3);
        assert!(load(&r, "../evil").is_err());
        delete(&r, "java-1").unwrap();
        assert_eq!(list(&r).len(), 1);
        let _ = std::fs::remove_dir_all(&root);
    }
}
