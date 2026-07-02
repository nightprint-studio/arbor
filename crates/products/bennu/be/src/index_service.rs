//! The per-project **index lifecycle**: build the symbol index off the IPC thread,
//! cache the resulting native provider, and serve completion from it.
//!
//! On `bennu_open_project` the be layer calls [`IndexService::open`], which spawns a
//! **background std thread** to walk the project's `.java` sources, persist the
//! fst+rkyv index under `bennu_data_dir()`, and build a [`NativeJavaProvider`] over it
//! + the resolved JDK. The IPC dispatcher never blocks on the build (the CLAUDE.md
//! gotcha: framed-IPC work that can stall must not run on the dispatcher / a tokio
//! worker — a plain background thread is the cleanest fit here, since the build does no
//! reverse-channel round-trips). While the build runs, `bennu_completion` serves the
//! **empty** provider (the FE shows nothing gracefully); the moment the build lands,
//! the cached provider is swapped in and completion goes live.
//!
//! A single-file edit re-extracts just that file and patches the persisted index
//! ([`IndexService::patch_file`]), then rebuilds the (cheap) provider handle — no
//! whole-project re-parse.

use std::collections::HashMap;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock, RwLock};

use bennu_intel::prelude::{
    build_project_index, collect_java, file_records_from_source, ingest_config_graph, ActionVerdict,
    CompletionItem, ConfigResolver, IntelProvider, NativeJavaProvider, Position, RenameEngine,
    RenamePlan,
};

use crate::web_discovery::discover_web_inputs;

