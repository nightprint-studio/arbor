//! Grants — pre-approved tool calls the user hands out *in advance*, for when they are away.
//!
//! The consent prompt answers "may it do this, now?" and needs a person at the screen; a
//! half-hour of autonomous work does not have one. A grant is the same answer given ahead of
//! time, narrowed on every axis a standing permission should be narrowed on:
//!
//! - **one project** — the tool call's paths must all lie under the grant's root;
//! - **named tools** — `corvus_commit`, not "anything that writes";
//! - **a budget** — N uses, a deadline, or both. A grant with neither is a permanent
//!   permission wearing a token's name, so [`mint`] refuses to create one.
//!
//! ## What a grant replaces, and what it does not
//!
//! It stands in for the **prompt** and nothing else. A call the policy *denies* — a path
//! outside the project scope, a product switched off for that project, a class set to
//! "never" — stays denied: the user asked for those, and a grant is a convenience layered on
//! top of the policy, not a hole in it. The caller ([`crate::mcp::catalog`]) therefore only
//! asks [`consume`] when the verdict was `Ask`.
//!
//! ## Where grants live
//!
//! **In memory, in this process.** Never written to the config or to disk: quit Arbor and
//! every grant is gone, which is the right failure for something meant to cover one
//! unattended stretch. They also die on [`clear`], and the settings list shows each one with
//! what is left of it so it can be revoked early.
//!
//! A grant is an ambient authorisation on the host, not a secret the model presents. The MCP
//! endpoint is already authenticated by its bearer token; what the grant adds is the *user's
//! advance consent* for one narrow class of calls.

use std::path::Path;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::mcp::{consent, policy};

/// The longest a grant may live. Beyond a day, "while I'm away" has become "indefinitely".
const MAX_TTL_SECS: u64 = 24 * 60 * 60;
/// The most uses one grant may carry.
const MAX_USES: u32 = 1_000;

/// What the user asks for when minting.
#[derive(Debug, Clone, Deserialize)]
pub struct GrantSpec {
    /// A name the user recognises in the list ("Overnight refactor").
    pub label: String,
    /// Absolute project root the grant covers.
    pub root: String,
    /// Tool names the grant covers.
    pub tools: Vec<String>,
    /// How many calls it covers. `None` = no count limit (then `ttl_secs` is required).
    pub max_uses: Option<u32>,
    /// How long it lasts, in seconds. `None` = no time limit (then `max_uses` is required).
    pub ttl_secs: Option<u64>,
}

/// A grant as the UI shows it.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct GrantInfo {
    pub id: String,
    pub label: String,
    pub root: String,
    pub tools: Vec<String>,
    /// Calls left, or `None` when only time limits it.
    pub uses_left: Option<u32>,
    /// Seconds until it lapses, or `None` when only the count limits it.
    pub expires_in_secs: Option<u64>,
}

struct Grant {
    id: String,
    label: String,
    root: String,
    tools: Vec<String>,
    uses_left: Option<u32>,
    expires_at: Option<Instant>,
}

impl Grant {
    fn live(&self, now: Instant) -> bool {
        self.uses_left != Some(0) && self.expires_at.map_or(true, |t| now < t)
    }

    fn covers(&self, tool: &str, paths: &[String]) -> bool {
        // A call that names no path cannot be attributed to the grant's project.
        !paths.is_empty()
            && self.tools.iter().any(|t| t == tool)
            && paths.iter().all(|p| policy::is_within(Path::new(&self.root), Path::new(p)))
    }

    fn info(&self, now: Instant) -> GrantInfo {
        GrantInfo {
            id: self.id.clone(),
            label: self.label.clone(),
            root: self.root.clone(),
            tools: self.tools.clone(),
            uses_left: self.uses_left,
            expires_in_secs: self.expires_at.map(|t| t.saturating_duration_since(now).as_secs()),
        }
    }
}

static GRANTS: Mutex<Vec<Grant>> = Mutex::new(Vec::new());

/// Create a grant. Errors say what to fix, because the user typed this.
pub fn mint(spec: GrantSpec) -> Result<GrantInfo, String> {
    let label = spec.label.trim();
    if label.is_empty() {
        return Err("Give the grant a name, so you can recognise it in the list.".into());
    }
    if spec.root.trim().is_empty() || !Path::new(&spec.root).is_absolute() {
        return Err("The project root must be an absolute path.".into());
    }
    if spec.tools.is_empty() {
        return Err("Choose at least one tool the grant covers.".into());
    }
    if spec.max_uses.is_none() && spec.ttl_secs.is_none() {
        return Err("A grant needs a limit: a number of uses, an expiry, or both.".into());
    }
    if spec.max_uses == Some(0) || spec.ttl_secs == Some(0) {
        return Err("Uses and duration must be greater than zero.".into());
    }
    let now = Instant::now();
    let grant = Grant {
        id: consent::new_id(),
        label: label.to_string(),
        root: spec.root.trim().to_string(),
        tools: spec.tools,
        uses_left: spec.max_uses.map(|n| n.min(MAX_USES)),
        expires_at: spec.ttl_secs.map(|s| now + Duration::from_secs(s.min(MAX_TTL_SECS))),
    };
    let info = grant.info(now);
    GRANTS.lock().map_err(|_| "grant store unavailable".to_string())?.push(grant);
    Ok(info)
}

