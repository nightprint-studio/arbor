//! [`AuthMethod`] — how a connection proves who it is *other than* by a stored password.
//!
//! A stored password is the default and needs no method. Managed databases increasingly want
//! something else: a short-lived token minted from a cloud identity (Google IAM for Cloud SQL, AWS
//! RDS IAM, Azure AD). Each of those is a method here, registered once and chosen per connection
//! through `params["auth"]` — so a new one is a crate and a `.with(...)` at boot, and no handler,
//! driver or form changes.
//!
//! The contract is deliberately narrow: a method turns a [`ConnectionSpec`] into the value the
//! driver sends where it would have sent the password. It never opens a socket to the database and
//! never persists anything; the token lives in a [`Secret`] for the length of one connect attempt.

use std::collections::BTreeMap;
use std::sync::Arc;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::connection::ConnectionSpec;
use crate::descriptor::ConnectionField;
use crate::error::{DbError, DbResult};
use crate::kind::EngineKind;
use crate::secret::Secret;

/// The `params` key that selects a method. Absent, empty or [`PASSWORD`] mean "a stored password".
pub const AUTH_PARAM: &str = "auth";
/// The built-in choice: the password in Arbor's keychain.
pub const PASSWORD: &str = "password";

/// What the connection form needs to offer a method — pure data, so the form never names a provider.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthMethodDescriptor {
    pub id: String,
    pub label: String,
    /// One or two sentences: what signs in, and where the sign-in comes from.
    pub help: String,
    /// Engines it can authenticate to. A method listed for the wrong engine is not offered.
    pub engines: Vec<EngineKind>,
    /// Whether the server refuses this credential over a plaintext connection.
    pub requires_tls: bool,
    /// Extra fields the method asks for. Their ids land in the spec's `params`.
    pub fields: Vec<ConnectionField>,
}

/// One way of authenticating without a stored password.
#[async_trait]
pub trait AuthMethod: Send + Sync {
    fn descriptor(&self) -> AuthMethodDescriptor;

    /// The database login the credential is for. Defaults to the form's user; a method overrides it
    /// when the identity decides the login (an impersonated service account, a mangled email).
    fn login_user(&self, spec: &ConnectionSpec) -> String {
        spec.user.clone()
    }

    /// A credential for one connect attempt. Called every time: tokens are short-lived and the
    /// connection they open outlives them.
    async fn credential(&self, spec: &ConnectionSpec) -> DbResult<Secret>;
}

/// What a method resolved to: the spec to connect with and the credential to present.
pub struct Resolved {
    pub spec: ConnectionSpec,
    pub credential: Secret,
}

/// Method id → implementation.
#[derive(Default, Clone)]
pub struct AuthRegistry {
    by_id: BTreeMap<String, Arc<dyn AuthMethod>>,
}

