//! Recipes: named stacks of services, in a per-project TOML file.
//!
//! ```toml
//! [[recipe]]
//! name = "shop"
//! description = "Gateway plus the orders service"
//! stop_order = ["gateway", "orders"]          # optional; default: reverse of the start order
//!
//!   [[recipe.service]]
//!   name = "orders"
//!   command = "mvn -q spring-boot:run"
//!   cwd = "orders"                            # relative to the project; may not leave it
//!   env = { SPRING_PROFILES_ACTIVE = "dev" }
//!   ports = [8081]
//!   ready = { port = 8081, log = "Started .*Application", timeout_s = 120 }
//!
//!   [[recipe.service]]
//!   name = "gateway"
//!   command = "npm run dev"
//!   depends_on = ["orders"]
//!   ready = { http = "http://localhost:3000/health", status = 200 }
//! ```
//!
//! Parsing validates everything it can without running anything: names, dependency cycles, regexes,
//! loopback-only health URLs, a working directory that stays inside the project. The file's content
//! hash is what an approval binds to — change a command and the user is asked again.

use std::collections::{BTreeMap, HashSet};
use std::path::{Component, Path, PathBuf};
use std::time::Duration;

use regex::Regex;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::http::parse_local_url;
use crate::ready::{Check, Readiness, DEFAULT_TIMEOUT, MAX_TIMEOUT};

