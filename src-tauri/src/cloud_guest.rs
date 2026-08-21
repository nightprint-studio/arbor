//! Routing the cloud's storage through a wasm provider, when one is installed.
//!
//! Two shapes, and the difference matters.
//!
//! **The five primitives** — `list`, `stat`, `delete`, `copy`, `test_connection` — map onto
//! `arbor:extensions/cloud-provider@1` one for one. Each is a whole answer, so each is one
//! call, and the fall-through is the whole function.
//!
//! **The transfers** do not map onto anything: a download is a loop with a job, a progress
//! ticker and a cancellation flag around it, and none of that belongs to a provider. So what
//! routes there is only the source of the bytes — [`GuestTransport`], handed to
//! `arbor-cloud`'s existing loops through [`arbor_cloud::transport`]. The loop stays; the
//! reader changes.
//!
//! Everything else in `arbor.cloud` — streaming listings, the job registry, the chunk-order
//! modal, sync — stays where it is. A guest that owned those would have to own the job
//! registry too, and the interface was deliberately drawn short of that.
//!
//! ## `Option<Result<T>>`, and why
//!
//! Every function here returns `None` for **"not ours, fall through"** and `Some(_)` for
//! **"ours, and this is the answer"** — the same two-shaped signal `bennu`'s `lsp_route` uses,
//! and for the same reason: the caller has a working implementation already, and the question
//! is only whether something better applies.
//!
//! It is what makes this a migration rather than a switch. With no provider package installed
//! — today's state — every call returns `None` and the behaviour is byte-identical to before.
//!
//! ## What routes, and what does not
//!
//! Only connections whose credential is a **bearer token**. A service-account connection
//! resolves to a JSON key that has to be signed into a JWT, and the guest speaks HTTP with an
//! `Authorization` header and nothing else — so those fall through, which is the honest
//! outcome rather than a guest failing in a way that looks like a network problem.
//!
//! And a guest that cannot authenticate falls through too, with a log line. Installing a
//! provider package must not be able to break a connection that was working: the in-process
//! implementation is still there and still correct.
//!
//! ## Blocking
//!
//! Guest calls block. Reached from the `__cloud_*` reverse-channel handlers, which already run
//! off the runtime's workers — the same rule as everywhere else (landmine #1).

use std::sync::{Arc, Mutex};

use arbor_cloud::prelude::{CloudError, ObjectTransport};
use arbor_plugin_wasm::prelude::{
    CloudGuest, ExtensionIndex, GuestCaps, GuestObject, GuestRange, Services,
};

use crate::cloud::types::{
    CloudConnection, CloudListPage, CloudObject, CloudTestReport, Provider,
};

/// The `[[provides]]` id a connection's provider would be implemented under.
fn provider_id(p: Provider) -> &'static str {
    match p {
        Provider::Gcs => "gcs",
        Provider::S3 => "s3",
        Provider::Azblob => "azblob",
    }
}

/// Whether this connection's credential is something a guest can use.
///
/// A guest sends an `Authorization: Bearer` header and holds no crypto. A service-account key
/// has to be signed into a JWT before it becomes a token, and that signing is the host's —
/// so an SA connection is not a guest's to serve, and saying so here is cheaper than
/// discovering it as a 401 three layers down.
fn guest_can_authenticate(conn: &CloudConnection) -> bool {
    use arbor_cloud::types::GcsAuth;
    match (&conn.provider, &conn.gcs) {
        (Provider::Gcs, Some(GcsAuth::Oauth { .. })) => true,
        // S3 and Azure sign each request rather than carrying a bearer token. When a provider
        // package for them exists it will do that signing itself, and this returns true for
        // them too — until then there is nothing to route to.
        _ => false,
    }
}

