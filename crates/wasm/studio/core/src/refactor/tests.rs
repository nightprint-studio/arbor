//! Unit tests for `core::refactor` — site enumeration, collision +
//! dirty-blocker detection, bulk site/op building with skip reasons,
//! 2-phase op ordering (delete reverse-index), applied/skipped counts,
//! and coercion via a stub `RefactorOps`. All FS-free: the multi-file
//! flush is exercised only up to the `RefactorOps` seam; the real disk
//! touch lives in `crate::persist` (covered by the human's manual pass).

use super::*;
use serde_json::json;

// ── Stub backend ─────────────────────────────────────────────────────
//
// Mimics a "simple" format with TOML-style null→delete policy. Tracks
// the ops handed to `apply_bulk_ops` so a test can assert ordering.

struct StubOps {
    /// When true, `null` set → `DeleteInstead` (TOML semantics);
    /// otherwise `null` stays a native `Set(Null)` (YAML semantics).
    null_as_delete: bool,
}

impl StubOps {
    fn toml() -> Self { StubOps { null_as_delete: true } }
    fn yaml() -> Self { StubOps { null_as_delete: false } }
}

impl RefactorOps for StubOps {
    fn parse_to_value(&self, text: &str) -> Option<serde_json::Value> {
        serde_json::from_str(text).ok()
    }

    fn apply_string_rename(
        &self,
        text:  &str,
        _paths: &[Vec<String>],
        _new:   &str,
    ) -> StudioResult<String> {
        Ok(text.to_string())
    }

    fn apply_bulk_ops(&self, text: &str, _ops: &[BulkOp]) -> StudioResult<String> {
        Ok(text.to_string())
    }

    fn node_kind(&self, v: &serde_json::Value) -> String {
        match v {
            serde_json::Value::Null      => "null",
            serde_json::Value::Bool(_)   => "bool",
            serde_json::Value::Number(n) => if n.is_i64() || n.is_u64() { "integer" } else { "float" },
            serde_json::Value::String(_) => "string",
            serde_json::Value::Array(_)  => "array",
            serde_json::Value::Object(_) => "object",
        }
        .to_string()
    }

    fn preview_for(&self, v: &serde_json::Value) -> String {
        v.to_string()
    }

    fn coerce_set_value(
        &self,
        target_kind: &str,
        raw:         &edit_expr::Value,
    ) -> Result<CoerceOutcome, CoerceSkip> {
        match raw {
            edit_expr::Value::Null => {
                if self.null_as_delete {
                    Ok(CoerceOutcome::DeleteInstead)
                } else {
                    Ok(CoerceOutcome::Set(SetValue::Null))
                }
            }
            edit_expr::Value::Bool(b) => Ok(CoerceOutcome::Set(SetValue::Bool(*b))),
            edit_expr::Value::Number(n) => Ok(CoerceOutcome::Set(coerce_number_default(*n))),
            edit_expr::Value::String(s) => {
                // Type-mismatch demo: refuse a string onto an integer node.
                if target_kind == "integer" {
                    Err(CoerceSkip("type mismatch: string onto integer".into()))
                } else {
                    Ok(CoerceOutcome::Set(SetValue::String(s.clone())))
                }
            }
        }
    }
}

// ── Synthetic index fixtures ─────────────────────────────────────────

fn def(id: &str, abs: &str, rel: &str, path: &[&str], field: &str) -> RenameDefInput {
    RenameDefInput {
        id_value:      id.to_string(),
        absolute_path: abs.to_string(),
        relative_path: rel.to_string(),
        file_name:     rel.rsplit('/').next().unwrap_or(rel).to_string(),
        def_path:      path.iter().map(|s| s.to_string()).collect(),
        def_field:     field.to_string(),
    }
}

fn usage(abs: &str, rel: &str, path: &[&str], key: &str) -> RenameUsageInput {
    RenameUsageInput {
        absolute_path: abs.to_string(),
        relative_path: rel.to_string(),
        file_name:     rel.rsplit('/').next().unwrap_or(rel).to_string(),
        field_path:    path.iter().map(|s| s.to_string()).collect(),
        key_name:      key.to_string(),
    }
}

