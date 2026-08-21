//! Where a transfer's bytes come from, when they do not come from opendal.
//!
//! A transfer is a loop with three things around it: a job, a progress ticker and a
//! cancellation flag. None of those change when a provider package serves the bucket — only
//! the source of the bytes does. So that is the only thing this makes pluggable, and
//! [`crate::transfer`] keeps exactly one of each loop.
//!
//! ## Why a resolver rather than a parameter
//!
//! Threading `Option<Arc<dyn ObjectTransport>>` through `download`, `upload`, `sync`,
//! `download_many` and every caller of those would put a wasm-shaped decision in five
//! signatures that have no business knowing wasm exists. Instead the shell installs a
//! resolver once at startup — the same shape [`crate::oauth_google::install_refresher`]
//! already uses — and the loops ask.
//!
//! With nothing installed, [`resolve`] is a `None` from a `OnceLock` and every path is
//! byte-identical to before. That is what keeps this a migration.
//!
//! ## Why the trait is synchronous
//!
//! Because the only implementation is a wasm guest, and a guest call runs to completion on
//! the calling thread. An async signature would mean every implementor wrapping a blocking
//! call in a future for the caller to drive — the blocking would still be there, just harder
//! to see. So it stays visible, and the transfer loop puts it on `spawn_blocking` once, where
//! the rule about not occupying a runtime worker is already written down.

use std::sync::{Arc, OnceLock};

use crate::error::Result;
use crate::types::CloudConnection;

/// A blocking byte source and sink, bound to one bucket.
///
/// Deliberately smaller than [`crate::ops`]: listing, stat-for-display, delete and copy route
/// through their own path. This is only what a *transfer* needs.
pub trait ObjectTransport: Send + Sync {
    /// Size in bytes, for the progress total. Blocking.
    fn size(&self, key: &str) -> Result<u64>;

    /// Read `len` bytes from `offset`. May return fewer at the end of the object; an empty
    /// vec means there was nothing left. Blocking.
    fn read_at(&self, key: &str, offset: u64, len: u64) -> Result<Vec<u8>>;

    /// Whether the key exists, for the overwrite check. Blocking.
    fn exists(&self, key: &str) -> Result<bool>;

    /// Write an object whole.
    ///
    /// Whole rather than streamed because that is what the interface offers — see the note in
    /// `wit/cloud-provider.wit` on why a stream out of a guest is the shape to avoid. The
    /// caller is responsible for not handing over more than [`MAX_WHOLE_WRITE`].
    fn write_whole(&self, key: &str, body: Vec<u8>, content_type: Option<&str>) -> Result<()>;
}

/// Bytes per ranged read.
///
/// Larger than the in-process path's 256 KiB because the costs are different: each read here
/// is a whole HTTP request the guest performs plus a copy across the component boundary, so
/// 256 KiB would mean four thousand round trips for a gigabyte. At 4 MiB the progress ticker
/// still updates several times a second on any connection worth reporting on.
pub const CHUNK: u64 = 4 * 1024 * 1024;

/// The largest object a transfer will hand to [`ObjectTransport::write_whole`].
///
/// The body exists twice at the moment of the call — once here, once in the guest's linear
/// memory — and that memory is 32-bit. Above this, a transfer falls back to the in-process
/// path, which streams. It is a real limit rather than a tuning knob: raising it does not
/// make a 4 GiB upload work.
pub const MAX_WHOLE_WRITE: u64 = 64 * 1024 * 1024;

type Resolver =
    Box<dyn Fn(&CloudConnection, &str) -> Option<Arc<dyn ObjectTransport>> + Send + Sync + 'static>;

static RESOLVER: OnceLock<Resolver> = OnceLock::new();