/// Open a guest for this connection, or `None` when nothing should route to one.
fn open(conn: &CloudConnection, bucket: &str) -> Option<CloudGuest> {
    if !guest_can_authenticate(conn) {
        return None;
    }
    let manifests = arbor_plugin_core::prelude::discover_plugins().ok()?;
    let enabled = arbor_plugin_core::prelude::load_plugin_states();
    let index = ExtensionIndex::build(&manifests, &enabled);
    let entry = index.resolve("cloud-provider", 1, provider_id(conn.provider))?;
    let manifest = manifests.iter().find(|m| m.name == entry.plugin)?;

    let caps = GuestCaps::from_manifest(manifest);
    let services: Services = Arc::new(crate::plugin_wasm::TauriHostServices::new(Box::new(
        {
            let plugin = entry.plugin.clone();
            move |_p: &str, level: &str, message: &str| {
                tracing::debug!("[{plugin}] {level}: {message}");
            }
        },
    )));

    // A package with no credential in its own slot cannot serve anything, and finding that
    // out from a failed listing is the shape this whole file exists to avoid: the user
    // installed a provider and their working panel started erroring. So the precondition is
    // checked here, and an unset slot is simply "not ours" like every other `None`.
    //
    // This is not the host reaching into a package's namespace. It resolves the account the
    // guest itself would name, through the guest's own capability envelope — the invariant is
    // that a package cannot NAME another's slot, and that Arbor never WRITES into one. Asking
    // the keychain whether it holds what it would be handing over is the broker's own job.
    if !has_credential(&caps) {
        tracing::debug!(
            "cloud: provider '{}' has no credential stored yet — using the built-in path",
            entry.plugin
        );
        return None;
    }

    let host = crate::plugin_wasm::engine().ok()?;
    let mut guest = match host.open_cloud(&entry.module, caps, services) {
        Ok(g) => g,
        Err(e) => {
            // A provider that will not instantiate is a real problem, but not one to fail a
            // listing over: the in-process path still works, and the Plugin Manager's probe is
            // where this gets surfaced as the package's own problem.
            tracing::warn!("cloud: provider '{}' would not instantiate: {e}", entry.plugin);
            return None;
        }
    };

    // Bucket bound at open, so authentication happens once rather than per call. A failed
    // connect must produce `None` and not a guest holding no connection — that guest would
    // answer every call with "no connection was opened", which reads to the user as the
    // bucket being broken rather than the package being unable to open it.
    if let Err(e) = guest.connect(bucket, "") {
        tracing::warn!(
            "cloud: provider '{}' could not open '{bucket}': {e} — using the built-in path",
            entry.plugin
        );
        return None;
    }
    Some(guest)
}

/// Whether the package has anything in the slot it declared.
///
/// Every slot, not a named one: a provider declares what it needs, and the host has no
/// business knowing that GCS calls its token `oauth`. One populated slot is the closest thing
/// to "this package has been set up" that does not require asking the network.
fn has_credential(caps: &GuestCaps) -> bool {
    use arbor_plugin_wasm::prelude::HostServices;
    let store = crate::plugin_wasm::TauriHostServices::new(Box::new(|_: &str, _: &str, _: &str| {}));
    caps.slots().iter().any(|key| {
        caps.credential_account(key)
            .ok()
            .and_then(|account| store.credential_get(&account).ok().flatten())
            .is_some_and(|v| !v.is_empty())
    })
}

// ── Type conversion ─────────────────────────────────────────────────────────────

/// Unix seconds → the ISO-8601 the existing surface already emits.
///
/// Hand-rolled because the workspace has no date crate and this is the only place that needs
/// one. The inverse of the guest's own parser, by the same civil-from-days algorithm — which
/// is worth a test precisely because "off by one in February" is invisible ten months a year.
fn iso8601(secs: u64) -> String {
    let days = (secs / 86_400) as i64;
    let rem = secs % 86_400;
    let (h, mi, s) = (rem / 3600, (rem % 3600) / 60, rem % 60);

    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };

    format!("{y:04}-{m:02}-{d:02}T{h:02}:{mi:02}:{s:02}Z")
}

fn to_cloud_object(o: GuestObject) -> CloudObject {
    CloudObject {
        path: o.key,
        is_dir: o.prefix,
        // A prefix has no size to report, and 0 would read as an empty file.
        size: (!o.prefix).then_some(o.size),
        etag: o.etag,
        content_type: o.content_type,
        last_modified: o.modified.map(iso8601),
    }
}

// ── The five ────────────────────────────────────────────────────────────────────

pub fn list(
    conn: &CloudConnection,
    bucket: &str,
    prefix: &str,
    limit: Option<usize>,
) -> Option<Result<CloudListPage, String>> {
    let mut g = open(conn, bucket)?;
    // `/` as the delimiter: the panel draws folders, and a listing without one walks the whole
    // bucket to render one directory.
    Some(
        g.list(prefix, Some("/"), None, limit.map(|n| n as u32))
            .map(|l| CloudListPage {
                truncated: l.cursor.is_some(),
                items: l.entries.into_iter().map(to_cloud_object).collect(),
            }),
    )
}

pub fn stat(
    conn: &CloudConnection,
    bucket: &str,
    path: &str,
) -> Option<Result<CloudObject, String>> {
    let mut g = open(conn, bucket)?;
    Some(g.stat(path).map(to_cloud_object))
}

pub fn copy(
    conn: &CloudConnection,
    bucket: &str,
    src: &str,
    dst: &str,
) -> Option<Result<(), String>> {
    let mut g = open(conn, bucket)?;
    Some(g.copy(src, dst))
}