// ── F12: rename preview ──────────────────────────────────────────────

#[test]
fn rename_sites_collected_from_defs_and_usages() {
    let defs = vec![
        def("goblin", "/r/a.toml", "a.toml", &["enemies", "0", "id"], "id"),
    ];
    let usages = vec![
        usage("/r/b.toml", "b.toml", &["spawn", "target"], "target"),
        usage("/r/a.toml", "a.toml", &["link"], "link"),
    ];
    let sites = build_rename_sites(&defs, &usages, "goblin", DefScopeStyle::Definition);
    assert_eq!(sites.len(), 3);
    // Sorted by (relative_path, field_path): a.toml/[enemies,0,id],
    // a.toml/[link], b.toml/[spawn,target].
    assert_eq!(sites[0].relative_path, "a.toml");
    assert_eq!(sites[0].field_path, vec!["enemies", "0", "id"]);
    assert_eq!(sites[0].scope, RenameSiteScope::Definition);
    assert_eq!(sites[0].key_name, "id");
    assert_eq!(sites[1].relative_path, "a.toml");
    assert_eq!(sites[1].field_path, vec!["link"]);
    assert_eq!(sites[1].scope, RenameSiteScope::Reference);
    assert_eq!(sites[2].relative_path, "b.toml");
}

#[test]
fn rename_sites_filter_by_old_value_and_dedup() {
    let defs = vec![
        def("goblin", "/r/a", "a", &["id"], "id"),
        def("orc", "/r/a", "a", &["other"], "id"), // wrong value → excluded
    ];
    // A usage at the same (abs, path) as the def — dedup keeps the def.
    let usages = vec![usage("/r/a", "a", &["id"], "id")];
    let sites = build_rename_sites(&defs, &usages, "goblin", DefScopeStyle::Definition);
    assert_eq!(sites.len(), 1);
    assert_eq!(sites[0].scope, RenameSiteScope::Definition);
}

#[test]
fn rename_def_scope_key_style_for_properties() {
    let defs = vec![def("my.key", "/r/x.properties", "x.properties", &["my", "key"], "ignored")];
    let sites = build_rename_sites(&defs, &[], "my.key", DefScopeStyle::Key);
    assert_eq!(sites.len(), 1);
    assert_eq!(sites[0].scope, RenameSiteScope::Key);
    assert_eq!(sites[0].key_name, "my.key"); // id_value, not def_field
}

#[test]
fn collisions_when_new_value_already_defined() {
    let defs = vec![
        def("orc", "/r/a", "a", &["id"], "id"),
        def("goblin", "/r/b", "b", &["id"], "id"),
    ];
    let cols = collisions_for(&defs, Some("orc"), "goblin");
    assert_eq!(cols.len(), 1);
    assert_eq!(cols[0].relative_path, "a");

    // No hint / hint == old / empty hint → no collisions.
    assert!(collisions_for(&defs, None, "goblin").is_empty());
    assert!(collisions_for(&defs, Some("goblin"), "goblin").is_empty());
    assert!(collisions_for(&defs, Some(""), "goblin").is_empty());
}

#[test]
fn dirty_blocker_when_open_doc_is_dirty_and_affected() {
    let affected = affected_path_set(["/Repo/A.toml", "/repo/b.toml"]);
    let docs = vec![
        OpenDocState { doc_id: "d2".into(), source_path: Some("/repo/A.toml".into()), dirty: true },
        OpenDocState { doc_id: "d1".into(), source_path: Some("/repo/c.toml".into()), dirty: true }, // not affected
        OpenDocState { doc_id: "d3".into(), source_path: Some("/repo/b.toml".into()), dirty: false }, // clean
    ];
    let blockers = dirty_blockers_for(&docs, &affected);
    assert_eq!(blockers.len(), 1);
    assert_eq!(blockers[0].doc_id, "d2"); // case-insensitive path match

    assert!(any_affected_dirty(&docs, &affected));
    let clean = vec![OpenDocState { doc_id: "d".into(), source_path: Some("/repo/c.toml".into()), dirty: true }];
    assert!(!any_affected_dirty(&clean, &affected));
}

