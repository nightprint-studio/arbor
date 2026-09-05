//! What is wrong with a pom, checked against the repository the build will actually use.
//!
//! ## The one that matters
//!
//! A dependency whose artifact is not in the local repository compiles to nothing: every type it
//! carries reads as *cannot resolve*, in files that are perfectly correct, and the pom — the one
//! file that could explain it — says nothing at all. Marking the coordinate where it is written is
//! the difference between "bennu is broken" and "this jar was never downloaded".
//!
//! ## The rules that keep it quiet
//!
//! A false underline in a pom is worse than none, because the pom is the file you trust least when
//! something does not build. So:
//!
//! - **no repository, no claims.** A machine whose `~/.m2` is empty gets nothing — every dependency
//!   would be "missing", which is information about the machine and not about the project;
//! - a **reactor module** is built from source and is never looked for in a repository;
//! - a **`<dependencyManagement>`** entry is a version for something this module may not use, so its
//!   artifact being absent is ordinary and is not reported;
//! - a coordinate holding an unexpanded `${…}` is not judged on existence — the property is the
//!   problem, and it is reported as itself;
//! - a **profile's** dependency, and a **plugin**, are warnings rather than errors: neither is
//!   necessarily fetched on a machine that has not run that profile or that goal.

use std::collections::HashMap;

use bennu_proto::prelude::{severity, Diagnostic};

use crate::blocks::{blocks, Block, BlockKind};
use crate::doc::Doc;
use crate::env::{is_own_property, property_references, PomEnv};
use crate::repo::Coord;

/// The scopes Maven understands. Anything else is a typo that silently changes what is on the
/// classpath — `<scope>runtime </scope>` included, which is why the value is trimmed first.
const SCOPES: &[&str] = &["compile", "provided", "runtime", "test", "system", "import"];

/// Everything wrong with this pom.
pub fn diagnostics(env: &PomEnv<'_>, doc: &Doc<'_>) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    let all = blocks(doc);
    for block in &all {
        check_coordinate_properties(env, block, &mut out);
        check_scope(block, &mut out);
        check_version(env, block, &mut out);
        check_parent(env, block, &mut out);
        check_exists(env, block, &mut out);
    }
    check_duplicates(doc, &all, &mut out);
    check_modules(env, doc, &mut out);
    check_loose_properties(env, doc, &all, &mut out);
    out
}

/// A `${property}` in a **coordinate** that this pom is expected to define and does not.
///
/// An error, and not a soft one: Maven resolves it to the literal `${…}` and then looks for an
/// artifact by that name, so the dependency is simply not on the classpath and every type in it is
/// unresolvable. It is the same failure as a missing jar with none of the visible cause.
fn check_coordinate_properties(env: &PomEnv<'_>, block: &Block, out: &mut Vec<Diagnostic>) {
    for field in [&block.group, &block.artifact, &block.version] {
        let Some((start, _)) = field.span else { continue };
        report_undefined(env, &field.text, start, severity::ERROR, out);
    }
}

