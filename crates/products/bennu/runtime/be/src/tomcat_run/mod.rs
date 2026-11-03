//! `tomcat_run` domain — run a web module on Apache Tomcat, from the Run button.
//!
//! The sibling [`crate::tomcat`] hot-swaps JSPs into a Tomcat somebody else started; this one
//! *starts* Tomcat. Its decisions (what the generated base contains, what the command line is,
//! which jar is the driver) live in `bennu-tomcat` and are tested there; what is here is the part
//! that touches the world — the file system, the network, the process.
//!
//! ## How a launch goes
//!
//! 1. the installation is read and the ports checked, so a mistake is a sentence and not a stack
//!    trace in a console;
//! 2. the module's run classpath (Maven, runtime scope) is resolved — the same one a plain run uses;
//! 3. a **live** deployment mounts `src/main/webapp` with the compiled classes and each dependency
//!    jar under it, so there is nothing to package; the server libraries the configuration lists
//!    (`libs`) and the JDBC drivers it finds are copied to the base's `lib/` instead, where Tomcat
//!    looks for them;
//! 4. the base is regenerated, the secrets are looked up, and the JVM is started through the same
//!    [`spawn_streamed`] every other run uses — so the console, stdin, Stop and the debugger need
//!    nothing new.
//!
//! The build before the launch is the front end's (`bennu_build`, as for any application): by the
//! time this is called the classes are current.

mod download;
mod driver_fetch;
mod libs;
mod secrets;
mod web_lib;

use std::collections::HashMap;
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use arbor_fs::prelude::{unzip_limited, UnzipLimits};
use arbor_ipc::prelude::EventSink;
use arbor_process_ext::prelude::NoWindowExt;
use bennu_core::prelude::BennuState;
use bennu_proto::prelude::{RunConfig, RunHandle, TomcatRunConfig};
use bennu_tomcat::prelude::{
    archive_url, checksum_url, context_file_name, inspect, java_args, list_installs, listing_url,
    parse_listing, parse_sha512, pick_driver_jars, prepare_base, render_context, BaseSetup, ContextInput,
    Installation, LaunchInput, Mount, MountKind, Scan, LINES,
};
use serde::{Deserialize, Serialize};
use serde_json::json;

use driver_fetch::Fetch;

use crate::task::{Task, TaskSpec};

use crate::build::{
    is_gradle_root, java_program, launching_jdk_major, resolve_java_home, run_classpath, spawn_streamed,
    RunClasspath, EVT_RUN_OUTPUT,
};

/// Progress of a download, as events the front end follows.
const EVT_INSTALL: &str = "arbor://bennu/tomcat-install";
const DEFAULT_BASE_DIR: &str = ".arbor/bennu/tomcat";
const MAX_DOWNLOAD_BYTES: u64 = 96 * 1024 * 1024;
const UNPACK: UnzipLimits =
    UnzipLimits { max_entries: 20_000, max_entry_bytes: 128 * 1024 * 1024, max_total_bytes: 512 * 1024 * 1024 };

// ── directories ────────────────────────────────────────────────────────────────────

/// `configured` (or `default_relative` when empty), made absolute against the project root.
fn resolve_dir(root: &str, configured: &str, default_relative: &str) -> PathBuf {
    let chosen = if configured.trim().is_empty() { default_relative } else { configured.trim() };
    let path = PathBuf::from(chosen);
    if path.is_absolute() { path } else { Path::new(root).join(path) }
}

/// Where the downloaded Tomcats live: `configured`, or — by default — **one folder in the Bennu
/// profile shared by every project**.
///
/// An installation is the program (`bin/`, `lib/`) and is the same for every project that uses that
/// version; what differs per project is the generated base (ports, `context.xml`, logs), which stays
/// under the project. Keeping the installations per project meant downloading the same release once
/// for each of them.
fn install_dir(root: &str, configured: &str) -> PathBuf {
    if configured.trim().is_empty() {
        return arbor_core::prelude::bennu_data_dir().join("tools").join("tomcat");
    }
    resolve_dir(root, configured, "")
}

fn forward(p: &Path) -> String {
    p.to_string_lossy().replace('\\', "/")
}

