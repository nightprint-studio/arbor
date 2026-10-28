//! Application Default Credentials — finding the file and reading what kind it is.
//!
//! Pure: no network, no environment reads except through the arguments, so every branch is
//! testable. The network half lives in [`crate::token`].
//!
//! ## Where the file is
//!
//! Google's own order, which is what `gcloud` and every client library follow:
//!
//! 1. an explicit path (here: the connection's own setting — one machine, two Google accounts);
//! 2. `GOOGLE_APPLICATION_CREDENTIALS`;
//! 3. the file `gcloud auth application-default login` writes: `%APPDATA%\gcloud\` on Windows,
//!    `~/.config/gcloud/` elsewhere, either moved by `CLOUDSDK_CONFIG`.

use std::path::{Path, PathBuf};

use serde::Deserialize;

pub const ADC_FILE: &str = "application_default_credentials.json";

/// What a credentials file holds, narrowed to the kinds this crate can turn into a token.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Credentials {
    /// `gcloud auth application-default login` — a user's refresh token.
    AuthorizedUser { client_id: String, client_secret: String, refresh_token: String },
    /// A service account's key file: signs a JWT with `private_key` and trades it for a token.
    ServiceAccount { client_email: String, private_key: String, private_key_id: String, token_uri: String },
    /// `gcloud auth application-default login --impersonate-service-account=…`: act as a service
    /// account, authenticating first as `source`.
    Impersonated { url: String, source: Box<Credentials> },
}

/// Why a file could not be used, phrased for the person who has to fix it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdcError(pub String);

impl std::fmt::Display for AdcError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// The environment, as arguments — so a test does not have to set process-wide variables.
#[derive(Debug, Clone, Default)]
pub struct Env {
    pub google_application_credentials: Option<String>,
    pub cloudsdk_config: Option<String>,
    pub appdata: Option<String>,
    pub home: Option<String>,
}

impl Env {
    pub fn from_process() -> Self {
        let get = |k: &str| std::env::var(k).ok().filter(|v| !v.is_empty());
        Self {
            google_application_credentials: get("GOOGLE_APPLICATION_CREDENTIALS"),
            cloudsdk_config: get("CLOUDSDK_CONFIG"),
            appdata: get("APPDATA"),
            home: get("HOME").or_else(|| get("USERPROFILE")),
        }
    }
}

/// The path to read, in Google's order. `None` only when there is nowhere to look at all.
pub fn locate(explicit: Option<&str>, env: &Env, windows: bool) -> Option<PathBuf> {
    if let Some(p) = explicit.map(str::trim).filter(|p| !p.is_empty()) {
        return Some(PathBuf::from(p));
    }
    if let Some(p) = &env.google_application_credentials {
        return Some(PathBuf::from(p));
    }
    let config_dir = match (&env.cloudsdk_config, windows) {
        (Some(dir), _) => PathBuf::from(dir),
        (None, true) => PathBuf::from(env.appdata.as_ref()?).join("gcloud"),
        (None, false) => PathBuf::from(env.home.as_ref()?).join(".config").join("gcloud"),
    };
    Some(config_dir.join(ADC_FILE))
}

/// The instruction every "no usable credentials" message ends with.
const LOGIN_HINT: &str = "run `gcloud auth application-default login`";

#[derive(Deserialize)]
struct Raw {
    #[serde(rename = "type")]
    kind: Option<String>,
    client_id: Option<String>,
    client_secret: Option<String>,
    refresh_token: Option<String>,
    service_account_impersonation_url: Option<String>,
    client_email: Option<String>,
    private_key: Option<String>,
    private_key_id: Option<String>,
    token_uri: Option<String>,
    source_credentials: Option<Box<Raw>>,
}

/// Read a credentials document.
pub fn parse(text: &str, origin: &Path) -> Result<Credentials, AdcError> {
    let raw: Raw = serde_json::from_str(text)
        .map_err(|_| AdcError(format!("{} is not a Google credentials file — {LOGIN_HINT}", origin.display())))?;
    from_raw(raw, origin)
}

fn from_raw(raw: Raw, origin: &Path) -> Result<Credentials, AdcError> {
    match raw.kind.as_deref() {
        Some("authorized_user") => {
            let field = |v: Option<String>, name: &str| {
                v.filter(|s| !s.is_empty())
                    .ok_or_else(|| AdcError(format!("{} has no {name} — {LOGIN_HINT} again", origin.display())))
            };
            Ok(Credentials::AuthorizedUser {
                client_id: field(raw.client_id, "client_id")?,
                client_secret: field(raw.client_secret, "client_secret")?,
                refresh_token: field(raw.refresh_token, "refresh_token")?,
            })
        }
        Some("impersonated_service_account") => {
            let url = raw
                .service_account_impersonation_url
                .filter(|u| !u.is_empty())
                .ok_or_else(|| AdcError(format!("{} names no service account to impersonate", origin.display())))?;
            let source = raw
                .source_credentials
                .ok_or_else(|| AdcError(format!("{} has no source credentials", origin.display())))?;
            Ok(Credentials::Impersonated { url, source: Box::new(from_raw(*source, origin)?) })
        }
        Some("service_account") => {
            let field = |v: Option<String>, name: &str| {
                v.filter(|s| !s.is_empty())
                    .ok_or_else(|| AdcError(format!("{} has no {name} — is it a complete key file?", origin.display())))
            };
            Ok(Credentials::ServiceAccount {
                client_email: field(raw.client_email, "client_email")?,
                private_key: field(raw.private_key, "private_key")?,
                private_key_id: raw.private_key_id.unwrap_or_default(),
                token_uri: raw.token_uri.filter(|u| !u.is_empty()).unwrap_or_else(|| "https://oauth2.googleapis.com/token".to_string()),
            })
        }
        Some("external_account") => Err(AdcError(
            "workload identity federation is not supported yet — use `gcloud auth application-default login`".to_string(),
        )),
        Some(other) => Err(AdcError(format!("{} has an unknown credentials type '{other}'", origin.display()))),
        None => Err(AdcError(format!("{} is not a Google credentials file — {LOGIN_HINT}", origin.display()))),
    }
}

