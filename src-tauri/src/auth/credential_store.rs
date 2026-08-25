//! Credential store — a single OS-keychain item backing *all* of Arbor's stored
//! secrets (git-provider OAuth tokens & PATs, refresh tokens, Jira/Linear keys).
//!
//! # Why one item
//!
//! macOS pops a Keychain authorisation dialog on *every* item read whose ACL
//! doesn't "Always Allow" the running binary — and for an unsigned / ad-hoc dev
//! build whose code signature changes on each compile, that's every read. With
//! one keychain item per credential, a couple of git operations turned into a
//! wall of password prompts (one per distinct credential, plus the startup token
//! probes). Windows never re-prompts a process already granted, hence the gap.
//!
//! So everything now lives in a **single** item ([`SERVICE`] / [`VAULT_ACCOUNT`])
//! holding a JSON object `{ account -> secret }`. The first credential access in
//! a process reads that one item (the single prompt) and mirrors it into an
//! in-memory map; every `get`/`save`/`delete` afterwards is pure memory plus, on
//! writes, one keychain write. Net effect: **at most one read prompt per
//! session** (and with a stable code signature, "Always Allow" on that one item
//! sticks across rebuilds).
//!
//! This module is the sole gate to the store: every write goes through
//! `save`/`save_for_host`, every read through `get`/`get_for_host`/
//! `resolve_credentials`, so the in-memory map is authoritative — the shell is
//! the only writer, so there is nothing external to invalidate against. The
//! public API is unchanged; only the backing storage moved from N items to one.
//!
//! # The size ceiling, and why it was invisible
//!
//! One item has a limit. Windows caps a credential blob at 2560 bytes and stores it as
//! UTF-16, so the *entire* vault had to fit in 1280 characters — a few OAuth tokens and
//! two database passwords go past that. What happened then was the worst available
//! outcome: `set_password` refused, but the in-memory mirror had already been updated,
//! so the secret was there for the rest of the session and every lookup succeeded. It
//! was missing at the next launch, hours later, with nothing to connect it to.
//!
//! Both halves of that are fixed here, and they are separate bugs:
//!
//!  * the vault **spills into numbered items** when it no longer fits in one, so a
//!    write that has somewhere to go is not refused;
//!  * a write that fails anyway **does not update the mirror**, so the error a caller
//!    gets is the truth about what is stored rather than a note about something that
//!    seemed to work.
//!
//! And a third, next to them: an item that is present but unparseable used to load as
//! an empty vault, which meant the next save of any credential overwrote it with just
//! that one. It is an error now — see [`read_vault_from_store`].

use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};

use keyring::Entry;

use crate::error::{AppError, Result};

/// Keychain service — shows as the item name in Keychain Access.
const SERVICE: &str = "Arbor";
/// The vault's first item: `Arbor` / `credentials`.
const VAULT_ACCOUNT: &str = "credentials";

/// Marker that turns the head item into a pointer at N pieces instead of the vault
/// itself. A JSON object starts with `{`, so this can never be mistaken for one —
/// which is what lets a vault written by an older build keep loading unchanged.
const CHUNK_MARKER: &str = "ARBOR-CHUNKED:";

/// How much of one keychain item a chunk may use, in UTF-16 code units.
///
/// **This is why chunking exists at all.** Windows caps a credential blob at
/// `CRED_MAX_CREDENTIAL_BLOB_SIZE` = 2560 *bytes*, and `keyring` stores the value as
/// UTF-16 — so the whole vault had to fit in **1280 characters**. A handful of OAuth
/// tokens and two database passwords is past that, and what happened then was not an
/// error anybody saw: `set_password` refused, the in-memory mirror had already been
/// updated, so every session kept working perfectly and the secret was simply never
/// written. It came back missing at the next launch. See the write path below for the
/// other half of that fix.
///
/// 1100 rather than 1280 leaves room for the platform's own accounting and for the
/// item's other fields.
const CHUNK_UNITS: usize = 1100;

/// The account holding piece `i` (0 is the head, which is the marker itself).
fn chunk_account(i: usize) -> String {
    format!("{VAULT_ACCOUNT}.{i}")
}

/// Does this string fit in one keychain item?
fn fits(text: &str) -> bool {
    text.encode_utf16().count() <= CHUNK_UNITS
}