/// Args for [`bennu_tomcat_environment`].
#[derive(Deserialize)]
pub struct EnvironmentArgs {
    pub root: String,
    /// The configured installs folder; empty = the default.
    #[serde(default)]
    pub install_dir: String,
    #[serde(default)]
    pub base_dir: String,
}

/// What the form needs to know about this machine and project.
#[derive(Serialize)]
pub struct TomcatEnvironment {
    /// The installs folder in force, absolute.
    pub install_dir: String,
    /// The generated bases' folder in force, absolute.
    pub base_dir: String,
    /// The installations found in `install_dir`, newest first.
    pub installs: Vec<Installation>,
    /// The supported release lines, for the download dialog.
    pub lines: Vec<LineInfo>,
}

#[derive(Serialize)]
pub struct LineInfo {
    pub line: String,
    pub namespace: String,
    pub servlet: String,
    pub min_java: u32,
}

/// The installs folder, the bases folder and what is installed — one call, because the form shows
/// all three at once.
#[arbor_rpc::handler]
fn bennu_tomcat_environment(_ctx: &BennuState, args: EnvironmentArgs) -> Result<TomcatEnvironment, String> {
    let installs_at = install_dir(&args.root, &args.install_dir);
    let base_dir = resolve_dir(&args.root, &args.base_dir, DEFAULT_BASE_DIR);
    Ok(TomcatEnvironment {
        installs: list_installs(&installs_at),
        install_dir: forward(&installs_at),
        base_dir: forward(&base_dir),
        lines: LINES
            .iter()
            .map(|l| LineInfo {
                line: l.line.to_string(),
                namespace: l.namespace.to_string(),
                servlet: l.servlet.to_string(),
                min_java: l.min_java,
            })
            .collect(),
    })
}

/// Args for [`bennu_tomcat_inspect`].
#[derive(Deserialize)]
pub struct InspectArgs {
    pub home: String,
}

/// Read an installation the user pointed at (one Bennu did not download).
#[arbor_rpc::handler]
fn bennu_tomcat_inspect(_ctx: &BennuState, args: InspectArgs) -> Result<Installation, String> {
    inspect(Path::new(&args.home))
}

// ── the project ────────────────────────────────────────────────────────────────────

/// Args for [`bennu_tomcat_scan`].
#[derive(Deserialize)]
pub struct ScanArgs {
    pub root: String,
    /// The web module, relative to the root; empty = the root.
    #[serde(default)]
    pub module: String,
}

#[derive(Serialize)]
pub struct RequirementDto {
    pub name: String,
    pub kind: String,
    pub source: String,
}

/// What the project's own files say about running it on Tomcat.
#[derive(Serialize)]
pub struct ProjectScan {
    /// The JNDI names the application looks up, with where each was found.
    pub requirements: Vec<RequirementDto>,
    /// `"javax"` / `"jakarta"`, or `None` when the sources do not say.
    pub namespace: Option<String>,
    /// The module's `src/main/webapp`, when it has one — without it a live deployment has nothing to serve.
    pub webapp_dir: Option<String>,
}

/// Directories that hold build output or somebody else's files; walking them finds nothing true.
const SKIPPED_DIRS: [&str; 5] = ["target", "build", "node_modules", "out", "dist"];
const MAX_SCANNED_FILES: usize = 20_000;
const MAX_SCANNED_BYTES: u64 = 2_000_000;

fn scan_tree(root: &Path) -> Scan {
    let mut scan = Scan::default();
    let (mut stack, mut seen) = (vec![root.to_path_buf()], 0usize);
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
        for entry in entries.flatten() {
            let (path, name) = (entry.path(), entry.file_name().to_string_lossy().to_string());
            let Ok(kind) = entry.file_type() else { continue };
            if kind.is_dir() {
                if !name.starts_with('.') && !SKIPPED_DIRS.contains(&name.as_str()) {
                    stack.push(path);
                }
                continue;
            }
            let lower = name.to_ascii_lowercase();
            if !(lower.ends_with(".java") || lower.ends_with(".xml")) {
                continue;
            }
            seen += 1;
            if seen > MAX_SCANNED_FILES {
                return scan;
            }
            if entry.metadata().map_or(true, |m| m.len() > MAX_SCANNED_BYTES) {
                continue;
            }
            let Ok(bytes) = std::fs::read(&path) else { continue };
            // Lossy: a legacy tree is often Cp1252, and the patterns looked for are ASCII.
            let text = String::from_utf8_lossy(&bytes);
            let rel = path.strip_prefix(root).unwrap_or(&path).to_string_lossy().replace('\\', "/");
            scan.file(&rel, &text);
        }
    }
    scan.requirements.sort_by(|a, b| a.name.cmp(&b.name));
    scan
}