/// The message for "the file is not there" — the most common failure, so the most useful sentence.
pub fn missing(path: &Path) -> AdcError {
    AdcError(format!(
        "no Google credentials at {} — {LOGIN_HINT}, or point the connection at a credentials file",
        path.display()
    ))
}

/// A Cloud SQL IAM login for a service account is its email without `.gserviceaccount.com`.
pub fn iam_login(user: &str) -> String {
    user.trim().strip_suffix(".gserviceaccount.com").unwrap_or(user.trim()).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    const USER: &str = r#"{"type":"authorized_user","client_id":"c","client_secret":"s","refresh_token":"r"}"#;

    #[test]
    fn an_explicit_path_wins_over_everything() {
        let env = Env { google_application_credentials: Some("/env.json".into()), ..Env::default() };
        assert_eq!(locate(Some("/mine.json"), &env, false), Some(PathBuf::from("/mine.json")));
    }

    #[test]
    fn the_environment_variable_beats_the_gcloud_default() {
        let env = Env { google_application_credentials: Some("/env.json".into()), home: Some("/h".into()), ..Env::default() };
        assert_eq!(locate(None, &env, false), Some(PathBuf::from("/env.json")));
    }

    #[test]
    fn the_default_is_appdata_on_windows_and_dot_config_elsewhere() {
        let env = Env { appdata: Some("C:/A".into()), home: Some("/h".into()), ..Env::default() };
        assert_eq!(locate(None, &env, true), Some(PathBuf::from("C:/A").join("gcloud").join(ADC_FILE)));
        assert_eq!(locate(None, &env, false), Some(PathBuf::from("/h").join(".config").join("gcloud").join(ADC_FILE)));
    }

    #[test]
    fn cloudsdk_config_moves_the_default() {
        let env = Env { cloudsdk_config: Some("/cfg".into()), ..Env::default() };
        assert_eq!(locate(None, &env, true), Some(PathBuf::from("/cfg").join(ADC_FILE)));
    }

    #[test]
    fn nowhere_to_look_is_none() {
        assert_eq!(locate(None, &Env::default(), true), None);
        assert_eq!(locate(Some("  "), &Env::default(), false), None);
    }

    #[test]
    fn an_authorized_user_file_is_read() {
        assert!(matches!(parse(USER, Path::new("f")), Ok(Credentials::AuthorizedUser { .. })));
    }

    #[test]
    fn a_missing_refresh_token_says_to_log_in_again() {
        let e = parse(r#"{"type":"authorized_user","client_id":"c","client_secret":"s"}"#, Path::new("f")).unwrap_err();
        assert!(e.0.contains("refresh_token") && e.0.contains("gcloud"));
    }

    #[test]
    fn impersonation_nests_its_source() {
        let text = format!(
            r#"{{"type":"impersonated_service_account","service_account_impersonation_url":"https://x/y:generateAccessToken","source_credentials":{USER}}}"#
        );
        match parse(&text, Path::new("f")) {
            Ok(Credentials::Impersonated { url, source }) => {
                assert!(url.ends_with(":generateAccessToken"));
                assert!(matches!(*source, Credentials::AuthorizedUser { .. }));
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn a_service_account_key_file_is_read() {
        let text = r#"{"type":"service_account","client_email":"sa@p.iam.gserviceaccount.com","private_key":"-----BEGIN PRIVATE KEY-----","private_key_id":"k"}"#;
        match parse(text, Path::new("f")) {
            Ok(Credentials::ServiceAccount { client_email, token_uri, .. }) => {
                assert_eq!(client_email, "sa@p.iam.gserviceaccount.com");
                assert!(token_uri.starts_with("https://oauth2.googleapis.com"));
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn an_incomplete_service_account_key_names_what_is_missing() {
        let e = parse(r#"{"type":"service_account","client_email":"x"}"#, Path::new("f")).unwrap_err();
        assert!(e.0.contains("private_key"));
    }

    #[test]
    fn garbage_is_not_a_credentials_file() {
        assert!(parse("not json", Path::new("f")).is_err());
        assert!(parse("{}", Path::new("f")).is_err());
    }

    #[test]
    fn a_service_account_login_drops_the_domain_suffix() {
        assert_eq!(iam_login("sa@proj.iam.gserviceaccount.com"), "sa@proj.iam");
        assert_eq!(iam_login("me@example.com"), "me@example.com");
    }
}
