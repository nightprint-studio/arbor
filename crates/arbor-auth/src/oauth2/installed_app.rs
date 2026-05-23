//! Installed-app OAuth2 flow (loopback redirect + PKCE).
//!
//! Lifecycle:
//!   1. Caller builds [`InstalledAppFlow`] with provider config.
//!   2. `start().await` binds the loopback listener (fail-fast on port
//!      conflict) and returns `(authorize_url, PendingAuth)`.
//!   3. Caller opens `authorize_url` in the browser AND spawns a task that
//!      awaits `PendingAuth::await_callback()` to receive the
//!      [`TokenResponse`].
//!   4. Caller persists tokens / fires hooks / does provider-specific
//!      follow-ups (e.g. fetch `cloud_resource` for Jira) — none of those
//!      live in this crate.

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

use crate::error::{AuthError, Result};
use crate::html::{default_error, default_success, render_error_template};
use crate::pkce::{code_challenge, code_verifier, extract_param, random_state, urlencoded};
use crate::types::{BodyFormat, TokenResponse, TokenWire};

/// Provider-tailored installed-app flow config.
///
/// `client_secret` is `Option` because public clients (Atlassian 3LO,
/// many GitHub installations) don't supply one. Where required (Google),
/// pass `Some(secret)`.
pub struct InstalledAppFlow {
    pub auth_url:               String,
    pub token_url:              String,
    pub client_id:              String,
    pub client_secret:          Option<String>,
    /// Space-separated scopes — URL-encoded internally for the authorize URL.
    pub scope:                  String,
    pub redirect_port:          u16,
    /// Extra query params appended to the authorize URL. Values are
    /// URL-encoded internally. Examples: `("access_type", "offline")` for
    /// Google, `("audience", "api.atlassian.com")` for Atlassian.
    pub extra_authorize_params: Vec<(String, String)>,
    /// How the token endpoint expects the request body — varies per
    /// provider; see [`BodyFormat`] docs.
    pub token_request_format:   BodyFormat,
    /// Human-friendly provider name (Google Cloud Storage, Jira, GitHub …)
    /// — interpolated into the default HTML pages so the user knows which
    /// integration just connected.
    pub provider_label:         String,
    /// Override the default success page. `None` → branded Arbor default.
    pub success_html:           Option<String>,
    /// Override the default error template. Must contain the literal
    /// `__MSG__` token where the failure reason will be substituted.
    /// `None` → branded Arbor default.
    pub error_html_template:    Option<String>,
}

/// Outcome of [`InstalledAppFlow::start`]: the listener is bound + the
/// authorize URL is built; awaiting [`PendingAuth::await_callback`] drives
/// the rest of the flow.
pub struct PendingAuth {
    listener:        TcpListener,
    verifier:        String,
    expected_state:  String,
    client_id:       String,
    client_secret:   Option<String>,
    redirect_uri:    String,
    token_url:       String,
    token_format:    BodyFormat,
    provider_label:  String,
    success_html:    Option<String>,
    error_template:  Option<String>,
}

impl InstalledAppFlow {
    /// Bind the loopback listener + build the authorize URL. Returns the
    /// URL the caller opens in the browser plus a [`PendingAuth`] handle.
    pub async fn start(self) -> Result<(String, PendingAuth)> {
        let verifier  = code_verifier();
        let challenge = code_challenge(&verifier);
        let state     = random_state();

        let listener = TcpListener::bind(("127.0.0.1", self.redirect_port))
            .await
            .map_err(|e| AuthError::Transport(format!(
                "cannot bind OAuth callback server on port {}: {e}\n\
                 Make sure nothing else is using that port.",
                self.redirect_port
            )))?;

        let redirect_uri = format!("http://127.0.0.1:{}/callback", self.redirect_port);

        // Build authorize URL. The standard params come first, then the
        // provider-specific extras get appended in declaration order.
        let mut auth_url = format!(
            "{base}\
             ?client_id={cid}\
             &response_type=code\
             &redirect_uri={redir}\
             &scope={scope}\
             &state={state}\
             &code_challenge={challenge}\
             &code_challenge_method=S256",
            base      = self.auth_url,
            cid       = urlencoded(&self.client_id),
            redir     = urlencoded(&redirect_uri),
            scope     = urlencoded(&self.scope),
            state     = urlencoded(&state),
            challenge = challenge,
        );
        for (k, v) in &self.extra_authorize_params {
            auth_url.push('&');
            auth_url.push_str(&urlencoded(k));
            auth_url.push('=');
            auth_url.push_str(&urlencoded(v));
        }

        let pending = PendingAuth {
            listener,
            verifier,
            expected_state: state,
            client_id:      self.client_id,
            client_secret:  self.client_secret,
            redirect_uri,
            token_url:      self.token_url,
            token_format:   self.token_request_format,
            provider_label: self.provider_label,
            success_html:   self.success_html,
            error_template: self.error_html_template,
        };

        Ok((auth_url, pending))
    }
}

