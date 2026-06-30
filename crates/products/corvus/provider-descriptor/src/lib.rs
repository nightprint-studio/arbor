//! `corvus-provider-descriptor` — the shared, domain-agnostic provider-connection
//! contract.
//!
//! Both connection layers (issue trackers and git hosts) and the single generic
//! frontend speak this one vocabulary, so the UI carries **zero** per-provider
//! knowledge: it renders [`ProviderDescriptor`](prelude::ProviderDescriptor),
//! interprets the declarative [`FieldRule`](prelude::FieldRule) /
//! [`FieldHint`](prelude::FieldHint) rules, drives auth through generic IPC keyed
//! by [`ProviderDescriptor::id`], and reflects [`AuthStatus`](prelude::AuthStatus)
//! / [`OAuthStart`](prelude::OAuthStart).
//!
//! Import via the [`prelude`].

pub mod descriptor;
pub mod prelude;

#[cfg(test)]
mod tests {
    use crate::prelude::*;

    #[test]
    fn domain_is_snake_case() {
        assert_eq!(serde_json::to_string(&ProviderDomain::IssueTracker).unwrap(), "\"issue_tracker\"");
        assert_eq!(serde_json::to_string(&ProviderDomain::GitHost).unwrap(), "\"git_host\"");
    }

    #[test]
    fn oauth_method_tag_is_oauth() {
        let m = AuthMethod {
            id: "oauth".into(),
            label: "OAuth".into(),
            kind: AuthMethodKind::OAuth { flow: OAuthFlow::Device },
        };
        let v: serde_json::Value = serde_json::from_str(&serde_json::to_string(&m).unwrap()).unwrap();
        assert_eq!(v["kind"]["type"], "oauth");
        assert_eq!(v["kind"]["flow"], "device");
    }

    #[test]
    fn fields_method_carries_rules() {
        let m = AuthMethod {
            id: "basic".into(),
            label: "API Token".into(),
            kind: AuthMethodKind::Fields {
                fields: vec![AuthField {
                    key: "email".into(),
                    label: "Email".into(),
                    widget: FieldWidget::Text,
                    required: false,
                    required_when: Some(FieldRule {
                        field: "domain".into(),
                        matches: FieldMatch::EndsWith(".atlassian.net".into()),
                    }),
                    placeholder: None,
                }],
                hints: vec![FieldHint {
                    text: "Jira Cloud".into(),
                    when: Some(FieldRule {
                        field: "domain".into(),
                        matches: FieldMatch::EndsWith(".atlassian.net".into()),
                    }),
                }],
            },
        };
        let v: serde_json::Value = serde_json::from_str(&serde_json::to_string(&m).unwrap()).unwrap();
        assert_eq!(v["kind"]["type"], "fields");
        assert_eq!(v["kind"]["fields"][0]["requiredWhen"]["matches"]["op"], "endsWith");
        assert_eq!(v["kind"]["fields"][0]["requiredWhen"]["matches"]["value"], ".atlassian.net");
        assert_eq!(v["kind"]["hints"][0]["when"]["field"], "domain");
    }

    #[test]
    fn oauth_start_is_tagged() {
        let d = OAuthStart::Device {
            user_code: "ABCD-1234".into(),
            verification_uri: "https://github.com/login/device".into(),
            expires_in: 900,
            interval: 5,
        };
        let v: serde_json::Value = serde_json::from_str(&serde_json::to_string(&d).unwrap()).unwrap();
        assert_eq!(v["type"], "device");
        assert_eq!(v["userCode"], "ABCD-1234");
    }
}
