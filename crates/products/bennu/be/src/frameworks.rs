//! `frameworks` domain — the framework-extension host and its handlers.
//!
//! This module is the **only** place bennu-be knows Spring or JPA exist, and even here it knows
//! them by name rather than by shape: everything below goes through
//! [`ExtensionRegistry`](bennu_ext::prelude::ExtensionRegistry). Adding the second framework
//! cost exactly one entry in the vector [`FrameworkService::build`] hands the registry — which
//! was the claim the seam was built on, now tested.
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
    ExtAction, ExtEntry, ExtGutterMark, ExtHighlight, ExtHover, ExtStat, ExtTarget,
    ExtensionRegistry, FileCtx, FrameworkExtension, ProjectScan, ScannedFile,
};
use bennu_project::prelude::{detect_capabilities, normalize_newlines, parse_pom};
use bennu_proto::prelude::{CompletionItem, Diagnostic};
use bennu_i18n::prelude::MessagesExtension;
use bennu_jpa::prelude::JpaExtension;
use bennu_jsp::prelude::JspExtension;
use bennu_spring::prelude::SpringExtension;
use bennu_xml::prelude::XmlExtension;
use serde::{Deserialize, Serialize};

use crate::index_service::{resolve_index_encoding, IndexService};

/// Directories never worth walking for framework config.
const SKIP_DIRS: &[&str] = &["target", "node_modules", ".git", ".idea", ".arbor"];

/// Cap on the files handed to an extension, per kind. A pathological tree (a checked-in
/// `node_modules`, a generated-sources explosion) must not turn one lazy build into a
/// minutes-long stall; the cap is far above any real project's Spring surface.
const MAX_FILES: usize = 20_000;

/// One project's active extensions, plus the direct handles to the two named ones.
///
/// The handles exist for the calls the generic trait has no verb for and should not grow one:
/// Spring's "resolve against this property file" is a setting, and JPA's generators produce
/// Java source. Neither is a question about a caret, which is what the trait models.
struct Slot {
    registry: ExtensionRegistry,
    spring: Option<Arc<SpringExtension>>,
    jpa: Option<Arc<JpaExtension>>,
    jsp: Option<Arc<JspExtension>>,
}

/// Process-wide extension host, one slot per project root.
pub struct FrameworkService {
    slots: Mutex<HashMap<String, Arc<Slot>>>,
    /// One build lock per root, so a cold slot is built ONCE. See [`FrameworkService::slot`].
    building: Mutex<HashMap<String, Arc<Mutex<()>>>>,
}

