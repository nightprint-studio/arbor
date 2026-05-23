//! Single-shot object operations: list / stat / delete / copy / test.
//!
//! Transfers (download / upload / sync) live in `transfer.rs` because they
//! carry a JobRegistry entry + progress events; everything here returns
//! synchronously (well, awaits once) and is safe to call from inside a
//! Tauri command without spawning a job.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use futures_util::StreamExt;
use opendal::Operator;

use crate::error::{CloudError, Result};
use crate::host::CloudHost;
use crate::operator::{build, map_op_err};
use crate::types::{CloudConnection, CloudListPage, CloudObject, CloudTestReport};

const PLUGIN_NAME:     &str = "cloud-storage";
const HOOK_LIST_CHUNK: &str = "cloud-storage:list-chunk";

/// Push one chunk payload into the cloud-storage plugin via the plugin host.
/// `host.emit_event(...)` would deliver to the JS frontend only — Lua
/// subscribers live in `__arbor_hooks__` and are reachable exclusively
/// through `fire_plugin_hook`.
fn deliver_chunk(host: &dyn CloudHost, payload: serde_json::Value) {
    let json = match serde_json::to_string(&payload) {
        Ok(s)  => s,
        Err(e) => { tracing::warn!("cloud chunk encode: {e}"); return; }
    };
    host.fire_plugin_hook(PLUGIN_NAME, HOOK_LIST_CHUNK, &json);
}

/// Default cap on `list` to keep the UI responsive on huge prefixes. The
/// plugin passes its own limit so this is just the guardrail for callers
/// that don't.
pub const DEFAULT_LIST_LIMIT: usize = 200;

// ── list ────────────────────────────────────────────────────────────────────

pub async fn list(
    conn:   &CloudConnection,
    bucket: &str,
    prefix: &str,
    limit:  Option<usize>,
) -> Result<CloudListPage> {
    let op = build(conn, bucket).await?;
    let limit = limit.unwrap_or(DEFAULT_LIST_LIMIT).max(1);

    // opendal `list_with(prefix)` defaults to non-recursive (folder view).
    // Entries cover both prefixes ("dirs") and objects.
    let path = normalize_prefix(prefix);
    let entries = op.list(&path).await.map_err(map_op_err)?;

    let mut items: Vec<CloudObject> = Vec::with_capacity(entries.len().min(limit));
    let mut truncated = false;
    for entry in entries {
        if items.len() >= limit { truncated = true; break; }
        // Skip the placeholder entry that opendal returns for the prefix itself.
        if entry.path() == path { continue; }
        items.push(entry_to_object(&entry));
    }

    // Sort: folders first, then files, alphabetical within each group.
    items.sort_by(|a, b| {
        b.is_dir.cmp(&a.is_dir).then_with(|| a.path.cmp(&b.path))
    });

    Ok(CloudListPage { items, truncated })
}

// ── stat ────────────────────────────────────────────────────────────────────

pub async fn stat(conn: &CloudConnection, bucket: &str, path: &str) -> Result<CloudObject> {
    let op = build(conn, bucket).await?;
    let meta = op.stat(path).await.map_err(map_op_err)?;
    Ok(CloudObject {
        path:          path.to_string(),
        is_dir:        meta.is_dir(),
        size:          if meta.is_file() { Some(meta.content_length()) } else { None },
        etag:          meta.etag().map(|s| s.to_string()),
        content_type:  meta.content_type().map(|s| s.to_string()),
        last_modified: meta.last_modified().map(|t| t.to_string()),
    })
}

// ── delete ──────────────────────────────────────────────────────────────────

pub async fn delete(
    conn:      &CloudConnection,
    bucket:    &str,
    path:      &str,
    recursive: bool,
) -> Result<()> {
    let op = build(conn, bucket).await?;
    if recursive {
        op.delete_with(path).recursive(true).await.map_err(map_op_err)?;
    } else {
        op.delete(path).await.map_err(map_op_err)?;
    }
    Ok(())
}

// ── copy ────────────────────────────────────────────────────────────────────

pub async fn copy(
    conn:   &CloudConnection,
    bucket: &str,
    src:    &str,
    dst:    &str,
) -> Result<()> {
    let op = build(conn, bucket).await?;
    op.copy(src, dst).await.map_err(map_op_err)?;
    Ok(())
}

