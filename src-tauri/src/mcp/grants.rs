//! Grants — a temporary override of the gates, handed out *in advance*, for when nobody is
//! at the screen.
//!
//! The consent prompt answers "may it do this, now?" and needs a person; half an hour of
//! autonomous work does not have one. A grant is the same answer given ahead of time, and it
//! is what makes the closed default usable rather than merely safe: the products gate can
//! stay **off** — no Corvus on the AI surface at all — and a grant switches it on for one
//! stretch, on named projects, for named tools, with a budget. Permission arrives when the
//! work does, and leaves with it.
//!
//! It is narrowed on every axis a standing permission should be narrowed on:
//!
//! - **named projects** — every path in the call must lie under one of the grant's roots;
//! - **named tools** — `corvus_commit`, not "anything that writes";
//! - **a budget** — N uses, a deadline, or both. A grant with neither is a permanent
//!   permission wearing a token's name, so [`mint`] refuses to create one.
//!
//! ## What a grant overrides, and what it cannot
//!
//! It overrides the three gates that decide a call: the **product** being off, the project
//! being out of **scope**, and the **policy** class for that kind of action. A `Deny` from
//! any of them is exactly what "let it commit to this project for the next hour" has to get
//! past, so a grant that stopped at the prompt would not do the job it exists for.
//!
//! What it cannot do is widen its own reach. [`covers`](Grant::covers) matches only when
//! every path in the call lies inside one of the grant's own roots — so the roots the user
//! picked *are* the scope statement, and a call that strays outside them is refused with no
//! prompt, exactly as before. Nor does it survive the endpoint being switched off: with
//! nothing listening there is no call to grant.
//!
//! A grant also has to make its tools **visible**, not merely permitted. An off product's
//! tools are absent from `tools/list` entirely, and a tool a client cannot see is a tool it
//! cannot call — so a grant contributes its [`programs`](Grant::programs) and tool names to
//! the catalogue, and minting or revoking one moves the exposed set (see
//! [`crate::mcp::grants_changed`]).
//!
//! ## Where grants live
//!
//! **In memory, in this process.** Never written to the config or to disk, and that is a
//! security property rather than a simplification: a grant on disk is a grant that can be
//! *written*, and the thing a grant authorises is an assistant with file tools. Quit Arbor
//! and every grant is gone, which is also the right failure for something meant to cover one
//! unattended stretch. They die on [`clear`] too, and the settings list shows each one with
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
/// The most projects one grant may cover. A grant over a dozen projects is no longer a
/// statement about a task.
const MAX_ROOTS: usize = 12;

/// What the user asks for when minting.
#[derive(Debug, Clone, Deserialize)]
pub struct GrantSpec {
    /// A name the user recognises in the list ("Overnight refactor").
    pub label: String,
    /// Absolute project roots the grant covers. At least one.
    pub roots: Vec<String>,
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
    pub roots: Vec<String>,
    pub tools: Vec<String>,
    /// Products this grant exposes for as long as it lives, whether or not the user has
    /// them switched on.
    pub programs: Vec<String>,
    /// Calls left, or `None` when only time limits it.
    pub uses_left: Option<u32>,
    /// Seconds until it lapses, or `None` when only the count limits it.
    pub expires_in_secs: Option<u64>,
    /// The budget it started with, so what is left can be shown as a fraction of it
    /// rather than as a bare number — "3 left" means nothing without "of 50".
    pub max_uses: Option<u32>,
    pub ttl_secs: Option<u64>,
}

/// What live grants add to the AI surface.
#[derive(Debug, Clone, Default)]
pub struct Unlocked {
    /// Programs to list even though the user has them switched off.
    pub programs: Vec<String>,
    /// The only tool names those programs may contribute. A grant for `corvus_commit` must
    /// not put `corvus_push` on the surface: visibility is granted as narrowly as the
    /// permission is.
    pub tools: Vec<String>,
}

impl Unlocked {
    pub fn exposes(&self, program: &str) -> bool {
        self.programs.iter().any(|p| p == program)
    }

