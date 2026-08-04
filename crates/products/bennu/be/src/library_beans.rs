//! `library_beans` domain — the Spring beans an allowlisted dependency declares.
//!
//! The three pieces this joins already exist and each knows nothing of the others:
//! `bennu-deps` reads a repository jar path back as a coordinate, `bennu-classpath` opens
//! a jar and decodes a class's annotations, and `bennu-spring` turns those annotations
//! into beans. What lives here is the walk — which jars, and in what order — plus the
//! cache, because opening a jar and decoding a thousand classes is not something to do on
//! every panel repaint.
//!
//! **Display only.** Nothing here feeds injection resolution, completion or a diagnostic.
//! A bean declared in a jar is a declaration Spring may or may not act on (see
//! `bennu-spring`'s `library_beans.rs` for why deciding that faithfully is out of reach),
//! so it is shown, grouped by the artifact it came from, labelled with the conditions
//! gating it — and believed by nothing.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Mutex;

use bennu_classpath::prelude::{parse_class_annotations, ClassAnnotations, ClassSource, JarSource};
use bennu_core::config::LibraryBeansConfig;
use bennu_core::prelude::BennuState;
use bennu_ext::prelude::{ExtEntry, ExtStat};
use bennu_deps::prelude::coord_of;
use bennu_spring::prelude::{beans_of_classes, LibraryBean, LibraryBeanAllowlist};
use serde::{Deserialize, Serialize};

use crate::index_service::IndexService;
use crate::library_bean_cache;

/// The wire shape of one artifact's beans.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LibraryBeanGroupDto {
    pub group_id: String,
    pub artifact_id: String,
    pub version: String,
    /// `com.acme:shared-security:2.1.0`, ready to show.
    pub coordinate: String,
    pub beans: Vec<LibraryBeanDto>,
}

/// The wire shape of one bean.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LibraryBeanDto {
    pub name: String,
    pub fqcn: String,
    pub stereotype: String,
    pub declared_in: String,
    /// The `@ConditionalOn…` gates, as written. **Non-empty means this bean may not exist
    /// in your application** — the UI has to say so rather than list it like the rest.
    pub conditions: Vec<String>,
    pub primary: bool,
}

impl From<LibraryBean> for LibraryBeanDto {
    fn from(b: LibraryBean) -> Self {
        Self {
            name: b.name,
            fqcn: b.fqcn,
            stereotype: b.stereotype,
            declared_in: b.declared_in,
            conditions: b.conditions,
            primary: b.primary,
        }
    }
}

/// The persisted config shape → the crate's own. The one place the two meet, and so the
/// one place they could drift.
fn allowlist_of(config: &LibraryBeansConfig) -> LibraryBeanAllowlist {
    LibraryBeanAllowlist {
        group_id: config.group_id.clone(),
        group_id_prefix: config.group_id_prefix.clone(),
        artifact_id: config.artifact_id.clone(),
        artifact_id_prefix: config.artifact_id_prefix.clone(),
    }
}

/// A class carrying a Spring annotation necessarily has the annotation's descriptor in its
/// constant pool, so this byte appears in the file. Testing for it before parsing is what
/// makes scanning a thousand-class jar reasonable: the overwhelming majority are plain
/// classes that can be rejected on a substring search instead of a full class-file decode.
const SPRING_MARKER: &[u8] = b"springframework";

/// Whether the raw class bytes could possibly carry a Spring annotation. A false positive
/// (a class merely *mentioning* the string) only costs one decode; a false negative is
/// impossible, which is the direction that matters.
fn may_carry_spring_annotation(bytes: &[u8]) -> bool {
    bytes.windows(SPRING_MARKER.len()).any(|w| w == SPRING_MARKER)
}

/// The annotated classes of one jar. Errors are absences: an unreadable jar contributes
/// nothing rather than failing the whole scan, because one bad artifact in a reactor
/// should not cost you the other forty.
fn annotated_classes(jar_path: &str) -> Vec<ClassAnnotations> {
    let Ok(source) = JarSource::open(jar_path) else { return Vec::new() };
    let mut out = Vec::new();
    for binary in source.class_names() {
        // Read once and test the bytes, rather than letting `class_annotations_of` re-read
        // them: the pre-filter is only worth having if it happens before the I/O repeats.
        let Ok(Some(bytes)) = source.class_bytes(&binary) else { continue };
        if !may_carry_spring_annotation(&bytes) {
            continue;
        }
        // Decoded from the bytes already in hand rather than through
        // `class_annotations_of`, which would re-read them: the pre-filter is only worth
        // having if it saves the second read as well as the parse.
        if let Ok(annotations) = parse_class_annotations(&bytes) {
            if !annotations.is_empty() {
                out.push(annotations);
            }
        }
    }
    out
}

