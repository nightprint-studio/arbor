//! `tomcat` domain — link a project to a local Tomcat and **hot-swap** its JSPs into the deployed
//! (exploded) webapp so Jasper recompiles them on next request — no redeploy, no server restart.
//!
//! Tomcat's Jasper runs in development mode by default: it recompiles a JSP whenever the file on
//! disk is newer than its generated servlet. So "deploying" a changed JSP is just **copying the file**
//! into `<tomcat>/webapps/<context>/…` at the same relative path it has under the project's webapp
//! source dir (`src/main/webapp` &co.). The next browser refresh picks it up.
//!
//! Two moving parts:
//!   * **config** — a per-repo `[bennu.tomcat]` section in `<repo>/.arbor/config.toml` (CLAUDE.md
//!     rule #11: filesystem, never localStorage). Same merge discipline as [`crate::run_config`]:
//!     parse the shared file into a dynamic table, replace only `bennu.tomcat`, write it back, so
//!     corvus's own keys in that file survive.
//!   * **smart resolution** — from the Tomcat root we auto-detect the deployed context directory
//!     (the single non-system exploded webapp, or the one whose name matches the project /
//!     `<finalName>` / artifactId), so the user picks only the Tomcat folder, not the context.
//!
//! Handlers: `bennu_get_tomcat_config` / `bennu_set_tomcat_config` (persistence),
//! `bennu_detect_tomcat` (validate + resolve, for the settings modal), `bennu_hotswap_jsp`
//! (copy one JSP or every JSP; fires a success/error toast over the event sink).

use std::path::{Path, PathBuf};

use bennu_core::prelude::BennuState;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::web_discovery::{discover_jsp_files, is_jsp_family, source_webapp_dir};

/// Tomcat context dirs under `webapps/` that ship with the server — never a user deployment, so
/// they're excluded from auto-detection.
const SYSTEM_WEBAPPS: &[&str] = &["ROOT", "manager", "host-manager", "docs", "examples"];

// ── config (`<repo>/.arbor/config.toml` `[bennu.tomcat]`) ────────────────────────

/// Per-repo Tomcat link. `webapp_name` empty = auto-detect the deployed context at swap time.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct TomcatConfig {
    /// The `CATALINA_BASE`/`CATALINA_HOME` directory the user picked (holds `webapps/`, `bin/`, …).
    #[serde(default)]
    pub tomcat_root: String,
    /// The deployed context directory name under `<tomcat_root>/webapps/` (auto-detected on link,
    /// user-overridable). Empty ⇒ resolve automatically each time.
    #[serde(default)]
    pub webapp_name: String,
}

/// Args carrying just a project `root`.
#[derive(Deserialize)]
pub struct RootArgs {
    pub root: String,
}

/// Args for [`bennu_set_tomcat_config`].
#[derive(Deserialize)]
pub struct SetTomcatConfigArgs {
    pub root: String,
    pub config: TomcatConfig,
}

/// Args for [`bennu_detect_tomcat`] — probe a candidate Tomcat root against the project.
#[derive(Deserialize)]
pub struct DetectTomcatArgs {
    pub root: String,
    pub tomcat_root: String,
}

/// Args for [`bennu_hotswap_jsp`]. `file` present ⇒ swap just that JSP; absent ⇒ swap every JSP.
#[derive(Deserialize)]
pub struct HotSwapArgs {
    pub root: String,
    #[serde(default)]
    pub file: Option<String>,
}

/// What [`bennu_detect_tomcat`] found — drives the settings modal (validity, the deployable
/// contexts, the best-guess pick, and how many JSPs the project would deploy).
#[derive(Debug, Clone, Default, Serialize)]
pub struct TomcatDetection {
    /// The `tomcat_root` looks like a Tomcat install (a `webapps/` directory exists).
    pub valid: bool,
    /// The project's webapp source dir (forward-slashed), or empty when it isn't a web project.
    pub source_webapp: String,
    /// Deployable exploded context names under `webapps/` (system apps excluded).
    pub webapps: Vec<String>,
    /// The best-match context for this project (empty when ambiguous / none deployed).
    pub suggested: String,
    /// Number of JSP-family files under the source webapp dir (what a full swap would copy).
    pub jsp_count: usize,
}