fn module_dir(root: &str, module: &str) -> PathBuf {
    let m = module.trim();
    if m.is_empty() { PathBuf::from(root) } else { Path::new(root).join(m) }
}

/// Read the project's declarations: JNDI names, servlet namespace, webapp folder. Walks the tree, so
/// it is the front end's to call once when the form opens and not on every keystroke.
#[arbor_rpc::handler]
fn bennu_tomcat_scan(_ctx: &BennuState, args: ScanArgs) -> Result<ProjectScan, String> {
    let scan = scan_tree(Path::new(&args.root));
    let webapp = module_dir(&args.root, &args.module).join("src/main/webapp");
    Ok(ProjectScan {
        namespace: scan.namespace().map(str::to_string),
        requirements: scan
            .requirements
            .into_iter()
            .map(|r| RequirementDto { name: r.name, kind: r.kind, source: r.source })
            .collect(),
        webapp_dir: webapp.is_dir().then(|| forward(&webapp)),
    })
}

// ── ports ──────────────────────────────────────────────────────────────────────────

/// Args for [`bennu_tomcat_ports`].
#[derive(Deserialize)]
pub struct PortsArgs {
    pub ports: Vec<u16>,
}

/// Which of `ports` something is already listening on. Asked the way Tomcat will bind (every
/// interface), because a port free on loopback alone is not free for a connector.
fn busy_ports(ports: &[u16]) -> Vec<u16> {
    ports.iter().copied().filter(|p| TcpListener::bind(("0.0.0.0", *p)).is_err()).collect()
}

#[arbor_rpc::handler]
fn bennu_tomcat_ports(_ctx: &BennuState, args: PortsArgs) -> Result<Vec<u16>, String> {
    Ok(busy_ports(&args.ports))
}

// ── the context ────────────────────────────────────────────────────────────────────

/// Args for [`bennu_tomcat_render_context`].
#[derive(Deserialize)]
pub struct RenderArgs {
    pub root: String,
    pub config: RunConfig,
}

/// The `context.xml` a configuration generates, for the form's live preview. The mounts a live
/// deployment adds at launch (classes, jars) depend on a classpath that is expensive to resolve and
/// are not part of what the person edits, so the preview leaves them out.
#[arbor_rpc::handler]
fn bennu_tomcat_render_context(_ctx: &BennuState, args: RenderArgs) -> Result<String, String> {
    let t = &args.config.tomcat;
    let doc_base = if t.deploy == "external" {
        t.doc_base.clone()
    } else {
        forward(&module_dir(&args.root, &args.config.module).join("src/main/webapp"))
    };
    Ok(render_context(t, &ContextInput { doc_base, mounts: Vec::new() }))
}

// ── secrets ────────────────────────────────────────────────────────────────────────

/// Args for [`bennu_tomcat_set_secret`].
#[derive(Deserialize)]
pub struct SetSecretArgs {
    pub root: String,
    pub config_id: String,
    pub resource_id: String,
    /// Empty forgets it.
    pub value: String,
}

#[arbor_rpc::handler]
fn bennu_tomcat_set_secret(_ctx: &BennuState, args: SetSecretArgs) -> Result<(), String> {
    secrets::set(&args.root, &args.config_id, &args.resource_id, &args.value)
}

/// Args for [`bennu_tomcat_secret_ids`].
#[derive(Deserialize)]
pub struct SecretIdsArgs {
    pub root: String,
    pub config_id: String,
}

