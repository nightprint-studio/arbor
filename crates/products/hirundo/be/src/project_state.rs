//! Where Hirundo keeps what belongs to *this machine's use* of a project — schedules and the runner's
//! history — as opposed to what belongs to the project (`.hirundo/`, committed). It lives in the profile,
//! `profiles/<p>/hirundo/state/<project-hash>/<name>.json`, so it never lands in a diff and is not shared
//! by a clone: a schedule that fires on one laptop is not a schedule that fires on every colleague's.

use std::path::PathBuf;

use serde::{de::DeserializeOwned, Serialize};

/// A stable, file-name-safe tag of a project path (FNV-1a; no dependency for a name).
pub fn project_tag(project: &str) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for b in project.trim_end_matches(['/', '\\']).to_lowercase().replace('\\', "/").bytes() {
        hash ^= u64::from(b);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{hash:016x}")
}

fn path_of(project: &str, name: &str) -> PathBuf {
    arbor_core::prelude::product_path("hirundo", format!("state/{}/{name}.json", project_tag(project)))
}

/// The stored value, or its default when there is none (or the file is unreadable: it is only history).
pub fn load<T: DeserializeOwned + Default>(project: &str, name: &str) -> T {
    std::fs::read_to_string(path_of(project, name)).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
}

pub fn save<T: Serialize>(project: &str, name: &str, value: &T) -> Result<(), String> {
    let path = path_of(project, name);
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    std::fs::write(&path, serde_json::to_string_pretty(value).map_err(|e| e.to_string())?).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_project_has_one_tag_however_its_path_is_spelled() {
        assert_eq!(project_tag("C:\\Work\\Api"), project_tag("c:/work/api/"));
        assert_ne!(project_tag("/a"), project_tag("/b"));
        assert_eq!(project_tag("/a").len(), 16);
    }
}