/// The outcome of a hot-swap — how many files landed, where, and how many were skipped.
#[derive(Debug, Clone, Default, Serialize)]
pub struct HotSwapResult {
    /// Files copied into the deployed webapp.
    pub copied: usize,
    /// The deployed webapp directory they were copied into (forward-slashed).
    pub target_dir: String,
    /// The context name that was resolved (`webapps/<name>`).
    pub webapp_name: String,
}

// ── handlers ─────────────────────────────────────────────────────────────────────

/// Read the per-repo Tomcat link from `<root>/.arbor/config.toml` `[bennu.tomcat]`. A project that
/// was never linked yields the default (empty) config — never an error.
#[arbor_rpc::handler]
fn bennu_get_tomcat_config(_ctx: &BennuState, args: RootArgs) -> Result<TomcatConfig, String> {
    Ok(load_tomcat_config(&args.root))
}

/// Persist the per-repo Tomcat link, preserving every other section of `.arbor/config.toml`.
#[arbor_rpc::handler]
fn bennu_set_tomcat_config(_ctx: &BennuState, args: SetTomcatConfigArgs) -> Result<(), String> {
    save_tomcat_config(&args.root, &args.config)
}

/// Validate a candidate Tomcat root against the project + resolve the best-match deployed context —
/// so the settings modal can confirm the folder is a Tomcat, list the deployable webapps, and
/// preselect the one this project maps to.
#[arbor_rpc::handler]
fn bennu_detect_tomcat(_ctx: &BennuState, args: DetectTomcatArgs) -> Result<TomcatDetection, String> {
    let tomcat = PathBuf::from(&args.tomcat_root);
    let webapps_dir = tomcat.join("webapps");
    let valid = webapps_dir.is_dir();
    let source = source_webapp_dir(Path::new(&args.root));
    let jsp_count = source.as_deref().map(|p| discover_jsp_files(p).len()).unwrap_or(0);
    let webapps = if valid { deployed_webapps(&webapps_dir) } else { Vec::new() };
    let suggested = suggest_context(&webapps, &args.root);
    Ok(TomcatDetection {
        valid,
        source_webapp: source.as_deref().map(fwd).unwrap_or_default(),
        webapps,
        suggested,
        jsp_count,
    })
}

/// Hot-swap the project's JSP(s) into the linked Tomcat's deployed webapp. With `file`, copies that
/// single JSP; without, copies every JSP under the webapp source dir. Fires a `plugin:notification`
/// toast (success with the count, or the failure reason) and returns the structured result.
#[arbor_rpc::handler]
fn bennu_hotswap_jsp(ctx: &BennuState, args: HotSwapArgs) -> Result<HotSwapResult, String> {
    let outcome = run_hotswap(&args.root, args.file.as_deref());
    let sink = ctx.event_sink();
    match &outcome {
        Ok(r) if args.file.is_some() => notify(
            &sink,
            "JSP deployed",
            &format!("Copied to {} in Tomcat", r.webapp_name),
            "success",
        ),
        Ok(r) => notify(
            &sink,
            "JSPs deployed",
            &format!("Copied {} JSP(s) to {} in Tomcat", r.copied, r.webapp_name),
            "success",
        ),
        Err(e) => notify(&sink, "JSP hot-swap failed", e, "error"),
    }
    outcome
}

// ── hot-swap core ─────────────────────────────────────────────────────────────────

/// Resolve the link + webapp dirs and copy the JSP(s). Kept separate from the handler so it stays a
/// pure(ish) function of the filesystem — the handler only adds the toast + sink.
fn run_hotswap(root: &str, file: Option<&str>) -> Result<HotSwapResult, String> {
    let cfg = load_tomcat_config(root);
    if cfg.tomcat_root.trim().is_empty() {
        return Err("No Tomcat is linked to this project — set one in Tomcat settings.".to_string());
    }
    let source = source_webapp_dir(Path::new(root))
        .ok_or_else(|| "This project has no webapp source dir (src/main/webapp).".to_string())?;
    let webapps_dir = PathBuf::from(&cfg.tomcat_root).join("webapps");
    if !webapps_dir.is_dir() {
        return Err(format!("`{}` doesn't look like a Tomcat (no webapps/).", cfg.tomcat_root));
    }
    let context = resolve_context(&cfg, &webapps_dir, root)?;
    let target_root = webapps_dir.join(&context);

    let copied = match file {
        Some(f) => {
            copy_one(&source, &target_root, Path::new(f))?;
            1
        }
        None => copy_all(&source, &target_root)?,
    };
    Ok(HotSwapResult { copied, target_dir: fwd(&target_root), webapp_name: context })
}