/// The same `${property}` check over **the rest of the document** — a `<module>`, a plugin's
/// version, a resource directory, one property defined in terms of another.
///
/// A warning rather than an error, because out here the pom is not necessarily the one supplying
/// the value: `${buildNumber}`, `${git.commit.id}` and friends are set by a plugin **during** the
/// build, and a pom that reads one is correct even though nothing in it defines it. Inside a
/// `<configuration>` — which is exactly where those live — nothing is reported at all: that subtree
/// is a plugin's own vocabulary, and this crate has no way to know what the plugin will populate.
///
/// The coordinates keep their own, sharper check above, where the pom really is the last word.
fn check_loose_properties(
    env: &PomEnv<'_>,
    doc: &Doc<'_>,
    blocks: &[Block],
    out: &mut Vec<Diagnostic>,
) {
    // The spans already judged as coordinates, so a `${…}` there is not reported twice in two
    // different words.
    let judged: Vec<(usize, usize)> = blocks
        .iter()
        .flat_map(|b| [&b.group, &b.artifact, &b.version])
        .filter_map(|f| f.span)
        .collect();

    let mut opaque_depth = 0usize;
    let mut depth = 0usize;
    for (i, tag) in doc.scan.tags.iter().enumerate() {
        match tag.kind {
            bennu_xml::prelude::TagKind::Open => {
                depth += 1;
                if opaque_depth == 0 && OPAQUE.contains(&doc.name(i)) {
                    opaque_depth = depth;
                }
            }
            bennu_xml::prelude::TagKind::Close => {
                if opaque_depth == depth {
                    opaque_depth = 0;
                }
                depth = depth.saturating_sub(1);
                continue;
            }
            bennu_xml::prelude::TagKind::SelfClose => continue,
        }
        if opaque_depth != 0 {
            continue;
        }
        // Only leaves carry text; `trimmed_span` answers `None` for an element holding elements,
        // which is what keeps this linear over a big pom.
        let Some((start, end)) = doc.trimmed_span(i) else { continue };
        if judged.contains(&(start, end)) {
            continue;
        }
        let text = &doc.source[start..end];
        report_undefined(env, text, start, severity::WARNING, out);
    }
}

/// Elements whose content belongs to something other than Maven's own interpolation.
///
/// `<configuration>` is a plugin's vocabulary and `<executions>` wraps them; what a `${…}` in there
/// means is the plugin's business.
const OPAQUE: &[&str] = &["configuration", "executions"];

/// Report every `${…}` in `text` that nothing in scope defines.
fn report_undefined(
    env: &PomEnv<'_>,
    text: &str,
    start: usize,
    level: &str,
    out: &mut Vec<Diagnostic>,
) {
    if !text.contains("${") {
        return;
    }
    for (name, from, to) in property_references(text) {
        if !is_own_property(&name) || !is_property_name(&name) {
            continue;
        }
        if env.effective.properties.contains_key(&name) {
            continue;
        }
        out.push(Diagnostic {
            message: format!("`${{{name}}}` is not defined by this pom or its parents"),
            severity: level.to_string(),
            code: "maven-undefined-property".to_string(),
            start: start + from,
            end: start + to,
        });
    }
}

/// Whether a `${…}` actually names a property.
///
/// `<delimiter>${*}</delimiter>` in a resources plugin is a *pattern*, not a reference, and there
/// are a handful of idioms like it. A name has to look like one — a letter or underscore, then word
/// characters, dots and dashes — or it is not this check's business.
fn is_property_name(name: &str) -> bool {
    let mut chars = name.chars();
    chars.next().is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_alphanumeric() || matches!(c, '.' | '-' | '_'))
}

/// A `<scope>` Maven does not know. It does not fail the build — it is silently treated as
/// `compile`, which is exactly why nobody finds it.
fn check_scope(block: &Block, out: &mut Vec<Diagnostic>) {
    let scope = block.scope.text.trim();
    if scope.is_empty() || scope.contains("${") || SCOPES.contains(&scope) {
        return;
    }
    let Some((start, end)) = block.scope.span else { return };
    out.push(Diagnostic {
        message: format!("`{scope}` is not a Maven scope (compile, provided, runtime, test, system, import)"),
        severity: severity::WARNING.to_string(),
        code: "maven-unknown-scope".to_string(),
        start,
        end,
    });
}