/// Which resources of a configuration have a stored password. The value never leaves the backend.
#[arbor_rpc::handler]
fn bennu_tomcat_secret_ids(_ctx: &BennuState, args: SecretIdsArgs) -> Result<Vec<String>, String> {
    Ok(secrets::stored_ids(&args.root, &args.config_id))
}

// ── downloading Tomcat ─────────────────────────────────────────────────────────────

/// Args for [`bennu_tomcat_versions`].
#[derive(Deserialize)]
pub struct VersionsArgs {
    /// A release line, `"9.0"`.
    pub line: String,
}

/// The stable releases of a line, newest first, read from Apache's own listing.
#[arbor_rpc::handler]
async fn bennu_tomcat_versions(_ctx: &BennuState, args: VersionsArgs) -> Result<Vec<String>, String> {
    if !LINES.iter().any(|l| l.line == args.line) {
        return Err(format!("{} is not a Tomcat line Bennu can run.", args.line));
    }
    let listing = download::get_text(&listing_url(&args.line), 2_000_000).await?;
    let versions = parse_listing(&listing, &args.line);
    if versions.is_empty() {
        return Err(format!("Apache's listing has no releases of Tomcat {}.", args.line));
    }
    Ok(versions)
}

/// Args for [`bennu_tomcat_download`].
#[derive(Deserialize)]
pub struct DownloadArgs {
    pub root: String,
    pub version: String,
    #[serde(default)]
    pub install_dir: String,
}

fn installing() -> &'static AtomicBool {
    static FLAG: AtomicBool = AtomicBool::new(false);
    &FLAG
}

/// Clears the in-progress flag on every exit path.
struct InstallGuard;
impl Drop for InstallGuard {
    fn drop(&mut self) {
        installing().store(false, Ordering::SeqCst);
    }
}

/// Removes the scratch directory on every exit path.
struct Scratch(PathBuf);
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn emit(sink: &Arc<dyn EventSink>, payload: serde_json::Value) {
    sink.emit(EVT_INSTALL, payload);
}

/// Download, verify and unpack a release. Returns at once; progress and the outcome are
/// [`EVT_INSTALL`] events and the job shows in the Jobs panel.
#[arbor_rpc::handler]
fn bennu_tomcat_download(ctx: &BennuState, args: DownloadArgs) -> Result<(), String> {
    let url = archive_url(&args.version)
        .ok_or_else(|| format!("{} is not a Tomcat version Bennu can run.", args.version))?;
    if installing().swap(true, Ordering::SeqCst) {
        return Err("A Tomcat is already being downloaded.".into());
    }
    let guard = InstallGuard;
    let sink = ctx.event_sink();
    let task = Task::start(
        ctx.host_caller().as_ref(),
        &sink,
        TaskSpec::new(format!("Download Tomcat {}", args.version), url.as_str()).category("downloads"),
    );
    let dir = install_dir(&args.root, &args.install_dir);
    std::thread::Builder::new()
        .name("bennu-tomcat-install".into())
        .spawn(move || {
            let _guard = guard;
            let result = run_install(&sink, &args.version, &dir);
            match &result {
                Ok(version) => emit(&sink, json!({ "kind": "done", "version": version, "home": forward(&dir.join(version)) })),
                Err(e) => emit(&sink, json!({ "kind": "error", "error": e })),
            }
            task.finish(result.map(|_| ()));
        })
        .map(|_| ())
        .map_err(|e| {
            installing().store(false, Ordering::SeqCst);
            format!("could not start the download: {e}")
        })
}