pub fn delete(
    conn: &CloudConnection,
    bucket: &str,
    path: &str,
    recursive: bool,
) -> Option<Result<(), String>> {
    let mut g = open(conn, bucket)?;
    if !recursive {
        return Some(g.delete(path));
    }
    // Recursive delete is not in the interface, deliberately: it is a list plus n deletes, and
    // an interface method that loops is a loop the host cannot show progress for or cancel.
    // Done here instead, where both are already possible.
    Some((|| {
        let mut cursor = None;
        loop {
            let page = g.list(path, None, cursor.clone(), Some(1000))?;
            for entry in &page.entries {
                if !entry.prefix {
                    g.delete(&entry.key)?;
                }
            }
            match page.cursor {
                Some(next) => cursor = Some(next),
                None => break,
            }
        }
        Ok(())
    })())
}

pub fn test_connection(
    conn: &CloudConnection,
    bucket: Option<&str>,
) -> Option<Result<CloudTestReport, String>> {
    // Without a bucket there is nothing for a provider to open — the in-process path can still
    // check credentials on their own, so this is a fall-through rather than a failure.
    let mut g = open(conn, bucket?)?;
    Some(Ok(match g.test() {
        Ok(()) => CloudTestReport {
            ok: true,
            error: None,
            auth_method: Some("oauth".into()),
            identity: None,
        },
        Err(e) => CloudTestReport {
            ok: false,
            error: Some(e),
            auth_method: Some("oauth".into()),
            identity: None,
        },
    }))
}

// ── Transfers ───────────────────────────────────────────────────────────────────

/// A provider, as `arbor-cloud`'s transfer loops want to see one.
///
/// The `Mutex` is what makes a guest usable from a transfer at all: a `Store` is not `Sync`
/// (that is the isolation, not an oversight), and a transfer holds one source across many
/// chunks. Uncontended in practice — one transfer, one guest, one bucket — so it costs an
/// atomic per chunk against an HTTP request.
struct GuestTransport {
    guest: Mutex<CloudGuest>,
}

impl GuestTransport {
    fn with<T>(&self, f: impl FnOnce(&mut CloudGuest) -> Result<T, String>) -> Result<T, CloudError> {
        let mut g = self
            .guest
            .lock()
            .map_err(|_| CloudError::Other("cloud provider is in a failed state".into()))?;
        f(&mut g).map_err(CloudError::Other)
    }
}

impl ObjectTransport for GuestTransport {
    fn size(&self, key: &str) -> Result<u64, CloudError> {
        self.with(|g| g.stat(key).map(|o| o.size))
    }

    fn read_at(&self, key: &str, offset: u64, len: u64) -> Result<Vec<u8>, CloudError> {
        // `end` is exclusive on both sides of the interface, so this is a straight pass.
        let part = GuestRange { start: offset, end: Some(offset + len) };
        self.with(|g| g.read(key, Some(part)))
    }

    fn exists(&self, key: &str) -> Result<bool, CloudError> {
        self.with(|g| g.stat_opt(key).map(|o| o.is_some()))
    }

    fn write_whole(
        &self,
        key: &str,
        body: Vec<u8>,
        content_type: Option<&str>,
    ) -> Result<(), CloudError> {
        self.with(|g| g.write(key, &body, content_type))
    }
}

/// The resolver `arbor-cloud` asks on every transfer.
///
/// Installed once at startup. Returning `None` is the normal answer — no provider for this
/// connection, or a connection a provider cannot authenticate — and it means the transfer
/// runs exactly as it did before any of this existed.
pub fn transport_resolver(
    conn: &CloudConnection,
    bucket: &str,
) -> Option<Arc<dyn ObjectTransport>> {
    let guest = open(conn, bucket)?;
    Some(Arc::new(GuestTransport { guest: Mutex::new(guest) }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_timestamp_round_trips_against_known_values() {
        assert_eq!(iso8601(0), "1970-01-01T00:00:00Z");
        assert_eq!(iso8601(1_787_234_591), "2026-08-20T14:03:11Z");
        assert_eq!(iso8601(951_868_800), "2000-03-01T00:00:00Z");
    }

    #[test]
    fn a_leap_day_is_not_off_by_one() {
        // The month shift in the civil-from-days algorithm is invisible ten months a year.
        assert_eq!(iso8601(1_709_164_800), "2024-02-29T00:00:00Z");
        assert_eq!(iso8601(1_709_251_200), "2024-03-01T00:00:00Z");
    }

    #[test]
    fn only_gcs_with_a_bearer_token_routes_to_a_guest() {
        use arbor_cloud::types::GcsAuth;
        let mut conn = CloudConnection {
            provider: Provider::Gcs,
            config_id: "c".into(),
            project_id: None,
            gcs: Some(GcsAuth::Oauth { secret_ref: "r".into() }),
            s3: None,
            azblob: None,
        };
        assert!(guest_can_authenticate(&conn));

        // A service-account key has to be signed into a JWT, and a guest holds no crypto —
        // routing it would produce a 401 that looks like a network problem.
        conn.gcs = Some(GcsAuth::SaFile { path: "/k.json".into() });
        assert!(!guest_can_authenticate(&conn));

        conn.provider = Provider::S3;
        assert!(!guest_can_authenticate(&conn));
    }
}
