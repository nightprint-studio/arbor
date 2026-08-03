//! `spring` domain — the framework-extension host and its handlers.
//!
//! This module is the **only** place bennu-be knows Spring exists, and even here it knows
//! it by name rather than by shape: everything below goes through
//! [`ExtensionRegistry`](bennu_ext::prelude::ExtensionRegistry), and adding a second
//! framework means adding one line to [`SpringService::registry_for`] — not a second copy
//! of any of this.
//!
//! ## Lifecycle
//!
//! The model is built **lazily, once per project**, on the first query that needs it, and
//! rebuilt on demand (`bennu_spring_refresh`, which the frontend fires after the semantic
//! index finishes and after a save that could change the wiring). It is deliberately not
//! tied to `bennu_open_project`: the scan costs a walk plus a parse of the Spring-relevant
//! subset, and a project the user never asks a Spring question about should never pay it.
//!
//! Building holds no lock — the (possibly slow) walk happens outside the map's mutex, so a
//! concurrent request for another project is never blocked behind it. Two racing builds of
//! the same root would each produce the same model; the second simply replaces the first.
//!
//! ## Capability gating
//!
//! A registry is built from the project's capability bitset, so a project that is not a
//! Spring project carries no Spring extension at all and every query below answers empty
//! without touching a file. Same rule the UI follows when it hides a tool that could only
//! ever be empty.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

use bennu_core::prelude::BennuState;
use bennu_ext::prelude::{
    ExtEntry, ExtGutterMark, ExtHighlight, ExtHover, ExtStat, ExtTarget, ExtensionRegistry,
    FileCtx, FrameworkExtension, ProjectScan, ScannedFile,
};
use bennu_project::prelude::{detect_capabilities, normalize_newlines, parse_pom};
use bennu_proto::prelude::{CompletionItem, Diagnostic};
use bennu_spring::prelude::{is_property_file, SpringExtension};
use serde::{Deserialize, Serialize};

use crate::index_service::{resolve_index_encoding, IndexService};

/// Directories never worth walking for framework config.
const SKIP_DIRS: &[&str] = &["target", "node_modules", ".git", ".idea", ".arbor"];

/// Cap on the files handed to an extension, per kind. A pathological tree (a checked-in
/// `node_modules`, a generated-sources explosion) must not turn one lazy build into a
/// minutes-long stall; the cap is far above any real project's Spring surface.
const MAX_FILES: usize = 20_000;

/// One project's active extensions, plus the handle to the Spring one for the
/// settings-shaped calls (`set active property file`) the generic trait has no verb for.
struct Slot {
    registry: ExtensionRegistry,
    spring: Option<Arc<SpringExtension>>,
}

/// Process-wide extension host, one slot per project root.
pub struct SpringService {
    slots: Mutex<HashMap<String, Arc<Slot>>>,
}

impl SpringService {
    pub fn global() -> &'static SpringService {
        static INSTANCE: OnceLock<SpringService> = OnceLock::new();
        INSTANCE.get_or_init(|| SpringService { slots: Mutex::new(HashMap::new()) })
    }

    /// The slot for `root`, building it if this is the first ask. `None` when the root
    /// isn't a directory we can read.
    fn slot(&self, root: &str) -> Option<Arc<Slot>> {
        let key = norm(root);
        if let Ok(map) = self.slots.lock() {
            if let Some(s) = map.get(&key) {
                return Some(Arc::clone(s));
            }
        }
        // Build OUTSIDE the lock: this walks and parses, and holding the map mutex here
        // would serialise every other project's queries behind it.
        let slot = Arc::new(Self::build(&key)?);
        if let Ok(mut map) = self.slots.lock() {
            map.insert(key, Arc::clone(&slot));
        }
        Some(slot)
    }

    /// The slot for the project owning `file`, via the index service's root map.
    fn slot_for_file(&self, file: &str) -> Option<Arc<Slot>> {
        let root = IndexService::global().root_for_file(file)?;
        self.slot(&root)
    }

    /// Drop and rebuild `root`'s slot — the escape hatch behind `bennu_spring_refresh`.
    /// The user's pinned property file survives, because [`Self::build`] re-reads it from
    /// the config rather than from the slot being replaced.
    pub fn refresh(&self, root: &str) -> bool {
        let key = norm(root);
        let Some(slot) = Self::build(&key) else { return false };
        if let Ok(mut map) = self.slots.lock() {
            map.insert(key, Arc::new(slot));
        }
        true
    }

    /// Scan `root` and build its registry.
    fn build(root: &str) -> Option<Slot> {
        let path = Path::new(root);
        if !path.is_dir() {
            return None;
        }
        let xml = std::fs::read_to_string(path.join("pom.xml")).unwrap_or_default();
        let caps = detect_capabilities(path, &parse_pom(&xml));

        let spring = Arc::new(SpringExtension::new());
        // The pinned property file is a persisted setting, so it is applied BEFORE the
        // scan — the model is then built resolving against the file the user chose,
        // and a restart doesn't need the frontend to replay the choice.
        if let Some(f) = bennu_core::config::load().spring_property_files.get(root).cloned() {
            spring.set_active_property_file(Some(f));
        }
        let registry =
            ExtensionRegistry::new(vec![Arc::clone(&spring) as Arc<dyn FrameworkExtension>], &caps);
        // Nothing applies → no walk, no parse, no model.
        if registry.is_empty() {
            return Some(Slot { registry, spring: None });
        }

        let encoding = resolve_index_encoding(root);
        let java: Vec<ScannedFile> = bennu_intel::prelude::read_java_sources(path, &encoding)
            .sources
            .into_iter()
            .take(MAX_FILES)
            .map(|(path, text)| ScannedFile { path, text })
            .collect();
        let (xml_files, resources) = collect_config_files(path);

        registry.reindex(&ProjectScan {
            root: path,
            java: &java,
            xml: &xml_files,
            resources: &resources,
        });
        Some(Slot { registry, spring: Some(spring) })
    }

    /// Apply the user's pinned property file to `root` (and remember it for rebuilds).
    fn pin_property_file(&self, root: &str, file: Option<String>) -> bool {
        match self.slot(root).and_then(|s| s.spring.clone()) {
            Some(ext) => {
                ext.set_active_property_file(file);
                true
            }
            None => false,
        }
    }
}