/// Copy every JSP-family file under `source` into `target_root`, preserving each file's path
/// relative to `source`. Returns the number copied.
fn copy_all(source: &Path, target_root: &Path) -> Result<usize, String> {
    let mut n = 0;
    for jsp in discover_jsp_files(source) {
        let rel = jsp.strip_prefix(source).map_err(|e| e.to_string())?;
        copy_into(&jsp, &target_root.join(rel))?;
        n += 1;
    }
    Ok(n)
}

/// Copy a single JSP `file` (absolute) into `target_root` at its path relative to `source`.
fn copy_one(source: &Path, target_root: &Path, file: &Path) -> Result<(), String> {
    if !is_jsp_family(&file.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()) {
        return Err("The active file is not a JSP.".to_string());
    }
    let rel = file.strip_prefix(source).map_err(|_| {
        "The JSP is not under this project's webapp source dir, so its deployed path is unknown."
            .to_string()
    })?;
    copy_into(file, &target_root.join(rel))
}

/// Copy `src` (bytes verbatim — the page's own encoding is preserved) to `dst`, creating parent
/// dirs. Byte copy, not a decode/re-encode: Tomcat reads the JSP with its declared page encoding.
fn copy_into(src: &Path, dst: &Path) -> Result<(), String> {
    if let Some(parent) = dst.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("create {}: {e}", fwd(parent)))?;
    }
    std::fs::copy(src, dst).map(|_| ()).map_err(|e| format!("copy to {}: {e}", fwd(dst)))
}

// ── context resolution (which webapps/ dir this project deploys to) ────────────────

/// The deployed context dir for a swap: the configured `webapp_name` (validated to exist), else the
/// auto-suggested one, else an error naming why it's ambiguous.
fn resolve_context(cfg: &TomcatConfig, webapps_dir: &Path, root: &str) -> Result<String, String> {
    let deployed = deployed_webapps(webapps_dir);
    if !cfg.webapp_name.trim().is_empty() {
        let name = cfg.webapp_name.trim().to_string();
        return if webapps_dir.join(&name).is_dir() {
            Ok(name)
        } else {
            Err(format!("The linked webapp `{name}` isn't deployed under Tomcat's webapps/."))
        };
    }
    let suggested = suggest_context(&deployed, root);
    if !suggested.is_empty() {
        return Ok(suggested);
    }
    if deployed.is_empty() {
        Err("No deployed web application found under Tomcat's webapps/.".to_string())
    } else {
        Err("Multiple web apps are deployed — pick which one in Tomcat settings.".to_string())
    }
}

/// Deployable exploded context names under `webapps/`: directories that hold a `WEB-INF/`, minus the
/// system apps (ROOT / manager / …). Sorted for a stable UI order.
fn deployed_webapps(webapps_dir: &Path) -> Vec<String> {
    let mut out: Vec<String> = std::fs::read_dir(webapps_dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            let path = e.path();
            if !path.join("WEB-INF").is_dir() {
                return None;
            }
            let name = e.file_name().to_string_lossy().into_owned();
            (!SYSTEM_WEBAPPS.iter().any(|s| s.eq_ignore_ascii_case(&name))).then_some(name)
        })
        .collect();
    out.sort();
    out
}

/// The best-match deployed context for the project: a `webapps/` name that matches (case-insensitive)
/// one of the project's context candidates (dir name / `<finalName>` / artifactId), else — when the
/// server hosts exactly one user webapp — that one. Empty when ambiguous or nothing's deployed.
fn suggest_context(deployed: &[String], root: &str) -> String {
    if deployed.is_empty() {
        return String::new();
    }
    let candidates = context_candidates(root);
    for cand in &candidates {
        if let Some(hit) = deployed.iter().find(|d| d.eq_ignore_ascii_case(cand)) {
            return hit.clone();
        }
    }
    if deployed.len() == 1 {
        return deployed[0].clone();
    }
    String::new()
}

/// The context-name candidates a project could deploy under, best first: the `<finalName>` from the
/// pom (explicit war name), then the artifactId, then the project directory name.
fn context_candidates(root: &str) -> Vec<String> {
    let mut out = Vec::new();
    let pom = PathBuf::from(root).join("pom.xml");
    if let Ok(text) = std::fs::read_to_string(&pom) {
        if let Some(fin) = xml_first_tag(&text, "finalName") {
            out.push(fin);
        }
        if let Some(art) = pom_artifact_id(&text) {
            out.push(art);
        }
    }
    if let Some(name) = PathBuf::from(root).file_name().map(|n| n.to_string_lossy().into_owned()) {
        out.push(name);
    }
    out.retain(|s| !s.trim().is_empty());
    out
}