impl PendingAuth {
    /// Block until the browser hits `/callback`, then exchange the code for
    /// tokens. Serves the success / error HTML in-line so the user sees a
    /// branded page before the tab can be closed.
    pub async fn await_callback(self) -> Result<TokenResponse> {
        let (mut stream, _) = self.listener.accept().await
            .map_err(|e| AuthError::Transport(format!("accept: {e}")))?;

        let mut buf = vec![0u8; 8192];
        let n = stream.read(&mut buf).await
            .map_err(|e| AuthError::Transport(format!("read: {e}")))?;
        let request = String::from_utf8_lossy(&buf[..n]).to_string();

        // Provider returned an error (e.g. user clicked "Deny").
        if let Some(err) = extract_param(&request, "error") {
            let desc = extract_param(&request, "error_description").unwrap_or_default();
            let msg = if desc.is_empty() { err.clone() } else { format!("{err}: {desc}") };
            send_error_html(&mut stream, &self.provider_label, &msg, self.error_template.as_deref()).await;
            return Err(AuthError::Provider(msg));
        }

        let code = extract_param(&request, "code")
            .ok_or_else(|| AuthError::Provider("no 'code' in OAuth callback".into()))?;
        let recv_state = extract_param(&request, "state")
            .ok_or_else(|| AuthError::Provider("no 'state' in OAuth callback".into()))?;
        if recv_state != self.expected_state {
            send_error_html(
                &mut stream, &self.provider_label,
                "State mismatch — possible CSRF, authorization rejected.",
                self.error_template.as_deref(),
            ).await;
            return Err(AuthError::StateMismatch);
        }

        // Exchange — serve the HTML only after we know the outcome so the
        // user doesn't see "Connected" if the token call then 500s.
        let result = exchange_code(
            &self.token_url,
            &self.client_id,
            self.client_secret.as_deref(),
            &code,
            &self.verifier,
            &self.redirect_uri,
            self.token_format,
        ).await;

        match result {
            Ok(token) => {
                send_success_html(&mut stream, &self.provider_label, self.success_html.as_deref()).await;
                Ok(token)
            }
            Err(e) => {
                send_error_html(&mut stream, &self.provider_label, &e.to_string(), self.error_template.as_deref()).await;
                Err(e)
            }
        }
    }
}

// ── internals ──────────────────────────────────────────────────────────────

async fn exchange_code(
    token_url:     &str,
    client_id:     &str,
    client_secret: Option<&str>,
    code:          &str,
    verifier:      &str,
    redirect_uri:  &str,
    format:        BodyFormat,
) -> Result<TokenResponse> {
    let client = reqwest::Client::new();
    // Accept JSON is harmless for providers that always reply JSON (Atlassian,
    // GitLab, Linear, Google) and required for the ones that default to
    // form-encoded text otherwise (GitHub).
    let req = client.post(token_url).header("Accept", "application/json");

    let req = match format {
        BodyFormat::Form => {
            let mut form: Vec<(&str, &str)> = vec![
                ("grant_type",    "authorization_code"),
                ("client_id",     client_id),
                ("code",          code),
                ("redirect_uri",  redirect_uri),
                ("code_verifier", verifier),
            ];
            if let Some(secret) = client_secret {
                if !secret.is_empty() { form.push(("client_secret", secret)); }
            }
            req.form(&form)
        }
        BodyFormat::Json => {
            let mut body = serde_json::json!({
                "grant_type":    "authorization_code",
                "client_id":     client_id,
                "code":          code,
                "redirect_uri":  redirect_uri,
                "code_verifier": verifier,
            });
            if let Some(secret) = client_secret {
                if !secret.is_empty() {
                    body.as_object_mut().unwrap()
                        .insert("client_secret".into(), serde_json::Value::String(secret.into()));
                }
            }
            req.json(&body)
        }
    };

    let resp = req.send().await
        .map_err(|e| AuthError::Http(format!("token exchange request: {e}")))?;
    let status = resp.status();
    if !status.is_success() {
        let body = resp.text().await.unwrap_or_default();
        return Err(AuthError::HttpStatus { status: status.as_u16(), body });
    }
    let raw: serde_json::Value = resp.json().await
        .map_err(|e| AuthError::Http(format!("token response decode: {e}")))?;
    let wire: TokenWire = serde_json::from_value(raw.clone())
        .map_err(|e| AuthError::Http(format!("token response shape: {e}")))?;
    Ok(TokenResponse {
        access_token:  wire.access_token,
        refresh_token: wire.refresh_token,
        expires_in:    wire.expires_in,
        raw,
    })
}

async fn send_html(stream: &mut TcpStream, body: &str) {
    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        body.len(), body
    );
    let _ = stream.write_all(response.as_bytes()).await;
}

async fn send_success_html(stream: &mut TcpStream, provider: &str, custom: Option<&str>) {
    let body = match custom {
        Some(s) => s.to_string(),
        None    => default_success(provider),
    };
    send_html(stream, &body).await;
}

async fn send_error_html(stream: &mut TcpStream, provider: &str, message: &str, custom_template: Option<&str>) {
    let body = match custom_template {
        Some(t) => render_error_template(t, message),
        None    => default_error(provider, message),
    };
    send_html(stream, &body).await;
}
