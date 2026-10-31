//! The project's log profile on disk, so the next start has it before the first file is read.
//!
//! What is written is what the profile already holds and no more: patterns (a level and a message template with
//! slots), logger names, and per file the pattern each call used plus a hash of the text that was counted. Never a
//! line of source — the line-frequency index keeps those, in memory only, and is not touched here.
//!
//! The file lives in the user's Arbor data directory, one per project root, and carries a format version and two
//! probes (the hash functions behind the pattern ids and the content hashes). A copy written by another version,
//! another Rust release or a damaged file is simply not loaded, which means "scan again".

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use bennu_flc::prelude::{LogSnapshot, Placement};
use serde::{Deserialize, Serialize};

const VERSION: u32 = 1;

#[derive(Serialize, Deserialize)]
struct Doc {
    version: u32,
    profile_probe: u64,
    text_probe: u64,
    patterns: Vec<(u64, String, String)>,
    files: Vec<FileDoc>,
}

#[derive(Serialize, Deserialize)]
struct FileDoc {
    /// Relative to the project root, forward slashes: the copy survives the project being moved.
    path: String,
    /// Hash of the text whose log calls are counted.
    hash: u64,
    calls: Vec<(u8, u64, String)>,
}

/// What a saved profile gives back, with absolute file keys.
pub(crate) struct Loaded {
    pub snapshot: LogSnapshot,
    /// File key → hash of the text that was counted, for every file the copy knew, log calls or not.
    pub hashes: HashMap<String, u64>,
}

/// Where the profile of `root` is kept.
pub(crate) fn profile_path(root: &Path) -> PathBuf {
    let mut hash: u64 = 0xcbf29ce484222325;
    for b in root.to_string_lossy().as_bytes() {
        hash ^= *b as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    arbor_core::prelude::bennu_data_dir().join("flc").join(format!("{hash:016x}.json"))
}

fn placement_code(p: Placement) -> u8 {
    match p {
        Placement::Entry => 0,
        Placement::Catch => 1,
        Placement::Body => 2,
    }
}

fn placement_of(code: u8) -> Option<Placement> {
    match code {
        0 => Some(Placement::Entry),
        1 => Some(Placement::Catch),
        2 => Some(Placement::Body),
        _ => None,
    }
}

fn relative(root: &Path, key: &str) -> Option<String> {
    let rel = Path::new(key).strip_prefix(root).ok()?;
    Some(rel.to_string_lossy().replace('\\', "/"))
}

/// Writes the part of `snapshot` and `hashes` that lies under `root`, atomically. `probes` are the current
/// `(profile hash probe, text hash probe)`.
pub(crate) fn save_to(
    path: &Path,
    root: &Path,
    snapshot: &LogSnapshot,
    hashes: &HashMap<String, u64>,
    probes: (u64, u64),
) -> std::io::Result<()> {
    let mut calls: HashMap<&str, &Vec<(Placement, u64, String)>> = HashMap::new();
    for (key, c) in &snapshot.files {
        calls.insert(key, c);
    }
    let mut files: Vec<FileDoc> = hashes
        .iter()
        .filter_map(|(key, hash)| {
            let path = relative(root, key)?;
            let calls = calls
                .get(key.as_str())
                .map(|c| c.iter().map(|(p, h, r)| (placement_code(*p), *h, r.clone())).collect())
                .unwrap_or_default();
            Some(FileDoc { path, hash: *hash, calls })
        })
        .collect();
    files.sort_by(|a, b| a.path.cmp(&b.path));
    let used: HashSet<u64> = files.iter().flat_map(|f| f.calls.iter().map(|c| c.1)).collect();
    let patterns = snapshot.patterns.iter().filter(|(h, _, _)| used.contains(h)).cloned().collect();
    let doc = Doc { version: VERSION, profile_probe: probes.0, text_probe: probes.1, patterns, files };
    let json = serde_json::to_vec(&doc).map_err(std::io::Error::other)?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, json)?;
    std::fs::rename(&tmp, path)
}