/// The project's own `<artifactId>` — the first one OUTSIDE the `<parent>…</parent>` block (a
/// child pom lists the parent's artifactId first). Best-effort text extraction.
fn pom_artifact_id(pom: &str) -> Option<String> {
    let without_parent = match (pom.find("<parent>"), pom.find("</parent>")) {
        (Some(a), Some(b)) if b > a => {
            let mut s = String::with_capacity(pom.len());
            s.push_str(&pom[..a]);
            s.push_str(&pom[b + "</parent>".len()..]);
            s
        }
        _ => pom.to_string(),
    };
    xml_first_tag(&without_parent, "artifactId")
}

/// The trimmed text content of the first `<tag>…</tag>` in `xml`, or `None`.
fn xml_first_tag(xml: &str, tag: &str) -> Option<String> {
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");
    let start = xml.find(&open)? + open.len();
    let end = xml[start..].find(&close)? + start;
    let val = xml[start..end].trim();
    (!val.is_empty()).then(|| val.to_string())
}

// ── persistence (TOML-table merge over the shared `.arbor/config.toml`) ────────────

/// `<repo>/.arbor/config.toml`.
fn config_path(root: &str) -> PathBuf {
    PathBuf::from(root).join(".arbor").join("config.toml")
}

/// Read `bennu.tomcat` from `<root>/.arbor/config.toml`, or the empty default (missing/corrupt).
fn load_tomcat_config(root: &str) -> TomcatConfig {
    let table = read_table(root);
    match table.get("bennu").and_then(|b| b.get("tomcat")) {
        Some(t) => t.clone().try_into().unwrap_or_default(),
        None => TomcatConfig::default(),
    }
}

/// Merge `cfg` into `bennu.tomcat` of the on-disk table (creating `.arbor/` as needed) and write the
/// whole file back, so unrelated sections (corvus's, bennu.run's) survive byte-for-byte.
fn save_tomcat_config(root: &str, cfg: &TomcatConfig) -> Result<(), String> {
    let mut table = read_table(root);
    let value = toml::Value::try_from(cfg).map_err(|e| e.to_string())?;
    let bennu = table
        .entry("bennu".to_string())
        .or_insert_with(|| toml::Value::Table(toml::value::Table::new()));
    let bennu_tbl = bennu
        .as_table_mut()
        .ok_or_else(|| "`.arbor/config.toml` `[bennu]` is not a table".to_string())?;
    bennu_tbl.insert("tomcat".to_string(), value);

    let path = config_path(root);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let text = toml::to_string_pretty(&table).map_err(|e| e.to_string())?;
    std::fs::write(&path, text).map_err(|e| e.to_string())
}

/// Parse `<root>/.arbor/config.toml` into a dynamic table (empty on missing/unparseable file).
fn read_table(root: &str) -> toml::value::Table {
    let Ok(text) = std::fs::read_to_string(config_path(root)) else {
        return toml::value::Table::new();
    };
    text.parse::<toml::Value>()
        .ok()
        .and_then(|v| v.as_table().cloned())
        .unwrap_or_default()
}

// ── small helpers ──────────────────────────────────────────────────────────────────

/// Forward-slash a path for the FE / cross-platform display.
fn fwd(p: &Path) -> String {
    p.to_string_lossy().replace('\\', "/")
}