/// Scan the allowlisted jars of `jars`, newest-coordinate-first order preserved from the
/// classpath. An artifact whose coordinate cannot be read from its path (a jar outside the
/// local repository layout) is skipped: without a coordinate there is nothing to match the
/// allowlist against, and nothing to group it under.
fn scan(jars: &[String], allow: &LibraryBeanAllowlist) -> Vec<LibraryBeanGroupDto> {
    if allow.is_empty() {
        return Vec::new();
    }
    let mut out = Vec::new();
    for jar in jars {
        let Some(coord) = coord_of(Path::new(jar)) else { continue };
        if !allow.admits(&coord.group_id, &coord.artifact_id) {
            continue;
        }
        // The on-disk memo is keyed by the jar, so it survives a restart and is shared by
        // every project that depends on this artifact. A miss scans and records — including
        // an EMPTY result, which is the one most worth remembering: an allowlisted jar with
        // no beans in it would otherwise be decoded in full on every single launch.
        let groups = match library_bean_cache::load(jar) {
            Some(hit) => hit,
            None => {
                let scanned = scan_jar(jar, &coord);
                library_bean_cache::store(jar, &scanned);
                scanned
            }
        };
        out.extend(groups);
    }
    out.sort_by(|a, b| a.coordinate.cmp(&b.coordinate));
    out
}

/// One jar's beans — a single group, or nothing when it declares none. A `Vec` rather than
/// an `Option` because that is what the memo round-trips, and "no beans" has to be a
/// storable answer and not an absent one.
fn scan_jar(jar: &str, coord: &bennu_deps::prelude::JarCoord) -> Vec<LibraryBeanGroupDto> {
    let beans = beans_of_classes(annotated_classes(jar).iter());
    if beans.is_empty() {
        return Vec::new();
    }
    vec![LibraryBeanGroupDto {
        coordinate: coord.coord(),
        group_id: coord.group_id.clone(),
        artifact_id: coord.artifact_id.clone(),
        version: coord.version.clone(),
        beans: beans.into_iter().map(LibraryBeanDto::from).collect(),
    }]
}

/// The **session** tier of the cache: the assembled answer for a project root, keyed by the
/// allowlist that produced it, so editing the setting re-scans and leaving it alone never
/// does.
///
/// The durable tier is per artifact and on disk ([`library_bean_cache`]) — that is what
/// survives a restart and is shared between projects. This one sits in front of it and
/// saves even the `stat` + read per jar when a panel is closed and reopened, which is the
/// commonest thing that happens to a panel.
static CACHE: Mutex<Option<HashMap<String, (LibraryBeanAllowlist, Vec<LibraryBeanGroupDto>)>>> =
    Mutex::new(None);

/// The session cache has two callers that name the same project differently — a handler,
/// with whatever the frontend sent, and the classpath watcher, with the slot map's own key.
/// On Windows those differ by separator alone, which would give one project two entries and
/// make the watcher's invalidation silently miss.
fn cache_key(root: &str) -> String {
    root.replace('\\', "/")
}

fn cached(root: &str, allow: &LibraryBeanAllowlist) -> Option<Vec<LibraryBeanGroupDto>> {
    let guard = CACHE.lock().ok()?;
    let map = guard.as_ref()?;
    let (cached_allow, groups) = map.get(&cache_key(root))?;
    (cached_allow == allow).then(|| groups.clone())
}

/// Drop `root`'s session answer — its dependency jars changed, so the assembled list is
/// stale even though the per-artifact memos on disk will correctly re-scan what moved.
pub fn forget(root: &str) {
    if let Ok(mut guard) = CACHE.lock() {
        if let Some(map) = guard.as_mut() {
            map.remove(&cache_key(root));
        }
    }
}

fn remember(root: &str, allow: &LibraryBeanAllowlist, groups: &[LibraryBeanGroupDto]) {
    if let Ok(mut guard) = CACHE.lock() {
        guard
            .get_or_insert_with(HashMap::new)
            .insert(cache_key(root), (allow.clone(), groups.to_vec()));
    }
}

// ── The framework-catalog seam ─────────────────────────────────────────────────

/// The catalog kind the frontend asks for. Already namespaced, unlike an extension's own
/// (which the registry namespaces on the way out): these are contributed by the **host**,
/// which owns the classpath and the allowlist, not by the Spring extension — which sees the
/// project's sources and nothing else. Sharing the `spring.` prefix is right anyway: to the
/// user this is Spring's, whoever computed it.
pub const CATALOG_KIND: &str = "spring.librarybeans";