// ── test ────────────────────────────────────────────────────────────────────

/// Probe that the connection actually works: build the Operator (which
/// resolves auth) and issue a tiny listing. Returns a structured report
/// instead of an `Err` so the UI can render the failure inline.
pub async fn test_connection(conn: &CloudConnection, bucket: Option<&str>) -> Result<CloudTestReport> {
    // For GCS we can probe at the bucket level. If no bucket was provided,
    // we have to take one — fall back to a placeholder and report the
    // expected `NotFound`/`PermissionDenied` clearly.
    let probe_bucket = bucket
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        // No bucket given: refuse rather than guessing — the user needs a
        // bucket to do anything useful, so this is a real failure.
        .ok_or_else(|| CloudError::Other("test_connection: a bucket is required".into()))?;

    let method = match &conn.provider {
        crate::types::Provider::Gcs    => conn.gcs.as_ref().map(|a| gcs_method_name(a).to_string()),
        crate::types::Provider::S3     => Some("aws_access_key".to_string()),
        crate::types::Provider::Azblob => Some("azure_account_key".to_string()),
    };

    // Resolve identity up-front (lets us surface SA email even when the
    // bucket probe later fails with permission denied).
    let identity = match (&conn.provider, &conn.gcs, &conn.s3, &conn.azblob) {
        (crate::types::Provider::Gcs, Some(a), _, _) => {
            crate::auth_gcs::resolve(a).await
                .ok()
                .and_then(|r| r.identity().map(|s| s.to_string()))
        }
        (crate::types::Provider::S3,     _, Some(a), _) => Some(a.access_key_id.clone()),
        (crate::types::Provider::Azblob, _, _, Some(a)) => Some(a.account_name.clone()),
        _ => None,
    };

    let probe_result = async {
        let op = build(conn, &probe_bucket).await?;
        // `op.list("/")` with limit 1 — cheap probe that validates auth +
        // bucket existence + ACLs in one call.
        let _ = op.list_with("/").limit(1).await.map_err(map_op_err)?;
        Ok::<_, CloudError>(())
    }.await;

    match probe_result {
        Ok(()) => Ok(CloudTestReport {
            ok:           true,
            error:        None,
            auth_method:  method,
            identity,
        }),
        Err(e) => Ok(CloudTestReport {
            ok:           false,
            error:        Some(e.to_string()),
            auth_method:  method,
            identity,
        }),
    }
}

// ── helpers ────────────────────────────────────────────────────────────────

fn gcs_method_name(a: &crate::types::GcsAuth) -> &'static str {
    use crate::types::GcsAuth::*;
    match a {
        SaFile { .. }   => "service_account_file",
        SaInline { .. } => "service_account_inline",
        Adc             => "adc",
        GcloudCli       => "gcloud_cli",
        Oauth { .. }    => "oauth",
    }
}

fn normalize_prefix(p: &str) -> String {
    // opendal treats `""` and `"/"` as "root" for the bucket. Ensure
    // non-empty prefixes end with `/` so the listing stays in folder mode.
    if p.is_empty() || p == "/" {
        return String::new();
    }
    if p.ends_with('/') {
        p.to_string()
    } else {
        format!("{p}/")
    }
}

fn entry_to_object(e: &opendal::Entry) -> CloudObject {
    let meta = e.metadata();
    let is_dir = meta.is_dir();
    CloudObject {
        path:          e.path().to_string(),
        is_dir,
        size:          if is_dir { None } else { Some(meta.content_length()) },
        etag:          meta.etag().map(|s| s.to_string()),
        content_type:  meta.content_type().map(|s| s.to_string()),
        last_modified: meta.last_modified().map(|t| t.to_string()),
    }
}

#[allow(dead_code)]
pub async fn open_operator(conn: &CloudConnection, bucket: &str) -> Result<Operator> {
    build(conn, bucket).await
}

// ── concat_files ───────────────────────────────────────────────────────────
//
// Byte-stream concatenation of N local files into one output. Used as the
// default "merge" primitive by chunk-handler plugins after `download_many`
// has finished. Streams via tokio::io::copy so big chunked archives don't
// load into RAM.