/// The two things a version can be wrong about: absent with nothing to supply it, or written out
/// when something already supplies exactly it.
fn check_version(env: &PomEnv<'_>, block: &Block, out: &mut Vec<Diagnostic>) {
    if !matches!(block.kind, BlockKind::Dependency | BlockKind::Managed) {
        return; // a plugin's version comes from the super-pom; an exclusion has none by definition
    }
    let raw = block.raw_coord();
    if raw.artifact_id.is_empty() {
        return;
    }
    let coord = Coord { version: String::new(), ..env.resolve_coord(&raw) };

    if block.version.text.is_empty() {
        // Managed entries are where versions are *declared*; a missing one there is a different
        // (and much rarer) mistake, and the pom is usually mid-edit.
        if block.kind != BlockKind::Dependency || env.managed(&coord).is_some() || env.is_reactor(&coord) {
            return;
        }
        out.push(Diagnostic {
            message: format!(
                "no version for `{}` — nothing in this pom's `<dependencyManagement>` or its parents supplies one",
                coord.ga()
            ),
            severity: severity::WARNING.to_string(),
            code: "maven-missing-version".to_string(),
            start: block.tag.0,
            end: block.tag.1,
        });
        return;
    }

    // Written, and identical to what management already says: harmless, but it is a version that
    // will not follow the parent when the parent moves — which is the whole reason to manage it.
    if block.kind == BlockKind::Dependency {
        if let Some(pin) = env.managed(&coord) {
            let written = env.expand(&block.version.text);
            if pin.version == written {
                if let Some((start, end)) = block.version.span {
                    out.push(Diagnostic {
                        message: format!("version is already managed by `{}` — this line pins it a second time", pin.from),
                        severity: severity::WEAK.to_string(),
                        code: "maven-redundant-version".to_string(),
                        start,
                        end,
                    });
                }
            }
        }
    }
}

/// A `<parent>` nothing could read.
///
/// Not the same question as "is it in the repository", and getting that wrong is a false positive
/// on the commonest layout there is: a reactor's own parent is built from source and is usually
/// **not installed**, so looking for it in `~/.m2` marks half the modules of a healthy project. The
/// real question is whether the parent chain resolved — on disk through `<relativePath>`, or in the
/// repository — and the effective pom already answers it: a chain of one is a pom whose parent was
/// not found anywhere.
///
/// It matters because an unread parent silently takes every managed version and every inherited
/// dependency with it, which surfaces as a dozen unrelated "no version" reports further down.
fn check_parent(env: &PomEnv<'_>, block: &Block, out: &mut Vec<Diagnostic>) {
    if block.kind != BlockKind::Parent || env.effective.chain.len() > 1 {
        return;
    }
    let coord = block.raw_coord();
    if coord.artifact_id.is_empty() || coord.version.is_empty() || coord.version.contains("${") {
        return; // mid-edit — there is nothing yet to fail to find
    }
    out.push(Diagnostic {
        message: format!(
            "parent `{}` was not found — neither beside this pom nor in the local repository. Its              managed versions and inherited dependencies are all missing.",
            coord.gav()
        ),
        severity: severity::ERROR.to_string(),
        code: "maven-unresolved-parent".to_string(),
        start: block.tag.0,
        end: block.tag.1,
    });
}

