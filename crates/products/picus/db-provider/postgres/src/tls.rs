//! TLS setup.
//!
//! rustls rather than native-tls, so nothing in the workspace links OpenSSL —
//! `rustls-native-certs` then reads the **OS trust store**, which is what makes an
//! internal corporate CA work without Picus shipping a certificate bundle.
//!
//! Plaintext stays a first-class option: an on-prem PostgreSQL on a trusted network
//! is the normal case for the repositories Picus maintains, and a managed cloud
//! database is the one that refuses it.

use picus_db_api::prelude::{DbError, DbResult};
use tokio_postgres::NoTls;
use tokio_postgres_rustls::MakeRustlsConnect;

/// Which connector a session was opened with. Kept so `cancel_query` — which opens
/// a *second* connection to the same server — can use the same transport.
#[derive(Clone)]
pub enum TlsChoice {
    Plain(NoTls),
    Rustls(MakeRustlsConnect),
}

impl TlsChoice {
    /// Build the connector for a connection spec.
    pub fn build(tls: bool) -> DbResult<Self> {
        if !tls {
            return Ok(Self::Plain(NoTls));
        }
        Ok(Self::Rustls(MakeRustlsConnect::new(client_config()?)))
    }
}

/// A rustls client config trusting the OS store.
///
/// A machine with no readable trust store is a real failure, not something to paper
/// over with an empty root set: an empty set would make every certificate invalid
/// and produce a baffling handshake error instead of an actionable one.
fn client_config() -> DbResult<rustls::ClientConfig> {
    let mut roots = rustls::RootCertStore::empty();
    let loaded = rustls_native_certs::load_native_certs();

    for cert in loaded.certs {
        // A single unparseable certificate in the OS store is not fatal — skip it
        // and keep the rest.
        let _ = roots.add(cert);
    }

    if roots.is_empty() {
        let detail = loaded
            .errors
            .first()
            .map(|e| e.to_string())
            .unwrap_or_else(|| "the OS trust store is empty".to_string());
        return Err(DbError::Connect(format!("cannot load system certificates: {detail}")));
    }

    Ok(rustls::ClientConfig::builder().with_root_certificates(roots).with_no_client_auth())
}