impl FrameworkService {
    pub fn global() -> &'static FrameworkService {
        static INSTANCE: OnceLock<FrameworkService> = OnceLock::new();
        INSTANCE.get_or_init(|| FrameworkService {
            slots: Mutex::new(HashMap::new()),
            building: Mutex::new(HashMap::new()),
        })
    }

    /// The slot for `root`, building it if this is the first ask. `None` when the root
    /// isn't a directory we can read.
    ///
    /// **Single-flight.** Every request gets its own thread, and the editor asks four
    /// questions per keystroke — highlights, gutter, actions, diagnostics — all of which land
    /// here. With a cold slot each of them used to start its own build of the same model:
    /// four full walks-and-parses of the project, concurrently, none of them reusing the
    /// others' work, and every one of them repeated on the next keystroke because none had
    /// finished to populate the map yet. That is a backend that stops answering for minutes
    /// on a project that would take seconds to read once.
    ///
    /// So the first caller for a root takes that root's build lock and builds; the rest block
    /// on it and then find the finished slot in the map. Still built outside the `slots`
    /// mutex — holding that across a parse would serialise every OTHER project's queries too.
    fn slot(&self, root: &str) -> Option<Arc<Slot>> {
        let key = norm(root);
        if let Some(s) = self.cached(&key) {
            return Some(s);
        }

        let gate = {
            let mut map = self.building.lock().unwrap_or_else(|p| p.into_inner());
            Arc::clone(map.entry(key.clone()).or_default())
        };
        let _building = gate.lock().unwrap_or_else(|p| p.into_inner());
        // Re-check under the gate: whoever held it before us has just filled the map, and
        // rebuilding on top of their work is the very thing the gate is for.
        if let Some(s) = self.cached(&key) {
            return Some(s);
        }

        let slot = Arc::new(Self::build(&key)?);
        if let Ok(mut map) = self.slots.lock() {
            map.insert(key, Arc::clone(&slot));
        }
        Some(slot)
    }

    /// The already-built slot for a normalized key, if any.
    fn cached(&self, key: &str) -> Option<Arc<Slot>> {
        let map = self.slots.lock().ok()?;
        map.get(key).map(Arc::clone)
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
        let jpa = Arc::new(JpaExtension::new());
        let jsp = Arc::new(JspExtension::new());
        // The whole registration surface. A further framework is one more entry here — and the
        // XML, JSP and message-bundle ones arriving as exactly that entry is the claim the seam
        // was built on, now made four times.
        let registry = ExtensionRegistry::new(
            vec![
                Arc::clone(&spring) as Arc<dyn FrameworkExtension>,
                Arc::clone(&jpa) as Arc<dyn FrameworkExtension>,
                Arc::new(XmlExtension::new()) as Arc<dyn FrameworkExtension>,
                Arc::clone(&jsp) as Arc<dyn FrameworkExtension>,
                Arc::new(MessagesExtension::new()) as Arc<dyn FrameworkExtension>,
            ],
            &caps,
        );
        // Nothing applies → no walk, no parse, no model.
        if registry.is_empty() {
            return Some(Slot { registry, spring: None, jpa: None, jsp: None });
        }

        // Reading the Java tree is by far the most expensive part of a scan, and the XML
        // extension has no use for it. Since that one applies to every project, skipping the
        // read when it is the *only* active extension is what keeps a plain Maven project from
        // paying a Spring-sized scan to get `pom.xml` completion.
        let wants_java = registry.ids().iter().any(|id| !matches!(*id, "xml" | "jsp"));
        let java: Vec<ScannedFile> = if wants_java {
            let encoding = resolve_index_encoding(root);
            bennu_intel::prelude::read_java_sources(path, &encoding)
                .sources
                .into_iter()
                .take(MAX_FILES)
                // Normalized to LF for the same reason the config files are: every byte offset
                // this extension records for a Java file (a bound field's name, a `@Value` key)
                // is handed to the editor, whose buffer was normalized on read. Leave the `\r`s
                // in and a jump from a yaml usage lands one line late per preceding line — fine
                // at the top of a file, thirty lines off at the bottom of a real one.
                .map(|(path, text)| ScannedFile { path, text: normalize_newlines(&text) })
                .collect()
        } else {
            Vec::new()
        };
        let walked = collect_config_files(path);
        let descriptors = collect_descriptors(path);
        let schemas = collect_schemas(path);
        // Only walked when something asked for it: on a project with no tag libraries the JSP
        // extension is not in the registry, and opening every dependency jar to find nothing
        // would be a project scan spent on a feature that is off.
        let taglibs = match registry.ids().contains(&"jsp") {
            true => collect_taglibs(path),
            false => Vec::new(),
        };

        registry.reindex(&ProjectScan {
            root: path,
            java: &java,
            xml: &walked.xml,
            resources: &walked.resources,
            pages: &walked.pages,
            schemas: &schemas,
            descriptors: &descriptors,
            taglibs: &taglibs,
        });
        // Each extension keeps only what its own `applies` admitted it to; a handle to one the
        // registry dropped would be a model nobody ever fills.
        let active = registry.ids();
        Some(Slot {
            spring: active.contains(&"spring").then_some(spring),
            jpa: active.contains(&"jpa").then_some(jpa),
            jsp: active.contains(&"jsp").then_some(jsp),
            registry,
        })
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

/// The buckets one walk of the project tree fills.
#[derive(Default)]
struct WalkedFiles {
    xml: Vec<ScannedFile>,
    resources: Vec<ScannedFile>,
    pages: Vec<ScannedFile>,
}

/// Whether a file name is a server-rendered page — the JSP family, including the `.tag` files
/// that are pages written as tags.
fn is_page_file(lower: &str) -> bool {
    [".jsp", ".jspf", ".jspx", ".tag", ".tagx"].iter().any(|e| lower.ends_with(e))
}

/// Whether a file name is a keyed resource: a `.properties` bundle or a YAML document.
///
/// Deliberately by extension alone. Naming the file is what tells you what it is FOR — Spring
/// configuration, a message bundle, a validator's messages — and that is each extension's
/// question, not the walk's.
fn is_resource_file(lower: &str) -> bool {
    [".properties", ".yml", ".yaml"].iter().any(|e| lower.ends_with(e))
}

/// Walk the project for the file kinds an extension is handed: XML (any of them — the extension
/// decides which are its own), property resources, and pages.
fn collect_config_files(root: &Path) -> WalkedFiles {
    let mut out = WalkedFiles::default();
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
            // EVERY `.properties` / `.yml`, not only the `application*` ones Spring reads: which
            // of them are configuration is Spring's rule, and it applies it itself. A
            // `messages_it.properties` is a resource too, and the extension that wants it should
            // not have to ask the host to widen a filter written for somebody else.
            let bucket = if lower.ends_with(".xml") {
                &mut out.xml
            } else if is_resource_file(&lower) {
                &mut out.resources
            } else if is_page_file(&lower) {
                &mut out.pages
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
    out
}

/// The framework descriptor files for `root`: the ones its **dependencies** ship inside their
/// jars, plus the ones the project writes for itself.
///
/// This is where a Spring project's real property vocabulary comes from — every starter on the
/// classpath packages a description of the keys it accepts, so the union of the jars is exactly
/// the set of properties this project can legally set, at the versions it actually depends on.
///
/// Two deliberate restraints:
///
/// - **Maven is never run from here.** Only the jar list the index service has already resolved
///   is read ([`cached_dep_jars`]). A property hover must not be able to trigger a multi-second
///   dependency resolve; when the list is not there yet the extension falls back to what it
///   knows on its own, and the next refresh picks the jars up.
/// - **The result is cached against the jar list.** Opening a few hundred archives is cheap but
///   not free, and a refresh (which the frontend fires after every index build) would otherwise
///   redo it every time. Keying on the jar list means a changed dependency set gets a fresh
///   read, and nothing else does.
///
/// [`cached_dep_jars`]: crate::dep_classpath::cached_dep_jars
fn collect_descriptors(root: &Path) -> Vec<ScannedFile> {
    use bennu_spring::prelude::{ADDITIONAL_METADATA_ENTRY, METADATA_ENTRY};

    let jars = crate::dep_classpath::cached_dep_jars(root);
    let mut out = descriptor_cache(&jars);

    // The project's own generated / hand-written metadata, which describes the keys of its own
    // `@ConfigurationProperties`. Present only when the annotation processor is on the build,
    // and worth having when it is: those are the keys nothing else can document.
    for rel in [
        format!("target/classes/{METADATA_ENTRY}"),
        format!("src/main/resources/{ADDITIONAL_METADATA_ENTRY}"),
    ] {
        let path = root.join(&rel);
        if let Ok(text) = std::fs::read_to_string(&path) {
            out.push(ScannedFile { path, text });
        }
    }
    out
}

/// Entry names that are a grammar. Nothing else opens jars looking for these, so the test is
/// here rather than in the reader.
fn is_schema_entry(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower.ends_with(".dtd") || lower.ends_with(".xsd")
}

/// Cap on the grammars (schemas, tag libraries) taken from any one jar. A framework ships a
/// handful (one per version it has published); a jar with hundreds is a generated artifact, and
/// reading all of them would cost a project scan for nothing.
const MAX_GRAMMARS_PER_JAR: usize = 32;

/// The `.xsd` / `.dtd` files this project can resolve a document against.
///
/// Two sources, and the second is the one that makes the feature work at all:
///
/// - **the project's own**, wherever they sit in the tree — a vendored schema, a hand-written
///   one beside the file that uses it;
/// - **the ones inside the dependency jars.** A document names its schema by URL, and fetching
///   it is out of the question; but frameworks ship their own grammar in their own artifact
///   (`struts2-core.jar` carries `struts-2.5.dtd`, `spring-beans.jar` carries every
///   `spring-beans.xsd` ever published), so the file the URL names is usually already on the
///   machine. Matching it by name is [`bennu_xml`]'s job; finding it is this one.
///
/// Same two restraints as [`collect_descriptors`]: Maven is never run from here, and the
/// jar-sourced half is cached against the jar list.
fn collect_schemas(root: &Path) -> Vec<ScannedFile> {
    let mut out = schema_cache(&crate::dep_classpath::cached_dep_jars(root));

    // Everything already in the cache: the jar entries extracted earlier, and anything
    // downloaded by [`bennu_xml_fetch_schema`]. The second is the point — once a schema nobody
    // ships has been fetched, the document that named it stops being answered by a fallback and
    // starts being answered by the real thing.
    let mut stack = vec![arbor_core::prelude::bennu_data_dir().join("schemas"), root.to_path_buf()];
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
            if !is_schema_entry(&name) || out.len() >= MAX_FILES {
                continue;
            }
            // The jar-extracted half is already in `out` under the same path; reading it twice
            // would give the catalog two entries with the same name and the same content.
            if out.iter().any(|f| f.path == p) {
                continue;
            }
            if let Ok(bytes) = std::fs::read(&p) {
                // Lossy and newline-normalized, like every other file handed across the seam:
                // the offsets a go-to reports are into this text.
                let text = normalize_newlines(&String::from_utf8_lossy(&bytes));
                out.push(ScannedFile { path: p, text });
            }
        }
    }
    out
}