fn run_install(sink: &Arc<dyn EventSink>, version: &str, dir: &Path) -> Result<String, String> {
    let target = dir.join(version);
    if target.exists() {
        return Err(format!("{} already exists.", target.display()));
    }
    let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().map_err(|e| e.to_string())?;
    let url = archive_url(version).ok_or("unknown version")?;
    let sum_url = checksum_url(version).ok_or("unknown version")?;

    emit(sink, json!({ "kind": "phase", "phase": "lookup" }));
    let expected = parse_sha512(&rt.block_on(download::get_text(&sum_url, 4096))?)
        .ok_or("Apache published no readable checksum for this release, so it was not installed.")?;

    std::fs::create_dir_all(dir).map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
    let scratch = Scratch(dir.join(format!(".download-{}", std::process::id())));
    let _ = std::fs::remove_dir_all(&scratch.0);
    std::fs::create_dir_all(&scratch.0).map_err(|e| e.to_string())?;
    let archive = scratch.0.join("tomcat.zip");

    emit(sink, json!({ "kind": "phase", "phase": "download" }));
    let progress_sink = Arc::clone(sink);
    let actual = rt.block_on(download::download(&url, &archive, MAX_DOWNLOAD_BYTES, &move |done, total| {
        emit(&progress_sink, json!({ "kind": "progress", "done": done, "total": total }));
    }))?;

    emit(sink, json!({ "kind": "phase", "phase": "verify" }));
    if actual != expected {
        return Err("The download does not match the checksum Apache publishes, so it was discarded.".into());
    }

    emit(sink, json!({ "kind": "phase", "phase": "extract" }));
    let unpacked = scratch.0.join("unpacked");
    unzip_limited(&archive, &unpacked, &UNPACK, true).map_err(|e| format!("The archive was rejected: {e}"))?;
    inspect(&unpacked)?;
    std::fs::rename(&unpacked, &target).map_err(|e| format!("cannot move Tomcat into {}: {e}", target.display()))?;
    Ok(version.to_string())
}

// ── running ────────────────────────────────────────────────────────────────────────

/// Args for [`bennu_run_tomcat`].
#[derive(Deserialize)]
pub struct RunTomcatArgs {
    pub root: String,
    /// The whole configuration — the Tomcat half is `config.tomcat`; `env`, `vm_args`, `module`,
    /// `classpath_scope` and `debug_suspend` are the shared fields.
    pub config: RunConfig,
    /// Environment for the process, already merged from the `.env` file by the front end.
    #[serde(default)]
    pub env: Option<HashMap<String, String>>,
    /// Launch under the debugger.
    #[serde(default)]
    pub debug: bool,
}

/// Whitespace-separated, honouring single and double quotes — what the other kinds' raw argument
/// strings mean.
fn split_args(raw: &str) -> Vec<String> {
    let (mut out, mut current, mut quote, mut started) = (Vec::new(), String::new(), None::<char>, false);
    for c in raw.chars() {
        match (quote, c) {
            (Some(q), c) if c == q => quote = None,
            (Some(_), c) => current.push(c),
            (None, c @ ('"' | '\'')) => {
                quote = Some(c);
                started = true;
            }
            (None, c) if c.is_whitespace() => {
                if started {
                    out.push(std::mem::take(&mut current));
                    started = false;
                }
            }
            (None, c) => {
                current.push(c);
                started = true;
            }
        }
    }
    if started {
        out.push(current);
    }
    out
}

fn quoted(s: &str) -> String {
    if s.contains(char::is_whitespace) { format!("\"{s}\"") } else { s.to_string() }
}

/// `(docBase, mounts)` for a live deployment: `src/main/webapp`, the classes at `/WEB-INF/classes`
/// and each non-driver jar at `/WEB-INF/lib`.
fn live_deployment(
    module: &Path,
    classpath: &[String],
    driver_jars: &[String],
) -> Result<(String, Vec<Mount>), String> {
    let webapp = module.join("src/main/webapp");
    if !webapp.is_dir() {
        return Err(format!(
            "{} has no src/main/webapp, so there is nothing for Tomcat to serve. \
             Pick the web module, or serve a built WAR (Deployment › External).",
            module.display()
        ));
    }
    let mut mounts = Vec::new();
    // Only the module's own classes are `/WEB-INF/classes` in a packaged WAR; the sibling modules it is built
    // from would be jars in `/WEB-INF/lib`. They stay loadable but unlisted, so code telling "plugins under
    // development" (classes) from installed ones (jars) does not take a sibling for the former.
    let own_classes = forward(&module.join("target/classes"));
    for entry in classpath.iter().filter(|e| !driver_jars.contains(e)) {
        let path = Path::new(entry);
        if path.is_dir() {
            let class_loader_only = forward(path) != own_classes;
            mounts.push(Mount { kind: MountKind::Dir, base: forward(path), web_app_mount: "/WEB-INF/classes".into(), class_loader_only });
        } else if path.is_file() && entry.to_ascii_lowercase().ends_with(".jar") {
            let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
            mounts.push(Mount { kind: MountKind::File, base: forward(path), web_app_mount: format!("/WEB-INF/lib/{name}"), class_loader_only: false });
        }
    }
    Ok((forward(&webapp), mounts))
}

