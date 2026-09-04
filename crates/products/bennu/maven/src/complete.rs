//! What can be typed here — from the repository first, from a table when the repository is empty.
//!
//! ## Where the candidates come from
//!
//! Two sources, and the order is the whole design. The **local repository** is exact: it carries the
//! versions, it is what the build will actually resolve against, and a coordinate offered from it is
//! guaranteed to work offline. The **built-in table** ([`crate::known`]) carries no versions and is
//! offered behind it, for the case the repository cannot serve: adding a library you have never had.
//!
//! Every item says which it came from, because the difference is actionable — one resolves now, the
//! other needs a download.
//!
//! ## The one non-obvious insertion
//!
//! Completing an `<artifactId>` while the `<groupId>` above it is still empty fills **both**, by
//! replacing the span from one value to the other and keeping the markup between them exactly as it
//! is in the buffer. It is what the editor you came from does, and the alternative — completing the
//! artifactId and leaving a coordinate that cannot resolve — is a completion that creates a
//! diagnostic.

use bennu_proto::prelude::CompletionItem;

use crate::blocks::{block_at, Block, BlockKind};
use crate::doc::Doc;
use crate::env::PomEnv;
use crate::known;
use crate::repo::Coord;

/// The most candidates worth sending: past this the popup is a directory listing.
const LIMIT: usize = 60;

/// The `<scope>` values, with what each one means for the classpath.
const SCOPES: &[(&str, &str)] = &[
    ("compile", "on every classpath, and inherited by dependents (the default)"),
    ("provided", "compile only — the container supplies it at runtime"),
    ("runtime", "not for compiling, needed to run"),
    ("test", "tests only, never inherited"),
    ("system", "a jar at an explicit <systemPath>"),
    ("import", "only in <dependencyManagement>: pull in a BOM's managed versions"),
];

const TYPES: &[(&str, &str)] = &[
    ("jar", "the default"),
    ("pom", "a BOM — import it in <dependencyManagement>"),
    ("war", "a web archive"),
    ("test-jar", "the artifact's test classes"),
    ("ejb", "an EJB module"),
    ("bundle", "an OSGi bundle — still a jar"),
    ("maven-plugin", "a plugin artifact"),
];

const PACKAGINGS: &[(&str, &str)] = &[
    ("jar", "the default"),
    ("war", "a web application"),
    ("pom", "an aggregator or a parent — builds nothing itself"),
    ("ear", "an enterprise archive"),
    ("maven-plugin", "a Maven plugin"),
    ("bundle", "an OSGi bundle"),
];

/// The build lifecycle, in order — what a `<phase>` can bind to.
const PHASES: &[(&str, &str)] = &[
    ("validate", "the project is correct and complete"),
    ("initialize", "set properties, create directories"),
    ("generate-sources", "generate sources for compilation"),
    ("process-resources", "copy and filter resources"),
    ("compile", "compile the sources"),
    ("process-classes", "post-process the compiled classes"),
    ("generate-test-sources", "generate test sources"),
    ("test-compile", "compile the tests"),
    ("test", "run the unit tests"),
    ("prepare-package", "before packaging"),
    ("package", "build the jar / war"),
    ("integration-test", "run the integration tests"),
    ("verify", "check the package"),
    ("install", "install into the local repository"),
    ("deploy", "publish to a remote repository"),
    ("clean", "delete the build output"),
    ("site", "build the project site"),
];

/// Candidates at `offset`, in the order they should be read.
///
/// The order is carried on the wire as `sort_text` rather than left implicit: what came out of the
/// repository has to stay above what came out of the table, and a consumer that re-sorts
/// alphabetically would put `zzz-utils` you have never heard of above the `spring-web` that is
/// sitting in `~/.m2`.
pub fn completions(env: &PomEnv<'_>, doc: &Doc<'_>, offset: usize) -> Vec<CompletionItem> {
    let mut items = candidates(env, doc, offset);
    for (i, item) in items.iter_mut().enumerate() {
        item.sort_text = Some(format!("{i:04}"));
    }
    items
}

