//! Whether a tool call may run — the decision the launcher makes and the AI client
//! cannot influence.
//!
//! Three questions, and all must say yes:
//!
//! 1. **Is this path in scope?** A read tool with no gate is a file-read primitive for
//!    anything that can reach the port. Path scoping is what keeps `bennu_read_file`
//!    pointed at projects the user actually works on.
//! 2. **Is this backend allowed on this project?** Globally on is not the same as on
//!    everywhere: a project can refuse a product that the profile as a whole permits.
//! 3. **What does policy say about this call, here?** Allow, ask, or refuse — the
//!    project's own rule for this exact tool when it names one, else its rule for the
//!    class of action, else the profile's. Most specific first, and every level may
//!    decline to have an opinion, so a rule states only what it disagrees with.
//!
//! Scope is checked first, deliberately: a path outside scope is refused *without*
//! prompting, so a model cannot turn "read my SSH key" into a consent dialog the user
//! might click through.
//!
//! **A call that names no path is decided globally.** `tyto_screenshot` is not about a
//! project and cannot be attributed to one, so a per-project rule cannot reach it. The
//! alternative — attributing it to whichever project happens to be open — would be a
//! guess presented as a permission, which is worse than a rule with a stated edge.
//!
//! **When a call names paths in several projects, the strictest answer wins.** A call
//! is one act; letting the most permissive project decide would make a second path an
//! escape hatch out of the first project's rule.

use std::path::{Path, PathBuf};

use arbor_rpc::prelude::Safety;

use crate::config::app_config::{McpConfig, McpDecision, McpProjectRule, McpScopeMode};
use crate::AppState;

/// What the host decided to do with a call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    /// Run it now.
    Allow,
    /// Ask the user first.
    Ask,
    /// Refuse, with a reason written for the model.
    Deny(String),
}

/// Decide, from config + the arguments' paths.
pub fn decide(
    state: &AppState,
    program: &str,
    tool: &str,
    safety: Safety,
    arguments: &serde_json::Value,
) -> Verdict {
    let cfg = match state.lock_config() {
        Ok(c) => c.clone(),
        // A config we cannot read is not a config that grants anything.
        Err(_) => return Verdict::Deny("Arbor could not read its permission settings.".into()),
    };

    if let Some(offending) = out_of_scope(&cfg.mcp, state, program, arguments) {
        // Only this mode's message distinguishes "open elsewhere" from "not open", and
        // only it pays for the extra lookup.
        let open_elsewhere = matches!(cfg.mcp.scope.mode, McpScopeMode::ByProduct)
            && open_project_roots(state)
                .iter()
                .any(|root| is_within(root, Path::new(&offending)));
        return Verdict::Deny(scope_refusal(
            &cfg.mcp.scope.mode,
            program,
            &offending,
            open_elsewhere,
        ));
    }

    let paths = candidate_paths(arguments);
    if paths.is_empty() {
        return from_decision(tier(&cfg.mcp.policy, safety), safety, None);
    }

    // One verdict per path, then the strictest — see the module note on multi-project
    // calls. `Verdict` has no ordering of its own because ordering is only meaningful
    // here, where "strictest" means "least likely to run".
    let mut worst: Option<Verdict> = None;
    for path in &paths {
        let rule = rule_for(&cfg.mcp.projects, Path::new(path));
        let verdict = match rule {
            Some(r) if r.products.get(program) == Some(&false) => Verdict::Deny(format!(
                "The user's rule for the project at `{}` does not allow {program} to be used \
                 there, even though {program} is enabled elsewhere in Arbor.",
                r.root
            )),
            Some(r) => from_decision(
                resolve(r, tool, safety, &cfg.mcp.policy),
                safety,
                Some(r.display_name()),
            ),
            None => from_decision(tier(&cfg.mcp.policy, safety), safety, None),
        };
        if worst.as_ref().map_or(true, |w| severity(&verdict) > severity(w)) {
            worst = Some(verdict);
        }
    }
    worst.unwrap_or(Verdict::Allow)
}

/// A project rule's answer for one call: the tool's own override, else the class's,
/// else the profile's.
///
/// Most specific wins, which is the only ordering that makes a per-tool override worth
/// having: it exists precisely to say something the class cannot, so a class that
/// outranked it would make it decorative.
fn resolve(
    rule: &McpProjectRule,
    tool: &str,
    safety: Safety,
    global: &crate::config::app_config::McpPolicy,
) -> McpDecision {
    rule.tools
        .get(tool)
        .copied()
        .or_else(|| rule.policy.get(safety))
        .unwrap_or_else(|| tier(global, safety))
}

/// How restrictive a verdict is. Only used to pick the strictest of several.
fn severity(v: &Verdict) -> u8 {
    match v {
        Verdict::Allow => 0,
        Verdict::Ask => 1,
        Verdict::Deny(_) => 2,
    }
}