/// Walk the project for the two config file kinds an extension is handed: XML (any of
/// them — the extension decides which are its own) and property resources.
fn collect_config_files(root: &Path) -> (Vec<ScannedFile>, Vec<ScannedFile>) {
    let mut xml = Vec::new();
    let mut resources = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
        for e in entries.flatten() {
            let p = e.path();
            let name = e.file_name().to_string_lossy().into_owned();
            if p.is_dir() {
                if !name.starts_with('.') && !SKIP_DIRS.contains(&name.as_str()) {
                    stack.push(p);
                }
                continue;
            }
            let lower = name.to_ascii_lowercase();
            let bucket = if lower.ends_with(".xml") {
                &mut xml
            } else if is_property_file(&name) {
                &mut resources
            } else {
                continue;
            };
            if bucket.len() >= MAX_FILES {
                continue;
            }
            // Config files are read as UTF-8 (with a lossy fallback): XML declares its own
            // encoding and Spring reads `.properties` as ISO-8859-1 historically, but a
            // mis-decoded byte in a value is far less costly here than dropping the file.
            //
            // Line endings are normalized to LF — NOT cosmetic. Every byte offset this
            // extension produces (a property key's span, a `<bean>` element's position) is
            // handed to the editor, whose buffer went through the same normalization on
            // read. Leave the `\r`s in and a go-to lands one byte late per preceding line:
            // fine at the top of a file, a line off by line thirty.
            if let Ok(bytes) = std::fs::read(&p) {
                let text = normalize_newlines(&String::from_utf8_lossy(&bytes));
                bucket.push(ScannedFile { path: p, text });
            }
        }
    }
    (xml, resources)
}

fn norm(p: &str) -> String {
    p.replace('\\', "/").trim_end_matches('/').to_string()
}

/// A file query, in the shape every positional handler below takes.
#[derive(Deserialize)]
pub struct FileArgs {
    /// Absolute path to the file (used to find the owning project AND to route by kind).
    pub file: String,
    /// The live buffer. Absent → read from disk, so a panel can ask about a file that
    /// isn't open.
    #[serde(default)]
    pub source: Option<String>,
    /// Caret byte offset, for the positional queries. Ignored by the rest.
    #[serde(default)]
    pub offset: usize,
}

impl FileArgs {
    /// The buffer to answer against: the live text when the editor sent it, else the file.
    fn text(&self) -> Option<String> {
        match &self.source {
            Some(s) => Some(s.clone()),
            None => std::fs::read_to_string(&self.file).ok(),
        }
    }
}