fn candidates(env: &PomEnv<'_>, doc: &Doc<'_>, offset: usize) -> Vec<CompletionItem> {
    let path = doc.path_at(offset);
    let Some(&leaf) = path.last() else { return Vec::new() };
    let Some((value_start, value_end)) = doc.text_span(leaf) else { return Vec::new() };
    if offset < value_start || offset > value_end {
        return Vec::new(); // inside the tag itself — the XML grammar answers there
    }
    let written = doc.source[value_start..offset].trim_start();
    let span = (offset - written.len(), value_end.max(offset));

    // A `${` anywhere in front of the caret means a property is being named, whatever element it is
    // in — the one case where the element does not decide the answer.
    if let Some(items) = property_completions(env, doc.source, value_start, offset) {
        return items;
    }

    let name = doc.name(leaf);
    let parent = path.get(path.len().wrapping_sub(2)).map(|i| doc.name(*i)).unwrap_or("");
    match name {
        "scope" => return fixed(SCOPES, written, span),
        "type" => return fixed(TYPES, written, span),
        "packaging" if parent == "project" => return fixed(PACKAGINGS, written, span),
        "phase" => return fixed(PHASES, written, span),
        "optional" => {
            return fixed(&[("true", "not inherited by dependents"), ("false", "the default")], written, span)
        }
        "module" => return module_completions(env, doc, written, span),
        _ => {}
    }

    let Some((block, _)) = block_at(doc, offset) else { return Vec::new() };
    match name {
        "groupId" => group_completions(env, written, span),
        "artifactId" => artifact_completions(env, doc, &block, written, span),
        "version" => version_completions(env, &block, written, span),
        _ => Vec::new(),
    }
}

/// One of a fixed vocabulary.
fn fixed(table: &[(&str, &str)], written: &str, span: (usize, usize)) -> Vec<CompletionItem> {
    let lower = written.to_ascii_lowercase();
    table
        .iter()
        .filter(|(value, _)| value.starts_with(&lower))
        .map(|(value, doc)| item(value, "value", doc, span))
        .collect()
}

/// Every groupId in the repository that starts with what is typed, then the ones only the table
/// knows.
fn group_completions(env: &PomEnv<'_>, written: &str, span: (usize, usize)) -> Vec<CompletionItem> {
    let mut out: Vec<CompletionItem> = env
        .catalog
        .groups_with_prefix(written, LIMIT)
        .into_iter()
        .map(|group| item(group, "module", "in your local repository", span))
        .collect();
    let lower = written.to_ascii_lowercase();
    let mut extra: Vec<&str> = known::LIBRARIES
        .iter()
        .chain(known::PLUGINS.iter())
        .map(|(g, _, _)| *g)
        .filter(|g| g.to_ascii_lowercase().starts_with(&lower))
        .collect();
    extra.sort_unstable();
    extra.dedup();
    for group in extra {
        if out.len() >= LIMIT {
            break;
        }
        if out.iter().any(|c| c.label == group) {
            continue;
        }
        out.push(item(group, "module", "not installed", span));
    }
    out
}

