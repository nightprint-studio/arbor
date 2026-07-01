//! `arbor-studio-api` unit tests (§6 api).
//!
//! Builds a hand-crafted fixture repo dir on disk (one file per format,
//! each carrying a definition + a valid reference + one broken reference)
//! and asserts the format-agnostic scanner finds the right kinds, the
//! cross-ref / usage / broken-ref scans return the expected counts for
//! ALL FIVE formats, and the registry + dispatch route correctly.

use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};

use crate::registry::{studio_registry, StudioRegistry};
use crate::scanner::{
    find_usages_for, scan_broken_refs_for, scan_cross_refs_for, scan_repo, StudioFileKind,
};

static COUNTER: AtomicU32 = AtomicU32::new(0);

/// Make a unique temp dir for one test, populate it via `build`, return
/// its path. Caller is responsible for nothing — the OS temp cleans up,
/// and we drop on a best-effort basis at the end of each test.
fn fixture_repo(build: impl FnOnce(&PathBuf)) -> PathBuf {
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let pid = std::process::id();
    let mut dir = std::env::temp_dir();
    dir.push(format!("arbor_studio_api_test_{pid}_{n}"));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("mkdir fixture repo");
    build(&dir);
    dir
}

fn write(dir: &PathBuf, name: &str, contents: &str) {
    fs::write(dir.join(name), contents).expect("write fixture file");
}

/// Build a repo with one file per format. Each file defines `id: "hero"`
/// (or the .properties equivalent), references the valid id `"hero"`, and
/// references a dangling id (`"missing"`) via a `*_id` reference field.
fn build_all_formats(dir: &PathBuf) {
    // RON — struct with id + a reference field + a broken reference field.
    write(dir, "unit.ron", r#"(
    id: "hero",
    target: "hero",
    enemy_id: "missing",
)"#);

    // JSON — same shape.
    write(dir, "unit.json", r#"{
    "id": "hero",
    "target": "hero",
    "enemy_id": "missing"
}"#);

    // TOML — id + reference fields.
    write(dir, "unit.toml", "id = \"hero\"\ntarget = \"hero\"\nenemy_id = \"missing\"\n");

    // YAML — id + reference fields.
    write(dir, "unit.yaml", "id: hero\ntarget: hero\nenemy_id: missing\n");

    // .properties — every key is a def, every value a ref. We give it a
    // key whose VALUE is "hero" (valid: matches the dotted key "hero"
    // below) and a value "missing" (broken: no key named "missing").
    // The def-set for .properties is the set of dotted KEYS.
    write(dir, "unit.properties", "hero=ok\ntarget=hero\nenemy_id=missing\n");
}