/// Where each project's index files live: `bennu_data_dir()/index/<hash-of-root>/`.
fn index_dir_for(root: &str) -> PathBuf {
    // A stable, filesystem-safe per-root directory name. A simple FNV-1a hash of the
    // absolute root keeps it short and collision-resistant enough for a local cache.
    let mut hash: u64 = 0xcbf29ce484222325;
    for b in root.as_bytes() {
        hash ^= *b as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    arbor_core::prelude::bennu_data_dir().join("index").join(format!("{hash:016x}"))
}

/// One project's slot in the cache: the paths + JDK level it was opened with, plus the
/// hot-swappable provider the completion query reads.
struct ProjectSlot {
    root: PathBuf,
    index_dir: PathBuf,
    jdk_version: String,
    /// simple name → binary name for the project's own declared types (seeds the
    /// resolver so bare project-type names resolve). Rebuilt on patch.
    simple_names: Mutex<BTreeMap<String, String>>,
    /// The live provider. `Arc<NativeJavaProvider::new()>` (empty) until the first
    /// build completes, then swapped for the index-backed one.
    provider: RwLock<Arc<NativeJavaProvider>>,
    /// The config-graph resolver (Struts/Spring/Tiles), built off-thread on open when
    /// the project has web config. `None` until built (or for a project with no config).
    /// Drives `bennu_definition` (JSP action → class/view) + the JSP action-existence
    /// diagnostic.
    config: RwLock<Option<Arc<ConfigResolver>>>,
    /// The rename engine (whole-project reference index + resolver + source sets), built
    /// off-thread on open alongside the provider. `None` until built. Drives
    /// `bennu_rename_plan` / `bennu_rename_apply` (docs §5 #10-12).
    rename: RwLock<Option<Arc<RenameEngine>>>,
}

/// The process-wide index service (one per `bennu-be`).
pub struct IndexService {
    slots: Mutex<HashMap<PathBuf, Arc<ProjectSlot>>>,
}

static SERVICE: OnceLock<IndexService> = OnceLock::new();

impl IndexService {
    /// The global service, created on first use.
    pub fn global() -> &'static IndexService {
        SERVICE.get_or_init(|| IndexService { slots: Mutex::new(HashMap::new()) })
    }

    /// Kick off (or restart) the index build for `root` at JDK `jdk_version`, on a
    /// background thread. Returns immediately; the provider goes live when the build
    /// finishes. Idempotent per root — a re-open rebuilds.
    pub fn open(&'static self, root: &str, jdk_version: &str) {
        let root_path = PathBuf::from(root);
        let index_dir = index_dir_for(root);
        let slot = Arc::new(ProjectSlot {
            root: root_path.clone(),
            index_dir: index_dir.clone(),
            jdk_version: jdk_version.to_string(),
            simple_names: Mutex::new(BTreeMap::new()),
            provider: RwLock::new(Arc::new(NativeJavaProvider::new())),
            config: RwLock::new(None),
            rename: RwLock::new(None),
        });
        self.slots.lock().unwrap_or_else(|p| p.into_inner()).insert(root_path.clone(), slot.clone());

        let jdk_version = jdk_version.to_string();
        std::thread::spawn(move || {
            if let Err(e) = std::fs::create_dir_all(&index_dir) {
                eprintln!("bennu-be: index dir {}: {e}", index_dir.display());
                return;
            }
            // Build + persist the project index.
            let (builder, types, members) = build_project_index(&root_path, &index_dir);
            if let Err(e) = builder.persist() {
                eprintln!("bennu-be: index persist failed: {e}");
                return;
            }
            eprintln!(
                "bennu-be: index built for {} ({types} types, {members} members)",
                root_path.display()
            );

            // Seed the project's own simple names for the resolver.
            let simple = bennu_intel::prelude::project_type_map(&root_path);
            let pairs: Vec<(String, String)> =
                simple.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
            *slot.simple_names.lock().unwrap_or_else(|p| p.into_inner()) = simple;

            // Build the index-backed provider and swap it in.
            match NativeJavaProvider::for_project(&index_dir, &jdk_version, &pairs) {
                Ok(p) => {
                    *slot.provider.write().unwrap_or_else(|p| p.into_inner()) = Arc::new(p);
                    eprintln!("bennu-be: completion live for {}", root_path.display());
                }
                Err(e) => eprintln!("bennu-be: provider build failed ({}): {e}", root_path.display()),
            }

            // Build the rename engine (whole-project reference index + resolver + source
            // sets) for find-usages / rename. The reference walk is the O(N) step; it runs
            // here on the background thread so `bennu_rename_plan` is cheap. Non-fatal: a
            // failure just leaves rename returning "index still building".
            build_rename_engine(&slot, &root_path, &index_dir, &jdk_version, &pairs);

            // Build the config-graph (Struts/Spring/Tiles) index + resolver, if the
            // project has any web config. Non-fatal: a project with no config just gets
            // no ConfigResolver (definition/diagnostics on JSP actions then return empty).
            let inputs = discover_web_inputs(&root_path);
            if !inputs.struts_roots.is_empty() || !inputs.spring_files.is_empty() {
                let (graph, report) = bennu_web::prelude::build_web_graph(&inputs);
                if !report.unresolved_includes.is_empty() {
                    eprintln!(
                        "bennu-be: {} unresolved config include(s) (jar-resident, non-fatal)",
                        report.unresolved_includes.len()
                    );
                }
                match ingest_config_graph(&graph, &index_dir) {
                    Ok(cfg) => {
                        let (a, b, r) = (cfg.action_count(), cfg.bean_count(), cfg.relation_count());
                        *slot.config.write().unwrap_or_else(|p| p.into_inner()) = Some(Arc::new(cfg));
                        eprintln!(
                            "bennu-be: config graph live for {} ({a} actions, {b} beans, {r} edges)",
                            root_path.display()
                        );
                    }
                    Err(e) => eprintln!("bennu-be: config ingest failed: {e}"),
                }
            }
        });
    }

    /// Rebuild the index for an already-open project (by root), reusing the JDK level it
    /// was opened at. A no-op when no slot owns `root`. Called after a successful
    /// `bennu_build` so freshly-compiled `target/classes` output (and any source changes
    /// the build picked up) are reflected in completion. Returns immediately; the
    /// rebuild runs on the same background thread `open` uses.
    pub fn reindex(&'static self, root: &str) {
        let jdk = {
            let slots = self.slots.lock().unwrap_or_else(|p| p.into_inner());
            slots.get(&PathBuf::from(root)).map(|s| s.jdk_version.clone())
        };
        if let Some(jdk) = jdk {
            self.open(root, &jdk);
        }
    }

    /// Serve completion at `file`:`offset` from the owning project's provider (matched
    /// by longest root prefix). Returns `[]` when no project owns the file, or its
    /// index is still building.
    pub fn completion(&self, file: &str, offset: usize) -> Vec<CompletionItem> {
        let Some(slot) = self.slot_for_file(file) else {
            return Vec::new();
        };
        let provider = {
            let g = slot.provider.read().unwrap_or_else(|p| p.into_inner());
            Arc::clone(&g)
        };
        let at = Position { file: file.to_string(), offset };
        provider.completion(&at).unwrap_or_default()
    }

    /// Resolve a JSP form/link action reference to its go-to-definition target (the C1
    /// chain: the config fragment it's declared in + the resolved class FQCN + view JSP).
    /// `file` is any file inside the owning project (to pick the project's config).
    /// Returns `None` when no project owns the file, its config isn't built yet, or the
    /// action is unknown.
    pub fn definition_action(&self, file: &str, action_qname: &str) -> Option<ActionDefinition> {
        let cfg = self.config_for_file(file)?;
        let target = cfg.action_class_ref(action_qname)?;
        Some(ActionDefinition {
            config_file: target.config_file,
            class_fqcn: target.class_fqcn,
            view_jsp: target.view_jsp,
        })
    }

    /// The conservative "action inesistente" verdict for a JSP action reference.
    /// Returns `Inconclusive` (never `Missing`) whenever the config isn't built yet, so
    /// the FE never shows a false "missing" while the index is still loading.
    pub fn diagnose_action(&self, file: &str, action_qname: &str) -> ActionVerdict {
        match self.config_for_file(file) {
            Some(cfg) => cfg.diagnose_action(action_qname),
            None => ActionVerdict::Inconclusive { reason: "config not built".into() },
        }
    }

    /// Plan a rename for the symbol at `file`:`offset` → `new_name`, over the owning
    /// project's rename engine (built off-thread on open). `source` is the current
    /// (possibly-unsaved) buffer. Returns `None` when no project owns the file, the engine
    /// is still building, or the caret isn't on a renameable identifier.
    pub fn plan_rename(
        &self,
        file: &str,
        source: &str,
        offset: usize,
        new_name: &str,
    ) -> Option<RenamePlan> {
        let slot = self.slot_for_file(file)?;
        let engine = {
            let g = slot.rename.read().unwrap_or_else(|p| p.into_inner());
            g.as_ref().map(Arc::clone)
        }?;
        engine.plan(file, source, offset, new_name)
    }

    /// The config resolver of the project owning `file`, if built.
    fn config_for_file(&self, file: &str) -> Option<Arc<ConfigResolver>> {
        let slot = self.slot_for_file(file)?;
        let g = slot.config.read().unwrap_or_else(|p| p.into_inner());
        g.as_ref().map(Arc::clone)
    }

    /// Incrementally patch one file after an edit: re-extract it, patch the persisted
    /// index, and rebuild the provider handle. `source == None` means the file was
    /// deleted. Runs synchronously (a single-file extract + persist is sub-ms — the
    /// disposable measured ~2 ms for a full project) but is cheap enough to call from a
    /// debounced save handler; move to a thread if a huge project ever makes it slow.
    ///
    /// Triggered by the `bennu_did_change { file, text }` handler on a live edit, so an
    /// edit reflects in completion/definition without reopening the project. Runs on the
    /// blocking pool (the handler dispatches it there); the single-file re-extract +
    /// persist is sub-ms in practice.
    pub fn patch_file(&self, file: &str, source: Option<&str>) {
        let Some(slot) = self.slot_for_file(file) else { return };

        // A config (`.xml`) edit changes the config graph, not the Java index → rebuild
        // the config resolver rather than mis-parsing XML as Java. (Cheap: bounded walk +
        // parse; runs on the blocking pool like the rest of this method.)
        if is_xml_config(file) {
            self.rebuild_config(&slot);
            return;
        }
        if !is_java(file) {
            return; // nothing to re-index for this file kind
        }

        let file_path = PathBuf::from(file);
        let simple = { slot.simple_names.lock().unwrap_or_else(|p| p.into_inner()).clone() };

        // Rebuild the builder's per-file record set from the current project, then patch
        // this one file. (The builder is re-derived rather than held in the slot to keep
        // the slot small; the project walk is bounded and the persist is fast.)
        let (mut builder, _t, _m) = build_project_index(&slot.root, &slot.index_dir);
        let records = source.map(|src| {
            file_records_from_source(&file_path, src, &simple, u32::MAX / 2)
        });
        if let Err(e) = builder.patch_file(file_path, records) {
            eprintln!("bennu-be: index patch failed: {e}");
            return;
        }
        // Refresh the simple-name map + provider so the new members resolve.
        let new_simple = bennu_intel::prelude::project_type_map(&slot.root);
        let pairs: Vec<(String, String)> =
            new_simple.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
        *slot.simple_names.lock().unwrap_or_else(|p| p.into_inner()) = new_simple;
        if let Ok(p) = NativeJavaProvider::for_project(&slot.index_dir, &slot.jdk_version, &pairs) {
            *slot.provider.write().unwrap_or_else(|p| p.into_inner()) = Arc::new(p);
        }
    }

    /// Re-parse + re-ingest the project's config graph (after a struts/spring/tiles XML
    /// edit), swapping the live [`ConfigResolver`] in. Non-fatal on failure.
    fn rebuild_config(&self, slot: &Arc<ProjectSlot>) {
        let inputs = discover_web_inputs(&slot.root);
        if inputs.struts_roots.is_empty() && inputs.spring_files.is_empty() {
            return;
        }
        let (graph, _report) = bennu_web::prelude::build_web_graph(&inputs);
        match ingest_config_graph(&graph, &slot.index_dir) {
            Ok(cfg) => {
                *slot.config.write().unwrap_or_else(|p| p.into_inner()) = Some(Arc::new(cfg));
            }
            Err(e) => eprintln!("bennu-be: config rebuild failed: {e}"),
        }
    }

    /// The slot whose root is the longest prefix of `file`.
    fn slot_for_file(&self, file: &str) -> Option<Arc<ProjectSlot>> {
        let slots = self.slots.lock().unwrap_or_else(|p| p.into_inner());
        let mut best: Option<&Arc<ProjectSlot>> = None;
        for slot in slots.values() {
            if Path::new(file).starts_with(&slot.root) {
                match best {
                    Some(b) if b.root.as_os_str().len() >= slot.root.as_os_str().len() => {}
                    _ => best = Some(slot),
                }
            }
        }
        best.cloned()
    }
}

