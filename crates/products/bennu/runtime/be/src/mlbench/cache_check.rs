//! The cache must not change a single number: M1 and M3 run twice on the same sample — once with `BENNU_MLB_REBUILD=1`
//! (projects re-assembled, indexes rebuilt from nothing) and once reopening what that left — and their outputs must be
//! identical. Each run is a child process, because the index and the ghost-text counters are process-global.
//!
//! `cargo test -p bennu-be mlbench::cache_check -- --ignored --nocapture`

use std::process::Command;

use super::common::*;

fn child(test: &str, tag: &str, rebuild: bool, extra: &[(&str, &str)]) {
    let exe = std::env::current_exe().expect("test binary");
    let mut cmd = Command::new(exe);
    cmd.args([test, "--ignored", "--exact", "--nocapture"]).env("BENNU_MLB_TAG", tag).env("BENNU_MLB_ONLY", "commons-lang");
    if rebuild {
        cmd.env("BENNU_MLB_REBUILD", "1");
    } else {
        cmd.env_remove("BENNU_MLB_REBUILD");
    }
    for (k, v) in extra {
        cmd.env(k, v);
    }
    let out = cmd.output().expect("child run");
    assert!(out.status.success(), "{test} ({tag}) failed:\n{}", String::from_utf8_lossy(&out.stderr).lines().rev().take(20).collect::<Vec<_>>().join("\n"));
}

fn read(tag: &str, file: &str) -> String {
    std::fs::read_to_string(work_dir().join("results").join(format!("{tag}_{file}"))).unwrap_or_default()
}

#[test]
#[ignore = "runs M1 and M3 twice in child processes: see the module docs"]
fn the_cache_changes_no_number() {
    let n = [("BENNU_MLB_N", "80")];
    child("mlbench::m1_cascade::m1_cascade_ablation", "cold", true, &n);
    child("mlbench::m1_cascade::m1_cascade_ablation", "warm", false, &n);
    let (cold, warm) = (read("cold", "m1_rows.tsv"), read("warm", "m1_rows.tsv"));
    assert!(!cold.is_empty(), "the cold run wrote nothing");
    assert_eq!(cold, warm, "M1 rows differ between the rebuilt and the reopened index");

    let m3 = [("BENNU_MLB_N", "60"), ("BENNU_MLB_SKIP_TRAIN", "1")];
    child("mlbench::m3_rank::m3_learned_ranking", "cold", true, &m3);
    child("mlbench::m3_rank::m3_learned_ranking", "warm", false, &m3);
    let (cold, warm) = (read("cold", "m3_report.md"), read("warm", "m3_report.md"));
    assert!(cold.contains("production ranker"), "the cold M3 run wrote no report");
    assert_eq!(cold, warm, "M3 differs between the rebuilt and the reopened index");
    eprintln!("identical: M1 rows ({} bytes), M3 report ({} bytes)", read("cold", "m1_rows.tsv").len(), cold.len());
}

#[cfg(test)]
mod tests {
    use super::super::common::*;

    #[test]
    fn the_key_follows_its_inputs_only() {
        let a = cache_key(|h| *h ^= 1);
        assert_eq!(a, cache_key(|h| *h ^= 1));
        assert_ne!(a, cache_key(|h| *h ^= 2));
    }

    #[test]
    fn a_folder_is_reused_while_its_key_holds_and_rebuilt_when_it_changes() {
        let root = std::env::temp_dir().join(format!("mlb-cache-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        assert!(needs_assembly(&root, "k1"));
        mark_assembled(&root, "k1");
        std::fs::write(root.join("x.java"), "class X {}").unwrap();
        assert!(!needs_assembly(&root, "k1"));
        assert!(root.join("x.java").exists());
        assert!(needs_assembly(&root, "k2"));
        assert!(!root.join("x.java").exists());
        let _ = std::fs::remove_dir_all(&root);
    }
}