impl AuthRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&mut self, method: Arc<dyn AuthMethod>) {
        self.by_id.insert(method.descriptor().id, method);
    }

    /// Builder form, for the registry assembled at boot.
    pub fn with(mut self, method: Arc<dyn AuthMethod>) -> Self {
        self.register(method);
        self
    }

    pub fn descriptors(&self) -> Vec<AuthMethodDescriptor> {
        self.by_id.values().map(|m| m.descriptor()).collect()
    }

    /// The method a spec asks for, or `None` for a stored password.
    pub fn method_of(spec: &ConnectionSpec) -> Option<&str> {
        spec.params
            .get(AUTH_PARAM)
            .map(|v| v.trim())
            .filter(|v| !v.is_empty() && *v != PASSWORD)
    }

    /// Resolve `spec`'s authentication. `Ok(None)` for a stored password — the caller then asks the
    /// keychain as before. Everything that can be wrong with the *choice* is refused here, in words:
    /// an unknown method (a connection file from a build that had more), the wrong engine, a missing
    /// TLS requirement.
    pub async fn resolve(&self, spec: &ConnectionSpec) -> DbResult<Option<Resolved>> {
        let Some(id) = Self::method_of(spec) else { return Ok(None) };
        let method = self.by_id.get(id).ok_or_else(|| {
            DbError::AuthFailed(format!("this connection uses the sign-in method '{id}', which this build does not have"))
        })?;
        let descriptor = method.descriptor();
        if !descriptor.engines.contains(&spec.engine) {
            return Err(DbError::AuthFailed(format!("{} cannot sign in to {}", descriptor.label, spec.engine)));
        }
        if descriptor.requires_tls && !spec.tls {
            return Err(DbError::AuthFailed(format!(
                "{} sends a token, which the server refuses over a plaintext connection — turn on \"Require TLS\"",
                descriptor.label
            )));
        }
        let credential = method.credential(spec).await?;
        let mut spec = spec.clone();
        spec.user = method.login_user(&spec);
        Ok(Some(Resolved { spec, credential }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fixed;

    #[async_trait]
    impl AuthMethod for Fixed {
        fn descriptor(&self) -> AuthMethodDescriptor {
            AuthMethodDescriptor {
                id: "fixed".into(),
                label: "Fixed".into(),
                help: String::new(),
                engines: vec![EngineKind::Postgres],
                requires_tls: true,
                fields: vec![],
            }
        }
        fn login_user(&self, _: &ConnectionSpec) -> String {
            "mapped".into()
        }
        async fn credential(&self, _: &ConnectionSpec) -> DbResult<Secret> {
            Ok(Secret::new("tok".into()))
        }
    }

    fn spec(auth: Option<&str>, tls: bool, engine: EngineKind) -> ConnectionSpec {
        ConnectionSpec {
            id: "c".into(),
            name: "c".into(),
            alias: String::new(),
            engine,
            host: "h".into(),
            port: 1,
            database: "d".into(),
            user: "u".into(),
            schema: String::new(),
            color_idx: 0,
            read_only: false,
            tls,
            script_root: None,
            params: auth.map(|a| (AUTH_PARAM.to_string(), a.to_string())).into_iter().collect(),
        }
    }

    fn block_on<F: std::future::Future>(f: F) -> F::Output {
        // The futures here never yield, so a no-op waker polled once is enough — no runtime needed.
        use std::task::{Context, Poll, Wake, Waker};
        struct Noop;
        impl Wake for Noop {
            fn wake(self: Arc<Self>) {}
        }
        let waker = Waker::from(Arc::new(Noop));
        let mut cx = Context::from_waker(&waker);
        let mut f = Box::pin(f);
        match f.as_mut().poll(&mut cx) {
            Poll::Ready(v) => v,
            Poll::Pending => panic!("future yielded"),
        }
    }

    #[test]
    fn a_password_connection_resolves_to_nothing() {
        let reg = AuthRegistry::new().with(Arc::new(Fixed));
        assert!(block_on(reg.resolve(&spec(None, false, EngineKind::Postgres))).unwrap().is_none());
        assert!(block_on(reg.resolve(&spec(Some("password"), false, EngineKind::Postgres))).unwrap().is_none());
    }

    #[test]
    fn a_method_supplies_the_credential_and_the_login() {
        let reg = AuthRegistry::new().with(Arc::new(Fixed));
        let r = block_on(reg.resolve(&spec(Some("fixed"), true, EngineKind::Postgres))).unwrap().unwrap();
        assert_eq!(r.credential.expose(), "tok");
        assert_eq!(r.spec.user, "mapped");
    }

    #[test]
    fn the_choice_is_refused_in_words_when_it_cannot_work() {
        let reg = AuthRegistry::new().with(Arc::new(Fixed));
        let unknown = block_on(reg.resolve(&spec(Some("nope"), true, EngineKind::Postgres))).err().unwrap();
        assert!(unknown.to_string().contains("'nope'"));
        let plaintext = block_on(reg.resolve(&spec(Some("fixed"), false, EngineKind::Postgres))).err().unwrap();
        assert!(plaintext.to_string().contains("Require TLS"));
        let wrong_engine = block_on(reg.resolve(&spec(Some("fixed"), true, EngineKind::Oracle))).err().unwrap();
        assert!(wrong_engine.to_string().contains("cannot sign in"));
    }
}
