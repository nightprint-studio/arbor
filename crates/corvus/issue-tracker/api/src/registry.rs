//! [`IssueTrackerRegistry`] — the trait-object registry of trackers.
//!
//! Trackers are held as `Arc<dyn IssueTracker>` keyed by their descriptor id, so
//! adding or removing a provider is a single register/unregister — no `match`
//! over hard-coded providers anywhere. The host registers what it ships at
//! startup; the FE drives off [`IssueTrackerRegistry::descriptors`].

use std::collections::HashMap;
use std::sync::Arc;

use crate::provider::ProviderDescriptor;
use crate::tracker::IssueTracker;

/// A registry of issue trackers keyed by [`ProviderDescriptor::id`].
#[derive(Default, Clone)]
pub struct IssueTrackerRegistry {
    providers: HashMap<String, Arc<dyn IssueTracker>>,
}

impl IssueTrackerRegistry {
    pub fn new() -> Self {
        Self { providers: HashMap::new() }
    }

    /// Register a tracker under its own descriptor id, replacing any prior one.
    pub fn register(&mut self, tracker: Arc<dyn IssueTracker>) {
        let id = tracker.descriptor().id;
        self.providers.insert(id, tracker);
    }

    /// Remove the tracker with this id, returning it if present.
    pub fn unregister(&mut self, id: &str) -> Option<Arc<dyn IssueTracker>> {
        self.providers.remove(id)
    }

    /// The tracker registered under `id`, if any.
    pub fn get(&self, id: &str) -> Option<Arc<dyn IssueTracker>> {
        self.providers.get(id).cloned()
    }

    /// The descriptors of all registered trackers (for the FE provider list).
    pub fn descriptors(&self) -> Vec<ProviderDescriptor> {
        self.providers.values().map(|t| t.descriptor()).collect()
    }

    /// The ids of all registered trackers.
    pub fn ids(&self) -> Vec<String> {
        self.providers.keys().cloned().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::Result;
    use crate::provider::{AuthMethod, AuthMethodKind, AuthStatus, NewIssue};
    use crate::tracker::IssueTracker;
    use crate::types::{Issue, IssueComment, IssueFilterOptions, IssueFilters};
    use async_trait::async_trait;

    struct FakeTracker {
        id: &'static str,
    }

    #[async_trait]
    impl IssueTracker for FakeTracker {
        fn descriptor(&self) -> ProviderDescriptor {
            ProviderDescriptor {
                id: self.id.into(),
                display_name: self.id.into(),
                icon: self.id.into(),
                auth_methods: vec![AuthMethod {
                    id: "oauth".into(),
                    label: "Connect".into(),
                    kind: AuthMethodKind::OAuth,
                }],
            }
        }
        async fn auth_status(&self) -> Result<AuthStatus> {
            Ok(AuthStatus { authenticated: false, user: None, domain: None, auth_method: None })
        }
        async fn search_issues(&self, _: IssueFilters) -> Result<Vec<Issue>> { Ok(vec![]) }
        async fn get_issue(&self, _: &str) -> Result<Issue> { unreachable!() }
        async fn lookup_by_identifier(&self, _: &str) -> Result<Option<Issue>> { Ok(None) }
        async fn get_filter_options(&self) -> Result<IssueFilterOptions> { unreachable!() }
        async fn transition_issue(&self, _: &str, _: &str) -> Result<Issue> { unreachable!() }
        async fn assign_issue(&self, _: &str, _: Option<&str>) -> Result<Issue> { unreachable!() }
        async fn add_comment(&self, _: &str, _: &str) -> Result<IssueComment> { unreachable!() }
        async fn create_issue(&self, _: NewIssue) -> Result<Issue> { unreachable!() }
        async fn fetch_image_bytes(&self, _: &str) -> Result<(Vec<u8>, Option<String>)> { unreachable!() }
    }

    #[test]
    fn register_get_and_list() {
        let mut reg = IssueTrackerRegistry::new();
        reg.register(Arc::new(FakeTracker { id: "linear" }));
        reg.register(Arc::new(FakeTracker { id: "jira" }));

        assert!(reg.get("linear").is_some());
        assert!(reg.get("github").is_none());

        let mut ids = reg.ids();
        ids.sort();
        assert_eq!(ids, vec!["jira".to_string(), "linear".to_string()]);
        assert_eq!(reg.descriptors().len(), 2);
    }

    #[test]
    fn register_replaces_and_unregister_removes() {
        let mut reg = IssueTrackerRegistry::new();
        reg.register(Arc::new(FakeTracker { id: "linear" }));
        reg.register(Arc::new(FakeTracker { id: "linear" })); // same id → replace
        assert_eq!(reg.ids().len(), 1);

        assert!(reg.unregister("linear").is_some());
        assert!(reg.get("linear").is_none());
    }
}