pub async fn concat_files(
    inputs:        Vec<String>,
    output:        String,
    delete_inputs: bool,
) -> Result<()> {
    if inputs.is_empty() {
        return Err(CloudError::Other("concat_files: inputs is empty".into()));
    }
    let out_path = std::path::Path::new(&output);
    if let Some(parent) = out_path.parent() {
        if !parent.as_os_str().is_empty() {
            tokio::fs::create_dir_all(parent).await
                .map_err(|e| CloudError::Other(format!("mkdir {}: {e}", parent.display())))?;
        }
    }

    let mut out = tokio::fs::File::create(out_path).await
        .map_err(|e| CloudError::Other(format!("create {}: {e}", out_path.display())))?;

    for input in &inputs {
        let mut f = tokio::fs::File::open(input).await
            .map_err(|e| CloudError::Other(format!("open {input}: {e}")))?;
        tokio::io::copy(&mut f, &mut out).await
            .map_err(|e| CloudError::Other(format!("copy {input}: {e}")))?;
    }
    // Flush before deleting inputs — otherwise a crash between the flush and
    // the unlinks would leave us with no source and a half-written output.
    use tokio::io::AsyncWriteExt;
    out.flush().await
        .map_err(|e| CloudError::Other(format!("flush {}: {e}", out_path.display())))?;
    drop(out);

    if delete_inputs {
        for input in &inputs {
            let _ = tokio::fs::remove_file(input).await;
        }
    }
    Ok(())
}

// ── Streaming list ─────────────────────────────────────────────────────────
//
// Fires the `"cloud-storage:list-chunk"` plugin hook with batches of entries
// as opendal pages through the listing. The plugin can render rows
// incrementally so the user sees the first ~100 entries within a few hundred
// milliseconds rather than waiting on the full ~N-thousand-item fetch.
//
// Hook payload:
//   { stream_id: String, items: [CloudObject], done: bool, truncated?: bool, error?: String }
//
// Cancellation: callers register a flag in `CloudHost::cancellations` keyed
// by `stream_id`; flipping it ends the loop on the next batch boundary.

/// Batch size — bigger batches reduce the number of `fire_plugin_hook` calls
/// (each one locks the host's PluginHost mutex for the duration of the Lua
/// handler). 1000 means ~10 hook fires for a 10k-item folder instead of 67;
/// the handler only renders a lightweight counter mid-stream anyway, so a
/// large batch costs us nothing on the UI side.
const STREAM_BATCH_SIZE: usize = 1000;

/// Absolute ceiling. The caller-provided cap is clamped to this, so even a
/// plugin that asks for 1_000_000 entries can't OOM the host. Sized to ~300MB
/// of Lua-table memory worst case — far above the documented 50_000 setting
/// max, gives us headroom without making the cap unreachable.
const STREAM_ABSOLUTE_CEILING: usize = 100_000;
/// Default cap used when the caller doesn't pass one.
const STREAM_DEFAULT_CAP: usize = 5_000;

/// Search-specific cap. Lower because results are flat (no folder grouping)
/// and the plugin renders every row in the sidebar; >5000 hits and the user
/// should refine the pattern instead.
const SEARCH_HARD_CAP: usize = 5_000;

pub async fn list_stream(
    host:      Arc<dyn CloudHost>,
    conn:      CloudConnection,
    bucket:    String,
    prefix:    String,
    stream_id: String,
    cap:       Option<usize>,
    cancel:    Arc<AtomicBool>,
) -> Result<()> {
    let op = build(&conn, &bucket).await?;
    let path = normalize_prefix(&prefix);

    let effective_cap = cap.unwrap_or(STREAM_DEFAULT_CAP)
        .clamp(100, STREAM_ABSOLUTE_CEILING);

    let mut lister = op.lister(&path).await.map_err(map_op_err)?;
    let mut batch: Vec<CloudObject> = Vec::with_capacity(STREAM_BATCH_SIZE);
    let mut total: usize = 0;
    let mut truncated = false;

    while let Some(entry) = lister.next().await {
        if cancel.load(Ordering::Relaxed) {
            // Silent cancel: don't emit a 'done' event — the plugin already
            // moved on (newer stream_id), this stream is now orphaned.
            return Ok(());
        }
        let entry = match entry {
            Ok(e) => e,
            Err(e) => {
                deliver_chunk(&*host, serde_json::json!({
                    "stream_id": stream_id,
                    "items":     Vec::<CloudObject>::new(),
                    "done":      true,
                    "error":     format!("opendal: {e}"),
                }));
                return Ok(());
            }
        };
        if entry.path() == path { continue; } // skip the prefix placeholder
        batch.push(entry_to_object(&entry));
        total += 1;
        if total >= effective_cap {
            truncated = true;
            break;
        }
        if batch.len() >= STREAM_BATCH_SIZE {
            deliver_chunk(&*host, serde_json::json!({
                "stream_id": stream_id,
                "items":     std::mem::take(&mut batch),
                "done":      false,
            }));
        }
    }

    deliver_chunk(&*host, serde_json::json!({
        "stream_id": stream_id,
        "items":     batch,
        "done":      true,
        "truncated": truncated,
    }));
    Ok(())
}