/// The artifacts of the group being written, or — when there is no group yet — a search across
/// everything, filling the groupId in on accept.
fn artifact_completions(
    env: &PomEnv<'_>,
    doc: &Doc<'_>,
    block: &Block,
    written: &str,
    span: (usize, usize),
) -> Vec<CompletionItem> {
    let group = env.expand(&block.group.text);
    let group = match group.is_empty() {
        true => block.kind.default_group().to_string(),
        false => group,
    };
    let mut out: Vec<CompletionItem> = Vec::new();

    if !group.is_empty() {
        for artifact in env.catalog.artifacts_in(&group, written, LIMIT) {
            let detail = match artifact.latest() {
                "" => "in your local repository".to_string(),
                latest => format!("{latest} · in your local repository"),
            };
            out.push(item(&artifact.artifact_id, "class", &detail, span));
        }
    }

    // Nothing to go on, or nothing found: search the whole repository and offer to fill the group
    // as well, which is the only way an artifactId alone can become a coordinate that resolves.
    if group.is_empty() || out.is_empty() {
        let fill = block.group.is_written().then(|| fill_group_span(doc, block, span)).flatten();
        for artifact in env.catalog.search(written, LIMIT) {
            if out.len() >= LIMIT {
                break;
            }
            let detail = format!("{} · {}", artifact.group_id, artifact.latest());
            out.push(match &fill {
                Some((wide, prefix, suffix)) => CompletionItem {
                    label: artifact.artifact_id.clone(),
                    kind: "class".to_string(),
                    detail: Some(format!("{detail} · fills the groupId too")),
                    insert_text: Some(format!("{}{prefix}{}{suffix}", artifact.group_id, artifact.artifact_id)),
                    replace_start: Some(wide.0),
                    replace_end: Some(wide.1),
                    ..CompletionItem::default()
                },
                None => item(&artifact.artifact_id, "class", &detail, span),
            });
        }
    }

    // And the table, for the library you have never had.
    let lower = written.to_ascii_lowercase();
    for (g, a, doc_line) in known::table(block.kind == BlockKind::Plugin) {
        if out.len() >= LIMIT {
            break;
        }
        if !group.is_empty() && *g != group {
            continue;
        }
        if !a.to_ascii_lowercase().starts_with(&lower) || out.iter().any(|c| c.label == *a) {
            continue;
        }
        out.push(item(a, "class", &format!("{doc_line} · not installed"), span));
    }
    out
}

/// The span from the (empty) groupId's value to the artifactId's, plus the markup between them —
/// so accepting a completion writes both without reformatting anything.
///
/// `None` unless the groupId is written, empty and **before** the artifactId, which is how every
/// pom in the world is laid out but not something to assume.
fn fill_group_span(
    doc: &Doc<'_>,
    block: &Block,
    artifact_span: (usize, usize),
) -> Option<((usize, usize), String, String)> {
    if !block.group.text.is_empty() {
        return None;
    }
    let (group_start, group_end) = block.group.span?;
    if group_end > artifact_span.0 {
        return None;
    }
    let between = doc.source.get(group_end..artifact_span.0)?.to_string();
    let after = String::new();
    Some(((group_start, artifact_span.1), between, after))
}

/// The versions installed for this coordinate, newest first, plus the managed one when something
/// supplies it (offered first — it is what the build uses if this line is deleted).
fn version_completions(
    env: &PomEnv<'_>,
    block: &Block,
    written: &str,
    span: (usize, usize),
) -> Vec<CompletionItem> {
    let coord = Coord { version: String::new(), ..env.resolve_coord(&block.raw_coord()) };
    if coord.artifact_id.is_empty() {
        return Vec::new();
    }
    let mut out = Vec::new();
    if let Some(pin) = env.managed(&coord) {
        if pin.version.starts_with(written) {
            out.push(item(&pin.version, "value", &format!("managed by {}", pin.from), span));
        }
    }
    for version in env.repo.versions(&coord.group_id, &coord.artifact_id) {
        if out.len() >= LIMIT {
            break;
        }
        if !version.starts_with(written) || out.iter().any(|c| c.label == version) {
            continue;
        }
        out.push(item(&version, "value", "in your local repository", span));
    }
    // A property that already holds a version for this artifact — the pom's own convention, and
    // the thing a hand-written `<version>` most often should have been.
    let stem = coord.artifact_id.split('-').next().unwrap_or_default();
    for (key, value) in &env.effective.properties {
        if out.len() >= LIMIT {
            break;
        }
        if stem.is_empty() || !key.ends_with(".version") || !key.starts_with(stem) {
            continue;
        }
        let placeholder = format!("${{{key}}}");
        if !placeholder.starts_with(written) {
            continue;
        }
        out.push(item(&placeholder, "value", &format!("property · {value}"), span));
    }
    out
}