    pub fn names(&self, tool: &str) -> bool {
        self.tools.iter().any(|t| t == tool)
    }
}

struct Grant {
    id: String,
    label: String,
    roots: Vec<String>,
    tools: Vec<String>,
    programs: Vec<String>,
    uses_left: Option<u32>,
    expires_at: Option<Instant>,
    max_uses: Option<u32>,
    ttl_secs: Option<u64>,
}

impl Grant {
    fn live(&self, now: Instant) -> bool {
        self.uses_left != Some(0) && self.expires_at.map_or(true, |t| now < t)
    }

    /// Whether this grant answers for one call.
    ///
    /// Every path must be inside *some* root of the grant — not all inside the same one, so
    /// a grant over two projects covers a call that touches both, but one stray path still
    /// spoils the whole call. A call that names no path cannot be attributed to any project
    /// and is never covered.
    fn covers(&self, tool: &str, paths: &[String]) -> bool {
        !paths.is_empty()
            && self.tools.iter().any(|t| t == tool)
            && paths.iter().all(|p| {
                self.roots
                    .iter()
                    .any(|root| policy::is_within(Path::new(root), Path::new(p)))
            })
    }

    fn info(&self, now: Instant) -> GrantInfo {
        GrantInfo {
            id: self.id.clone(),
            label: self.label.clone(),
            roots: self.roots.clone(),
            tools: self.tools.clone(),
            programs: self.programs.clone(),
            uses_left: self.uses_left,
            expires_in_secs: self.expires_at.map(|t| t.saturating_duration_since(now).as_secs()),
            max_uses: self.max_uses,
            ttl_secs: self.ttl_secs,
        }
    }
}

static GRANTS: Mutex<Vec<Grant>> = Mutex::new(Vec::new());