// ── F12: rename apply validation ─────────────────────────────────────

#[test]
fn rename_apply_rejects_bad_inputs() {
    let ops = StubOps::toml();
    let site = RenameSite {
        absolute_path: "/r/a".into(),
        relative_path: "a".into(),
        file_name:     "a".into(),
        field_path:    vec!["id".into()],
        key_name:      "id".into(),
        scope:         RenameSiteScope::Definition,
        preview:       String::new(),
    };
    assert!(rename_apply_files(&ops, "old", "", &[site.clone()]).is_err()); // empty new
    assert!(rename_apply_files(&ops, "x", "x", &[site.clone()]).is_err());  // same
    assert!(rename_apply_files(&ops, "old", "new", &[]).is_err());          // no sites
}

// ── F13: bulk site building skip reasons ─────────────────────────────

fn site(path: &[&str]) -> Vec<String> { path.iter().map(|s| s.to_string()).collect() }

fn lit_str(s: &str) -> Option<BulkEditValueSource> {
    Some(BulkEditValueSource::Literal { literal: BulkEditLiteral::String(s.to_string()) })
}
fn lit_null() -> Option<BulkEditValueSource> {
    Some(BulkEditValueSource::Literal { literal: BulkEditLiteral::Null })
}

#[test]
fn bulk_site_set_on_container_skips() {
    let ops = StubOps::yaml();
    let target = json!({ "a": 1 });
    let s = build_bulk_site(
        &ops, "/r/a", "a", "a", &site(&["root"]), &target,
        BulkEditAction::Set, &lit_str("x"), None,
    );
    assert!(s.will_skip);
    assert!(s.skip_reason.contains("descend deeper"));
}

#[test]
fn bulk_site_delete_root_skips() {
    let ops = StubOps::yaml();
    let target = json!("x");
    let s = build_bulk_site(
        &ops, "/r/a", "a", "a", &[], &target,
        BulkEditAction::Delete, &None, None,
    );
    assert!(s.will_skip);
    assert!(s.skip_reason.contains("document root"));
}

#[test]
fn bulk_site_type_mismatch_skips_with_eval_reason() {
    let ops = StubOps::yaml();
    let target = json!(42); // integer node
    let s = build_bulk_site(
        &ops, "/r/a", "a", "a", &site(&["n"]), &target,
        BulkEditAction::Set, &lit_str("hello"), None,
    );
    assert!(s.will_skip);
    assert!(s.skip_reason.contains("type mismatch"));
}

#[test]
fn bulk_site_set_null_under_as_delete_shows_removed_via_null() {
    let ops = StubOps::toml(); // null_as_delete
    let target = json!("x");
    let s = build_bulk_site(
        &ops, "/r/a", "a", "a", &site(&["k"]), &target,
        BulkEditAction::Set, &lit_null(), None,
    );
    assert!(!s.will_skip);
    assert_eq!(s.new_preview, "(removed via null)");
}

#[test]
fn bulk_site_set_null_native_yaml_keeps_value() {
    let ops = StubOps::yaml();
    let target = json!("x");
    let s = build_bulk_site(
        &ops, "/r/a", "a", "a", &site(&["k"]), &target,
        BulkEditAction::Set, &lit_null(), None,
    );
    assert!(!s.will_skip);
    assert_eq!(s.new_preview, "null");
}

// ── F13: op building, counts, and ordering ───────────────────────────

fn accepted_site(path: &[&str]) -> BulkEditSite {
    BulkEditSite {
        absolute_path: "/r/a".into(),
        relative_path: "a".into(),
        file_name:     "a".into(),
        field_path:    site(path),
        kind:          "string".into(),
        old_preview:   String::new(),
        new_preview:   String::new(),
        will_skip:     false,
        skip_reason:   String::new(),
    }
}