/// Whether this coordinate is in the repository, and if not, which half of it is wrong.
fn check_exists(env: &PomEnv<'_>, block: &Block, out: &mut Vec<Diagnostic>) {
    if !env.repo_is_usable() {
        return;
    }
    // A managed entry is a version for something the module may never use, and an exclusion names
    // an artifact that by definition is not on the classpath. Neither is a resolution failure, and
    // a parent is judged by whether it was *read* rather than by where it lives — see
    // [`check_parent`].
    if matches!(block.kind, BlockKind::Managed | BlockKind::Exclusion | BlockKind::Parent) {
        return;
    }
    // A `system`-scoped dependency lives at a `<systemPath>`, not in the repository.
    if block.scope.text.trim() == "system" {
        return;
    }
    let raw = block.raw_coord();
    if raw.artifact_id.is_empty() || raw.group_id.is_empty() {
        return; // mid-edit, or a plugin whose group defaults — nothing to judge yet
    }
    let coord = env.resolve_coord(&raw);
    if coord.group_id.contains("${") || coord.artifact_id.contains("${") || coord.version.contains("${") {
        return; // the property is the problem, and it is already reported
    }
    if env.is_reactor(&coord) || coord.version.is_empty() || is_range(&coord.version) {
        return;
    }
    if env.repo.has(&coord) {
        note_newer(env, block, &coord, out);
        return;
    }

    let (start, end) = target_span(block);
    let level = match (block.kind, block.profile.is_empty()) {
        // A plugin is fetched when its goal first runs, and a profile's dependency when that
        // profile is first built — neither absence means the code in front of you cannot compile.
        (BlockKind::Dependency, true) => severity::ERROR,
        _ => severity::WARNING,
    };
    let installed = env.repo.versions(&coord.group_id, &coord.artifact_id);
    // Three states, not two, and the third is the commonest one on a project that has never been
    // built. Telling them apart is the whole value of this diagnostic: they have three different
    // causes and three different cures, and only one of them is "you wrote the wrong version".
    let message = if installed.is_empty() {
        let hint = nearest_artifact(env, &coord)
            .map(|near| format!(" Did you mean `{near}`?"))
            .unwrap_or_default();
        format!("`{}` is not in the local repository — nothing has ever downloaded it.{hint}", coord.gav())
    } else if installed.iter().any(|v| v == &coord.version) {
        // The version IS here — as a POM, without its jar. Maven fetches a pom to walk the
        // dependency graph and the jar only when something compiles against it, so a resolved-but
        // -never-built project has the folder and none of the code. Saying "not in the local
        // repository" here sent a reader to check a folder that was right there.
        format!(
            "`{}` is in the local repository as a pom, but its jar was never downloaded. Maven \
             fetches a pom to read the dependency graph and the jar only when something compiles \
             against it — build once, or turn on Download missing dependencies.",
            coord.gav()
        )
    } else {
        format!(
            "version `{}` of `{}` is not in the local repository. Installed: {}",
            coord.version,
            coord.ga(),
            installed.iter().take(4).cloned().collect::<Vec<_>>().join(", ")
        )
    };
    out.push(Diagnostic {
        message,
        severity: level.to_string(),
        code: "maven-unresolved-dependency".to_string(),
        start,
        end,
    });
}

/// A newer version of a dependency that is **already on this machine** — so acting on it costs
/// nothing and cannot fail. Deliberately not a report about what exists on Maven Central: this
/// crate never goes to the network, and a suggestion that needs a download to try is a different
/// feature with a different failure mode.
///
/// Only for a version written *here*: one that comes from a parent or a BOM is not this file's to
/// change, and marking it would send the reader to a line they cannot edit.
fn note_newer(env: &PomEnv<'_>, block: &Block, coord: &Coord, out: &mut Vec<Diagnostic>) {
    if block.kind != BlockKind::Dependency || block.version.text.is_empty() {
        return;
    }
    let Some((start, end)) = block.version.span else { return };
    let installed = env.repo.versions(&coord.group_id, &coord.artifact_id);
    let Some(newest) = installed.first() else { return };
    if newest == &coord.version {
        return;
    }
    // A pre-release is never suggested over a release: having a `3.0.0-M2` in the repository is not
    // a reason to be told your `2.7.18` is out of date.
    if is_prerelease(newest) && !is_prerelease(&coord.version) {
        return;
    }
    if crate::repo::compare_versions(newest, &coord.version) != std::cmp::Ordering::Greater {
        return;
    }
    out.push(Diagnostic {
        message: format!("`{newest}` is newer and already in your local repository"),
        severity: severity::HINT.to_string(),
        code: "maven-newer-version".to_string(),
        start,
        end,
    });
}