/// Teach the crate how to find a provider for a connection. Called once, at startup.
pub fn install_resolver<F>(f: F)
where
    F: Fn(&CloudConnection, &str) -> Option<Arc<dyn ObjectTransport>> + Send + Sync + 'static,
{
    if RESOLVER.set(Box::new(f)).is_err() {
        tracing::warn!("cloud: a transport resolver is already installed — ignoring the second");
    }
}

/// Whether anything could resolve at all. Cheap, and the reason a call site with no provider
/// installed never pays for a thread hop.
pub fn is_installed() -> bool {
    RESOLVER.get().is_some()
}

/// Find the provider for this connection, or `None` to use the in-process path.
///
/// **Blocking** — resolving instantiates a module. Prefer [`resolve_off_thread`] from async
/// code.
pub fn resolve(conn: &CloudConnection, bucket: &str) -> Option<Arc<dyn ObjectTransport>> {
    RESOLVER.get()?(conn, bucket)
}

/// [`resolve`], off the runtime's workers.
pub async fn resolve_off_thread(
    conn: &CloudConnection,
    bucket: &str,
) -> Option<Arc<dyn ObjectTransport>> {
    if !is_installed() {
        return None;
    }
    let conn = conn.clone();
    let bucket = bucket.to_string();
    tokio::task::spawn_blocking(move || resolve(&conn, &bucket))
        .await
        .unwrap_or(None)
}

// ── Driving a transport ─────────────────────────────────────────────────────────

/// One object being pulled through a transport, chunk by chunk.
///
/// Owns the arithmetic — offsets, the short final chunk, knowing when there is no more — so
/// the two loops that stream a download differ only in where they put the bytes and how they
/// report progress, which is what actually differs between them. Sharing more than this would
/// mean a callback that has to satisfy both a synchronous ticker and an async aggregate lock,
/// and that is a lot of type for six lines.
pub struct TransportReader {
    transport: Arc<dyn ObjectTransport>,
    key:       String,
    offset:    u64,
    total:     u64,
}

impl TransportReader {
    /// Stat the object and prepare to read it. Off-thread.
    pub async fn open(transport: Arc<dyn ObjectTransport>, key: &str) -> Result<Self> {
        let total = {
            let t = transport.clone();
            let k = key.to_string();
            spawn_blocking_or_err(move || t.size(&k)).await?
        };
        Ok(Self { transport, key: key.to_string(), offset: 0, total })
    }

    /// Total bytes, for the progress denominator.
    pub fn total(&self) -> u64 {
        self.total
    }

    /// The next chunk, or `None` at the end of the object. Off-thread.
    pub async fn next_chunk(&mut self) -> Result<Option<Vec<u8>>> {
        if self.total > 0 && self.offset >= self.total {
            return Ok(None);
        }
        let t = self.transport.clone();
        let key = self.key.clone();
        let at = self.offset;
        let bytes = spawn_blocking_or_err(move || t.read_at(&key, at, CHUNK)).await?;
        if bytes.is_empty() {
            // A zero-length answer ends the read even when `total` disagreed — a store that
            // reported a stale size is a real thing, and looping on it forever is worse than
            // a short file.
            return Ok(None);
        }
        self.offset += bytes.len() as u64;
        Ok(Some(bytes))
    }
}