/// Start Tomcat for `config` and stream it to the Run console.
#[arbor_rpc::handler]
fn bennu_run_tomcat(ctx: &BennuState, args: RunTomcatArgs) -> Result<RunHandle, String> {
    let (cfg, t): (&RunConfig, &TomcatRunConfig) = (&args.config, &args.config.tomcat);
    let root = PathBuf::from(&args.root);
    if is_gradle_root(&root) {
        return Err("Running on Tomcat needs a Maven project for now: the deployment is built from Maven's run classpath.".into());
    }

    let install = inspect(Path::new(&t.home)).map_err(|e| format!("Choose a Tomcat installation: {e}"))?;
    if t.http_port == t.shutdown_port {
        return Err("The HTTP port and the shutdown port must be different.".into());
    }
    let busy = busy_ports(&[t.http_port, t.shutdown_port]);
    if let Some(port) = busy.first() {
        return Err(format!("Port {port} is already in use. Stop whatever is listening on it, or choose another port."));
    }
    let jdk_major = launching_jdk_major(&args.root);
    if jdk_major < install.min_java {
        return Err(format!(
            "Tomcat {} needs Java {} or newer, and this project builds with JDK {jdk_major}.",
            install.version, install.min_java
        ));
    }

    let module = Some(cfg.module.trim()).filter(|m| !m.is_empty());
    let java_home = resolve_java_home(&args.root);
    let java = java_program(java_home.as_deref());
    let scope = if cfg.classpath_scope.trim().is_empty() { "runtime" } else { cfg.classpath_scope.trim() };
    let RunClasspath { classpath, mut warnings } =
        run_classpath(&root, module, java_home.as_deref(), scope, Some(&ctx.event_sink()))?;
    let separator = if cfg!(windows) { ';' } else { ':' };
    let entries: Vec<String> = classpath.split(separator).filter(|e| !e.is_empty()).map(str::to_string).collect();

    let datasources: Vec<_> = t.resources.iter().filter(|r| r.kind == "datasource").collect();
    // What the person put in the server libraries goes into `lib/` as it is, and comes first: a
    // driver found there is not looked for anywhere else.
    let libs::LibSet { jars: mut lib_jars, warnings: lib_warnings } =
        libs::expand(&t.lib_paths, |p| resolve_dir(&args.root, p, ""));
    warnings.extend(lib_warnings);
    let lib_names: Vec<String> = lib_jars.iter().map(|j| j.to_string_lossy().into_owned()).collect();
    let wanted: Vec<&str> = datasources.iter().map(|r| r.driver.as_str()).filter(|d| !d.is_empty()).collect();
    let drivers: Vec<&str> = wanted.iter().copied().filter(|d| pick_driver_jars(&lib_names, &[*d]).is_empty()).collect();
    // Then the drivers the project itself depends on. Whichever copy of a driver the project has, it is
    // not also mounted into the web application: the server's own `lib/` provides it.
    let driver_jars = pick_driver_jars(&entries, &drivers);
    let unmounted = pick_driver_jars(&entries, &wanted);
    for jar in &driver_jars {
        libs::add(&mut lib_jars, PathBuf::from(jar));
    }
    // And a driver found nowhere comes from Maven Central, once, and is kept.
    let mut asked: Vec<&str> = Vec::new();
    for d in drivers.iter().copied().filter(|d| pick_driver_jars(&entries, &[*d]).is_empty()) {
        if asked.contains(&d) {
            continue;
        }
        asked.push(d);
        match driver_fetch::ensure(d, crate::dep_classpath::auto_download()) {
            Ok(Fetch::Cached(jar)) => libs::add(&mut lib_jars, jar),
            Ok(Fetch::Downloaded(jar)) => {
                warnings.push(format!("Downloaded the JDBC driver {d} from Maven Central: {}\n", jar.display()));
                libs::add(&mut lib_jars, jar);
            }
            Ok(Fetch::NotAllowed) => warnings.push(format!(
                "The JDBC driver {d} is not a dependency of the project. Automatic downloads are off — turn them on, add the dependency, or add the jar to the server libraries.\n"
            )),
            Ok(Fetch::Unknown) => warnings.push(format!(
                "No jar for the JDBC driver {d} is on the project's runtime classpath, and Bennu cannot fetch it — add its dependency or the jar to the server libraries, or it will fail to open.\n"
            )),
            Err(e) => warnings.push(format!("The JDBC driver {d} could not be downloaded: {e}\n")),
        }
    }
    warnings.extend(libs::conflicts(&lib_jars));

    let (doc_base, mounts) = if t.deploy == "external" {
        if t.doc_base.trim().is_empty() {
            return Err("Deployment is set to External but no WAR or exploded folder is named.".into());
        }
        (t.doc_base.trim().to_string(), Vec::new())
    } else {
        live_deployment(&module_dir(&args.root, &cfg.module), &entries, &unmounted)?
    };

    let mut secrets_in_use: Vec<(String, String)> = Vec::new();
    for r in datasources.iter().filter(|r| r.password_mode != "clear") {
        match secrets::get(&args.root, &cfg.id, &r.id) {
            Some(password) => secrets_in_use.push((r.name.clone(), password)),
            None => warnings.push(format!("No password is stored for {} — the connection will be refused.\n", r.name)),
        }
    }

    let base = resolve_dir(&args.root, &t.base_dir, DEFAULT_BASE_DIR).join(&cfg.id);
    // The application's `/WEB-INF/lib` must be a folder Tomcat can list, not a set of single-jar mounts.
    let staged_jars = mounts.iter().filter(|m| m.web_app_mount.starts_with("/WEB-INF/lib/")).count();
    let mounts = web_lib::stage(&base, mounts)?;
    if t.deploy != "external" {
        // Tomcat answers `null` to a listing of an empty folder, so a count of zero explains the NPE at once.
        warnings.push(if staged_jars == 0 {
            "The runtime classpath holds no dependency jars, so /WEB-INF/lib is empty: code that lists it at start-up gets null. \
             Check the classpath scope (Launch), or that the dependencies are not system/provided.\n"
                .to_string()
        } else {
            format!("/WEB-INF/lib: {staged_jars} jars staged in {}\n", forward(&base.join("web-lib")))
        });
    }
    prepare_base(
        Path::new(&install.home),
        &base,
        &BaseSetup {
            http_port: t.http_port,
            shutdown_port: t.shutdown_port,
            context_file: context_file_name(&t.context_path),
            context_xml: render_context(t, &ContextInput { doc_base, mounts }),
            lib_jars,
        },
    )?;

    let mut jvm_args = split_args(&t.jvm_args);
    jvm_args.extend(split_args(&cfg.vm_args));
    let plan = java_args(&LaunchInput {
        home: install.home.clone(),
        base: forward(&base),
        jvm_args,
        secrets: secrets_in_use,
        jdk_major,
        path_separator: separator,
    });

    // Under the debugger the port has to be listening before the process exists — see `bennu_run`.
    let launch = args.debug.then(crate::debug::prepare).flatten();
    let mut cmd = Command::new(&java);
    let mut shown = vec![quoted(&java)];
    if let Some(l) = &launch {
        let agent = crate::debug::agent_arg(l.port, cfg.debug_suspend);
        shown.push(agent.clone());
        cmd.arg(agent);
    }
    shown.extend(plan.display.iter().map(|a| quoted(a)));
    cmd.args(&plan.args)
        .current_dir(&base)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .stdin(Stdio::piped());
    if let Some(env) = &args.env {
        for (k, v) in env {
            cmd.env(k, v);
        }
    }
    cmd.no_window();

    let (sink, root_for_debug) = (ctx.event_sink(), args.root.clone());
    spawn_streamed(
        cmd,
        cfg.name.clone(),
        shown.join(" "),
        base.display().to_string(),
        &args.root,
        sink.clone(),
        |run_id| {
            for text in &warnings {
                sink.emit(EVT_RUN_OUTPUT, json!({ "run_id": run_id, "stream": "stderr", "text": text }));
            }
            if let Some(launch) = launch {
                crate::debug::start(run_id.to_string(), root_for_debug, launch, sink.clone());
            }
        },
    )
    .map_err(|e| format!("spawn java ({java}): {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("bennu-tomcat-run-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn a_relative_folder_is_the_projects_and_an_empty_one_is_the_default() {
        let root = if cfg!(windows) { "C:/p" } else { "/p" };
        assert_eq!(forward(&resolve_dir(root, "", DEFAULT_BASE_DIR)), format!("{root}/.arbor/bennu/tomcat"));
        assert_eq!(forward(&resolve_dir(root, "tc", DEFAULT_BASE_DIR)), format!("{root}/tc"));
        let abs = if cfg!(windows) { "D:/x" } else { "/x" };
        assert_eq!(forward(&resolve_dir(root, abs, DEFAULT_BASE_DIR)), abs);
    }

    #[test]
    fn arguments_split_on_whitespace_and_keep_quoted_runs_together() {
        assert_eq!(split_args(r#"-Xmx1g -Dname="a b" '-Dc=d e'"#), ["-Xmx1g", "-Dname=a b", "-Dc=d e"]);
        assert!(split_args("   ").is_empty());
        assert_eq!(split_args(r#"-Dempty="" x"#), ["-Dempty=", "x"]);
    }

    #[test]
    fn a_port_something_listens_on_is_reported_busy() {
        let held = TcpListener::bind(("0.0.0.0", 0)).unwrap();
        let port = held.local_addr().unwrap().port();
        assert_eq!(busy_ports(&[port]), [port]);
        drop(held);
        assert!(busy_ports(&[port]).is_empty());
    }

    #[test]
    fn the_tree_scan_reads_declarations_and_skips_build_output() {
        let d = temp("scan");
        std::fs::create_dir_all(d.join("src/main/webapp/WEB-INF")).unwrap();
        std::fs::create_dir_all(d.join("target/classes")).unwrap();
        std::fs::write(
            d.join("src/main/webapp/WEB-INF/web.xml"),
            "<web-app><resource-ref><res-ref-name>jdbc/G</res-ref-name><res-type>javax.sql.DataSource</res-type></resource-ref></web-app>",
        )
        .unwrap();
        std::fs::write(d.join("target/classes/web.xml"), "<web-app><env-entry><env-entry-name>never/seen</env-entry-name></env-entry></web-app>").unwrap();
        let scan = scan_tree(&d);
        let names: Vec<&str> = scan.requirements.iter().map(|r| r.name.as_str()).collect();
        assert_eq!(names, ["jdbc/G"]);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn a_live_deployment_mounts_classes_and_jars_but_not_the_drivers() {
        let d = temp("live");
        std::fs::create_dir_all(d.join("src/main/webapp")).unwrap();
        std::fs::create_dir_all(d.join("target/classes")).unwrap();
        let (jar, driver) = (d.join("lib.jar"), d.join("postgresql-42.jar"));
        std::fs::write(&jar, b"j").unwrap();
        std::fs::write(&driver, b"d").unwrap();
        let cp: Vec<String> = [d.join("target/classes"), jar, driver.clone()].iter().map(|p| p.to_string_lossy().to_string()).collect();
        let (doc_base, mounts) = live_deployment(&d, &cp, &[driver.to_string_lossy().to_string()]).unwrap();
        assert!(doc_base.ends_with("src/main/webapp"));
        assert_eq!(mounts.len(), 2, "{mounts:?}");
        assert_eq!(mounts[0].web_app_mount, "/WEB-INF/classes");
        assert_eq!(mounts[1].web_app_mount, "/WEB-INF/lib/lib.jar");
        assert!(live_deployment(&d.join("nowhere"), &cp, &[]).unwrap_err().contains("src/main/webapp"));
        let _ = std::fs::remove_dir_all(&d);
    }
}