/// Turn a decision into a verdict, with a refusal the model can act on.
///
/// The reason names the *project* when a project rule produced it: "disabled in this
/// profile" and "disabled on this one project" call for different things from the user,
/// and a client told only the first will suggest the wrong fix.
fn from_decision(decision: McpDecision, safety: Safety, project: Option<&str>) -> Verdict {
    match decision {
        McpDecision::Allow => Verdict::Allow,
        McpDecision::Ask => Verdict::Ask,
        McpDecision::Deny => {
            let what = match safety {
                Safety::Read => "Read tools",
                Safety::Write => "Tools that modify files",
                Safety::Destructive => "Destructive tools (delete, bulk rewrite, running code)",
            };
            Verdict::Deny(match project {
                Some(name) => format!(
                    "{what} are disabled for the project \"{name}\". The user can change that \
                     project's rule in Settings → AI tool access."
                ),
                None => format!(
                    "{what} are disabled in this Arbor profile. The user can enable them in \
                     Settings → AI tool access, per class."
                ),
            })
        }
    }
}

/// The rule governing `path`: the one whose root contains it, longest root first.
///
/// Longest wins because nested checkouts are real (a workspace holding its own
/// sub-projects), and the inner root is the more specific statement about that file.
fn rule_for<'a>(rules: &'a [McpProjectRule], path: &Path) -> Option<&'a McpProjectRule> {
    rules
        .iter()
        .filter(|r| !r.root.is_empty() && is_within(Path::new(&r.root), path))
        .max_by_key(|r| r.root.len())
}

fn tier(policy: &crate::config::app_config::McpPolicy, safety: Safety) -> McpDecision {
    match safety {
        Safety::Read => policy.read,
        Safety::Write => policy.write,
        Safety::Destructive => policy.destructive,
    }
}

/// The first path argument that falls outside scope, if any.
/// Why a path was refused, said so the model can tell the user what would fix it.
///
/// Under [`McpScopeMode::ByProduct`] the two ways to be out of scope are different
/// problems — the project is not open at all, or it is open somewhere else — and a
/// single sentence covering both would send the user to the wrong place.
/// Takes `open_elsewhere` rather than the state so it stays a pure function of the
/// facts: the wording is the thing worth testing, and a message builder that reads a
/// live `AppState` cannot be tested without standing one up.
fn scope_refusal(
    mode: &McpScopeMode,
    program: &str,
    path: &str,
    open_elsewhere: bool,
) -> String {
    if matches!(mode, McpScopeMode::ByProduct) {
        return if open_elsewhere {
            format!(
                "The project at `{path}` is open in Arbor, but not in {program}, and this \
                 profile scopes each product to what it has open itself. Ask the user to \
                 open it in {program} — opening it elsewhere does not grant it here."
            )
        } else {
            format!(
                "No project at `{path}` is open in {program}. This profile scopes each \
                 product to the projects it has open, so ask the user to open it there."
            )
        };
    }
    format!(
        "The path `{path}` is outside the project scope this Arbor profile allows. Ask the \
         user to open that project in Arbor, or to add its root to the MCP project list in \
         Settings."
    )
}

fn out_of_scope(
    cfg: &McpConfig,
    state: &AppState,
    program: &str,
    arguments: &serde_json::Value,
) -> Option<String> {
    let allowed: Vec<PathBuf> = match cfg.scope.mode {
        McpScopeMode::Anywhere => return None,
        // The project list IS the allowlist — one list, so a project cannot be in scope
        // with no rule to its name, or carry a rule and silently not be in scope.
        McpScopeMode::Allowlist => cfg.projects.iter().map(|r| PathBuf::from(&r.root)).collect(),
        McpScopeMode::OpenProjects => open_project_roots(state),
        McpScopeMode::ByProduct => product_project_roots(state, program),
    };
    // Nothing in scope and a path was asked for → refuse, and the message above tells
    // the model what would fix it.
    let offending = candidate_paths(arguments)
        .into_iter()
        .find(|p| !allowed.iter().any(|root| is_within(root, Path::new(p))));

    // A refusal the user disagrees with is otherwise undiagnosable: the message names
    // the path but never what it was compared against, and "that project IS open" is
    // exactly the report this has to be able to answer.
    if let Some(path) = &offending {
        tracing::debug!(
            "mcp: `{path}` is out of scope under {:?}; allowed roots: {:?}",
            cfg.scope.mode,
            allowed,
        );
    }
    offending
}

