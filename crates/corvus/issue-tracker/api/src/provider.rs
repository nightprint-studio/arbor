//! Self-description a tracker exposes to the frontend, plus the shared
//! auth-status and issue-creation shapes.
//!
//! A provider declares **what the FE must render** to connect it — which auth
//! methods exist, which fields each needs, which widget renders each field, and
//! the brand icon — so the settings UI is generic: add a provider, it shows up
//! with the right form, no bespoke Svelte per tracker.

use serde::{Deserialize, Serialize};

use crate::types::IssueUser;

/// Everything the FE needs to list and connect a tracker.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderDescriptor {
    /// Stable id used as the registry key and in repo config (e.g. `"linear"`).
    pub id: String,
    /// Human label (e.g. `"Linear"`).
    pub display_name: String,
    /// Brand icon id the FE's `BrandIcon` resolves (e.g. `"linear"`).
    pub icon: String,
    /// The ways a user can authenticate this tracker, in display order.
    pub auth_methods: Vec<AuthMethod>,
}

/// One way to connect a tracker (OAuth button, or a field form like a PAT).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthMethod {
    /// Stable id (e.g. `"oauth"`, `"pat"`, `"basic"`).
    pub id: String,
    /// Button / section label (e.g. `"Connect with Linear"`, `"API key"`).
    pub label: String,
    /// What the FE renders for this method.
    pub kind: AuthMethodKind,
}

/// The render shape of an auth method.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum AuthMethodKind {
    /// A single "connect" button that kicks off the provider's OAuth flow.
    #[serde(rename = "oauth")]
    OAuth,
    /// A form of fields the user fills in (PAT, email+token+domain, …).
    Fields { fields: Vec<AuthField> },
}

/// One input in a [`AuthMethodKind::Fields`] form.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthField {
    /// Key the FE returns the value under (e.g. `"token"`, `"email"`).
    pub key: String,
    /// Field label.
    pub label: String,
    /// Which widget renders the field.
    pub widget: FieldWidget,
    /// Whether the FE blocks submit until it's filled.
    pub required: bool,
    /// Optional placeholder / hint.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub placeholder: Option<String>,
}

/// The widget the FE uses for an [`AuthField`].
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FieldWidget {
    /// Plain single-line text.
    Text,
    /// Masked secret (token / password).
    Secret,
    /// URL / host input.
    Url,
}

/// Provider-agnostic auth status (superset of the per-provider shapes).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthStatus {
    pub authenticated: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user: Option<IssueUser>,
    /// Tenant host where it applies (Jira); `None` for single-tenant trackers.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub domain: Option<String>,
    /// Which auth method is active (`"oauth"` | `"pat"` | `"basic"`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auth_method: Option<String>,
}

/// Fields for creating an issue — the superset across trackers; an impl uses
/// what it supports and ignores the rest.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewIssue {
    pub title: String,
    pub description: Option<String>,
    /// Linear team id / Jira project key — the container the issue lands in.
    pub team_id: Option<String>,
    pub status_id: Option<String>,
    pub assignee_id: Option<String>,
    pub label_ids: Vec<String>,
    pub priority: Option<u32>,
    pub project_id: Option<String>,
    pub milestone_id: Option<String>,
    pub due_date: Option<String>,
    pub estimate: Option<f64>,
    /// Jira issue type (`"Bug"`, `"Task"`, …); ignored by trackers without types.
    pub issue_type: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn descriptor_serializes_with_fe_contract_shape() {
        let d = ProviderDescriptor {
            id: "linear".into(),
            display_name: "Linear".into(),
            icon: "linear".into(),
            auth_methods: vec![
                AuthMethod { id: "oauth".into(), label: "Connect with Linear".into(), kind: AuthMethodKind::OAuth },
                AuthMethod {
                    id: "pat".into(),
                    label: "API key".into(),
                    kind: AuthMethodKind::Fields {
                        fields: vec![AuthField {
                            key: "token".into(),
                            label: "API key".into(),
                            widget: FieldWidget::Secret,
                            required: true,
                            placeholder: None,
                        }],
                    },
                },
            ],
        };
        let v: serde_json::Value = serde_json::from_str(&serde_json::to_string(&d).unwrap()).unwrap();

        // camelCase keys for the FE.
        assert_eq!(v["displayName"], "Linear");
        assert_eq!(v["authMethods"][0]["kind"]["type"], "oauth"); // tagged enum
        assert_eq!(v["authMethods"][1]["kind"]["type"], "fields");
        assert_eq!(v["authMethods"][1]["kind"]["fields"][0]["widget"], "secret");
        // placeholder omitted when None.
        assert!(v["authMethods"][1]["kind"]["fields"][0].get("placeholder").is_none());
    }

    #[test]
    fn descriptor_round_trips() {
        let d = ProviderDescriptor {
            id: "jira".into(),
            display_name: "Jira".into(),
            icon: "jira".into(),
            auth_methods: vec![AuthMethod {
                id: "basic".into(),
                label: "API token".into(),
                kind: AuthMethodKind::Fields {
                    fields: vec![
                        AuthField { key: "email".into(), label: "Email".into(), widget: FieldWidget::Text, required: true, placeholder: None },
                        AuthField { key: "domain".into(), label: "Site".into(), widget: FieldWidget::Url, required: true, placeholder: Some("you.atlassian.net".into()) },
                    ],
                },
            }],
        };
        let json = serde_json::to_string(&d).unwrap();
        let back: ProviderDescriptor = serde_json::from_str(&json).unwrap();
        assert_eq!(back.id, "jira");
        assert_eq!(back.auth_methods.len(), 1);
    }
}