/// Directories beside this pom that hold one and are not listed yet.
fn module_completions(
    env: &PomEnv<'_>,
    doc: &Doc<'_>,
    written: &str,
    span: (usize, usize),
) -> Vec<CompletionItem> {
    let dir = std::path::PathBuf::from(env.dir());
    let listed: Vec<String> = doc
        .root()
        .and_then(|p| doc.child(p, "modules"))
        .map(|m| doc.children(m).into_iter().map(|c| doc.text(c).to_string()).collect())
        .unwrap_or_default();
    let Ok(entries) = std::fs::read_dir(&dir) else { return Vec::new() };
    let mut out = Vec::new();
    for entry in entries.flatten() {
        if !entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        if !name.starts_with(written) || listed.iter().any(|l| l == &name) {
            continue;
        }
        if !entry.path().join("pom.xml").is_file() {
            continue;
        }
        out.push(item(&name, "module", "a module beside this one", span));
    }
    out.sort_by(|a, b| a.label.cmp(&b.label));
    out
}

/// Property names, when the caret is inside a `${…}`.
///
/// `None` — rather than an empty list — when there is no placeholder in front of the caret, so the
/// caller falls through to the element's own answer.
fn property_completions(
    env: &PomEnv<'_>,
    source: &str,
    value_start: usize,
    offset: usize,
) -> Option<Vec<CompletionItem>> {
    let before = source.get(value_start..offset)?;
    let open = before.rfind("${")?;
    if before[open..].contains('}') {
        return None;
    }
    let written = &before[open + 2..];
    let start = value_start + open;
    // Swallow a `}` the editor already closed, so accepting does not leave `${spring.version}}`.
    let end = match source.get(offset..offset + 1) {
        Some("}") => offset + 1,
        _ => offset,
    };
    let mut names: Vec<(&String, &String)> = env
        .effective
        .properties
        .iter()
        .filter(|(k, _)| k.starts_with(written))
        .collect();
    // The pom's own first, then what it inherits, then Maven's implicit `project.*` — which is the
    // order they are worth reading in, and the reverse of alphabetical.
    names.sort_by(|(a, _), (b, _)| rank(env, a).cmp(&rank(env, b)).then_with(|| a.cmp(b)));
    Some(
        names
            .into_iter()
            .take(LIMIT)
            .map(|(key, value)| CompletionItem {
                label: format!("${{{key}}}"),
                kind: "value".to_string(),
                // The value, and where it was decided when that is not this file — the same
                // question the hover card answers, asked before the property is even written.
                detail: Some(match origin_of(env, key) {
                    Some(pom) => format!("{value} · {pom}"),
                    None => value.clone(),
                }),
                replace_start: Some(start),
                replace_end: Some(end),
                ..CompletionItem::default()
            })
            .collect(),
    )
}

/// Where a property is worth reading in the list: this pom's own, then an inherited one, then the
/// implicit `project.*` that every pom has and nobody is looking for.
fn rank(env: &PomEnv<'_>, name: &str) -> u8 {
    if name.starts_with("project.") || name.starts_with("pom.") || name == "version" {
        return 2;
    }
    match env.effective.property_sites.get(name) {
        Some(site) if site == env.path => 0,
        _ => 1,
    }
}

/// The pom that defines a property, as a name, when it is not the one being edited.
fn origin_of(env: &PomEnv<'_>, name: &str) -> Option<String> {
    let site = env.effective.property_sites.get(name)?;
    if site == env.path {
        return None;
    }
    let parts: Vec<&str> = site.rsplit('/').take(2).collect();
    Some(parts.into_iter().rev().collect::<Vec<_>>().join("/"))
}

fn item(label: &str, kind: &str, detail: &str, span: (usize, usize)) -> CompletionItem {
    CompletionItem {
        label: label.to_string(),
        kind: kind.to_string(),
        detail: (!detail.is_empty()).then(|| detail.to_string()),
        replace_start: Some(span.0),
        replace_end: Some(span.1),
        ..CompletionItem::default()
    }
}