/// The project roots the user has actually opened, across products.
///
/// **Read from the shared recents list alone, deliberately.** This used to also ask
/// `corvus-be` for its open repositories, over framed IPC, from inside the permission
/// decision — three things wrong at once: a security decision that makes a blocking
/// round-trip (landmine #1), one that silently loses roots whenever that backend is
/// slow or down, and one that answers differently depending on which *other* product
/// happens to be running. Every product records into recents as it opens something,
/// Corvus included, so the extra call bought only "open right now and never recorded",
/// and paid for it with a permission check that could fail open-endedly.
///
/// A missing root here is a refusal, never an accidental grant, so the safe direction
/// is preserved: if this list is somehow empty, everything path-bearing is refused.
fn open_project_roots(state: &AppState) -> Vec<PathBuf> {
    match state.lock_config() {
        Ok(cfg) => cfg.recents.iter().map(|r| PathBuf::from(&r.path)).collect(),
        Err(_) => Vec::new(),
    }
}

/// The roots `product` itself has open.
///
/// Recents are keyed by `(product, path)` — re-opening moves an entry up, it does not
/// overwrite another product's — so one project opened in two products is two rows and
/// each product's reach is recorded separately. That is what makes this mode meaningful
/// rather than a relabelling of [`open_project_roots`].
fn product_project_roots(state: &AppState, product: &str) -> Vec<PathBuf> {
    match state.lock_config() {
        Ok(cfg) => cfg
            .recents
            .iter()
            .filter(|r| r.product == product)
            .map(|r| PathBuf::from(&r.path))
            .collect(),
        Err(_) => Vec::new(),
    }
}

/// Every argument value that looks like an absolute filesystem path.
///
/// Walks the whole argument tree rather than checking a known set of key names: the
/// tools are declared by their own crates and a new one may call its path argument
/// anything. Checking by *shape* means a new tool is in scope by default rather than
/// unguarded by default — the safe direction to be wrong in.
fn candidate_paths(value: &serde_json::Value) -> Vec<String> {
    let mut out = Vec::new();
    collect_paths(value, &mut out);
    out
}

fn collect_paths(value: &serde_json::Value, out: &mut Vec<String>) {
    match value {
        serde_json::Value::String(s) => {
            if looks_absolute(s) {
                out.push(s.clone());
            }
        }
        serde_json::Value::Array(items) => items.iter().for_each(|v| collect_paths(v, out)),
        serde_json::Value::Object(map) => map.values().for_each(|v| collect_paths(v, out)),
        _ => {}
    }
}

/// Unix `/…` or Windows `C:\…` / `\\server\share`. Relative strings are not paths for
/// this purpose: a tool resolves them against a root that was itself checked.
fn looks_absolute(s: &str) -> bool {
    if s.starts_with('/') || s.starts_with("\\\\") {
        return true;
    }
    let bytes = s.as_bytes();
    bytes.len() >= 3
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && (bytes[2] == b'\\' || bytes[2] == b'/')
}