/// Create a grant. `programs` is resolved by the caller from the real inventories — see
/// [`crate::mcp::catalog::programs_of`] — because what a grant exposes is established
/// host-side, not taken from whoever asked for it.
///
/// Errors say what to fix, because the user typed this.
pub fn mint(spec: GrantSpec, programs: Vec<String>) -> Result<GrantInfo, String> {
    let label = spec.label.trim();
    if label.is_empty() {
        return Err("Give the grant a name, so you can recognise it in the list.".into());
    }
    let roots: Vec<String> = spec.roots.iter().map(|r| r.trim().to_string()).filter(|r| !r.is_empty()).collect();
    if roots.is_empty() {
        return Err("Choose at least one project.".into());
    }
    if roots.len() > MAX_ROOTS {
        return Err(format!("A grant covers at most {MAX_ROOTS} projects."));
    }
    if let Some(bad) = roots.iter().find(|r| !Path::new(r).is_absolute()) {
        return Err(format!("'{bad}' is not an absolute path."));
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
    let max_uses = spec.max_uses.map(|n| n.min(MAX_USES));
    let ttl_secs = spec.ttl_secs.map(|s| s.min(MAX_TTL_SECS));
    let grant = Grant {
        id: consent::new_id(),
        label: label.to_string(),
        roots,
        tools: spec.tools,
        programs,
        uses_left: max_uses,
        expires_at: ttl_secs.map(|s| now + Duration::from_secs(s)),
        max_uses,
        ttl_secs,
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

/// What the live grants add to the exposed tool set.
///
/// Called on every listing and every call, so it stays a read of the in-memory store and
/// nothing more. Fails closed: an unreadable store unlocks nothing.
pub fn unlocked() -> Unlocked {
    let now = Instant::now();
    let Ok(mut grants) = GRANTS.lock() else { return Unlocked::default() };
    grants.retain(|g| g.live(now));
    let mut out = Unlocked::default();
    for grant in grants.iter() {
        for program in &grant.programs {
            if !out.exposes(program) {
                out.programs.push(program.clone());
            }
        }
        for tool in &grant.tools {
            if !out.names(tool) {
                out.tools.push(tool.clone());
            }
        }
    }
    out
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

    // Grants are process-global; each test uses roots of its own so parallel tests can't
    // spend each other's uses.
    fn spec(roots: &[&str], uses: Option<u32>, ttl: Option<u64>) -> GrantSpec {
        GrantSpec {
            label: "t".into(),
            roots: roots.iter().map(|r| (*r).to_string()).collect(),
            tools: vec!["corvus_commit".into()],
            max_uses: uses,
            ttl_secs: ttl,
        }
    }

    fn mint_one(roots: &[&str], uses: Option<u32>, ttl: Option<u64>) -> Result<GrantInfo, String> {
        mint(spec(roots, uses, ttl), vec!["corvus".into()])
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
        assert!(mint_one(&[&root("nolimit")], None, None).is_err());
        assert!(mint_one(&[&root("zero")], Some(0), None).is_err());
        assert!(mint_one(&["relative/path"], Some(1), None).is_err());
        assert!(mint_one(&[], Some(1), None).is_err(), "no project");
    }

    #[test]
    fn uses_are_spent_then_the_grant_is_gone() {
        let r = root("uses");
        mint_one(&[&r], Some(2), None).unwrap();
        let paths = [inside(&r, "a.rs")];
        assert!(consume("corvus_commit", &paths).is_some());
        assert!(consume("corvus_commit", &paths).is_some());
        assert!(consume("corvus_commit", &paths).is_none());
    }

    #[test]
    fn a_grant_covers_only_its_tool_and_projects() {
        let r = root("scope");
        mint_one(&[&r], Some(5), None).unwrap();
        let paths = [inside(&r, "a.rs")];
        assert!(consume("corvus_blame", &paths).is_none(), "other tool");
        assert!(consume("corvus_commit", &[inside(&root("elsewhere"), "a.rs")]).is_none(), "other project");
        assert!(consume("corvus_commit", &[]).is_none(), "no path to attribute");
        // One path outside the grant's projects spoils the whole call.
        assert!(consume("corvus_commit", &[inside(&r, "a.rs"), inside(&root("elsewhere"), "b.rs")]).is_none());
    }

    #[test]
    fn several_projects_are_covered_by_one_grant() {
        let (a, b) = (root("multi-a"), root("multi-b"));
        mint_one(&[&a, &b], Some(5), None).unwrap();
        // Either project on its own…
        assert!(consume("corvus_commit", &[inside(&a, "x.rs")]).is_some());
        assert!(consume("corvus_commit", &[inside(&b, "y.rs")]).is_some());
        // …and a single call that touches both, since both were named.
        assert!(consume("corvus_commit", &[inside(&a, "x.rs"), inside(&b, "y.rs")]).is_some());
        // A third project is still outside it.
        assert!(consume("corvus_commit", &[inside(&root("multi-c"), "z.rs")]).is_none());
    }

    #[test]
    fn unlocked_reports_the_programs_and_tools_of_live_grants() {
        let r = root("unlocked");
        let info = mint_one(&[&r], Some(1), None).unwrap();
        let open = unlocked();
        assert!(open.exposes("corvus"));
        assert!(open.names("corvus_commit"));
        assert!(!open.names("corvus_push"), "visibility is as narrow as the permission");
        revoke(&info.id);
    }

    #[test]
    fn revoke_removes_exactly_one() {
        let r = root("revoke");
        let a = mint_one(&[&r], Some(3), None).unwrap();
        assert!(revoke(&a.id));
        assert!(!revoke(&a.id));
        assert!(consume("corvus_commit", &[inside(&r, "a.rs")]).is_none());
    }

    #[test]
    fn expiry_is_enforced() {
        let g = Grant {
            id: "x".into(),
            label: "x".into(),
            roots: vec![root("ttl")],
            tools: vec![],
            programs: vec![],
            uses_left: None,
            expires_at: Some(Instant::now()),
            max_uses: None,
            ttl_secs: Some(1),
        };
        assert!(!g.live(Instant::now() + Duration::from_millis(1)));
    }
}