#[test]
fn scan_repo_finds_all_kinds() {
    let dir = fixture_repo(build_all_formats);
    let folder = dir.to_string_lossy().to_string();

    let entries = scan_repo(&folder, &[]).expect("scan_repo");
    // One file per format → five entries.
    assert_eq!(entries.len(), 5, "expected 5 files, got {}", entries.len());

    let mut kinds: Vec<StudioFileKind> = entries.iter().map(|e| e.kind).collect();
    kinds.sort_by_key(|k| format!("{k:?}"));
    assert!(kinds.contains(&StudioFileKind::Ron));
    assert!(kinds.contains(&StudioFileKind::Json));
    assert!(kinds.contains(&StudioFileKind::Toml));
    assert!(kinds.contains(&StudioFileKind::Yaml));
    assert!(kinds.contains(&StudioFileKind::Properties));

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn scan_repo_kind_filter() {
    let dir = fixture_repo(build_all_formats);
    let folder = dir.to_string_lossy().to_string();

    let ron_only = scan_repo(&folder, &[StudioFileKind::Ron]).expect("scan_repo ron");
    assert_eq!(ron_only.len(), 1);
    assert_eq!(ron_only[0].kind, StudioFileKind::Ron);

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn cross_refs_one_def_per_format() {
    let dir = fixture_repo(build_all_formats);
    let folder = dir.to_string_lossy().to_string();

    // RON / JSON / TOML / YAML each have exactly one `id`/`name` def.
    for kind in [
        StudioFileKind::Ron,
        StudioFileKind::Json,
        StudioFileKind::Toml,
        StudioFileKind::Yaml,
    ] {
        let defs = scan_cross_refs_for(&folder, &[kind]).expect("scan_cross_refs");
        assert_eq!(defs.len(), 1, "{kind:?}: expected 1 def, got {}", defs.len());
        assert_eq!(defs[0].id_value, "hero", "{kind:?}: def value");
    }

    // .properties: every dotted key is a def → 3 keys (hero, target, enemy_id).
    let props_defs =
        scan_cross_refs_for(&folder, &[StudioFileKind::Properties]).expect("props cross_refs");
    assert_eq!(props_defs.len(), 3, "properties: expected 3 key-defs");

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn find_usages_valid_ref_per_format() {
    let dir = fixture_repo(build_all_formats);
    let folder = dir.to_string_lossy().to_string();

    // The `target: "hero"` reference resolves to the `hero` def in
    // RON/JSON/TOML/YAML — one usage hit each.
    for kind in [
        StudioFileKind::Ron,
        StudioFileKind::Json,
        StudioFileKind::Toml,
        StudioFileKind::Yaml,
    ] {
        let hits = find_usages_for(&folder, "hero", &[kind]).expect("find_usages");
        assert_eq!(hits.len(), 1, "{kind:?}: expected 1 usage of `hero`, got {}", hits.len());
    }

    // .properties: every leaf value is a ref. The value "hero" appears
    // once (target=hero).
    let props_hits =
        find_usages_for(&folder, "hero", &[StudioFileKind::Properties]).expect("props usages");
    assert_eq!(props_hits.len(), 1, "properties: expected 1 usage of `hero`");

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn broken_refs_one_per_format() {
    let dir = fixture_repo(build_all_formats);
    let folder = dir.to_string_lossy().to_string();

    // Each of RON/JSON/TOML/YAML has exactly one broken ref:
    // `enemy_id: "missing"` (no `missing` def exists). `target: "hero"`
    // is valid so it is not flagged.
    for kind in [
        StudioFileKind::Ron,
        StudioFileKind::Json,
        StudioFileKind::Toml,
        StudioFileKind::Yaml,
    ] {
        let broken = scan_broken_refs_for(&folder, &[kind]).expect("scan_broken_refs");
        assert_eq!(broken.len(), 1, "{kind:?}: expected 1 broken ref, got {}", broken.len());
        assert_eq!(broken[0].value, "missing", "{kind:?}: broken value");
    }

    // .properties: defs = keys {hero, target, enemy_id}; refs = values
    // {ok, hero, missing}. "ok" and "missing" are not keys → 2 broken.
    let props_broken =
        scan_broken_refs_for(&folder, &[StudioFileKind::Properties]).expect("props broken");
    assert_eq!(props_broken.len(), 2, "properties: expected 2 broken refs");

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn broken_refs_namespaces_isolated() {
    // A def in one format must not satisfy a ref in another. With the
    // all-format fixture, each format's `enemy_id -> missing` stays broken
    // regardless of the other formats present (none define `missing`).
    let dir = fixture_repo(build_all_formats);
    let folder = dir.to_string_lossy().to_string();
    let all = scan_broken_refs_for(&folder, &[]).expect("scan_broken_refs all");
    // 4 single-broken (ron/json/toml/yaml) + 2 from .properties = 6.
    assert_eq!(all.len(), 6, "expected 6 broken refs across all formats, got {}", all.len());
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn registry_get_and_list() {
    let reg = studio_registry();
    assert!(reg.get("ron").is_ok());
    assert!(reg.get("json").is_ok());
    assert!(reg.get("toml").is_ok());
    assert!(reg.get("yaml").is_ok());
    assert!(reg.get("properties").is_ok());

    // Unknown format → UnknownFormat.
    match reg.get("xml") {
        Err(arbor_studio_types::prelude::StudioError::UnknownFormat(f)) => {
            assert_eq!(f, "xml");
        }
        Err(other) => panic!("expected UnknownFormat, got {other:?}"),
        Ok(_) => panic!("expected UnknownFormat for `xml`, got Ok"),
    }

    // list_descriptors → sorted by id, length 5.
    let descriptors = reg.list_descriptors();
    assert_eq!(descriptors.len(), 5, "expected 5 descriptors");
    let ids: Vec<&str> = descriptors.iter().map(|d| d.id.as_str()).collect();
    let mut sorted = ids.clone();
    sorted.sort();
    assert_eq!(ids, sorted, "descriptors must be sorted by id");
}

#[test]
fn dispatch_routes_list_descriptors() {
    let reg = studio_registry();
    let out = crate::dispatch::dispatch(&reg, "list_descriptors", &serde_json::Value::Null)
        .expect("dispatch list_descriptors");
    let arr = out.as_array().expect("list_descriptors → array");
    assert_eq!(arr.len(), 5, "dispatch list_descriptors → 5 entries");
}

#[test]
fn dispatch_routes_describe() {
    let reg = studio_registry();
    let params = serde_json::json!({ "format_id": "ron" });
    let out = crate::dispatch::dispatch(&reg, "describe", &params).expect("dispatch describe");
    assert_eq!(out.get("id").and_then(|v| v.as_str()), Some("ron"));
}

#[test]
fn dispatch_unknown_method_errors() {
    let reg = studio_registry();
    let err = crate::dispatch::dispatch(&reg, "studio_parse", &serde_json::Value::Null);
    assert!(err.is_err(), "context-requiring method must not route through dispatch");
}

#[allow(dead_code)]
fn _registry_type_check(_r: &StudioRegistry) {}