// ── Raw (file) shape ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawFile {
    #[serde(default)]
    recipe: Vec<RawRecipe>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawRecipe {
    name: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    stop_order: Vec<String>,
    #[serde(default)]
    service: Vec<RawService>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawService {
    name: String,
    command: String,
    #[serde(default)]
    cwd: Option<String>,
    #[serde(default)]
    env: BTreeMap<String, String>,
    #[serde(default)]
    ports: Vec<u16>,
    #[serde(default)]
    depends_on: Vec<String>,
    #[serde(default)]
    ready: Option<RawReady>,
    #[serde(default)]
    max_minutes: Option<u64>,
}

#[derive(Debug, Default, Deserialize, Serialize, Clone)]
#[serde(deny_unknown_fields)]
pub struct RawReady {
    #[serde(default)]
    pub port: Option<u16>,
    #[serde(default)]
    pub log: Option<String>,
    #[serde(default)]
    pub http: Option<String>,
    #[serde(default)]
    pub status: Option<u16>,
    #[serde(default)]
    pub delay_ms: Option<u64>,
    #[serde(default)]
    pub timeout_s: Option<u64>,
}

impl RawReady {
    /// The checks this spec stands for, with the timeout. Invalid parts were rejected at parse time;
    /// here a bad regex is skipped rather than panicking.
    pub fn to_readiness(&self) -> Readiness {
        let mut checks = Vec::new();
        if let Some(p) = self.port {
            checks.push(Check::Port(p));
        }
        if let Some(re) = self.log.as_deref().and_then(|s| Regex::new(s).ok()) {
            checks.push(Check::Log(re));
        }
        if let Some(url) = &self.http {
            checks.push(Check::Http { url: url.clone(), status: self.status });
        }
        if let Some(ms) = self.delay_ms {
            checks.push(Check::Delay(Duration::from_millis(ms)));
        }
        let timeout = self.timeout_s.map(Duration::from_secs).unwrap_or(DEFAULT_TIMEOUT).min(MAX_TIMEOUT);
        Readiness { checks, timeout }
    }

    /// Problems with the spec, appended to errors under service's name.
    pub fn validate(&self, service: &str, errors: &mut Vec<String>) {
        if let Some(re) = &self.log {
            if let Err(e) = Regex::new(re) {
                errors.push(format!("service `{service}`: ready.log is not a valid regex: {e}"));
            }
        }
        if let Some(url) = &self.http {
            if let Err(e) = parse_local_url(url) {
                errors.push(format!("service `{service}`: ready.http: {e}"));
            }
        }
        if self.status.is_some() && self.http.is_none() {
            errors.push(format!("service `{service}`: ready.status needs ready.http"));
        }
    }
}

// ── Validated shape ──────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct Service {
    pub name: String,
    pub command: String,
    /// Relative to the project root.
    pub cwd: String,
    pub env: BTreeMap<String, String>,
    pub ports: Vec<u16>,
    pub depends_on: Vec<String>,
    pub ready: RawReady,
    /// Overrides the manager's default lifetime cap.
    pub max_minutes: Option<u64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Recipe {
    pub name: String,
    pub description: String,
    pub services: Vec<Service>,
    pub stop_order: Vec<String>,
}

impl Recipe {
    /// Services in an order where every one comes after what it depends on; ties keep file order.
    pub fn start_order(&self) -> Vec<&Service> {
        let mut done: Vec<&Service> = Vec::new();
        let mut remaining: Vec<&Service> = self.services.iter().collect();
        while !remaining.is_empty() {
            let next = remaining.iter().position(|s| s.depends_on.iter().all(|d| done.iter().any(|x| &x.name == d)));
            // Validation rejected cycles; if one slipped through, take the rest in file order.
            let i = next.unwrap_or(0);
            done.push(remaining.remove(i));
        }
        done
    }

    /// The order to stop in: `stop_order` first, then whatever it did not name, last-started first.
    pub fn stop_sequence(&self) -> Vec<&Service> {
        let mut out: Vec<&Service> = self.stop_order.iter().filter_map(|n| self.services.iter().find(|s| &s.name == n)).collect();
        for s in self.start_order().into_iter().rev() {
            if !out.iter().any(|x| x.name == s.name) {
                out.push(s);
            }
        }
        out
    }

    pub fn service(&self, name: &str) -> Option<&Service> {
        self.services.iter().find(|s| s.name == name)
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct RecipeFile {
    pub recipes: Vec<Recipe>,
    /// SHA-256 of the file, line endings normalised. What an approval is bound to.
    pub hash: String,
}

impl RecipeFile {
    pub fn recipe(&self, name: &str) -> Option<&Recipe> {
        self.recipes.iter().find(|r| r.name.eq_ignore_ascii_case(name))
    }
}

/// Hash of a file's text. CRLF and LF hash alike so a checkout with another line-ending setting does
/// not ask the user again for a recipe that did not change.
pub fn content_hash(text: &str) -> String {
    let normalised = text.replace("\r\n", "\n");
    let digest = Sha256::digest(normalised.as_bytes());
    digest.iter().map(|b| format!("{b:02x}")).collect()
}

fn valid_name(name: &str) -> bool {
    !name.is_empty() && name.len() <= 48 && name.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.'))
}

/// Parse and validate a recipe file. All problems are reported together, so a user fixes the file once.
pub fn parse_recipes(text: &str) -> Result<RecipeFile, Vec<String>> {
    let raw: RawFile = toml::from_str(text).map_err(|e| vec![format!("recipes file: {}", e.message())])?;
    let mut errors = Vec::new();
    let mut recipes = Vec::new();
    let mut seen_recipes = HashSet::new();
    for r in raw.recipe {
        if !valid_name(&r.name) {
            errors.push(format!("recipe name `{}` must be 1-48 letters, digits, `_`, `-` or `.`", r.name));
        }
        if !seen_recipes.insert(r.name.to_ascii_lowercase()) {
            errors.push(format!("recipe `{}` is defined twice", r.name));
        }
        if r.service.is_empty() {
            errors.push(format!("recipe `{}` has no [[recipe.service]]", r.name));
        }
        let mut services = Vec::new();
        let mut seen = HashSet::new();
        let mut ports_seen: Vec<(u16, String)> = Vec::new();
        for s in r.service {
            let ctx = format!("recipe `{}`, service `{}`", r.name, s.name);
            if !valid_name(&s.name) {
                errors.push(format!("{ctx}: the name must be 1-48 letters, digits, `_`, `-` or `.`"));
            }
            if !seen.insert(s.name.clone()) {
                errors.push(format!("{ctx}: defined twice"));
            }
            if s.command.trim().is_empty() {
                errors.push(format!("{ctx}: command is empty"));
            }
            let cwd = s.cwd.clone().unwrap_or_else(|| ".".to_string());
            if let Err(e) = resolve_cwd(Path::new("/project"), &cwd) {
                errors.push(format!("{ctx}: {e}"));
            }
            for k in s.env.keys() {
                if k.is_empty() || k.contains(['=', '\0']) {
                    errors.push(format!("{ctx}: `{k}` is not an environment variable name"));
                }
            }
            for p in &s.ports {
                if let Some((_, other)) = ports_seen.iter().find(|(q, _)| q == p) {
                    errors.push(format!("{ctx}: port {p} is also declared by `{other}`"));
                }
                ports_seen.push((*p, s.name.clone()));
            }
            let ready = s.ready.clone().unwrap_or_default();
            ready.validate(&s.name, &mut errors);
            services.push(Service { name: s.name, command: s.command, cwd, env: s.env, ports: s.ports, depends_on: s.depends_on, ready, max_minutes: s.max_minutes });
        }
        check_graph(&r.name, &services, &r.stop_order, &mut errors);
        recipes.push(Recipe { name: r.name, description: r.description, services, stop_order: r.stop_order });
    }
    if errors.is_empty() {
        Ok(RecipeFile { recipes, hash: content_hash(text) })
    } else {
        Err(errors)
    }
}

/// Dependencies exist and do not loop; `stop_order` names real services.
fn check_graph(recipe: &str, services: &[Service], stop_order: &[String], errors: &mut Vec<String>) {
    let names: HashSet<&str> = services.iter().map(|s| s.name.as_str()).collect();
    for s in services {
        for d in &s.depends_on {
            if d == &s.name {
                errors.push(format!("recipe `{recipe}`, service `{}`: depends on itself", s.name));
            } else if !names.contains(d.as_str()) {
                errors.push(format!("recipe `{recipe}`, service `{}`: depends_on `{d}` is not a service of this recipe", s.name));
            }
        }
    }
    for n in stop_order {
        if !names.contains(n.as_str()) {
            errors.push(format!("recipe `{recipe}`: stop_order names `{n}`, which is not a service"));
        }
    }
    // Kahn's algorithm: whatever cannot be placed is in or behind a cycle.
    let mut placed: HashSet<&str> = HashSet::new();
    loop {
        let before = placed.len();
        for s in services {
            if !placed.contains(s.name.as_str()) && s.depends_on.iter().all(|d| placed.contains(d.as_str()) || !names.contains(d.as_str())) {
                placed.insert(&s.name);
            }
        }
        if placed.len() == before {
            break;
        }
    }
    if placed.len() < services.len() {
        let stuck: Vec<&str> = services.iter().map(|s| s.name.as_str()).filter(|n| !placed.contains(n)).collect();
        errors.push(format!("recipe `{recipe}`: dependency cycle among {}", stuck.join(", ")));
    }
}

/// `cwd` inside `root`, lexically: absolute paths and `..` that climbs out are refused. (A symlink
/// inside the project that points out is not detected here; the user wrote or approved the file.)
pub fn resolve_cwd(root: &Path, cwd: &str) -> Result<PathBuf, String> {
    let rel = Path::new(cwd.trim());
    let trimmed = cwd.trim();
    let drive_like = trimmed.as_bytes().get(1) == Some(&b':');
    if rel.is_absolute() || rel.has_root() || drive_like || trimmed.starts_with(['/', '\\']) {
        return Err(format!("cwd `{cwd}` must be relative to the project"));
    }
    let mut out = root.to_path_buf();
    let mut depth = 0usize;
    for c in rel.components() {
        match c {
            Component::CurDir => {}
            Component::Normal(p) => {
                out.push(p);
                depth += 1;
            }
            Component::ParentDir => {
                if depth == 0 {
                    return Err(format!("cwd `{cwd}` leaves the project"));
                }
                out.pop();
                depth -= 1;
            }
            _ => return Err(format!("cwd `{cwd}` must be relative to the project")),
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    const GOOD: &str = r#"
[[recipe]]
name = "shop"
description = "demo"
stop_order = ["web"]

  [[recipe.service]]
  name = "web"
  command = "npm run dev"
  depends_on = ["api"]
  ready = { http = "http://localhost:3000/health", status = 200 }

  [[recipe.service]]
  name = "api"
  command = "mvn spring-boot:run"
  cwd = "api"
  env = { PROFILE = "dev" }
  ports = [8081]
  ready = { port = 8081, log = "Started .*", timeout_s = 90 }
"#;

    #[test]
    fn a_good_file_parses_and_orders_dependencies_first() {
        let f = parse_recipes(GOOD).unwrap();
        let r = f.recipe("SHOP").unwrap();
        assert_eq!(r.start_order().iter().map(|s| s.name.as_str()).collect::<Vec<_>>(), vec!["api", "web"]);
        assert_eq!(r.stop_sequence().iter().map(|s| s.name.as_str()).collect::<Vec<_>>(), vec!["web", "api"]);
        let ready = r.service("api").unwrap().ready.to_readiness();
        assert_eq!(ready.checks.len(), 2);
        assert_eq!(ready.timeout, Duration::from_secs(90));
        assert_eq!(f.hash.len(), 64);
    }

    #[test]
    fn the_hash_ignores_line_endings_but_not_a_changed_command() {
        assert_eq!(content_hash("a\r\nb\r\n"), content_hash("a\nb\n"));
        assert_ne!(content_hash(GOOD), content_hash(&GOOD.replace("npm run dev", "npm run evil")));
    }

    fn errors(text: &str) -> String {
        parse_recipes(text).unwrap_err().join(" | ")
    }

    #[test]
    fn every_kind_of_mistake_is_reported() {
        let bad = r#"
[[recipe]]
name = "bad name!"
  [[recipe.service]]
  name = "a"
  command = ""
  cwd = "../outside"
  depends_on = ["b", "ghost"]
  ports = [80]
  ready = { log = "(", http = "http://example.com/", status = 200 }
  [[recipe.service]]
  name = "b"
  command = "x"
  depends_on = ["a"]
  ports = [80]
"#;
        let e = errors(bad);
        for needle in ["bad name!", "command is empty", "leaves the project", "`ghost`", "dependency cycle", "valid regex", "not loopback", "port 80 is also"] {
            assert!(e.contains(needle), "missing `{needle}` in: {e}");
        }
    }

    #[test]
    fn structure_errors_are_reported() {
        assert!(errors("[[recipe]]\nname = \"x\"\n").contains("no [[recipe.service]]"));
        assert!(errors("[[recipe]]\nname=\"x\"\nbogus = 1\n").contains("unknown field"));
        let dup = "[[recipe]]\nname=\"x\"\n[[recipe.service]]\nname=\"s\"\ncommand=\"c\"\n[[recipe]]\nname=\"X\"\n[[recipe.service]]\nname=\"s\"\ncommand=\"c\"\n";
        assert!(errors(dup).contains("defined twice"));
        let stop = "[[recipe]]\nname=\"x\"\nstop_order=[\"nope\"]\n[[recipe.service]]\nname=\"s\"\ncommand=\"c\"\n";
        assert!(errors(stop).contains("stop_order"));
    }

    #[test]
    fn an_empty_file_is_a_file_with_no_recipes() {
        assert!(parse_recipes("").unwrap().recipes.is_empty());
    }

    #[test]
    fn cwd_stays_inside_the_project() {
        let root = Path::new("/p");
        assert_eq!(resolve_cwd(root, ".").unwrap(), PathBuf::from("/p"));
        assert_eq!(resolve_cwd(root, "a/b").unwrap(), PathBuf::from("/p/a/b"));
        assert_eq!(resolve_cwd(root, "a/../b").unwrap(), PathBuf::from("/p/b"));
        assert!(resolve_cwd(root, "..").is_err());
        assert!(resolve_cwd(root, "a/../../b").is_err());
        assert!(resolve_cwd(root, "/etc").is_err());
        assert!(resolve_cwd(root, "C:\\x").is_err());
    }
}