/// Cut `text` into pieces that each fit.
///
/// By characters, measuring UTF-16 units: a password with an accent in it costs one
/// unit here and two bytes there, and a split that landed mid-character would write a
/// piece that cannot be reassembled.
fn split_chunks(text: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut units = 0usize;
    for ch in text.chars() {
        let size = ch.len_utf16();
        if units + size > CHUNK_UNITS && !current.is_empty() {
            out.push(std::mem::take(&mut current));
            units = 0;
        }
        current.push(ch);
        units += size;
    }
    if !current.is_empty() {
        out.push(current);
    }
    out
}

/// What the head item turned out to be.
enum Head {
    /// The vault itself, as an older build wrote it.
    Plain(String),
    /// A pointer at this many pieces.
    Chunked(usize),
}

fn read_head(text: String) -> Head {
    // Bound to a `let` rather than matched inline: the borrow `strip_prefix` takes would
    // otherwise live for the whole `match` — temporaries in a scrutinee do — and the
    // `Plain` arm needs to move `text` out.
    let count = text.strip_prefix(CHUNK_MARKER).and_then(|n| n.trim().parse::<usize>().ok());
    match count {
        Some(n) => Head::Chunked(n),
        None => Head::Plain(text),
    }
}

/// How many pieces the vault was last stored in, so a shrinking vault cleans up after
/// itself instead of leaving secrets behind in items nothing points at any more.
///
/// Only ever touched by [`read_vault_from_store`] and [`write_vault_to_store`], both of
/// which run with the [`VAULT`] lock already held — so the order is always VAULT then
/// this one, never the reverse, and there is no second path to invert it. It is a
/// `Mutex` rather than a plain `static mut` for safety, not for the locking.
static CHUNKS_ON_DISK: Mutex<usize> = Mutex::new(0);

fn entry_for(account: &str) -> Result<Entry> {
    Entry::new(SERVICE, account).map_err(|e| AppError::AuthFailed(e.to_string()))
}