/// Run one blocking transport call off the runtime's workers.
pub(crate) async fn spawn_blocking_or_err<T, F>(f: F) -> Result<T>
where
    F: FnOnce() -> Result<T> + Send + 'static,
    T: Send + 'static,
{
    match tokio::task::spawn_blocking(f).await {
        Ok(r) => r,
        Err(e) => Err(crate::error::CloudError::Other(format!(
            "cloud provider call did not finish: {e}"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    /// A transport over an in-memory object, recording the ranges it was asked for.
    struct Fake {
        body:  Vec<u8>,
        /// What `size` reports. Separate from `body.len()` so a store that lies can be tested.
        says:  u64,
        asked: Mutex<Vec<(u64, u64)>>,
    }

    impl Fake {
        fn new(len: usize) -> Self {
            Self {
                body:  (0..len).map(|i| (i % 251) as u8).collect(),
                says:  len as u64,
                asked: Mutex::new(Vec::new()),
            }
        }
    }

    impl ObjectTransport for Fake {
        fn size(&self, _key: &str) -> Result<u64> {
            Ok(self.says)
        }
        fn read_at(&self, _key: &str, offset: u64, len: u64) -> Result<Vec<u8>> {
            self.asked.lock().unwrap().push((offset, len));
            let start = (offset as usize).min(self.body.len());
            let end = ((offset + len) as usize).min(self.body.len());
            Ok(self.body[start..end].to_vec())
        }
        fn exists(&self, _key: &str) -> Result<bool> {
            Ok(true)
        }
        fn write_whole(&self, _key: &str, _body: Vec<u8>, _ct: Option<&str>) -> Result<()> {
            Ok(())
        }
    }

    async fn drain(t: Arc<Fake>) -> Vec<u8> {
        let mut rd = TransportReader::open(t, "k").await.unwrap();
        let mut out = Vec::new();
        while let Some(c) = rd.next_chunk().await.unwrap() {
            out.extend_from_slice(&c);
        }
        out
    }

    #[tokio::test]
    async fn an_object_arrives_whole_and_in_order() {
        // Two full chunks and a short one — the case where getting the final offset wrong
        // truncates a file without failing.
        let f = Arc::new(Fake::new((CHUNK * 2 + 17) as usize));
        let got = drain(f.clone()).await;
        assert_eq!(got, f.body);

        let asked = f.asked.lock().unwrap().clone();
        assert_eq!(asked, vec![(0, CHUNK), (CHUNK, CHUNK), (CHUNK * 2, CHUNK)]);
    }

    #[tokio::test]
    async fn a_zero_size_is_read_once_rather_than_trusted() {
        // Zero means two different things — an empty object, and a store that did not say —
        // and only the read distinguishes them. Skipping it would turn every store that
        // reports no size into an empty file, which is the silent half of the pair.
        let f = Arc::new(Fake::new(0));
        assert!(drain(f.clone()).await.is_empty());
        assert_eq!(f.asked.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn a_store_that_overstates_the_size_still_terminates() {
        // A stale size is a real thing. Ending on the empty answer is a short file; looping
        // on the mismatch is a transfer that never finishes.
        let mut f = Fake::new(100);
        f.says = CHUNK * 4;
        let f = Arc::new(f);
        assert_eq!(drain(f.clone()).await.len(), 100);
        assert_eq!(f.asked.lock().unwrap().len(), 2, "one short read, then the empty one");
    }

    #[tokio::test]
    async fn the_reported_total_is_what_the_store_said() {
        // The progress denominator comes from `size`, not from what has been read so far —
        // otherwise a transfer shows 100% at every chunk.
        let f = Arc::new(Fake::new(4096));
        let rd = TransportReader::open(f, "k").await.unwrap();
        assert_eq!(rd.total(), 4096);
    }

    #[tokio::test]
    async fn a_resolver_is_asked_only_once_installed() {
        // Both halves in one test on purpose: the `OnceLock` is process-wide, so a separate
        // "nothing is installed" test would pass or fail on ordering. Nothing else in this
        // crate installs one, so the first half is deterministic here.
        assert!(!is_installed());
        assert!(resolve_off_thread(&sample_conn(), "b").await.is_none());

        install_resolver(|_conn, _bucket| Some(Arc::new(Fake::new(8)) as Arc<dyn ObjectTransport>));
        assert!(is_installed());
        assert!(resolve_off_thread(&sample_conn(), "b").await.is_some());
    }

    fn sample_conn() -> CloudConnection {
        CloudConnection {
            provider:   crate::types::Provider::Gcs,
            config_id:  "c".into(),
            project_id: None,
            gcs:        None,
            s3:         None,
            azblob:     None,
        }
    }
}