// ── Wildcard search ────────────────────────────────────────────────────────
//
// Glob → regex compilation, with literal-prefix extraction so we can scope
// the recursive listing as tight as possible. Semantics intentionally mirror
// IntelliJ Big Data Tools / VS Code "files-to-include" rather than POSIX:
// `*` is permissive and crosses path separators, `**` is an alias for `*`,
// and `?` matches exactly one non-separator char. Strict POSIX would force
// users to remember `**` for every recursive search — and asymmetry between
// "search a cloud bucket" and "search a flat folder of objects" is what
// confused the first beta tester.
//
// Examples (input glob → (literal_prefix, regex)):
//   "data/2024/chunk_*"       → ("data/2024/chunk_",  "^data/2024/chunk_.*$")
//   "data/2024/*/chunk_*"     → ("data/2024/",        "^data/2024/.*/chunk_.*$")
//   "*/0"                      → ("",                  "^.*/0$")
//   "logs/*/error.log"        → ("logs/",             "^logs/.*/error\\.log$")

struct CompiledGlob {
    literal_prefix: String,
    regex:          regex::Regex,
}

fn compile_glob(pattern: &str) -> Result<CompiledGlob> {
    // 1. Literal prefix: everything up to the first wildcard char.
    let mut literal_end = pattern.len();
    for (i, c) in pattern.char_indices() {
        if matches!(c, '*' | '?' | '[' | '{') {
            literal_end = i;
            break;
        }
    }
    // Trim back to the last `/` before the wildcard so the prefix stays on
    // a directory boundary — opendal's lister needs that.
    let lp = if literal_end < pattern.len() {
        match pattern[..literal_end].rfind('/') {
            Some(i) => pattern[..=i].to_string(),
            None    => String::new(),
        }
    } else {
        pattern.to_string()
    };

    // 2. Glob → regex. Both `*` and `**` map to `.*` (permissive — crosses
    //    `/`). `?` stays segment-local so `a?b` doesn't sneak across folders.
    let mut re = String::from("^");
    let bytes = pattern.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'*' if i + 1 < bytes.len() && bytes[i + 1] == b'*' => {
                re.push_str(".*");
                i += 2;
                // Eat a trailing `/` after `**` so `**/foo` matches `foo`
                // at root too (no leading `/` boundary required).
                if i < bytes.len() && bytes[i] == b'/' { i += 1; }
            }
            b'*' => { re.push_str(".*"); i += 1; }
            b'?' => { re.push_str("[^/]");  i += 1; }
            c @ (b'.' | b'+' | b'(' | b')' | b'^' | b'$' | b'|' | b'\\' | b'{' | b'}' | b'[' | b']') => {
                re.push('\\'); re.push(c as char); i += 1;
            }
            c => { re.push(c as char); i += 1; }
        }
    }
    re.push('$');
    let regex = regex::Regex::new(&re)
        .map_err(|e| CloudError::Other(format!("invalid pattern '{pattern}': {e}")))?;
    Ok(CompiledGlob { literal_prefix: lp, regex })
}