/// Reads a saved profile of `root`; `None` when there is none, it is damaged, or it was written under other rules.
pub(crate) fn load_from(path: &Path, root: &Path, probes: (u64, u64)) -> Option<Loaded> {
    let doc: Doc = serde_json::from_slice(&std::fs::read(path).ok()?).ok()?;
    if doc.version != VERSION || (doc.profile_probe, doc.text_probe) != probes {
        return None;
    }
    let mut hashes = HashMap::new();
    let mut files = Vec::new();
    for f in doc.files {
        let key = root.join(&f.path).to_string_lossy().into_owned();
        hashes.insert(key.clone(), f.hash);
        let calls: Vec<_> = f.calls.into_iter().filter_map(|(p, h, r)| Some((placement_of(p)?, h, r))).collect();
        if !calls.is_empty() {
            files.push((key, calls));
        }
    }
    Some(Loaded { snapshot: LogSnapshot { patterns: doc.patterns, files }, hashes })
}

/// The files a copy knew that the disk no longer has.
pub(crate) fn vanished(known: &HashMap<String, u64>, walked: &HashSet<String>) -> Vec<String> {
    let mut gone: Vec<String> = known.keys().filter(|k| !walked.contains(*k)).cloned().collect();
    gone.sort();
    gone
}

#[cfg(test)]
mod tests {
    use super::*;
    use bennu_flc::prelude::LogProfile;

    const SRC: &str = "class A {\n    void save(String id) {\n        log.debug(\"START - save({})\", id);\n    }\n}\n";

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("arbor-flc-persist-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn a_saved_profile_comes_back_with_its_counters_and_hashes() {
        let dir = scratch("roundtrip");
        let root = dir.join("proj");
        let a = root.join("A.java").to_string_lossy().into_owned();
        let b = root.join("B.java").to_string_lossy().into_owned();
        let mut profile = LogProfile::new();
        profile.add_file(&a, SRC);
        let hashes = HashMap::from([(a.clone(), 7u64), (b.clone(), 9u64)]); // B has no log calls
        let file = dir.join("p.json");
        save_to(&file, &root, &profile.snapshot(|_| true), &hashes, (1, 2)).unwrap();

        let loaded = load_from(&file, &root, (1, 2)).expect("loads");
        assert_eq!(loaded.hashes, hashes);
        let mut back = LogProfile::new();
        back.restore(&loaded.snapshot);
        assert_eq!(back.call_count(), 1);
        // no source line in the file
        let text = std::fs::read_to_string(&file).unwrap();
        assert!(!text.contains("class A") && !text.contains("void save"), "{text}");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_copy_written_under_other_rules_or_damaged_is_not_loaded() {
        let dir = scratch("rules");
        let root = dir.join("proj");
        let file = dir.join("p.json");
        save_to(&file, &root, &LogSnapshot::default(), &HashMap::new(), (1, 2)).unwrap();
        assert!(load_from(&file, &root, (1, 3)).is_none(), "another text hash");
        assert!(load_from(&file, &root, (9, 2)).is_none(), "another pattern hash");
        std::fs::write(&file, b"{ not json").unwrap();
        assert!(load_from(&file, &root, (1, 2)).is_none());
        assert!(load_from(&dir.join("missing.json"), &root, (1, 2)).is_none());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_file_outside_the_root_is_not_written() {
        let dir = scratch("outside");
        let root = dir.join("proj");
        let hashes = HashMap::from([(dir.join("other").join("X.java").to_string_lossy().into_owned(), 1u64)]);
        let file = dir.join("p.json");
        save_to(&file, &root, &LogSnapshot::default(), &hashes, (1, 2)).unwrap();
        assert!(load_from(&file, &root, (1, 2)).unwrap().hashes.is_empty());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn the_files_the_disk_no_longer_has_are_the_ones_not_walked() {
        let known = HashMap::from([("a".to_string(), 1u64), ("b".to_string(), 2), ("c".to_string(), 3)]);
        let walked = HashSet::from(["a".to_string(), "c".to_string()]);
        assert_eq!(vanished(&known, &walked), ["b"]);
    }
}