/// Whether `path` is `root` or lives under it.
///
/// Compares the **canonicalised** forms when both exist on disk, so `..` and symlinks
/// cannot walk out of an allowed root. A path that does not exist yet (a file about to
/// be created) falls back to a literal component comparison, which still rejects `..`
/// because the components are compared in order.
fn is_within(root: &Path, path: &Path) -> bool {
    let root_c = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let path_c = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    if path_c.components().any(|c| matches!(c, std::path::Component::ParentDir)) {
        return false;
    }
    path_c.starts_with(&root_c)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::app_config::McpProjectRule;
    use serde_json::json;

    #[test]
    fn candidate_paths_walks_the_whole_argument_tree() {
        let args = json!({
            "root": "/home/u/proj",
            "nested": { "file": "/home/u/proj/A.java", "n": 3 },
            "list": ["/tmp/x", "relative/y"],
        });
        let mut found = candidate_paths(&args);
        found.sort();
        assert_eq!(found, vec!["/home/u/proj", "/home/u/proj/A.java", "/tmp/x"]);
    }

    #[test]
    fn windows_paths_count_as_absolute() {
        assert!(looks_absolute(r"C:\Users\u\proj"));
        assert!(looks_absolute(r"\\server\share\f"));
        assert!(looks_absolute("/usr/lib"));
        assert!(!looks_absolute("src/main.rs"));
        assert!(!looks_absolute("just a sentence"));
    }

    #[test]
    fn a_parent_traversal_never_counts_as_inside() {
        // The whole point: `<root>/../../.ssh/id_rsa` must not read as inside <root>.
        assert!(!is_within(Path::new("/home/u/proj"), Path::new("/home/u/proj/../../.ssh/id_rsa")));
    }

    fn rule(root: &str) -> McpProjectRule {
        McpProjectRule { root: root.into(), ..Default::default() }
    }

    #[test]
    fn the_innermost_project_owns_the_file() {
        // A workspace holding its own sub-projects: the inner root is the more
        // specific statement about a file inside it, so it decides.
        let rules = vec![rule("/w"), rule("/w/apps/api"), rule("/w/apps")];
        let hit = rule_for(&rules, Path::new("/w/apps/api/src/main.rs")).unwrap();
        assert_eq!(hit.root, "/w/apps/api");
        // A file under no sub-project still lands on the outer one.
        assert_eq!(rule_for(&rules, Path::new("/w/README.md")).unwrap().root, "/w");
        assert!(rule_for(&rules, Path::new("/elsewhere/f")).is_none());
    }

    #[test]
    fn an_empty_root_matches_nothing() {
        // Otherwise a half-filled row (added, folder never chosen) would silently
        // become a rule over every path on disk.
        assert!(rule_for(&[rule("")], Path::new("/anything")).is_none());
    }

    #[test]
    fn the_strictest_path_decides_the_call() {
        assert_eq!(severity(&Verdict::Allow), 0);
        assert!(severity(&Verdict::Deny(String::new())) > severity(&Verdict::Ask));
        assert!(severity(&Verdict::Ask) > severity(&Verdict::Allow));
    }

    #[test]
    fn a_refusal_names_the_project_that_caused_it() {
        // "off in this profile" and "off on this one project" call for different
        // fixes, so a client told only the first would suggest the wrong one.
        let global = from_decision(McpDecision::Deny, Safety::Write, None);
        let scoped = from_decision(McpDecision::Deny, Safety::Write, Some("api"));
        match (global, scoped) {
            (Verdict::Deny(g), Verdict::Deny(s)) => {
                assert!(g.contains("this Arbor profile"), "{g}");
                assert!(s.contains("\"api\""), "{s}");
            }
            _ => panic!("a refusal must stay a refusal"),
        }
    }

    #[test]
    fn a_rule_that_says_nothing_inherits() {
        let r = rule("/w");
        assert!(r.policy.is_empty());
        assert_eq!(r.policy.get(Safety::Write), None);
        // And it names itself from the folder rather than showing an empty string.
        assert_eq!(r.display_name(), "w");
    }

    #[test]
    fn a_tool_override_outranks_the_class_it_belongs_to() {
        use crate::config::app_config::McpPolicy;
        let global = McpPolicy::default();
        let mut r = rule("/w");
        r.policy.write = Some(McpDecision::Deny);
        r.tools.insert("bennu_write_file".into(), McpDecision::Allow);

        // The whole reason a per-tool override exists: saying something about one
        // endpoint that loosening its class would say about every endpoint in it.
        assert_eq!(resolve(&r, "bennu_write_file", Safety::Write, &global), McpDecision::Allow);
        assert_eq!(resolve(&r, "some_other_write", Safety::Write, &global), McpDecision::Deny);
    }

    #[test]
    fn a_rule_naming_neither_falls_through_to_the_profile() {
        use crate::config::app_config::McpPolicy;
        let global = McpPolicy { read: McpDecision::Allow, ..McpPolicy::default() };
        // Silence at both levels is inheritance, not a decision — that is what keeps a
        // rule following the profile as the profile changes.
        assert_eq!(resolve(&rule("/w"), "bennu_read_file", Safety::Read, &global), McpDecision::Allow);
    }

    #[test]
    fn a_by_product_refusal_separates_the_two_ways_to_be_out() {
        // "not open anywhere" and "open, but in another product" send the user to two
        // different places, so one sentence covering both sends them to the wrong one.
        let elsewhere = scope_refusal(&McpScopeMode::ByProduct, "bennu", "/p", true);
        let nowhere = scope_refusal(&McpScopeMode::ByProduct, "bennu", "/p", false);
        assert!(elsewhere.contains("open in Arbor, but not in bennu"), "{elsewhere}");
        assert!(nowhere.contains("No project at `/p` is open in bennu"), "{nowhere}");
        assert_ne!(elsewhere, nowhere);

        // The other modes keep the generic sentence, which names the project list —
        // naming a product there would be advice that does not apply.
        let generic = scope_refusal(&McpScopeMode::Allowlist, "bennu", "/p", false);
        assert!(generic.contains("project list"), "{generic}");
        assert!(!generic.contains("bennu"), "{generic}");
    }

    #[test]
    fn a_sibling_directory_is_outside() {
        assert!(is_within(Path::new("/home/u/proj"), Path::new("/home/u/proj/src/A.java")));
        assert!(is_within(Path::new("/home/u/proj"), Path::new("/home/u/proj")));
        assert!(!is_within(Path::new("/home/u/proj"), Path::new("/home/u/other")));
        // Prefix-of-a-name is not prefix-of-a-path.
        assert!(!is_within(Path::new("/home/u/proj"), Path::new("/home/u/proj-secrets/f")));
    }
}