/// The same artifact declared twice in one `<dependencies>` block. Maven takes the last one and
/// says nothing, so the first declaration — usually the one being read — is a lie.
fn check_duplicates(doc: &Doc<'_>, all: &[Block], out: &mut Vec<Diagnostic>) {
    let mut seen: HashMap<(usize, String), usize> = HashMap::new();
    for block in all {
        if !matches!(block.kind, BlockKind::Dependency | BlockKind::Managed) {
            continue;
        }
        let coord = block.raw_coord();
        if coord.artifact_id.is_empty() {
            continue;
        }
        // Keyed on the containing `<dependencies>` as well as the coordinate: the same artifact in
        // two profiles, or in management and in the dependencies, is the normal way to write a pom.
        let parent = doc.path_at(block.tag.0).last().copied().unwrap_or(block.element);
        let key = (parent, coord.key());
        match seen.get(&key) {
            Some(_) => out.push(Diagnostic {
                message: format!("`{}` is already declared in this block — Maven keeps the last one", coord.ga()),
                severity: severity::WARNING.to_string(),
                code: "maven-duplicate-dependency".to_string(),
                start: block.tag.0,
                end: block.tag.1,
            }),
            None => {
                seen.insert(key, block.element);
            }
        }
    }
}

/// A `<module>` naming a directory that is not there. The reactor silently loses the module, and
/// every type in it stops resolving in every other module — with nothing anywhere saying why.
fn check_modules(env: &PomEnv<'_>, doc: &Doc<'_>, out: &mut Vec<Diagnostic>) {
    let Some(project) = doc.root() else { return };
    let Some(modules) = doc.child(project, "modules") else { return };
    let dir = std::path::PathBuf::from(env.dir());
    for child in doc.children(modules) {
        if doc.name(child) != "module" {
            continue;
        }
        let name = doc.text(child);
        if name.is_empty() || name.contains("${") {
            continue;
        }
        let candidate = dir.join(name.trim_end_matches('/'));
        if candidate.join("pom.xml").is_file() || candidate.is_file() {
            continue;
        }
        let Some((start, end)) = doc.trimmed_span(child) else { continue };
        out.push(Diagnostic {
            message: format!("`{name}` has no `pom.xml` — the reactor will not build this module"),
            severity: severity::ERROR.to_string(),
            code: "maven-missing-module".to_string(),
            start,
            end,
        });
    }
}

/// Where an unresolved coordinate is underlined: the artifactId when it is written (the half a
/// reader looks at), else the whole opening tag.
fn target_span(block: &Block) -> (usize, usize) {
    match block.artifact.span {
        Some((start, end)) if end > start => (start, end),
        _ => block.tag,
    }
}

fn is_range(version: &str) -> bool {
    let v = version.trim();
    v.starts_with('[') || v.starts_with('(')
}

fn is_prerelease(version: &str) -> bool {
    let lower = version.to_ascii_lowercase();
    ["snapshot", "alpha", "beta", "-m", "-rc", "-cr", "-pre"].iter().any(|q| lower.contains(q))
}

/// The artifact in the repository whose id is closest to the one written, when it is close enough
/// to be a typo rather than a different library.
fn nearest_artifact(env: &PomEnv<'_>, coord: &Coord) -> Option<String> {
    let mut best: Option<(usize, String)> = None;
    for candidate in env.catalog.artifacts_in(&coord.group_id, "", 400) {
        let distance = edit_distance(&coord.artifact_id, &candidate.artifact_id);
        if best.as_ref().is_none_or(|(d, _)| distance < *d) {
            best = Some((distance, candidate.ga()));
        }
    }
    // Two edits on a name of any length is a typo; more than that is a different artifact, and
    // suggesting one would be worse than suggesting nothing.
    best.filter(|(d, _)| *d <= 2).map(|(_, name)| name)
}

/// Levenshtein, over chars, with the usual two-row table.
fn edit_distance(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    if a.is_empty() || b.is_empty() {
        return a.len().max(b.len());
    }
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    let mut cur = vec![0usize; b.len() + 1];
    for (i, ca) in a.iter().enumerate() {
        cur[0] = i + 1;
        for (j, cb) in b.iter().enumerate() {
            let cost = usize::from(ca != cb);
            cur[j + 1] = (prev[j + 1] + 1).min(cur[j] + 1).min(prev[j] + cost);
        }
        std::mem::swap(&mut prev, &mut cur);
    }
    prev[b.len()]
}