/// Emit a toast to the bennu window (`plugin:notification`, re-emitted by the shell). `target`
/// MUST be `"bennu"`: the feedback router drops untagged notifications for a non-main host, so a
/// missing target silently shows nothing. `persist:false` + `toast:true` makes it a transient
/// toast (a deploy confirmation) rather than a bell-archived notification.
fn notify(sink: &std::sync::Arc<dyn arbor_ipc::prelude::EventSink>, title: &str, message: &str, level: &str) {
    sink.emit(
        "plugin:notification",
        json!({
            "plugin": "bennu",
            "target": "bennu",
            "title": title,
            "message": message,
            "level": level,
            "persist": false,
            "toast": true,
        }),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("bennu-tomcat-{}-{tag}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn config_round_trips_and_merge_preserves_siblings() {
        let root = tmp("cfg");
        // Seed a sibling corvus section — it must survive our write.
        std::fs::create_dir_all(root.join(".arbor")).unwrap();
        std::fs::write(root.join(".arbor/config.toml"), "[corvus]\ndisplay_name = \"keep\"\n").unwrap();

        let cfg = TomcatConfig { tomcat_root: "/opt/tomcat".into(), webapp_name: "app".into() };
        save_tomcat_config(root.to_str().unwrap(), &cfg).unwrap();
        let back = load_tomcat_config(root.to_str().unwrap());
        assert_eq!(back, cfg);

        let text = std::fs::read_to_string(root.join(".arbor/config.toml")).unwrap();
        assert!(text.contains("keep"), "sibling corvus section clobbered:\n{text}");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn missing_config_is_empty_default() {
        let root = tmp("missing");
        assert_eq!(load_tomcat_config(root.to_str().unwrap()), TomcatConfig::default());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn deployed_webapps_excludes_system_and_war_only() {
        let webapps = tmp("webapps");
        for app in ["ROOT", "manager", "myapp", "other"] {
            std::fs::create_dir_all(webapps.join(app).join("WEB-INF")).unwrap();
        }
        // A bare dir with no WEB-INF isn't a webapp.
        std::fs::create_dir_all(webapps.join("stray")).unwrap();
        let got = deployed_webapps(&webapps);
        assert_eq!(got, vec!["myapp".to_string(), "other".to_string()]);
        let _ = std::fs::remove_dir_all(&webapps);
    }

    #[test]
    fn suggest_matches_by_name_then_falls_back_to_single() {
        // The project directory has to be *named* `myapp` for the name match to
        // mean anything — `tmp` prefixes its tag with the process id, so the
        // candidate it produced was `bennu-tomcat-1234-myapp` and matched nothing.
        let proj = tmp("suggest").join("myapp");
        std::fs::create_dir_all(&proj).unwrap();
        // Two deployed apps → matches the project dir name.
        assert_eq!(suggest_context(&["myapp".into(), "other".into()], proj.to_str().unwrap()), "myapp");
        // Ambiguous, no name match → empty (user must pick).
        assert_eq!(suggest_context(&["a".into(), "b".into()], proj.to_str().unwrap()), "");
        // Exactly one deployed → that one, regardless of name.
        assert_eq!(suggest_context(&["whatever".into()], proj.to_str().unwrap()), "whatever");
        let _ = std::fs::remove_dir_all(&proj);
    }

    #[test]
    fn pom_artifact_id_ignores_parent() {
        let pom = r#"<project>
          <parent><artifactId>parent-pom</artifactId></parent>
          <artifactId>the-webapp</artifactId>
          <build><finalName>deployed-name</finalName></build>
        </project>"#;
        assert_eq!(pom_artifact_id(pom).as_deref(), Some("the-webapp"));
        assert_eq!(xml_first_tag(pom, "finalName").as_deref(), Some("deployed-name"));
    }

    #[test]
    fn copy_all_preserves_relative_paths() {
        let source = tmp("src-webapp");
        std::fs::create_dir_all(source.join("WEB-INF/jsp")).unwrap();
        std::fs::write(source.join("index.jsp"), b"<html>").unwrap();
        std::fs::write(source.join("WEB-INF/jsp/tree.jsp"), b"<tree>").unwrap();
        std::fs::write(source.join("style.css"), b"x").unwrap(); // not a JSP → skipped

        let target = tmp("deployed");
        let n = copy_all(&source, &target).unwrap();
        assert_eq!(n, 2, "only the two JSPs");
        assert!(target.join("index.jsp").is_file());
        assert!(target.join("WEB-INF/jsp/tree.jsp").is_file(), "nested path preserved");
        assert!(!target.join("style.css").exists(), "non-JSP not copied");
        let _ = std::fs::remove_dir_all(&source);
        let _ = std::fs::remove_dir_all(&target);
    }

    #[test]
    fn copy_one_rejects_file_outside_webapp() {
        let source = tmp("src-webapp2");
        std::fs::create_dir_all(&source).unwrap();
        let target = tmp("deployed2");
        let outside = tmp("elsewhere").join("foo.jsp");
        std::fs::create_dir_all(outside.parent().unwrap()).unwrap();
        std::fs::write(&outside, b"x").unwrap();
        assert!(copy_one(&source, &target, &outside).is_err());
        let _ = std::fs::remove_dir_all(&source);
        let _ = std::fs::remove_dir_all(&target);
    }
}