/// The jar-sourced schemas for a jar list, read once per distinct list.
fn schema_cache(jars: &[PathBuf]) -> Vec<ScannedFile> {
    static CACHE: OnceLock<Mutex<HashMap<u64, Arc<Vec<ScannedFile>>>>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));

    let key = jar_set_key(jars);
    if let Ok(map) = cache.lock() {
        if let Some(hit) = map.get(&key) {
            return hit.as_ref().clone();
        }
    }
    let read: Vec<ScannedFile> = bennu_classpath::prelude::read_jar_entries_matching(
        jars,
        |name| is_schema_entry(name),
        MAX_GRAMMARS_PER_JAR,
    )
    .into_iter()
    .map(|r| {
        // A schema shipped by a framework of that era is as likely to be Latin-1 as UTF-8 —
        // decoded by the one rule that reads every jar entry (`jar_entry_text`), so a `<xs:documentation>`
        // with an accent in it is not a row of replacement characters when someone opens it.
        let text = normalize_newlines(&crate::dep_classpath::jar_entry_text(&r.bytes));
        // Unlike the Spring descriptors, a schema is something the user will want to *open*:
        // ctrl+clicking the URL a document names it by is the first thing anyone tries. So it is
        // materialised into the cache and identified by that real path, rather than by the
        // `<jar>!/<entry>` display form which nothing can open. Falls back to the display form
        // when the cache is not writable — a go-to that does nothing is better than a crash.
        let path =
            cache_jar_entry("schemas", &r.jar, &r.entry, &text).unwrap_or_else(|| PathBuf::from(&r.id));
        ScannedFile { path, text }
    })
    .collect();

    let shared = Arc::new(read);
    if let Ok(mut map) = cache.lock() {
        map.insert(key, Arc::clone(&shared));
    }
    shared.as_ref().clone()
}

/// A jar entry that is a tag-library descriptor. The spec puts them under `META-INF`, and
/// nothing else in a jar is a `.tld`, so the extension is the whole test.
fn is_taglib_entry(name: &str) -> bool {
    name.to_ascii_lowercase().ends_with(".tld")
}

/// The `.tld` files this project can resolve a `<%@ taglib %>` against.
///
/// Same two sources and the same restraints as [`collect_schemas`], for the same reason: the
/// library a page declares by `uri="/struts-tags"` is not in the project at all, it is inside
/// `struts2-core.jar`, and that is where 90% of the tags in a legacy page come from.
///
/// Jar-sourced ones are **materialised into the cache** and identified by that real path,
/// because Ctrl+click on a `uri` is the whole point and `<jar>!/<entry>` opens nothing.
fn collect_taglibs(root: &Path) -> Vec<ScannedFile> {
    let mut out = taglib_cache(&crate::dep_classpath::cached_dep_jars(root));

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
            if !is_taglib_entry(&name) || out.len() >= MAX_FILES {
                continue;
            }
            if let Ok(bytes) = std::fs::read(&p) {
                let text = normalize_newlines(&String::from_utf8_lossy(&bytes));
                out.push(ScannedFile { path: p, text });
            }
        }
    }
    out
}

/// The jar-sourced tag libraries for a jar list, read once per distinct list.
fn taglib_cache(jars: &[PathBuf]) -> Vec<ScannedFile> {
    static CACHE: OnceLock<Mutex<HashMap<u64, Arc<Vec<ScannedFile>>>>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));

    let key = jar_set_key(jars);
    if let Ok(map) = cache.lock() {
        if let Some(hit) = map.get(&key) {
            return hit.as_ref().clone();
        }
    }
    let read: Vec<ScannedFile> = bennu_classpath::prelude::read_jar_entries_matching(
        jars,
        |name| is_taglib_entry(name),
        MAX_GRAMMARS_PER_JAR,
    )
    .into_iter()
    .map(|r| {
        // A TLD of that era is as likely to be Latin-1 as UTF-8, and its `<description>`s are
        // what the hover shows — decoded by the one rule that reads every jar entry.
        let text = normalize_newlines(&crate::dep_classpath::jar_entry_text(&r.bytes));
        let path =
            cache_jar_entry("taglibs", &r.jar, &r.entry, &text).unwrap_or_else(|| PathBuf::from(&r.id));
        ScannedFile { path, text }
    })
    .collect();

    let shared = Arc::new(read);
    if let Ok(mut map) = cache.lock() {
        map.insert(key, Arc::clone(&shared));
    }
    shared.as_ref().clone()
}

/// The jar-sourced descriptors for a jar list, read once per distinct list.
fn descriptor_cache(jars: &[PathBuf]) -> Vec<ScannedFile> {
    use bennu_spring::prelude::{ADDITIONAL_METADATA_ENTRY, METADATA_ENTRY};

    static CACHE: OnceLock<Mutex<HashMap<u64, Arc<Vec<ScannedFile>>>>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));

    let key = jar_set_key(jars);
    if let Ok(map) = cache.lock() {
        if let Some(hit) = map.get(&key) {
            return hit.as_ref().clone();
        }
    }
    let read: Vec<ScannedFile> = bennu_classpath::prelude::read_jar_entries(
        jars,
        &[METADATA_ENTRY, ADDITIONAL_METADATA_ENTRY],
    )
    .into_iter()
    // `id` is `<jar>!/<entry>` — a display identity, not a path to open. The extension only
    // ever matches on it, and it is what a hover shows as the provenance of a key.
    // These two are JSON, which is UTF-8 by specification — but they go through the same decode as
    // everything else read out of a jar, because "the one that is different" is how a rule stops
    // being one.
    .map(|r| ScannedFile { path: PathBuf::from(r.id), text: crate::dep_classpath::jar_entry_text(&r.bytes) })
    .collect();

    let shared = Arc::new(read);
    if let Ok(mut map) = cache.lock() {
        map.insert(key, Arc::clone(&shared));
    }
    shared.as_ref().clone()
}

