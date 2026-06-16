//! Router — maps a FE `invoke` to the right backend over [`arbor_ipc`].
//!
//! The shell registers one [`BrokerClient`] per product (keyed by id, e.g.
//! `"corvus"` / `"merula"` / `"sitta"`) and forwards `(product, method, params)`
//! to it. In-process today (a `LoopbackBroker`), pipe/socket-backed later — the
//! router is unchanged across the flip because it only sees `BrokerClient`.
//!
//! M1c scope: the registry + dispatch. Relaying backend push events to the FE
//! as Tauri events folds in when the shell takes over the WebView (M3).

use std::collections::HashMap;
use std::sync::Arc;

use arbor_ipc::prelude::{BrokerClient, Bytes, IpcError};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum RouterError {
    #[error("no backend registered for product '{0}'")]
    UnknownProduct(String),
    #[error(transparent)]
    Ipc(#[from] IpcError),
}

pub type Result<T> = std::result::Result<T, RouterError>;

/// Routes FE commands to per-product backends.
#[derive(Default)]
pub struct Router {
    backends: HashMap<String, Arc<dyn BrokerClient>>,
}

impl Router {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register (or replace) the backend client for `product`.
    pub fn register(&mut self, product: impl Into<String>, client: Arc<dyn BrokerClient>) {
        self.backends.insert(product.into(), client);
    }

    /// Forward `method` + `params` to `product`'s backend.
    pub fn call(&self, product: &str, method: &str, params: Bytes) -> Result<Bytes> {
        let client = self
            .backends
            .get(product)
            .ok_or_else(|| RouterError::UnknownProduct(product.to_string()))?;
        Ok(client.call(method, params)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use arbor_ipc::prelude::LoopbackBroker;

    #[test]
    fn routes_to_registered_backend() {
        let mut router = Router::new();
        router.register("corvus", Arc::new(LoopbackBroker::ping_only()));
        let out = router.call("corvus", "ping", b"hi".to_vec()).expect("routed");
        assert_eq!(out, b"hi".to_vec());
    }

    #[test]
    fn unknown_product_errors() {
        let router = Router::new();
        let err = router.call("nope", "ping", Vec::new()).unwrap_err();
        assert!(matches!(err, RouterError::UnknownProduct(_)));
    }
}