#[test]
fn build_bulk_ops_counts_applied_and_skipped() {
    let ops = StubOps::yaml();
    let root = json!({ "a": "x", "b": { "nested": 1 }, "items": [10, 20] });
    let sites = vec![
        accepted_site(&["a"]),                 // set ok
        // Container sites are skipped at PREVIEW time (will_skip=true);
        // build_bulk_ops honors that flag rather than re-checking.
        BulkEditSite { will_skip: true, ..accepted_site(&["b"]) }, // skipped via flag
        accepted_site(&["missing"]),           // unresolved → skipped
    ];
    let (built, applied, skipped) = build_bulk_ops(
        &ops, &root, &sites, BulkEditAction::Set, &lit_str("z"), None,
    );
    assert_eq!(applied, 1);
    assert_eq!(skipped, 2);
    assert_eq!(built.len(), 1);
    assert!(matches!(&built[0], BulkOp::Set { path, .. } if path == &site(&["a"])));
}

#[test]
fn build_bulk_ops_delete_collects_paths() {
    let ops = StubOps::yaml();
    let root = json!({ "items": [10, 20, 30] });
    let sites = vec![
        accepted_site(&["items", "0"]),
        accepted_site(&["items", "2"]),
        // root delete → skipped
        BulkEditSite { field_path: vec![], ..accepted_site(&["items", "1"]) },
    ];
    let (built, applied, skipped) = build_bulk_ops(
        &ops, &root, &sites, BulkEditAction::Delete, &None, None,
    );
    assert_eq!(applied, 2);
    assert_eq!(skipped, 1);
    // The orchestrator collects deletes in site order; the leaf
    // `apply_bulk_ops` is responsible for the reverse-index ordering.
    let paths: Vec<&[String]> = built.iter().map(|o| o.path()).collect();
    assert_eq!(paths[0], site(&["items", "0"]).as_slice());
    assert_eq!(paths[1], site(&["items", "2"]).as_slice());
}

#[test]
fn coerce_number_default_picks_int_then_float() {
    assert_eq!(coerce_number_default(42.0), SetValue::Int(42));
    assert!(matches!(coerce_number_default(3.5), SetValue::Float(_)));
    assert!(matches!(coerce_number_default(f64::INFINITY), SetValue::Float(_)));
}

#[test]
fn compile_expression_surfaces_errors_and_skips_non_set() {
    // Delete action → nothing to compile.
    assert!(matches!(compile_expression(BulkEditAction::Delete, &None), Ok(None)));
    // Literal set → nothing to compile.
    assert!(matches!(compile_expression(BulkEditAction::Set, &lit_str("x")), Ok(None)));
    // Bad expression → Err(message).
    let bad = Some(BulkEditValueSource::Expression { source: "((".into() });
    assert!(compile_expression(BulkEditAction::Set, &bad).is_err());
    // Good expression → Ok(Some).
    let good = Some(BulkEditValueSource::Expression { source: "old".into() });
    assert!(matches!(compile_expression(BulkEditAction::Set, &good), Ok(Some(_))));
}

#[test]
fn expression_eval_binds_old() {
    let ops = StubOps::yaml();
    let compiled = edit_expr::compile("old + 1").unwrap();
    let root = json!({ "n": 41 });
    let sites = vec![accepted_site(&["n"])];
    let src = Some(BulkEditValueSource::Expression { source: "old + 1".into() });
    let (built, applied, _) = build_bulk_ops(
        &ops, &root, &sites, BulkEditAction::Set, &src, Some(&compiled),
    );
    assert_eq!(applied, 1);
    assert_eq!(built[0], BulkOp::Set { path: site(&["n"]), value: SetValue::Int(42) });
}

#[test]
fn synth_active_doc_paths_handles_none_and_some() {
    let (a, r, n) = synth_active_doc_paths(&None);
    assert_eq!((a.as_str(), r.as_str(), n.as_str()), ("(active doc)", "(active doc)", "(active doc)"));
    let (_, r, n) = synth_active_doc_paths(&Some("C:\\x\\y.toml".into()));
    assert_eq!(r, "C:/x/y.toml");
    assert_eq!(n, "y.toml");
}
