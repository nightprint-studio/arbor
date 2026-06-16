use serde::{Deserialize, Serialize};

// ── Domain ────────────────────────────────────────────────────────────────

/// Which family a connectable provider belongs to. The frontend uses it only
/// to decide which settings section a descriptor belongs to — never to branch
/// on a specific provider.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderDomain {
    IssueTracker,
    GitHost,
}

// ── Descriptor ──────────────────────────────────────────────────────────────

/// Everything the frontend needs to render a provider's connect UI and drive
/// its auth flow — without knowing anything provider-specific. The backend is
/// the single source of truth; the UI is a pure interpreter of this struct.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderDescriptor {
    /// Stable provider id (e.g. `"linear"`, `"jira"`, `"github"`, `"gitlab"`),
    /// used as the routing key for every generic IPC call + OAuth event.
    pub id: String,
    pub domain: ProviderDomain,
    pub display_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Brand icon id the frontend resolves to a logo (e.g. `"linear"`).
    pub icon: String,
    /// Optional brand accent (CSS color or var) for the connect tile.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub brand_color: Option<String>,
    /// Display order matters — `auth_methods[0]` is the recommended/default.
    pub auth_methods: Vec<AuthMethod>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthMethod {
    pub id: String,
    pub label: String,
    pub kind: AuthMethodKind,
}

/// Tagged union (`tag = "type"`): an OAuth button or a credential form.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum AuthMethodKind {
    #[serde(rename = "oauth")]
    OAuth { flow: OAuthFlow },
    Fields {
        fields: Vec<AuthField>,
        /// Hint lines shown under the form; the FE picks by `when` rule.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        hints: Vec<FieldHint>,
    },
}

/// How an OAuth method completes — tells the FE which affordance to render
/// after `start_oauth`.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OAuthFlow {
    /// Device Authorization Grant (RFC 8628): show user code + verification URL.
    Device,
    /// Authorization Code + PKCE: open the returned URL in the browser.
    Redirect,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthField {
    pub key: String,
    pub label: String,
    pub widget: FieldWidget,
    /// Always required.
    #[serde(default)]
    pub required: bool,
    /// Required *additionally* only when this rule matches the current field
    /// values — lets the backend express "email required only for Jira Cloud"
    /// with zero provider-specific logic in the UI.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub required_when: Option<FieldRule>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub placeholder: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FieldWidget {
    Text,
    Secret,
    Url,
}

// ── Declarative rules (the FE interprets these; no provider logic in the UI) ──

/// A predicate over another field's current value, evaluated client-side.
/// Keeps every conditional form behavior in the backend-authored descriptor.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FieldRule {
    /// The field key whose current value is tested.
    pub field: String,
    pub matches: FieldMatch,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "op", content = "value", rename_all = "camelCase")]
pub enum FieldMatch {
    /// The field has a non-empty (trimmed) value.
    NonEmpty,
    EndsWith(String),
    Equals(String),
    Contains(String),
}

/// A hint line under a fields form. `when = None` is the default/fallback; with
/// a rule, the FE shows the first hint whose rule matches the current values
/// (falling back to the `when = None` hint, if any).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FieldHint {
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub when: Option<FieldRule>,
}

// ── Auth status + OAuth start (generic IPC return shapes) ─────────────────────

/// Authenticated state echoed to the settings UI for any provider.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthStatus {
    pub authenticated: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user: Option<ProviderUserInfo>,
    /// A user-facing sub-label for the connected account — e.g. a Jira tenant
    /// domain or a self-hosted git host. `None` for fixed-endpoint providers.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub account_label: Option<String>,
    /// Which `AuthMethod.id` is currently active (`"oauth"`, `"pat"`, …).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub method: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderUserInfo {
    pub display_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub avatar_url: Option<String>,
}

/// What a generic `*_provider_start_oauth` returns so the FE renders the right
/// next step. Completion is signalled out-of-band by the single Tauri event
/// `arbor://provider-oauth-done` whose payload carries the provider `id`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum OAuthStart {
    /// Open `url` in the browser (Authorization Code + PKCE).
    Redirect { url: String },
    /// Show `user_code` + `verification_uri` (Device Authorization Grant).
    Device {
        user_code: String,
        verification_uri: String,
        expires_in: u64,
        interval: u64,
    },
}