/// Build + swap in the rename engine for `slot`: read every project `.java` source and
/// the Spring `.xml` fragments (the only XML that can carry `<bean class=>`), then build
/// the whole-project reference index + resolver. Non-fatal on failure (rename then just
/// returns "still building"). Runs on the index background thread.
fn build_rename_engine(
    slot: &Arc<ProjectSlot>,
    root: &Path,
    index_dir: &Path,
    jdk_version: &str,
    simple_names: &[(String, String)],
) {
    // Java sources (path normalized to forward slashes to match the FE's file keys).
    let mut java_paths = Vec::new();
    collect_java(root, &mut java_paths);
    let java: Vec<(String, String)> = java_paths
        .iter()
        .filter_map(|p| std::fs::read_to_string(p).ok().map(|s| (norm_path(p), s)))
        .collect();

    // Spring bean XML fragments (any `.xml` with a `<beans` root) — the class-rename
    // config-aware edit target set.
    let xml: Vec<(String, String)> = discover_web_inputs(root)
        .spring_files
        .iter()
        .filter_map(|p| std::fs::read_to_string(p).ok().map(|s| (norm_path(p), s)))
        .collect();

    match RenameEngine::for_project(index_dir, jdk_version, simple_names, java, xml) {
        Ok(engine) => {
            *slot.rename.write().unwrap_or_else(|p| p.into_inner()) = Some(Arc::new(engine));
            eprintln!("bennu-be: rename engine live for {}", root.display());
        }
        Err(e) => eprintln!("bennu-be: rename engine build failed ({}): {e}", root.display()),
    }
}

/// Normalize a path to forward slashes (the FE keys files by forward-slash paths).
fn norm_path(p: &Path) -> String {
    p.to_string_lossy().replace('\\', "/")
}

/// Whether `file` is a `.java` source (re-indexed into the Java symbol index on edit).
fn is_java(file: &str) -> bool {
    Path::new(file).extension().and_then(|e| e.to_str()) == Some("java")
}

/// Whether `file` is an `.xml` config fragment (a struts/spring/tiles edit → rebuild the
/// config graph on save rather than mis-parsing it as Java).
fn is_xml_config(file: &str) -> bool {
    Path::new(file).extension().and_then(|e| e.to_str()) == Some("xml")
}

/// A resolved go-to-definition target for a JSP action reference (the be-layer view of
/// [`bennu_intel::prelude::ActionTarget`]).
#[derive(Debug, Clone)]
pub struct ActionDefinition {
    /// The struts config fragment the `<action>` is declared in.
    pub config_file: String,
    /// The resolved implementation class FQCN (the C1 chain), if resolvable.
    pub class_fqcn: Option<String>,
    /// The resolved view JSP (the Tiles chain), if resolvable.
    pub view_jsp: Option<String>,
}