pub async fn search_stream(
    host:        Arc<dyn CloudHost>,
    conn:        CloudConnection,
    bucket:      String,
    root_prefix: String,
    pattern:     String,
    stream_id:   String,
    cancel:      Arc<AtomicBool>,
) -> Result<()> {
    // Anything that fails before the streaming loop needs to surface as a
    // terminal `done` event with `error` set — otherwise the plugin sits on
    // "Searching… (0 matches)" forever because no async chunk ever arrives.
    let emit_done_err = |msg: String| {
        deliver_chunk(&*host, serde_json::json!({
            "stream_id": stream_id,
            "items":     Vec::<CloudObject>::new(),
            "done":      true,
            "error":     msg,
            "kind":      "search",
        }));
    };

    let op = match build(&conn, &bucket).await {
        Ok(o)  => o,
        Err(e) => { emit_done_err(format!("connection: {e}")); return Ok(()); }
    };
    let glob = match compile_glob(&pattern) {
        Ok(g)  => g,
        Err(e) => { emit_done_err(format!("{e}")); return Ok(()); }
    };

    // Effective search prefix: caller's root_prefix (current breadcrumb) +
    // the literal head of the pattern. Both normalised to end with `/`.
    let root = normalize_prefix(&root_prefix);
    let combined = if root.is_empty() {
        glob.literal_prefix.clone()
    } else if glob.literal_prefix.is_empty() {
        root.clone()
    } else {
        format!("{root}{}", glob.literal_prefix)
    };

    let mut lister = match op.lister_with(&combined).recursive(true).await {
        Ok(l)  => l,
        Err(e) => { emit_done_err(format!("opendal: {e}")); return Ok(()); }
    };
    let mut batch: Vec<CloudObject> = Vec::with_capacity(STREAM_BATCH_SIZE);
    let mut matched: usize = 0;
    let mut scanned: usize = 0;
    let mut truncated = false;
    // First 5 paths actually returned by opendal — surfaced in the final
    // `done` chunk when matches==0 so the plugin can show a diagnostic
    // ("scanned X, here's what opendal saw"). Cheap to collect; lets us
    // distinguish "regex too strict" from "opendal returned nothing".
    let mut samples: Vec<String> = Vec::with_capacity(5);

    tracing::info!(
        "cloud search: bucket={bucket} root_prefix={root_prefix} pattern={pattern} \
         combined={combined} literal_prefix={lp} regex={re}",
        lp = glob.literal_prefix, re = glob.regex.as_str(),
    );

    while let Some(entry) = lister.next().await {
        if cancel.load(Ordering::Relaxed) {
            return Ok(()); // silent orphan; plugin already moved on
        }
        let entry = match entry {
            Ok(e) => e,
            Err(e) => {
                deliver_chunk(&*host, serde_json::json!({
                    "stream_id": stream_id,
                    "items":     Vec::<CloudObject>::new(),
                    "done":      true,
                    "error":     format!("opendal: {e}"),
                    "kind":      "search",
                }));
                return Ok(());
            }
        };
        scanned += 1;
        let path = entry.path();
        if path == combined { continue; } // skip prefix placeholder
        if samples.len() < 5 { samples.push(path.to_string()); }
        // Strip the optional root_prefix so the pattern matches against the
        // user-visible relative path (the pattern is written relative to the
        // breadcrumb the user is in).
        let rel = if !root.is_empty() && path.starts_with(&root) {
            &path[root.len()..]
        } else { path };
        if !glob.regex.is_match(rel) { continue; }

        batch.push(entry_to_object(&entry));
        matched += 1;
        if matched >= SEARCH_HARD_CAP { truncated = true; break; }
        if batch.len() >= STREAM_BATCH_SIZE {
            deliver_chunk(&*host, serde_json::json!({
                "stream_id": stream_id,
                "items":     std::mem::take(&mut batch),
                "done":      false,
                "kind":      "search",
                "scanned":   scanned,
            }));
        }
    }

    tracing::info!(
        "cloud search done: scanned={scanned} matched={matched} samples={samples:?}"
    );

    deliver_chunk(&*host, serde_json::json!({
        "stream_id":     stream_id,
        "items":         batch,
        "done":          true,
        "kind":          "search",
        "scanned":       scanned,
        "matched":       matched,
        "truncated":     truncated,
        "regex":         glob.regex.as_str(),
        "combined":      combined,
        "sample_paths":  samples,
    }));
    Ok(())
}