fn read_item(account: &str) -> Result<Option<String>> {
    match entry_for(account)?.get_password() {
        Ok(text) => Ok(Some(text)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(AppError::AuthFailed(e.to_string())),
    }
}

// ── In-memory vault ────────────────────────────────────────────────────────────

/// In-memory mirror of the vault. `None` until the first access, `Some(map)`
/// once loaded — the map is the in-process source of truth (the shell is the
/// sole writer, so we never reload). Keys are credential "accounts"
/// (`github.com/arbor`, a bare host, `github.com/arbor-refresh`, Jira keys, …);
/// values are the raw secrets (`get_for_host` stores/parses `"user\tpass"`).
static VAULT: LazyLock<Mutex<Option<HashMap<String, String>>>> =
    LazyLock::new(|| Mutex::new(None));

/// Read + reassemble the vault from the OS store.
///
/// A **missing** item is an empty vault — that is a first run, and it is the one case
/// where starting from nothing is right.
///
/// An item that is present and does **not** parse is an **error**, and that is a
/// deliberate reversal. It used to fall back to an empty map on the reasoning that a
/// corrupt vault only means re-auth; but the map is what the next write persists, so
/// "start empty" meant the next credential saved anywhere in the app would overwrite
/// the item with just itself and delete every other secret in it. Refusing to load is
/// recoverable — re-auth, or fix the item — and silently deleting everything is not.
fn read_vault_from_store() -> Result<HashMap<String, String>> {
    // A missing item and a blank one are the same thing: nothing is stored, and nothing
    // can be lost by starting from an empty map. Only a non-blank item that will not
    // parse is the dangerous case the doc above is about.
    let head = read_item(VAULT_ACCOUNT)?.unwrap_or_default();
    if head.trim().is_empty() {
        *CHUNKS_ON_DISK.lock().unwrap_or_else(|p| p.into_inner()) = 0;
        return Ok(HashMap::new());
    }

    let (json, chunks) = match read_head(head) {
        Head::Plain(text) => (text, 0),
        Head::Chunked(count) => {
            let mut joined = String::new();
            for i in 1..=count {
                let account = chunk_account(i);
                let piece = read_item(&account)?.ok_or_else(|| {
                    AppError::AuthFailed(format!(
                        "the credential vault is stored in {count} parts and `{account}` is \
                         missing — nothing has been overwritten; the remaining parts are intact"
                    ))
                })?;
                joined.push_str(&piece);
            }
            (joined, count)
        }
    };

    let map = serde_json::from_str(&json).map_err(|e| {
        AppError::AuthFailed(format!(
            "the credential vault could not be read ({e}). It has been left exactly as it is \
             rather than replaced, so nothing is lost — but stored credentials are unavailable \
             until it can be parsed."
        ))
    })?;
    *CHUNKS_ON_DISK.lock().unwrap_or_else(|p| p.into_inner()) = chunks;
    Ok(map)
}

/// Persist the whole map, across as many items as it takes.
///
/// One item while it fits, which is the common case and the one the single-item design
/// was for: one macOS prompt per session. Past that the vault spills into
/// `credentials.1`, `.2`, … and the head becomes a marker naming the count. More items
/// means more prompts on macOS, and that is the right trade — a prompt is an
/// inconvenience, a secret that silently failed to save is data loss.
///
/// Stale pieces from a larger previous write are deleted, so a vault that shrinks does
/// not leave secrets sitting in items nothing points at.
fn write_vault_to_store(map: &HashMap<String, String>) -> Result<()> {
    let json = serde_json::to_string(map).map_err(|e| AppError::AuthFailed(e.to_string()))?;
    let previous = *CHUNKS_ON_DISK.lock().unwrap_or_else(|p| p.into_inner());

    let written = if fits(&json) {
        entry_for(VAULT_ACCOUNT)?
            .set_password(&json)
            .map_err(|e| AppError::AuthFailed(e.to_string()))?;
        0
    } else {
        let pieces = split_chunks(&json);
        // The pieces go down BEFORE the head points at them: a crash between the two
        // leaves a head naming a count whose parts are all present, which reads back
        // fine. The other order would leave a head pointing at pieces that do not
        // exist yet.
        for (i, piece) in pieces.iter().enumerate() {
            entry_for(&chunk_account(i + 1))?
                .set_password(piece)
                .map_err(|e| AppError::AuthFailed(e.to_string()))?;
        }
        entry_for(VAULT_ACCOUNT)?
            .set_password(&format!("{CHUNK_MARKER}{}", pieces.len()))
            .map_err(|e| AppError::AuthFailed(e.to_string()))?;
        pieces.len()
    };

    for i in (written + 1)..=previous {
        // Best effort: a leftover that will not delete is untidy, not incorrect, and
        // failing the whole write over it would be worse than leaving it.
        let _ = entry_for(&chunk_account(i)).and_then(|e| {
            e.delete_credential().map_err(|err| AppError::AuthFailed(err.to_string()))
        });
    }
    *CHUNKS_ON_DISK.lock().unwrap_or_else(|p| p.into_inner()) = written;
    Ok(())
}

/// Look up one account in the vault, loading it from the store on first use.
fn read_vault(key: &str) -> Result<Option<String>> {
    let mut guard = VAULT.lock().unwrap_or_else(|p| p.into_inner());
    if guard.is_none() {
        *guard = Some(read_vault_from_store()?);
    }
    Ok(guard.as_ref().expect("vault loaded above").get(key).cloned())
}

/// Mutate the vault (loading it on first use) and persist the result. The whole
/// read-modify-write runs under the lock so concurrent saves serialise.
///
/// **The change is applied to a copy and only adopted once the write succeeded.** It
/// used to mutate the live map first and write afterwards, which is the other half of
/// how a password could be reported saved and not be: a refused write left memory
/// holding a secret the store did not have, every lookup for the rest of the session
/// answered from memory and worked, and the loss only surfaced at the next launch —
/// by which time nothing connected it to the write that failed hours earlier.
///
/// Now a failed write leaves the mirror exactly as the store has it. The caller's
/// error is then the whole truth: the secret is not saved, and nothing in the process
/// will pretend otherwise.
fn mutate_vault(f: impl FnOnce(&mut HashMap<String, String>)) -> Result<()> {
    let mut guard = VAULT.lock().unwrap_or_else(|p| p.into_inner());
    if guard.is_none() {
        *guard = Some(read_vault_from_store()?);
    }
    let mut next = guard.as_ref().expect("vault loaded above").clone();
    f(&mut next);
    write_vault_to_store(&next)?;
    *guard = Some(next);
    Ok(())
}

// ── Per-account credential (OAuth tokens, refresh tokens, Jira/Linear keys) ─────

/// Save (or update) a credential under `host` (an opaque account key).
pub fn save(host: &str, _username: &str, password: &str) -> Result<()> {
    let (host, password) = (host.to_string(), password.to_string());
    mutate_vault(move |m| {
        m.insert(host, password);
    })
}

/// Retrieve a stored credential. Returns `None` if not present.
pub fn get(host: &str, _username: &str) -> Result<Option<String>> {
    read_vault(host)
}

/// Delete a stored credential.
pub fn delete(host: &str, _username: &str) -> Result<()> {
    let host = host.to_string();
    mutate_vault(move |m| {
        m.remove(&host);
    })
}

// ── Plugin-owned credentials ───────────────────────────────────────────────────

/// Remove every credential a plugin owned.
///
/// Called when a plugin is uninstalled. Deleting its directory does not touch the keychain,
/// so without this a plugin the user removed leaves tokens behind that nothing on disk
/// explains — and that nothing will ever clean up, because the manifest listing the slots is
/// gone with the folder.
///
/// Ownership is decided by `arbor_plugin_types::credentials::belongs_to`, the same function
/// that builds the names in the first place, so there is no second definition of "this
/// plugin's account" to drift from the first.
pub fn forget_plugin(plugin: &str) -> Result<()> {
    let plugin = plugin.to_string();
    mutate_vault(move |m| {
        let before = m.len();
        m.retain(|account, _| {
            !arbor_plugin_types::prelude::credential_belongs_to(account, &plugin)
        });
        let removed = before - m.len();
        if removed > 0 {
            tracing::info!("credentials: forgot {removed} entries owned by '{plugin}'");
        }
    })
}

// ---------------------------------------------------------------------------
// Host-based default credential (used for automatic fetch/push auth)
//
// Stores a single "default" credential per hostname so that network operations
// can look up credentials without knowing the username in advance.
// Vault key: the bare host — value: "username\ttoken_or_password".
// ---------------------------------------------------------------------------

/// Save or replace the default credential for a host/URL.
pub fn save_for_host(url_or_host: &str, username: &str, password: &str) -> Result<()> {
    let host = extract_host(url_or_host).unwrap_or_else(|| url_or_host.to_string());
    let combined = format!("{username}\t{password}");
    mutate_vault(move |m| {
        m.insert(host, combined);
    })
}

/// Retrieve the default (username, password/token) for a host/URL.
pub fn get_for_host(url_or_host: &str) -> Result<Option<(String, String)>> {
    let host = extract_host(url_or_host).unwrap_or_else(|| url_or_host.to_string());
    Ok(read_vault(&host)?.and_then(|s| {
        s.split_once('\t')
            .map(|(user, pass)| (user.to_string(), pass.to_string()))
    }))
}

/// Delete the default credential for a host/URL.
pub fn delete_for_host(url_or_host: &str) -> Result<()> {
    let host = extract_host(url_or_host).unwrap_or_else(|| url_or_host.to_string());
    mutate_vault(move |m| {
        m.remove(&host);
    })
}

// ---------------------------------------------------------------------------
// Unified credential resolution (used by git remote operations)
//
// Priority:
//   1. OAuth token stored by Device Flow: key = "{host}/arbor"
//   2. Default (PAT / username+password) stored via Settings → Credentials
//
// Returns (username, password/token) or None if nothing is stored.
// For OAuth tokens, the username is "x-oauth-basic" (accepted by GitHub and GitLab).
// ---------------------------------------------------------------------------

pub fn resolve_credentials(url_or_host: &str) -> Result<Option<(String, String)>> {
    let host = extract_host(url_or_host).unwrap_or_else(|| url_or_host.to_string());

    // 1. OAuth token (Device Flow)
    let oauth_key = format!("{host}/arbor");
    if let Some(token) = read_vault(&oauth_key)? {
        return Ok(Some(("x-oauth-basic".to_string(), token)));
    }

    // 2. PAT / username+password (Settings → Credentials)
    get_for_host(url_or_host)
}

// ---------------------------------------------------------------------------
// URL → hostname extraction (re-exported from git::url for convenience)
// ---------------------------------------------------------------------------

/// Extract the bare hostname from HTTPS, HTTP, or SSH (git@host:path) URLs.
/// Delegates to [`crate::git::url::extract_host`].
pub fn extract_host(url: &str) -> Option<String> {
    crate::git::url::extract_host(url)
}

#[cfg(test)]
mod tests {
    use super::*;

    // Seeding the in-memory vault sets it to `Some(..)`, so subsequent reads
    // never touch the OS keychain — the parsing / priority logic is exercised
    // without a live store. Each test uses unique keys to stay parallel-safe.
    fn seed(pairs: &[(&str, &str)]) {
        let mut g = VAULT.lock().unwrap_or_else(|p| p.into_inner());
        let m = g.get_or_insert_with(HashMap::new);
        for (k, v) in pairs {
            m.insert((*k).to_string(), (*v).to_string());
        }
    }

    #[test]
    fn get_for_host_splits_value_on_tab() {
        seed(&[("split.test", "alice\ttok123")]);
        let got = get_for_host("https://split.test/owner/repo.git").unwrap();
        assert_eq!(got, Some(("alice".to_string(), "tok123".to_string())));
    }

    #[test]
    fn get_for_host_returns_none_without_tab_separator() {
        seed(&[("notab.test", "just-a-token")]);
        assert_eq!(get_for_host("notab.test").unwrap(), None);
    }

    #[test]
    fn resolve_prefers_oauth_token() {
        seed(&[("oauth.test/arbor", "gho_xxx")]);
        let got = resolve_credentials("https://oauth.test/o/r.git").unwrap();
        assert_eq!(got, Some(("x-oauth-basic".to_string(), "gho_xxx".to_string())));
    }

    #[test]
    fn resolve_falls_back_to_host_default() {
        // No OAuth key for this host, only a PAT-style default.
        seed(&[("fallback.test", "bob\tpat456")]);
        let got = resolve_credentials("https://fallback.test/o/r.git").unwrap();
        assert_eq!(got, Some(("bob".to_string(), "pat456".to_string())));
    }

    #[test]
    fn missing_key_reads_none() {
        // Force the vault loaded (empty is fine) so we don't hit the store.
        seed(&[]);
        assert_eq!(get("definitely-absent.test", "").unwrap(), None);
    }

    // ── The chunking, which is what keeps a vault from silently failing to save ──

    #[test]
    fn a_small_vault_still_fits_in_one_item() {
        let json = r#"{"github.com/arbor":"gho_short","picus/local":"hunter2"}"#;
        assert!(fits(json));
        assert_eq!(split_chunks(json).len(), 1);
    }

    /// The case the bug was: more than 1280 characters of credentials. Windows refuses
    /// the write outright at that size, so what matters is that it never gets there.
    #[test]
    fn a_vault_past_the_windows_ceiling_is_split_and_reassembles_exactly() {
        let json = format!(r#"{{"a":"{}","b":"{}"}}"#, "x".repeat(1500), "y".repeat(1500));
        assert!(!fits(&json), "this is the size that used to be unwritable");

        let pieces = split_chunks(&json);
        assert!(pieces.len() > 1, "it has to be split to be storable at all");
        assert!(pieces.iter().all(|p| fits(p)), "every piece has to fit on its own");
        assert_eq!(pieces.concat(), json, "and rejoin byte for byte");
    }

    /// A split that landed inside a character would write pieces that cannot be
    /// reassembled — a password with an accent in it is enough to hit this.
    #[test]
    fn chunks_never_cut_a_character_in_half() {
        let json = format!(r#"{{"pw":"{}"}}"#, "è🔑".repeat(700));
        let pieces = split_chunks(&json);
        assert!(pieces.len() > 1);
        assert!(pieces.iter().all(|p| fits(p)));
        assert_eq!(pieces.concat(), json);
    }

    #[test]
    fn the_head_tells_a_pointer_from_a_vault() {
        // A vault written by a build from before chunking existed still loads.
        assert!(matches!(read_head(r#"{"a":"b"}"#.to_string()), Head::Plain(_)));
        assert!(matches!(read_head(format!("{CHUNK_MARKER}4")), Head::Chunked(4)));
        // Anything that is not the marker is the vault itself, never a count.
        assert!(matches!(read_head("CHUNKED:4".to_string()), Head::Plain(_)));
    }
}
