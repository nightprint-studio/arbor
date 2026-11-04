//! Where a secret variable's value lives: the keyring, asked for through the shell.
//!
//! A variable whose value is `@secret` has no value in any file. Its account is derived from *which project,
//! which layer, which name* — [`secret_account`] — so the same variable name in two projects, or in staging
//! and prod, are different secrets. The shell namespaces the account further (`hirundo:`) before it touches
//! the keyring; the backend can name only its own accounts.

use hirundo_model::prelude::{VarScope, VarSource};

use crate::state::HirundoState;

fn scope_word(scope: VarScope) -> &'static str {
    match scope {
        VarScope::Collection => "collection",
        VarScope::Folder => "folder",
        VarScope::Type => "type",
        VarScope::Environment => "environment",
        VarScope::Local => "local",
        VarScope::Runtime => "runtime",
    }
}

/// The keyring account of a secret: `<project>|<layer>:<id>|<name>`.
pub fn secret_account(project: &str, source: &VarSource, name: &str) -> String {
    format!("{project}|{}:{}|{name}", scope_word(source.scope), source.id)
}

/// The secret's value, `None` when the keyring has none (or the shell cannot be asked).
pub fn read_secret(state: &HirundoState, project: &str, source: &VarSource, name: &str) -> Option<String> {
    let value = state.host_call("__hirundo_secret", serde_json::json!(secret_account(project, source, name))).ok()?;
    value.as_str().map(str::to_string).filter(|s| !s.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_same_name_in_two_layers_or_projects_is_two_secrets() {
        let staging = VarSource::new(VarScope::Environment, "staging");
        let prod = VarSource::new(VarScope::Environment, "prod");
        assert_ne!(secret_account("/p/a", &staging, "token"), secret_account("/p/a", &prod, "token"));
        assert_ne!(secret_account("/p/a", &staging, "token"), secret_account("/p/b", &staging, "token"));
        assert_eq!(secret_account("/p/a", &staging, "token"), "/p/a|environment:staging|token");
    }
}
