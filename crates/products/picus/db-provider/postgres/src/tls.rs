//! TLS setup.
//!
//! rustls rather than native-tls, so nothing in the workspace links OpenSSL —
//! `rustls-native-certs` then reads the **OS trust store**, which is what makes an
//! internal corporate CA work without Picus shipping a certificate bundle.
//!
//! Plaintext stays a first-class option: an on-prem PostgreSQL on a trusted network
//! is the normal case for the repositories Picus maintains, and a managed cloud
//! database is the one that refuses it.
//!
//! ## A CA of the connection's own
//!
//! Some managed databases sign each instance with its own CA that no OS store contains
//! (Google Cloud SQL's per-instance "server CA"). `tls_ca` points at that PEM file and is
//! trusted **in addition to** the OS store. Such a certificate also names the instance
//! (`project:instance`) rather than the address you connect to, so `tls_verify_name = false`
//! keeps the chain check and drops only the host-name match — PostgreSQL's `verify-ca`.
//! The default stays full verification.

use std::path::Path;
use std::sync::Arc;

use picus_db_api::prelude::{ConnectionSpec, DbError, DbResult};
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::client::WebPkiServerVerifier;
use rustls::pki_types::pem::PemObject;
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::{CertificateError, DigitallySignedStruct, Error as TlsError, SignatureScheme};
use tokio_postgres::NoTls;
use tokio_postgres_rustls::MakeRustlsConnect;

/// `params` key: a PEM file of extra trusted CA certificates.
pub const PARAM_CA: &str = "tls_ca";
/// `params` key: `false` keeps the certificate-chain check and skips the host-name match.
pub const PARAM_VERIFY_NAME: &str = "tls_verify_name";

/// Which connector a session was opened with. Kept so `cancel_query` — which opens
/// a *second* connection to the same server — can use the same transport.
#[derive(Clone)]
pub enum TlsChoice {
    Plain(NoTls),
    Rustls(MakeRustlsConnect),
}

impl TlsChoice {
    /// Build the connector for a connection spec.
    pub fn build(spec: &ConnectionSpec) -> DbResult<Self> {
        if !spec.tls {
            return Ok(Self::Plain(NoTls));
        }
        let ca = spec.params.get(PARAM_CA).map(|p| p.trim()).filter(|p| !p.is_empty());
        let verify_name = spec.params.get(PARAM_VERIFY_NAME).map(|v| v.trim() != "false").unwrap_or(true);
        Ok(Self::Rustls(MakeRustlsConnect::new(client_config(ca.map(Path::new), verify_name)?)))
    }
}

/// The OS trust store, plus `extra_ca` when given.
///
/// A machine with no readable trust store is a real failure, not something to paper
/// over with an empty root set: an empty set would make every certificate invalid
/// and produce a baffling handshake error instead of an actionable one.
fn client_config(extra_ca: Option<&Path>, verify_name: bool) -> DbResult<rustls::ClientConfig> {
    let mut roots = rustls::RootCertStore::empty();
    let loaded = rustls_native_certs::load_native_certs();

    for cert in loaded.certs {
        // A single unparseable certificate in the OS store is not fatal — skip it
        // and keep the rest.
        let _ = roots.add(cert);
    }

    if let Some(path) = extra_ca {
        for cert in read_ca(path)? {
            roots
                .add(cert)
                .map_err(|e| DbError::Connect(format!("{} holds a certificate that cannot be trusted: {e}", path.display())))?;
        }
    } else if roots.is_empty() {
        let detail = loaded
            .errors
            .first()
            .map(|e| e.to_string())
            .unwrap_or_else(|| "the OS trust store is empty".to_string());
        return Err(DbError::Connect(format!("cannot load system certificates: {detail}")));
    }

    let builder = rustls::ClientConfig::builder();
    if verify_name {
        return Ok(builder.with_root_certificates(roots).with_no_client_auth());
    }
    let inner = WebPkiServerVerifier::builder(Arc::new(roots))
        .build()
        .map_err(|e| DbError::Connect(format!("cannot set up certificate checking: {e}")))?;
    Ok(builder
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(IgnoreServerName { inner }))
        .with_no_client_auth())
}

/// The certificates of a PEM file, or the reason it is not one.
fn read_ca(path: &Path) -> DbResult<Vec<CertificateDer<'static>>> {
    let bytes = std::fs::read(path)
        .map_err(|e| DbError::Connect(format!("cannot read the CA certificate {}: {e}", path.display())))?;
    let certs: Vec<CertificateDer<'static>> = CertificateDer::pem_slice_iter(&bytes)
        .collect::<Result<_, _>>()
        .map_err(|e| DbError::Connect(format!("{} is not a PEM certificate file: {e}", path.display())))?;
    if certs.is_empty() {
        return Err(DbError::Connect(format!("{} contains no certificate", path.display())));
    }
    Ok(certs)
}

/// The ordinary chain verification with exactly one failure forgiven: the certificate not being
/// issued for the name connected to. Signature, expiry, issuer and revocation-relevant checks all
/// still run through `inner`.
#[derive(Debug)]
struct IgnoreServerName {
    inner: Arc<WebPkiServerVerifier>,
}

impl ServerCertVerifier for IgnoreServerName {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        intermediates: &[CertificateDer<'_>],
        server_name: &ServerName<'_>,
        ocsp_response: &[u8],
        now: UnixTime,
    ) -> Result<ServerCertVerified, TlsError> {
        match self.inner.verify_server_cert(end_entity, intermediates, server_name, ocsp_response, now) {
            Err(TlsError::InvalidCertificate(
                CertificateError::NotValidForName | CertificateError::NotValidForNameContext { .. },
            )) => Ok(ServerCertVerified::assertion()),
            other => other,
        }
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, TlsError> {
        self.inner.verify_tls12_signature(message, cert, dss)
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, TlsError> {
        self.inner.verify_tls13_signature(message, cert, dss)
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.inner.supported_verify_schemes()
    }
}