/// The beans as catalog rows: **one row per artifact**, its beans as children.
///
/// The nesting is the grouping. `ExtEntry::children` exists precisely so a list with detail
/// rows renders in the one catalog panel instead of growing its own, and "which dependency
/// declared this" is the only grouping this list wants — so it gets it structurally, with no
/// panel and no FE grouping rule.
pub fn catalog_entries(root: &str) -> Vec<ExtEntry> {
    groups_for(root)
        .into_iter()
        .map(|group| ExtEntry {
            id: group.coordinate.clone(),
            primary: format!("{}:{}", group.group_id, group.artifact_id),
            secondary: group.version.clone(),
            kind: "artifact".to_string(),
            tags: vec![format!("{} beans", group.beans.len())],
            children: group.beans.into_iter().map(bean_entry).collect(),
            ..Default::default()
        })
        .collect()
}

fn bean_entry(bean: LibraryBeanDto) -> ExtEntry {
    let mut tags = Vec::new();
    if bean.primary {
        tags.push("primary".to_string());
    }
    // The conditions ARE the caveat, so they are the tags: a row that shows a gated bean
    // exactly like an ungated one is the confident lie this whole tier exists to avoid.
    tags.extend(bean.conditions);
    ExtEntry {
        id: format!("{}#{}", bean.declared_in, bean.name),
        primary: bean.name,
        secondary: bean.fqcn,
        kind: bean.stereotype,
        tags,
        // No file/offset: there is no source on disk to point at. Opening the declaring
        // class goes through the library source view, by name — a different gesture than a
        // catalog row's jump, and offering a jump that lands nowhere would be worse than
        // offering none.
        ..Default::default()
    }
}

/// The headline count, for the overview that decides whether the panel is offered at all —
/// so a project whose allowlist matches nothing gets no button rather than an empty list.
pub fn stat(root: &str) -> Option<ExtStat> {
    let total: usize = groups_for(root).iter().map(|g| g.beans.len()).sum();
    (total > 0).then(|| ExtStat {
        label: "Library beans".to_string(),
        value: total,
        catalog: Some(CATALOG_KIND.to_string()),
    })
}

/// The scan for `root`, through both cache tiers. Shared by the handler, the catalog and the
/// stat — all three ask the identical question, and the first of them to be called pays.
fn groups_for(root: &str) -> Vec<LibraryBeanGroupDto> {
    let allow = allowlist_of(&bennu_core::config::load().library_beans);
    if allow.is_empty() {
        return Vec::new();
    }
    if let Some(hit) = cached(root, &allow) {
        return hit;
    }
    let groups = scan(&IndexService::global().dep_jars_of(root), &allow);
    remember(root, &allow, &groups);
    groups
}

/// Args for [`bennu_library_beans`].
#[derive(serde::Deserialize)]
pub struct LibraryBeansArgs {
    /// Absolute path to the project root (to pick its resolved dependency jars).
    pub root: String,
}

/// The Spring beans declared by the project's **allowlisted dependencies**, grouped by
/// artifact.
///
/// Empty — and free — when the allowlist is empty, which is the default: no jar is opened
/// until somebody names the artifacts they want read.
#[arbor_rpc::handler]
fn bennu_library_beans(
    _state: &BennuState,
    args: LibraryBeansArgs,
) -> Result<Vec<LibraryBeanGroupDto>, String> {
    Ok(groups_for(&args.root))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_empty_allowlist_opens_nothing() {
        // The default state: no jar path is even looked at, so a bad path is harmless.
        let groups = scan(&["/does/not/exist.jar".to_string()], &LibraryBeanAllowlist::default());
        assert!(groups.is_empty());
    }

    #[test]
    fn a_jar_outside_the_repository_layout_is_skipped() {
        // No coordinate → nothing to match and nothing to group under.
        let allow =
            LibraryBeanAllowlist { group_id_prefix: vec!["com.".into()], ..Default::default() };
        assert!(scan(&["/tmp/loose.jar".to_string()], &allow).is_empty());
    }

    #[test]
    fn the_spring_prefilter_never_rejects_a_class_that_carries_one() {
        assert!(may_carry_spring_annotation(
            b"\xca\xfe\xba\xbeLorg/springframework/stereotype/Service;"
        ));
        assert!(!may_carry_spring_annotation(b"\xca\xfe\xba\xbeLjava/lang/Object;"));
        // Shorter than the marker — the window iterator yields nothing, which must read as
        // "no" rather than panic.
        assert!(!may_carry_spring_annotation(b"ca"));
    }
}
