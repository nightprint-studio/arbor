//! What the user has already said yes to.
//!
//! Starting a process runs arbitrary commands, so it needs the user's OK. A *recipe* is the one place
//! where that OK can be given once for several commands: approving it approves exactly the file it
//! was read from, identified by the content hash, until that hash changes.
//!
//! Approvals live in memory only. A file in the repository that said "approved" would let a cloned
//! repository approve itself, which is the exact thing the prompt exists to prevent.

use std::collections::HashMap;
use std::sync::Mutex;

#[derive(Default)]
pub struct Approvals {
    /// `project key` -> hash of the recipe file the user approved.
    recipes: Mutex<HashMap<String, String>>,
}

impl Approvals {
    pub fn is_recipe_approved(&self, project: &str, hash: &str) -> bool {
        self.recipes.lock().unwrap_or_else(|p| p.into_inner()).get(project).is_some_and(|h| h == hash)
    }

    pub fn approve_recipes(&self, project: &str, hash: &str) {
        self.recipes.lock().unwrap_or_else(|p| p.into_inner()).insert(project.to_string(), hash.to_string());
    }

    pub fn revoke(&self, project: &str) {
        self.recipes.lock().unwrap_or_else(|p| p.into_inner()).remove(project);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_approval_holds_until_the_hash_changes() {
        let a = Approvals::default();
        assert!(!a.is_recipe_approved("p", "h1"));
        a.approve_recipes("p", "h1");
        assert!(a.is_recipe_approved("p", "h1"));
        assert!(!a.is_recipe_approved("p", "h2"), "an edited file asks again");
        assert!(!a.is_recipe_approved("q", "h1"), "another project is not covered");
        a.revoke("p");
        assert!(!a.is_recipe_approved("p", "h1"));
    }
}