/// Every live grant, oldest first. Lapsed ones are dropped on the way.
pub fn list() -> Vec<GrantInfo> {
    let now = Instant::now();
    let Ok(mut grants) = GRANTS.lock() else { return Vec::new() };
    grants.retain(|g| g.live(now));
    grants.iter().map(|g| g.info(now)).collect()
}

/// Revoke one grant. `false` when it was already gone.
pub fn revoke(id: &str) -> bool {
    let Ok(mut grants) = GRANTS.lock() else { return false };
    let before = grants.len();
    grants.retain(|g| g.id != id);
    grants.len() != before
}

/// Revoke every grant.
pub fn clear() {
    if let Ok(mut grants) = GRANTS.lock() {
        grants.clear();
    }
}

/// Try to cover a call with a grant, spending one use of it. Returns the grant's label when
/// one applied.
///
/// Fails closed: a poisoned lock or an expired grant covers nothing. When several grants
/// match, the one nearest to running out is spent first, so a broad grant is not drained
/// while a narrow one lingers.
pub fn consume(tool: &str, paths: &[String]) -> Option<String> {
    let now = Instant::now();
    let mut grants = GRANTS.lock().ok()?;
    grants.retain(|g| g.live(now));
    let chosen = grants
        .iter_mut()
        .filter(|g| g.covers(tool, paths))
        .min_by_key(|g| g.uses_left.unwrap_or(u32::MAX))?;
    if let Some(n) = chosen.uses_left.as_mut() {
        *n -= 1;
    }
    Some(chosen.label.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    // Grants are process-global; each test uses a root of its own so parallel tests can't
    // spend each other's uses.
    fn spec(root: &str, uses: Option<u32>, ttl: Option<u64>) -> GrantSpec {
        GrantSpec {
            label: "t".into(),
            root: root.into(),
            tools: vec!["corvus_commit".into()],
            max_uses: uses,
            ttl_secs: ttl,
        }
    }

    #[cfg(windows)]
    const ROOT: &str = r"C:\arbor-grant-test";
    #[cfg(not(windows))]
    const ROOT: &str = "/arbor-grant-test";

    fn root(tag: &str) -> String {
        format!("{ROOT}-{tag}")
    }

    fn inside(root: &str, rel: &str) -> String {
        format!("{root}/{rel}")
    }

    #[test]
    fn a_grant_needs_a_limit() {
        assert!(mint(spec(&root("nolimit"), None, None)).is_err());
        assert!(mint(spec(&root("zero"), Some(0), None)).is_err());
        assert!(mint(spec("relative/path", Some(1), None)).is_err());
    }

    #[test]
    fn uses_are_spent_then_the_grant_is_gone() {
        let r = root("uses");
        mint(spec(&r, Some(2), None)).unwrap();
        let paths = [inside(&r, "a.rs")];
        assert!(consume("corvus_commit", &paths).is_some());
        assert!(consume("corvus_commit", &paths).is_some());
        assert!(consume("corvus_commit", &paths).is_none());
    }

    #[test]
    fn a_grant_covers_only_its_tool_and_project() {
        let r = root("scope");
        mint(spec(&r, Some(5), None)).unwrap();
        let paths = [inside(&r, "a.rs")];
        assert!(consume("corvus_blame", &paths).is_none(), "other tool");
        assert!(consume("corvus_commit", &[inside(&root("elsewhere"), "a.rs")]).is_none(), "other project");
        assert!(consume("corvus_commit", &[]).is_none(), "no path to attribute");
        // One path outside the project spoils the whole call.
        assert!(consume("corvus_commit", &[inside(&r, "a.rs"), inside(&root("elsewhere"), "b.rs")]).is_none());
    }

    #[test]
    fn revoke_removes_exactly_one() {
        let r = root("revoke");
        let a = mint(spec(&r, Some(3), None)).unwrap();
        assert!(revoke(&a.id));
        assert!(!revoke(&a.id));
        assert!(consume("corvus_commit", &[inside(&r, "a.rs")]).is_none());
    }

    #[test]
    fn expiry_is_enforced() {
        let g = Grant {
            id: "x".into(),
            label: "x".into(),
            root: root("ttl"),
            tools: vec![],
            uses_left: None,
            expires_at: Some(Instant::now()),
        };
        assert!(!g.live(Instant::now() + Duration::from_millis(1)));
    }
}
