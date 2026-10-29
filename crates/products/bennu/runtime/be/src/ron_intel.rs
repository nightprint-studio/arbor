//! `ron_intel` — diagnostics for a `.ron` file.
//!
//! Two tiers, split the way the Java path splits them. **Syntax** (brackets, then the real parser)
//! is a scan of the buffer and runs on every request. **Validation against a Rust type** needs the
//! type's crate read and resolved, which is a question about the project and not the buffer — so it
//! runs on the full tier only, and the schema is cached: parsing a crate on every pause in typing
//! would be the single most expensive thing the editor did.
//!
//! Which type is said by the file itself, with the directive Studio already reads
//! (`// ron-studio: schema="src/lib.rs", root="crate::Config"`) or a `.arbor/studio.toml` sidecar.
//! A file with neither is syntax-checked and nothing more: guessing a root type is how a correct
//! file gets a wrong squiggle.
//!
//! Each entry point returns `Option`, `None` meaning "not mine", like `wgsl_intel` and `cargo_intel`.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant, SystemTime};

use bennu_proto::prelude::{severity, Diagnostic};
use bennu_ron::prelude::{
    directive_span, load_schema, schema_hint, syntax_diagnostics, validate, RonDiagnostic,
    RonSeverity, Schema, SchemaHint,
};

/// How long a loaded schema is trusted before the crate is read again. The `.rs` the directive
/// names is also stat'ed, so the file you are most likely editing refreshes at once; the TTL is for
/// the rest of the crate, which this does not watch.
const SCHEMA_TTL: Duration = Duration::from_secs(60);
/// A failed load is retried sooner: the usual cause is a half-typed directive.
const FAILURE_TTL: Duration = Duration::from_secs(5);

/// Whether this file is ours.
pub(crate) fn is_ron(file: &str) -> bool {
    file.rsplit('.').next().is_some_and(|e| e.eq_ignore_ascii_case("ron"))
}

/// Diagnostics for a `.ron`. `None` for a file that is not one. `resolved` is the editor's tier:
/// `false` is the fast pass after a keystroke and gets syntax only.
pub(crate) fn diagnostics(file: &str, source: Option<&str>, resolved: bool) -> Option<Vec<Diagnostic>> {
    if !is_ron(file) {
        return None;
    }
    let text = match source {
        Some(s) => s.to_string(),
        None => std::fs::read_to_string(file).ok()?,
    };
    let syntax = syntax_diagnostics(&text);
    if !syntax.is_empty() || !resolved {
        return Some(syntax.into_iter().map(into_wire).collect());
    }
    let Some(hint) = schema_hint(&text, file) else { return Some(Vec::new()) };
    let found = match cached_schema(&hint) {
        Ok(schema) => validate(&text, &schema),
        Err(reason) => {
            // The binding itself is what is wrong, so the squiggle goes on it. A sidecar has no
            // line in this file; the top of the file is the nearest honest place.
            let (start, end) = directive_span(&text).unwrap_or((0, 0));
            vec![RonDiagnostic {
                start,
                end: end.max(start + 1).min(text.len().max(1)),
                severity: RonSeverity::Warning,
                code: "ron-schema",
                message: format!("cannot check this file against `{}`: {reason}", hint.root_type),
            }]
        }
    };
    Some(found.into_iter().map(into_wire).collect())
}

fn into_wire(d: RonDiagnostic) -> Diagnostic {
    Diagnostic {
        message: d.message,
        severity: match d.severity {
            RonSeverity::Error => severity::ERROR,
            RonSeverity::Warning => severity::WARNING,
            RonSeverity::Info => severity::INFO,
        }
        .to_string(),
        code: d.code.to_string(),
        start: d.start,
        end: d.end,
    }
}

struct Cached {
    at: Instant,
    /// Modification time of the `.rs` the directive names, when it could be read.
    stamp: Option<SystemTime>,
    result: Result<Arc<Schema>, String>,
}

type Key = (String, String);

fn cache() -> &'static Mutex<HashMap<Key, Cached>> {
    static CACHE: OnceLock<Mutex<HashMap<Key, Cached>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn stamp_of(path: &str) -> Option<SystemTime> {
    std::fs::metadata(path).and_then(|m| m.modified()).ok()
}

/// The schema for a binding: from the cache while fresh, else read from the crate.
///
/// The crate is parsed with the lock released, so a slow load for one project does not hold up the
/// diagnostics of every other `.ron` open; two simultaneous first loads of the same schema do the
/// work twice, which costs a few hundred milliseconds once and keeps the lock out of the slow path.
fn cached_schema(hint: &SchemaHint) -> Result<Arc<Schema>, String> {
    let key: Key = (hint.rs_file.clone(), hint.root_type.clone());
    let stamp = stamp_of(&hint.rs_file);
    if let Ok(map) = cache().lock() {
        if let Some(c) = map.get(&key) {
            let ttl = if c.result.is_ok() { SCHEMA_TTL } else { FAILURE_TTL };
            if c.at.elapsed() < ttl && c.stamp == stamp {
                return c.result.clone();
            }
        }
    };
    let result = load_schema(&hint.rs_file, &hint.root_type).map(Arc::new).map_err(|e| e.to_string());
    if let Ok(mut map) = cache().lock() {
        map.insert(key, Cached { at: Instant::now(), stamp, result: result.clone() });
    };
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_ron_files_are_claimed() {
        assert!(is_ron("/p/assets/level.ron"));
        assert!(is_ron("C:/p/Scene.SCN.RON"));
        assert!(!is_ron("/p/src/main.rs"));
        assert!(diagnostics("/p/a.toml", Some("(a: 1)"), true).is_none());
    }

    #[test]
    fn the_fast_tier_reports_syntax_only() {
        let broken = diagnostics("/p/a.ron", Some("(a: 1"), false).unwrap();
        assert_eq!(broken.len(), 1);
        assert_eq!(broken[0].severity, severity::ERROR);
        assert_eq!(broken[0].code, "ron-brackets");
        assert!(diagnostics("/p/a.ron", Some("(a: 1)"), false).unwrap().is_empty());
    }

    #[test]
    fn a_file_with_no_binding_is_not_validated() {
        assert!(diagnostics("/p/a.ron", Some("(anything: 1)"), true).unwrap().is_empty());
    }

    #[test]
    fn a_binding_that_cannot_load_says_so_on_the_directive() {
        let text = "// ron-studio: schema=\"missing/lib.rs\", root=\"crate::Nope\"\n(a: 1)";
        let d = diagnostics("/nonexistent/dir/a.ron", Some(text), true).unwrap();
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].code, "ron-schema");
        assert_eq!(d[0].severity, severity::WARNING);
        assert_eq!(d[0].start, 0);
    }
}