// ── Fetching a schema nobody ships ───────────────────────────────────────────

#[derive(Deserialize)]
pub struct FetchSchemaArgs {
    /// The `http(s)` location a document named its schema by.
    pub url: String,
}

/// A schema is XML or a DTD; either way it is small. Anything past this is not the file we
/// asked for — most likely a captive portal or an error page — and writing it into the catalog
/// would give every document that names that URL a grammar made of HTML.
const MAX_SCHEMA_BYTES: usize = 4 * 1024 * 1024;

/// Download the schema a document names, cache it, and return where it landed.
///
/// The alternative was opening the URL in a browser, and this is better for a reason that has
/// nothing to do with convenience: **a downloaded schema joins the catalog.** A `pom.xml` whose
/// grammar is the built-in table stops being answered by a curated approximation and starts
/// being answered by the real Maven schema, with its own documentation and its own enumerations.
/// Reading the file was never the point; having it was.
///
/// Fetched **only when the user asks** — this is the far end of a ctrl+click, never something a
/// scan does on its own. Cached by URL, so asking twice costs nothing and works offline after.
#[arbor_rpc::handler]
async fn bennu_xml_fetch_schema(_ctx: &BennuState, args: FetchSchemaArgs) -> Result<String, String> {
    let url = args.url.trim().to_string();
    if !(url.starts_with("http://") || url.starts_with("https://")) {
        return Err("that is not a downloadable schema location".to_string());
    }
    let path = remote_schema_path(&url).ok_or("could not work out where to cache that")?;
    if path.is_file() {
        return Ok(norm(&path.to_string_lossy()));
    }

    let resp = arbor_core::prelude::client()
        .get(&url)
        .send()
        .await
        .map_err(|e| format!("could not reach {url} — {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("{url} answered {}", resp.status()));
    }
    let text = resp.text().await.map_err(|e| format!("could not read {url} — {e}"))?;
    if text.len() > MAX_SCHEMA_BYTES {
        return Err(format!("{url} returned {} bytes — that is not a schema", text.len()));
    }
    // A schema starts with markup. Anything else is a login page or an error document dressed
    // as a 200, and caching it would poison every document that names this URL.
    if !text.trim_start().starts_with('<') {
        return Err(format!("{url} did not return a schema"));
    }

    let parent = path.parent().ok_or("bad cache path")?;
    tokio::fs::create_dir_all(parent).await.map_err(|e| format!("create cache dir: {e}"))?;
    tokio::fs::write(&path, normalize_newlines(&text))
        .await
        .map_err(|e| format!("write cache: {e}"))?;
    Ok(norm(&path.to_string_lossy()))
}

/// Where a downloaded schema is cached: `<data dir>/schemas/remote/<host>/<url path>`.
///
/// The host is kept so two projects naming `schema.xsd` at different addresses do not collide,
/// and the file keeps its own name so the catalog's file-name match finds it without knowing it
/// was downloaded.
fn remote_schema_path(url: &str) -> Option<PathBuf> {
    let after_scheme = url.split_once("://")?.1;
    let (host, path) = after_scheme.split_once('/')?;
    let mut out = arbor_core::prelude::bennu_data_dir().join("schemas").join("remote");
    out.push(sanitize(host.split(':').next().unwrap_or(host)));
    // Query strings and fragments are not part of the file's identity for our purposes, and
    // they are exactly the characters a filesystem refuses.
    let path = path.split(['?', '#']).next().unwrap_or(path);
    let mut any = false;
    for segment in path.split('/').filter(|s| !s.is_empty() && *s != "." && *s != "..") {
        any = true;
        out.push(sanitize(segment));
    }
    any.then_some(out)
}

/// Keep what a filesystem is happy with; everything else becomes `_`.
fn sanitize(segment: &str) -> String {
    segment
        .chars()
        .map(|c| if c.is_alphanumeric() || matches!(c, '-' | '_' | '.') { c } else { '_' })
        .collect()
}

/// Write a jar-shipped grammar into the bennu cache and return where it landed.
///
/// `<data dir>/<kind>/<jar file name>/<entry path>` — the jar's name is kept in the path so two
/// artifacts shipping `struts-2.5.dtd` do not overwrite each other, and so the tab title a user
/// ends up looking at says which dependency it came from.
///
/// `kind` separates the two things extracted this way (`schemas`, `taglibs`) so a cache sweep can
/// speak about one of them.
///
/// Idempotent: a file inside a jar cannot change without the jar changing, so an existing file
/// of the same length is left alone rather than rewritten on every project scan.
fn cache_jar_entry(kind: &str, jar: &Path, entry: &str, text: &str) -> Option<PathBuf> {
    let jar_name = jar.file_name()?.to_str()?;
    let mut path = arbor_core::prelude::bennu_data_dir().join(kind).join(jar_name);
    for segment in entry.split('/').filter(|s| !s.is_empty() && *s != "." && *s != "..") {
        path.push(segment);
    }
    if std::fs::metadata(&path).is_ok_and(|m| m.len() as usize == text.len()) {
        return Some(path);
    }
    std::fs::create_dir_all(path.parent()?).ok()?;
    std::fs::write(&path, text).ok()?;
    Some(path)
}

/// An order-independent key for a jar set (FNV-1a over the sorted paths).
fn jar_set_key(jars: &[PathBuf]) -> u64 {
    let mut names: Vec<String> = jars.iter().map(|j| j.to_string_lossy().into_owned()).collect();
    names.sort();
    let mut hash: u64 = 0xcbf29ce484222325;
    for n in names {
        for b in n.as_bytes() {
            hash ^= *b as u64;
            hash = hash.wrapping_mul(0x100000001b3);
        }
    }
    hash
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
    ///
    /// Normalized to LF on the disk path, like every other file this seam hands an extension. Every
    /// offset an extension reports is handed to the editor, whose buffer went through the same
    /// normalization on read — leave the `\r`s in and a squiggle drifts one byte per preceding
    /// line, which on a CRLF page reads as a warning a line or two below the thing it is about.
    fn text(&self) -> Option<String> {
        match &self.source {
            Some(s) => Some(s.clone()),
            None => Some(normalize_newlines(&String::from_utf8_lossy(
                &std::fs::read(&self.file).ok()?,
            ))),
        }
    }
}

/// Run `f` with the registry owning `args.file` and a context over its text.
fn with_file<T: Default>(
    args: &FileArgs,
    f: impl FnOnce(&ExtensionRegistry, &FileCtx<'_>) -> T,
) -> T {
    let Some(slot) = FrameworkService::global().slot_for_file(&args.file) else {
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

/// The tag-library catalog of the project `file` belongs to, when the JSP extension applies to
/// it and found libraries to resolve.
///
/// The model tab's one dependency on the project. Every other JSP answer travels the
/// `FrameworkExtension` seam because it is *about a caret*; a model of the whole page is not,
/// and inventing a trait method that only one framework could implement would be the wrong
/// generalisation of exactly one case.
pub fn taglib_catalog_for(file: &str) -> Option<Arc<bennu_jsp::prelude::TaglibCatalog>> {
    FrameworkService::global().slot_for_file(file)?.jsp.as_ref()?.resolved()
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

/// The continuation that certainly follows the caret, drawn as ghost text and accepted with
/// Tab. `None` — the normal answer — leaves the editor alone.
#[arbor_rpc::handler]
fn bennu_ext_inline_hint(_ctx: &BennuState, args: FileArgs) -> Result<Option<String>, String> {
    let offset = args.offset;
    Ok(with_file(&args, |r, ctx| r.inline_hint(ctx, offset)))
}

/// What the active frameworks offer to write into this file.
///
/// The toolbar is contributed, not enumerated: an extension returns an action only when the
/// buffer can take it, so the answer to "what kind of file is this" and "what buttons should
/// there be" is one question asked once. Nothing here generates — choosing an action opens the
/// form, and the form calls the generator.
#[arbor_rpc::handler]
fn bennu_ext_actions(_ctx: &BennuState, args: FileArgs) -> Result<Vec<ExtAction>, String> {
    Ok(with_file(&args, |r, ctx| r.actions(ctx)))
}

/// A configuration key rendered as the environment variable that overrides it, in each
/// paste-ready form.
///
/// Read-only by design: the frontend shows this, and the user copies whichever line they
/// need. Writing an override into the file would be the opposite of the point — the whole
/// reason to ask is that the value is going to live somewhere else.
///
/// Needs no model, only the buffer, so it answers for any Spring property file whether or
/// not the project has been indexed yet.
#[derive(Serialize, Default)]
pub struct EnvVarView {
    pub key: String,
    pub value: String,
    pub name: String,
    /// `[label, text]` pairs — `.env`, shell, `docker run`, compose.
    pub forms: Vec<[String; 2]>,
}

#[arbor_rpc::handler]
fn bennu_spring_env_var(_ctx: &BennuState, args: FileArgs) -> Result<Option<EnvVarView>, String> {
    let source = match &args.source {
        Some(s) => s.clone(),
        None => normalize_newlines(&String::from_utf8_lossy(
            &std::fs::read(&args.file).map_err(|e| e.to_string())?,
        )),
    };
    let path = args.file.replace('\\', "/");
    Ok(bennu_spring::prelude::env_var_at(&path, &source, args.offset).map(|v| EnvVarView {
        key: v.key,
        value: v.value,
        name: v.name,
        forms: v.forms.into_iter().map(|(label, text)| [label, text]).collect(),
    }))
}

// ── JPA generation ───────────────────────────────────────────────────────────
//
// The form is thin on purpose: everything it needs to render — which entities exist, which
// properties each has, which keywords are on offer — comes from here, and everything it
// produces goes back through one verb. Nothing is written to disk by either call; the frontend
// applies the result the same way it applies any other edit, so a generation is undoable like
// any other edit rather than being a special case.

/// What the query-builder form renders itself from.
#[derive(Serialize, Default)]
pub struct JpaFormModel {
    /// Every entity, with the property paths a query can address.
    pub entities: Vec<JpaEntityView>,
    /// The repositories that already exist, so the form can offer to add to one.
    pub repositories: Vec<JpaRepoView>,
    /// `[verb, what it does]`.
    pub subjects: Vec<[String; 2]>,
    /// The comparison vocabulary, each with how many arguments it binds.
    pub keywords: Vec<JpaKeywordView>,
    /// `[event, when it fires]` — the seven JPA lifecycle callbacks.
    pub lifecycle: Vec<[String; 2]>,
    /// The relation annotations an attribute may carry.
    pub relations: Vec<String>,
}

#[derive(Serialize, Default)]
pub struct JpaEntityView {
    pub fqcn: String,
    pub simple: String,
    pub file: String,
    /// `entity` | `embeddable` | `mapped-superclass`. Sent rather than filtered on: an
    /// attribute can be added to any of the three, a repository only to the first, and which
    /// rule applies is the form's business.
    pub kind: String,
    /// Property paths, one level of relations followed — `total`, `customer.name`.
    pub properties: Vec<JpaPropertyView>,
    /// Whether a repository already manages it.
    pub has_repository: bool,
}

#[derive(Serialize, Default)]
pub struct JpaPropertyView {
    pub path: String,
    pub type_text: String,
}

/// One comparison keyword the form offers.
///
/// `args` is the half that was missing and the reason the form could not say *what* a condition
/// compares against: `Between` binds two parameters, `IsNull` binds none, and everything else
/// binds one. Without it the form showed "not equal to" and left the reader to guess the rest of
/// the sentence.
#[derive(Serialize, Default)]
pub struct JpaKeywordView {
    /// `""` is plain equality.
    pub keyword: String,
    pub label: String,
    /// How many bound arguments it consumes.
    pub args: usize,
    /// Whether the single argument it binds is a **collection** (`In` / `NotIn`), which the
    /// generated parameter says by being plural.
    pub collection: bool,
}

#[derive(Serialize, Default)]
pub struct JpaRepoView {
    pub fqcn: String,
    pub simple: String,
    pub entity: String,
    pub file: String,
}

/// Everything the JPA generation form needs, in one call.
#[arbor_rpc::handler]
fn bennu_jpa_form_model(_ctx: &BennuState, args: RootArgs) -> Result<JpaFormModel, String> {
    let Some(ext) = FrameworkService::global().slot(&args.root).and_then(|s| s.jpa.clone()) else {
        return Ok(JpaFormModel::default());
    };
    let m = ext.model();
    Ok(JpaFormModel {
        entities: m
            .entities
            .iter()
            .map(|e| JpaEntityView {
                fqcn: e.fqcn.clone(),
                simple: e.simple.clone(),
                file: e.file.clone(),
                kind: e.kind.clone(),
                has_repository: !m.repositories_of(&e.simple).is_empty(),
                properties: property_paths(&m, e),
            })
            .collect(),
        repositories: m
            .repositories
            .iter()
            .map(|r| JpaRepoView {
                fqcn: r.fqcn.clone(),
                simple: r.simple.clone(),
                entity: r.entity.clone(),
                file: r.file.clone(),
            })
            .collect(),
        subjects: bennu_jpa::prelude::QUERY_SUBJECTS
            .iter()
            .map(|(a, b)| [a.to_string(), b.to_string()])
            .collect(),
        keywords: bennu_jpa::prelude::QUERY_KEYWORDS
            .iter()
            .map(|(keyword, label)| JpaKeywordView {
                keyword: keyword.to_string(),
                label: label.to_string(),
                args: bennu_jpa::prelude::keyword_args(keyword),
                collection: bennu_jpa::prelude::keyword_binds_collection(keyword),
            })
            .collect(),
        lifecycle: bennu_jpa::prelude::LIFECYCLE_EVENTS
            .iter()
            .map(|(a, b)| [a.to_string(), b.to_string()])
            .collect(),
        relations: bennu_jpa::prelude::RELATIONS.iter().map(|r| r.to_string()).collect(),
    })
}

/// The property paths a query may address on `entity`: its own fields, plus one level through
/// each relation.
///
/// One level, not all of them: two is already a list nobody scrolls, and a deeper path is
/// something you type rather than pick. Transient fields are excluded — they are mapped by
/// nothing and cannot appear in a query at all.
fn property_paths<'a>(
    model: &'a bennu_jpa::prelude::JpaModel,
    entity: &'a bennu_jpa::prelude::Entity,
) -> Vec<JpaPropertyView> {
    let mut out = Vec::new();
    for f in model.fields_of(entity) {
        if f.transient {
            continue;
        }
        out.push(JpaPropertyView {
            path: f.name.clone(),
            type_text: bennu_jpa::prelude::simple_name(&f.type_text).to_string(),
        });
        if !f.is_navigable() {
            continue;
        }
        let Some(target) = model.entity(&f.target) else { continue };
        if target.fqcn == entity.fqcn {
            continue;
        }
        for nested in model.fields_of(target) {
            if nested.transient || nested.is_navigable() {
                continue;
            }
            out.push(JpaPropertyView {
                path: format!("{}.{}", f.name, nested.name),
                type_text: bennu_jpa::prelude::simple_name(&nested.type_text).to_string(),
            });
        }
    }
    out
}

/// What the form asks to generate.
#[derive(Deserialize)]
pub struct JpaGenerateArgs {
    pub root: String,
    /// `repository` | `projection` | `query-method` | `attribute` | `named-query` |
    /// `lifecycle` | `modify-method`.
    pub kind: String,
    /// The entity, by fully-qualified or simple name.
    pub entity: String,
    /// For `repository`: the base interface to extend. Defaults to `JpaRepository`.
    #[serde(default)]
    pub base: Option<String>,
    /// The name being given: the projection interface, the named query, the callback method.
    #[serde(default)]
    pub name: Option<String>,
    /// For `projection`: the property paths to expose.
    #[serde(default)]
    pub fields: Vec<String>,
    /// For `projection` (nested), `query-method` and `modify-method`: the repository to write
    /// into.
    #[serde(default)]
    pub repository: Option<String>,
    /// For `query-method`: the built query.
    #[serde(default)]
    pub query: Option<JpaQuerySpecArgs>,
    /// For `attribute`: the field being added.
    #[serde(default)]
    pub attribute: Option<JpaAttributeArgs>,
    /// For `modify-method`: the bulk write being added.
    #[serde(default)]
    pub modify: Option<JpaModifyArgs>,
    /// For `lifecycle`: which callback (`PrePersist`, …).
    #[serde(default)]
    pub event: Option<String>,
    /// For `named-query`: the JPQL. Empty gets a `select … from … ` skeleton.
    #[serde(default)]
    pub text: Option<String>,
    /// Source root for a generated file. Defaults to `<root>/src/main/java`.
    #[serde(default)]
    pub source_root: Option<String>,
    /// The buffer a file is open in, as `[path, live text]`.
    ///
    /// Used instead of what is on disk whenever the insertion targets that path. An offset
    /// computed from a stale disk copy lands somewhere else entirely in a buffer with unsaved
    /// edits, and "generate into the file I am looking at" is the normal case, not the rare one.
    #[serde(default)]
    pub open: Option<[String; 2]>,
}

/// The field an "add attribute" form collected. Mirrors `bennu_jpa`'s `AttributeSpec`.
#[derive(Deserialize, Default)]
pub struct JpaAttributeArgs {
    pub name: String,
    /// The field's type, or — for a relation — the entity on the other end.
    #[serde(default)]
    pub type_text: String,
    /// How the value is mapped: `base` (or empty), `enum`, `embedded`, `lob`.
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub column: String,
    /// `nullable = false` is written when this is off.
    #[serde(default)]
    pub optional: bool,
    #[serde(default)]
    pub unique: bool,
    #[serde(default)]
    pub length: Option<u32>,
    /// A field initializer, written as given.
    #[serde(default)]
    pub default_value: String,
    /// Bean Validation constraints, unqualified (`NotNull`, `Size`, `Email`).
    #[serde(default)]
    pub validation: Vec<String>,
    /// `""` for a plain column, else one of the four relation annotations.
    #[serde(default)]
    pub relation: String,
    /// `Set` (default) | `List` | `Map` — how a to-many relation is held.
    #[serde(default)]
    pub collection: String,
    #[serde(default)]
    pub mapped_by: String,
    #[serde(default)]
    pub lazy: bool,
    /// `cascade = {…}` members, unqualified.
    #[serde(default)]
    pub cascade: Vec<String>,
    #[serde(default)]
    pub orphan_removal: bool,
    #[serde(default)]
    pub accessors: bool,
}

/// The bulk write an "add modify method" form collected.
#[derive(Deserialize, Default)]
pub struct JpaModifyArgs {
    #[serde(default)]
    pub name: String,
    /// `true` for a delete, `false` for an update.
    #[serde(default)]
    pub delete: bool,
    #[serde(default)]
    pub assignments: Vec<String>,
    #[serde(default)]
    pub conditions: Vec<JpaConditionArgs>,
    #[serde(default)]
    pub returns_count: bool,
}

#[derive(Deserialize, Default)]
pub struct JpaQuerySpecArgs {
    /// A hand-written name instead of the derived one. Empty = derived, which is the default
    /// and the safer one.
    #[serde(default)]
    pub name: String,
    /// Write the `@Query` out even though the derived name would resolve on its own.
    #[serde(default)]
    pub with_query: bool,
    #[serde(default)]
    pub subject: String,
    #[serde(default)]
    pub distinct: bool,
    #[serde(default)]
    pub limit: Option<u32>,
    #[serde(default)]
    pub conditions: Vec<JpaConditionArgs>,
    /// `[path, "desc"|"asc"]`.
    #[serde(default)]
    pub order_by: Vec<[String; 2]>,
    /// What the finder hands back: `optional` (default) | `single` | `list` | `page` | `slice` |
    /// `stream`. Replaces the old `many` + `paged` pair, which could not express half of them.
    #[serde(default)]
    pub returns: String,
    /// Take a `Sort` parameter. Dropped on a paged method, whose `Pageable` already carries one.
    #[serde(default)]
    pub sorted: bool,
    #[serde(default)]
    pub projection: String,
}

#[derive(Deserialize, Default)]
pub struct JpaConditionArgs {
    pub path: String,
    #[serde(default)]
    pub keyword: String,
    #[serde(default)]
    pub ignore_case: bool,
    #[serde(default)]
    pub or: bool,
}

/// What a generation produced — both destinations, when both are honest.
#[derive(Serialize, Default)]
pub struct JpaGenerated {
    /// A file to create: `[path, content]`. Absent when this can only go inside an existing file.
    pub file: Option<[String; 2]>,
    /// Text to splice into an existing file.
    pub insertion: Option<JpaInsertion>,
    /// What the form shows in its preview pane.
    pub preview: String,
    /// The `alter table` the change implies, for the preview's second tab. Empty when there is
    /// none to write honestly — everything that is not a column on this entity's own table.
    ///
    /// A starting point rather than a migration: no dialect, no back-fill for a `not null` added
    /// to a populated table. It is here because the field and the column are one decision made in
    /// two places, and the second place is usually a file somebody writes later from memory.
    pub ddl: String,
}

#[derive(Serialize)]
pub struct JpaInsertion {
    pub file: String,
    /// Byte offset in that file's CURRENT text.
    pub offset: usize,
    pub text: String,
}

/// One wire condition → the generator's own. Two forms need it identically; a third would have
/// been the third copy.
fn condition_of(c: JpaConditionArgs) -> bennu_jpa::prelude::Condition {
    bennu_jpa::prelude::Condition {
        path: c.path,
        keyword: c.keyword,
        ignore_case: c.ignore_case,
        or: c.or,
    }
}

/// Generate anything the toolbar offers: a repository, a projection, a query method, an entity
/// attribute, a named query, a lifecycle callback, or a bulk modify method. Returns text; writes
/// nothing.
#[arbor_rpc::handler]
fn bennu_jpa_generate(_ctx: &BennuState, args: JpaGenerateArgs) -> Result<JpaGenerated, String> {
    use bennu_jpa::prelude as jpa;

    let ext = FrameworkService::global()
        .slot(&args.root)
        .and_then(|s| s.jpa.clone())
        .ok_or("this project has no JPA on its classpath")?;
    let model = ext.model();
    let entity = model
        .entity(&args.entity)
        .ok_or_else(|| format!("no entity called `{}`", args.entity))?;
    let source_root = args
        .source_root
        .clone()
        .unwrap_or_else(|| format!("{}/src/main/java", norm(&args.root)));

    // The text an insertion offset is computed against: the live buffer when the target file is
    // the one open in the editor, what is on disk otherwise. Never the frontend's word for a
    // file it is not showing.
    let text_of = |file: &str| -> Option<String> {
        if let Some([path, text]) = &args.open {
            if norm(path).eq_ignore_ascii_case(&norm(file)) {
                return Some(normalize_newlines(text));
            }
        }
        std::fs::read_to_string(file).ok().map(|t| normalize_newlines(&t))
    };

    // Filled by the arms that have a second view of their result. Declared here because the match
    // below is an expression producing the `Generated`, and only one arm has anything to say.
    let mut ddl = String::new();

    // The repository the result goes into, with its text.
    let repo = args.repository.as_ref().and_then(|fqcn| {
        let r = model.repositories.iter().find(|r| &r.fqcn == fqcn || &r.simple == fqcn)?;
        Some((r.clone(), text_of(&r.file)?))
    });

    let generated = match args.kind.as_str() {
        "repository" => jpa::repository(
            &model,
            entity,
            args.base.as_deref().unwrap_or("JpaRepository"),
            &source_root,
        ),
        "projection" => jpa::projection(
            &model,
            entity,
            args.name.as_deref().unwrap_or("Projection"),
            &args.fields,
            repo.as_ref().map(|(r, t)| (r, t.as_str())),
            &source_root,
        ),
        "query-method" => {
            let (r, text) = repo.as_ref().ok_or("a query method needs a repository to live in")?;
            let q = args.query.unwrap_or_default();
            jpa::query_method(
                &model,
                r,
                text,
                &jpa::QuerySpec {
                    name: q.name,
                    with_query: q.with_query,
                    subject: q.subject,
                    distinct: q.distinct,
                    limit: q.limit,
                    conditions: q.conditions.into_iter().map(condition_of).collect(),
                    order_by: q
                        .order_by
                        .into_iter()
                        .map(|[path, dir]| (path, dir == "desc"))
                        .collect(),
                    returns: jpa::ReturnShape::parse(&q.returns),
                    sorted: q.sorted,
                    projection: q.projection,
                },
            )
        }
        "attribute" => {
            let a = args.attribute.unwrap_or_default();
            if a.name.trim().is_empty() {
                return Err("an attribute needs a name".to_string());
            }
            let text = text_of(&entity.file)
                .ok_or_else(|| format!("could not read {}", entity.file))?;
            let spec = jpa::AttributeSpec {
                    name: a.name,
                    type_text: if a.type_text.trim().is_empty() {
                        "String".to_string()
                    } else {
                        a.type_text
                    },
                    kind: a.kind,
                    column: a.column,
                    optional: a.optional,
                    unique: a.unique,
                    length: a.length,
                    default_value: a.default_value,
                    validation: a.validation,
                    relation: a.relation,
                    collection: a.collection,
                    mapped_by: a.mapped_by,
                    lazy: a.lazy,
                    cascade: a.cascade,
                    orphan_removal: a.orphan_removal,
                    accessors: a.accessors,
            };
            // The one generator with a second view of its result: the column it implies.
            ddl = jpa::attribute_ddl(entity, &spec);
            jpa::entity_attribute(entity, &text, &spec)
        }
        "named-query" => {
            let text = text_of(&entity.file)
                .ok_or_else(|| format!("could not read {}", entity.file))?;
            jpa::named_query(
                entity,
                &text,
                &jpa::NamedQuerySpec {
                    name: args.name.clone().unwrap_or_default(),
                    query: args.text.clone().unwrap_or_default(),
                },
            )
        }
        "lifecycle" => {
            let event = args.event.as_deref().unwrap_or("PrePersist");
            if !jpa::LIFECYCLE_EVENTS.iter().any(|(e, _)| *e == event) {
                return Err(format!("`{event}` is not a JPA lifecycle callback"));
            }
            let text = text_of(&entity.file)
                .ok_or_else(|| format!("could not read {}", entity.file))?;
            jpa::lifecycle_callback(entity, &text, event, args.name.as_deref().unwrap_or_default())
        }
        "modify-method" => {
            let (r, text) = repo.as_ref().ok_or("a modify method needs a repository to live in")?;
            let m = args.modify.unwrap_or_default();
            if !m.delete && m.assignments.is_empty() {
                return Err("an update needs at least one property to set".to_string());
            }
            jpa::modify_method(
                &model,
                r,
                text,
                &jpa::ModifySpec {
                    name: m.name,
                    delete: m.delete,
                    assignments: m.assignments,
                    conditions: m.conditions.into_iter().map(condition_of).collect(),
                    returns_count: m.returns_count,
                },
            )
        }
        other => return Err(format!("unknown generation kind `{other}`")),
    };

    Ok(JpaGenerated {
        file: generated.file.map(|f| [f.path, f.content]),
        insertion: generated
            .insertion
            .map(|i| JpaInsertion { file: i.file, offset: i.offset, text: i.text }),
        preview: generated.preview,
        ddl,
    })
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
    // One catalog is contributed by the HOST rather than by an extension: the beans an
    // allowlisted dependency declares are read out of jars, and the classpath and the
    // allowlist are the host's, not the Spring extension's — which sees this project's
    // sources and nothing else. Answered before the registry because no extension claims
    // this kind, so falling through would return an empty list rather than the answer.
    if args.kind == crate::library_beans::CATALOG_KIND {
        return Ok(crate::library_beans::catalog_entries(&args.root));
    }
    if args.kind == crate::struts_endpoints::CATALOG_KIND {
        return Ok(crate::struts_endpoints::catalog_entries(&args.root));
    }
    let mut rows = FrameworkService::global()
        .slot(&args.root)
        .map(|s| s.registry.catalog(&args.kind))
        .unwrap_or_default();
    // A BARE kind is the concept, not one framework's version of it — so the host's own
    // contributions join it too. `endpoints` is the case that matters: a half-migrated legacy
    // application answers URLs through both Struts actions and `@GetMapping`s, and a panel that
    // showed one of them would be lying about the other.
    if args.kind == "endpoints" {
        rows.extend(crate::struts_endpoints::catalog_entries(&args.root));
    }
    Ok(rows)
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
    let Some(slot) = FrameworkService::global().slot(&args.root) else {
        return Ok(ExtOverview::default());
    };
    let mut out = ExtOverview {
        extensions: slot.registry.ids().into_iter().map(str::to_string).collect(),
        ready: slot.registry.is_ready(),
        stats: slot.registry.stats(),
        ..ExtOverview::default()
    };
    // The host's own contribution (see `bennu_ext_catalog`). Present only when the allowlist
    // matched something that actually declares beans, so a project that configured nothing —
    // or configured a coordinate that turns out to have none — gets no button rather than a
    // door onto an empty list.
    if let Some(stat) = crate::library_beans::stat(&args.root) {
        out.stats.push(stat);
    }
    // Likewise the Struts actions: the config graph is the index build's, so the count comes
    // from there rather than from an extension.
    if let Some(stat) = crate::struts_endpoints::stat(&args.root) {
        out.stats.push(stat);
    }
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
    Ok(FrameworkService::global().refresh(&args.root))
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
    Ok(FrameworkService::global().pin_property_file(&args.root, chosen))
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
        std::fs::write(res.join("home.jsp"), "<html/>").unwrap();
        std::fs::write(target.join("application.yml"), "a: 2").unwrap();

        let walked = collect_config_files(&dir);
        let (xml, resources) = (&walked.xml, &walked.resources);
        assert_eq!(xml.len(), 1);
        assert_eq!(resources.len(), 2, "a message bundle is a resource; Spring filters its own");
        assert_eq!(walked.pages.len(), 1, "a page is neither xml nor a resource");
        assert!(resources.iter().any(|r| r.path.ends_with("application.yml")));
        assert!(
            !resources.iter().any(|r| r.path.to_string_lossy().contains("target")),
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

        let walked = collect_config_files(&dir);
        let text = &walked.resources[0].text;
        assert!(!text.contains('\r'), "the editor's buffer has no CR either");
        assert_eq!(text.find("c=3"), Some(8), "3 lines of 4 bytes, not of 5");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_root_that_is_not_a_directory_yields_no_slot() {
        assert!(FrameworkService::build("/definitely/not/a/real/root").is_none());
    }
}