/// Run `f` with the registry owning `args.file` and a context over its text.
fn with_file<T: Default>(
    args: &FileArgs,
    f: impl FnOnce(&ExtensionRegistry, &FileCtx<'_>) -> T,
) -> T {
    let Some(slot) = SpringService::global().slot_for_file(&args.file) else {
        return T::default();
    };
    if slot.registry.is_empty() {
        return T::default();
    }
    let Some(text) = args.text() else { return T::default() };
    let path = PathBuf::from(&args.file);
    let ctx = FileCtx { path: &path, source: &text };
    f(&slot.registry, &ctx)
}

/// Framework-contributed diagnostics for a file. Called by the `intel` domain as part of
/// `bennu_diagnostics` (so squiggles arrive through the one pipe the editor already
/// listens to) and exposed here for a panel that wants them on their own.
pub fn diagnostics_for(file: &str, source: Option<&str>) -> Vec<Diagnostic> {
    with_file(
        &FileArgs { file: file.to_string(), source: source.map(str::to_string), offset: 0 },
        |r, ctx| r.diagnostics(ctx),
    )
}

// ── Handlers ─────────────────────────────────────────────────────────────────

/// Spans of framework syntax to colour (property placeholders, SpEL, path variables).
#[arbor_rpc::handler]
fn bennu_ext_highlights(_ctx: &BennuState, args: FileArgs) -> Result<Vec<ExtHighlight>, String> {
    Ok(with_file(&args, |r, ctx| r.highlights(ctx)))
}

/// Framework diagnostics for a file, on their own (the editor gets these merged into
/// `bennu_diagnostics`).
#[arbor_rpc::handler]
fn bennu_ext_diagnostics(_ctx: &BennuState, args: FileArgs) -> Result<Vec<Diagnostic>, String> {
    Ok(with_file(&args, |r, ctx| r.diagnostics(ctx)))
}

/// Gutter marks (bean / injection / endpoint) for a file.
#[arbor_rpc::handler]
fn bennu_ext_gutter(_ctx: &BennuState, args: FileArgs) -> Result<Vec<ExtGutterMark>, String> {
    Ok(with_file(&args, |r, ctx| r.gutter(ctx)))
}

/// Go-to targets at a caret. Several means the frontend shows a picker.
#[arbor_rpc::handler]
fn bennu_ext_navigate(_ctx: &BennuState, args: FileArgs) -> Result<Vec<ExtTarget>, String> {
    let offset = args.offset;
    Ok(with_file(&args, |r, ctx| r.navigate(ctx, offset)))
}

/// Hover card at a caret, when a framework knows something the language doesn't.
#[arbor_rpc::handler]
fn bennu_ext_hover(_ctx: &BennuState, args: FileArgs) -> Result<Option<ExtHover>, String> {
    let offset = args.offset;
    Ok(with_file(&args, |r, ctx| r.hover(ctx, offset)))
}

/// Framework completion candidates at a caret (property keys, bean names, classes).
#[arbor_rpc::handler]
fn bennu_ext_completion(_ctx: &BennuState, args: FileArgs) -> Result<Vec<CompletionItem>, String> {
    let offset = args.offset;
    Ok(with_file(&args, |r, ctx| r.completions(ctx, offset)))
}

/// Args for the project-scoped handlers.
#[derive(Deserialize)]
pub struct CatalogArgs {
    /// Absolute project root.
    pub root: String,
    /// Catalog kind, optionally namespaced by extension id (`"spring.beans"`, `"beans"`).
    pub kind: String,
}

/// The rows of one catalog — what every framework list panel (Beans, Endpoints) reads.
#[arbor_rpc::handler]
fn bennu_ext_catalog(_ctx: &BennuState, args: CatalogArgs) -> Result<Vec<ExtEntry>, String> {
    Ok(SpringService::global()
        .slot(&args.root)
        .map(|s| s.registry.catalog(&args.kind))
        .unwrap_or_default())
}

/// Args for [`bennu_ext_overview`] / [`bennu_spring_refresh`].
#[derive(Deserialize)]
pub struct RootArgs {
    pub root: String,
}

/// What the frontend needs to decide whether to offer the framework tooling at all.
#[derive(Serialize, Default)]
pub struct ExtOverview {
    /// Ids of the extensions active for this project (`["spring"]`).
    pub extensions: Vec<String>,
    /// Whether every active extension has finished building its model.
    pub ready: bool,
    /// Headline counts, in display order.
    pub stats: Vec<ExtStat>,
    /// The property files the project declares, and which one resolves first.
    pub property_files: Vec<PropertyFileInfo>,
    pub active_property_file: Option<String>,
}

/// One `application*.yml` / `.properties`, for the picker.
#[derive(Serialize)]
pub struct PropertyFileInfo {
    /// Absolute path, forward-slashed.
    pub path: String,
    /// File name (`application-dev.yml`).
    pub name: String,
    /// Profile from the name, empty for the base file.
    pub profile: String,
    /// How many keys it declares.
    pub keys: usize,
}

/// The framework overview for a project: which extensions are active, their headline
/// counts, and the property-file picker's contents. Empty (not an error) for a project no
/// extension applies to — the frontend hides the tooling on that.
#[arbor_rpc::handler]
fn bennu_ext_overview(_ctx: &BennuState, args: RootArgs) -> Result<ExtOverview, String> {
    let Some(slot) = SpringService::global().slot(&args.root) else {
        return Ok(ExtOverview::default());
    };
    let mut out = ExtOverview {
        extensions: slot.registry.ids().into_iter().map(str::to_string).collect(),
        ready: slot.registry.is_ready(),
        stats: slot.registry.stats(),
        ..ExtOverview::default()
    };
    if let Some(ext) = &slot.spring {
        let model = ext.model();
        out.property_files = model
            .props
            .files()
            .iter()
            .map(|f| PropertyFileInfo {
                path: f.path.clone(),
                name: f.name.clone(),
                profile: f.profile.clone(),
                keys: f.entries.len(),
            })
            .collect();
        out.active_property_file = model.props.active_path().map(str::to_string);
    }
    Ok(out)
}

/// Rebuild a project's framework model. Cheap to call — the frontend fires it after the
/// semantic index lands and after saving a file that could change the wiring.
#[arbor_rpc::handler]
fn bennu_spring_refresh(_ctx: &BennuState, args: RootArgs) -> Result<bool, String> {
    Ok(SpringService::global().refresh(&args.root))
}

/// Args for [`bennu_spring_set_property_file`].
#[derive(Deserialize)]
pub struct SetPropertyFileArgs {
    pub root: String,
    /// Absolute path of the file to resolve against first. `None` / empty clears the pin
    /// and falls back to the profile-less files.
    #[serde(default)]
    pub file: Option<String>,
}

/// Pin which `application*.yml` a project's `${…}` placeholders resolve against.
///
/// A launch-time choice the sources cannot reveal, so it is the user's to make. Persisted
/// per project in the bennu config (`spring_property_files`), applied to the live model
/// immediately — no reindex, since it changes no facts, only which file answers first.
#[arbor_rpc::handler]
fn bennu_spring_set_property_file(
    _ctx: &BennuState,
    args: SetPropertyFileArgs,
) -> Result<bool, String> {
    let chosen = args.file.filter(|f| !f.is_empty());
    let mut cfg = bennu_core::config::load();
    let key = norm(&args.root);
    match &chosen {
        Some(f) => {
            cfg.spring_property_files.insert(key, f.replace('\\', "/"));
        }
        None => {
            cfg.spring_property_files.remove(&key);
        }
    }
    bennu_core::config::save(&cfg)?;
    Ok(SpringService::global().pin_property_file(&args.root, chosen))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn root_normalization_is_stable_across_separators() {
        assert_eq!(norm(r"C:\p\proj\"), "C:/p/proj");
        assert_eq!(norm("/p/proj"), "/p/proj");
    }

    #[test]
    fn config_walk_splits_xml_from_property_resources_and_skips_build_output() {
        let dir = std::env::temp_dir().join(format!("bennu-spring-walk-{}", std::process::id()));
        let res = dir.join("src/main/resources");
        let target = dir.join("target/classes");
        std::fs::create_dir_all(&res).unwrap();
        std::fs::create_dir_all(&target).unwrap();
        std::fs::write(res.join("beans.xml"), "<beans/>").unwrap();
        std::fs::write(res.join("application.yml"), "a: 1").unwrap();
        std::fs::write(res.join("messages.properties"), "x=1").unwrap();
        std::fs::write(target.join("application.yml"), "a: 2").unwrap();

        let (xml, resources) = collect_config_files(&dir);
        assert_eq!(xml.len(), 1);
        assert_eq!(resources.len(), 1, "messages.properties is not a Spring config source");
        assert!(resources[0].path.ends_with("application.yml"));
        assert!(
            !resources[0].path.to_string_lossy().contains("target"),
            "build output must not shadow the real config"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn config_files_are_normalized_to_lf_so_offsets_match_the_editor() {
        // The regression this guards: a CRLF `application.properties` gave every key an
        // offset one byte too large per preceding line, and go-to landed a line late.
        let dir = std::env::temp_dir().join(format!("bennu-spring-crlf-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("application.properties"), "a=1\r\nb=2\r\nc=3\r\n").unwrap();

        let (_, resources) = collect_config_files(&dir);
        let text = &resources[0].text;
        assert!(!text.contains('\r'), "the editor's buffer has no CR either");
        assert_eq!(text.find("c=3"), Some(8), "3 lines of 4 bytes, not of 5");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_root_that_is_not_a_directory_yields_no_slot() {
        assert!(SpringService::build("/definitely/not/a/real/root").is_none());
    }
}
