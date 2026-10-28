//! `picus-auth-google` — Google IAM database authentication.
//!
//! Cloud SQL (PostgreSQL) can log a database user in with a Google identity instead of a stored
//! password: the "password" is a short-lived OAuth access token, and the user is the identity's
//! email. There is no browser step inside Picus — the sign-in already happened when the person ran
//! `gcloud auth application-default login`, which left a credentials file on the machine; this crate
//! reads it and mints a fresh token for every connect attempt.
//!
//! What this is **not**: Windows/Kerberos domain login (that is the OS authenticating to the
//! server, a different mechanism) and workload identity federation. Both say so in the error
//! instead of failing obscurely. A service-account key file is supported: its JWT is signed here.
//!
//! ## Public API: use the [`prelude`]

pub mod adc;
pub mod prelude;
pub mod token;

use async_trait::async_trait;
use picus_db_api::prelude::{
    AuthMethod, AuthMethodDescriptor, ConnectionField, ConnectionSpec, DbError, DbResult, EngineKind, Secret,
};

/// The method's id — what a connection stores in `params["auth"]`.
pub const METHOD_ID: &str = "google-iam";
/// `params` key: a credentials file to use instead of the machine's default.
pub const PARAM_CREDENTIALS: &str = "google_credentials";
/// `params` key: a service account to act as.
pub const PARAM_IMPERSONATE: &str = "google_impersonate";

/// Google IAM login for PostgreSQL on Cloud SQL.
#[derive(Default)]
pub struct GoogleIam;

impl GoogleIam {
    pub fn new() -> Self {
        Self
    }
}

fn param<'a>(spec: &'a ConnectionSpec, key: &str) -> Option<&'a str> {
    spec.params.get(key).map(|v| v.trim()).filter(|v| !v.is_empty())
}

#[async_trait]
impl AuthMethod for GoogleIam {
    fn descriptor(&self) -> AuthMethodDescriptor {
        AuthMethodDescriptor {
            id: METHOD_ID.to_string(),
            label: "Google IAM (Cloud SQL)".to_string(),
            help: "Signs in with the Google account from `gcloud auth application-default login` — \
                   no password is stored. The user is the IAM email."
                .to_string(),
            engines: vec![EngineKind::Postgres],
            // Cloud SQL refuses a token over a plaintext connection.
            requires_tls: true,
            fields: vec![
                ConnectionField::text(PARAM_CREDENTIALS, "Credentials file")
                    .optional()
                    .with_placeholder("default: the gcloud sign-in")
                    .with_help("Only to use a different Google account than the one gcloud is signed in with."),
                ConnectionField::text(PARAM_IMPERSONATE, "Impersonate service account")
                    .optional()
                    .with_placeholder("sa@project.iam.gserviceaccount.com")
                    .with_help("Log in as this service account. Your account needs Service Account Token Creator on it."),
            ],
        }
    }

    fn login_user(&self, spec: &ConnectionSpec) -> String {
        // With an impersonation the database user is the service account, whatever the form says.
        let user = param(spec, PARAM_IMPERSONATE).unwrap_or(&spec.user);
        adc::iam_login(user)
    }

    async fn credential(&self, spec: &ConnectionSpec) -> DbResult<Secret> {
        let fail = |m: String| DbError::AuthFailed(m);

        let env = adc::Env::from_process();
        let path = adc::locate(param(spec, PARAM_CREDENTIALS), &env, cfg!(windows)).ok_or_else(|| {
            fail("cannot tell where the Google credentials are — run `gcloud auth application-default login`".to_string())
        })?;
        // A blocking read of a few hundred bytes: not worth an async filesystem, and the connect
        // path already runs on its own task.
        let text = std::fs::read_to_string(&path).map_err(|e| match e.kind() {
            std::io::ErrorKind::NotFound => fail(adc::missing(&path).0),
            _ => fail(format!("cannot read {}: {e}", path.display())),
        })?;
        let creds = adc::parse(&text, &path).map_err(|e| fail(e.0))?;

        let http = token::client().map_err(fail)?;
        let access = token::sql_login_token(&http, &creds, param(spec, PARAM_IMPERSONATE))
            .await
            .map_err(fail)?;
        Ok(Secret::new(access))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn spec(user: &str, params: &[(&str, &str)]) -> ConnectionSpec {
        ConnectionSpec {
            id: "c".into(),
            name: "c".into(),
            alias: String::new(),
            engine: EngineKind::Postgres,
            host: "h".into(),
            port: 5432,
            database: "d".into(),
            user: user.into(),
            schema: String::new(),
            color_idx: 0,
            read_only: false,
            tls: true,
            script_root: None,
            params: params.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect::<BTreeMap<_, _>>(),
        }
    }

    #[test]
    fn the_login_is_the_form_user_without_the_service_account_suffix() {
        assert_eq!(GoogleIam.login_user(&spec("sa@p.iam.gserviceaccount.com", &[])), "sa@p.iam");
        assert_eq!(GoogleIam.login_user(&spec("me@example.com", &[])), "me@example.com");
    }

    #[test]
    fn impersonation_overrides_the_form_user() {
        let s = spec("me@example.com", &[(PARAM_IMPERSONATE, "sa@p.iam.gserviceaccount.com")]);
        assert_eq!(GoogleIam.login_user(&s), "sa@p.iam");
    }

    #[test]
    fn the_descriptor_demands_tls_and_names_its_fields() {
        let d = GoogleIam.descriptor();
        assert!(d.requires_tls);
        assert!(d.fields.iter().any(|f| f.id == PARAM_CREDENTIALS) && d.fields.iter().any(|f| f.id == PARAM_IMPERSONATE));
    }
}
