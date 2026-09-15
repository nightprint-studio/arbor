//! `build_units` domain — `bennu_build_units`.
//!
//! What a **build unit** is, said on its own row: the directory where a build target is declared —
//! a Maven module, a Cargo crate. The sidebar already draws those rows differently (a filled,
//! accented folder); this is the line of text beside the name.
//!
//! ## What is worth writing there
//!
//! The **language level the unit compiles at**, and it is the same question in both ecosystems:
//! Java's `21` and Rust's `2024` edition are the one property that varies between siblings of the
//! same reactor and that nothing else on screen shows. A project part-way through a migration is
//! the case that makes it worth having — two modules on 21, one still on 8, and no way to tell
//! which is which without opening three poms.
//!
//! Then the **packaging**, and only when it is not the default: a `war` or a `pom` module behaves
//! differently from its siblings, and a `jar` said out loud on twenty rows is twenty rows of noise.
//! For Cargo the analogue is a crate that builds a binary.
//!
//! The rest — the artifact id, the version, the source of the level — goes in the tooltip. A row
//! that says everything says nothing.
//!
//! ## Asked about directories the caller already picked
//!
//! The editor knows which rows are build units: it drew them. So this describes the directories it
//! is given rather than re-deriving the reactor, which keeps one definition of "build unit" in one
//! place and means a module Maven does not list is still described when the tree shows it.

use std::path::Path;

use bennu_core::prelude::BennuState;
use serde::{Deserialize, Serialize};

/// Args for [`bennu_build_units`].
#[derive(Deserialize)]
pub struct BuildUnitsArgs {
    /// Absolute paths of the directories to describe — the rows the tree drew as build units.
    pub dirs: Vec<String>,
}

/// One build unit, on the wire.
#[derive(Debug, Clone, Serialize)]
pub struct BuildUnitWire {
    /// The directory asked about, echoed so the caller can key by it without relying on order.
    pub dir: String,
    /// What the unit is CALLED to the build — the Maven artifactId, the crate name.
    ///
    /// Worth its own field rather than being folded into the label: it is the one part that is
    /// often the same as the folder name, and only the editor is in a position to see that and
    /// leave it out. Elided in the middle when it is long, since an artifactId's ends are what
    /// distinguish it (`acme-portal-service-api` and `acme-portal-service-impl` differ in the last
    /// word) — a plain truncation would draw the same three rows.
    pub artifact: String,
    /// The short line beside the name — `JDK 21`, `JDK 21 · war`, `Rust 2024 · bin`.
    pub label: String,
    /// The whole of it, for the tooltip.
    pub detail: String,
}

/// How much of an artifactId a tree row can carry before the middle is elided.
const ARTIFACT_MAX: usize = 22;

/// Shorten an artifactId from the MIDDLE, which is where the part that repeats between siblings
/// lives. `acme-portal-service-api` → `acme-por…-api`.
fn elide(name: &str) -> String {
    let chars: Vec<char> = name.chars().collect();
    if chars.len() <= ARTIFACT_MAX {
        return name.to_string();
    }
    // The tail is what tells siblings apart, so it keeps more of the budget than the head.
    let tail = (ARTIFACT_MAX - 1) / 2;
    let head = ARTIFACT_MAX - 1 - tail;
    let mut out: String = chars[..head].iter().collect();
    out.push('…');
    out.extend(&chars[chars.len() - tail..]);
    out
}

/// Describe each directory that holds a build manifest. A directory that holds none — or whose
/// manifest cannot be read — is simply absent from the answer, which the caller draws as a plain
/// row. Never errors: a decoration that failed loudly would be a dialog about a tree row.
#[arbor_rpc::handler]
fn bennu_build_units(
    _ctx: &BennuState,
    args: BuildUnitsArgs,
) -> Result<Vec<BuildUnitWire>, String> {
    Ok(args.dirs.iter().filter_map(|dir| describe(Path::new(dir))).collect())
}

/// The one description, whichever ecosystem the directory belongs to.
fn describe(dir: &Path) -> Option<BuildUnitWire> {
    if dir.join("pom.xml").is_file() {
        return maven(dir);
    }
    if dir.join("Cargo.toml").is_file() {
        return cargo(dir);
    }
    None
}

fn maven(dir: &Path) -> Option<BuildUnitWire> {
    let xml = std::fs::read_to_string(dir.join("pom.xml")).ok()?;
    let pom = bennu_project::prelude::parse_pom(&xml);
    // The level as the module EFFECTIVELY has it: a module that declares none inherits its
    // parent's, and reading only its own pom would leave most rows blank on a project whose
    // aggregator declares the level once for everybody.
    let level = crate::index_service::module_jdk_for(dir);
    let mut label: Vec<String> = Vec::new();
    let mut detail: Vec<String> = Vec::new();
    if let Some(level) = &level {
        label.push(format!("JDK {}", level.version));
        detail.push(format!("Java {} (from {})", level.version, level.source));
    }
    // `jar` is what a module is unless it says otherwise, so it is not worth a word.
    if let Some(packaging) = pom.packaging.as_deref().filter(|p| !p.eq_ignore_ascii_case("jar")) {
        label.push(packaging.to_string());
        detail.push(format!("packaging {packaging}"));
    }
    if !pom.artifact_id.is_empty() {
        detail.insert(0, pom.artifact_id.clone());
    }
    if !pom.modules.is_empty() {
        detail.push(format!("{} module{}", pom.modules.len(), plural(pom.modules.len())));
    }
    finish(dir, &pom.artifact_id, label, detail)
}

fn cargo(dir: &Path) -> Option<BuildUnitWire> {
    let text = std::fs::read_to_string(dir.join("Cargo.toml")).ok()?;
    let manifest = bennu_project::prelude::parse_cargo_manifest(&text);
    let mut label: Vec<String> = Vec::new();
    let mut detail: Vec<String> = Vec::new();
    if let Some(edition) = manifest.edition.as_deref().filter(|e| !e.is_empty()) {
        label.push(format!("Rust {edition}"));
        detail.push(format!("Rust edition {edition}"));
    }
    // The analogue of a `war`: a crate that produces something you run rather than something you
    // link. Read from the file layout, which is where Cargo's own default reads it from.
    if dir.join("src/main.rs").is_file() || text.contains("[[bin]]") {
        label.push("bin".to_string());
        detail.push("builds a binary".to_string());
    }
    if !manifest.name.is_empty() {
        detail.insert(0, manifest.name.clone());
    }
    if !manifest.members.is_empty() {
        detail.push(format!("{} member{}", manifest.members.len(), plural(manifest.members.len())));
    }
    finish(dir, &manifest.name, label, detail)
}

fn plural(n: usize) -> &'static str {
    if n == 1 {
        ""
    } else {
        "s"
    }
}

/// `None` when there was nothing to say: an empty chip is worse than no chip.
///
/// The artifact name is dropped when it only repeats the folder the row already shows — which is
/// the common case, and the reason it is not simply appended to the label. Compared
/// case-insensitively, since `Acme-Portal` in a folder called `acme-portal` is the same name.
fn finish(dir: &Path, artifact: &str, label: Vec<String>, detail: Vec<String>) -> Option<BuildUnitWire> {
    if label.is_empty() && detail.is_empty() {
        return None;
    }
    let folder = dir.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    let artifact = if artifact.is_empty() || artifact.eq_ignore_ascii_case(&folder) {
        String::new()
    } else {
        elide(artifact)
    };
    Some(BuildUnitWire {
        dir: dir.display().to_string(),
        artifact,
        label: label.join(" · "),
        detail: detail.join(" · "),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_short_artifact_name_is_left_alone() {
        assert_eq!(elide("service"), "service");
        assert_eq!(elide("acme-portal-service"), "acme-portal-service");
    }

    /// Elided in the MIDDLE, and the tail keeps more of the budget: siblings of a reactor differ in
    /// their last word, so a plain truncation would draw them identically.
    #[test]
    fn a_long_artifact_name_keeps_both_ends() {
        let api = elide("acme-portal-service-api");
        let impl_ = elide("acme-portal-service-impl");
        assert!(api.ends_with("api"), "{api}");
        assert!(impl_.ends_with("impl"), "{impl_}");
        assert_ne!(api, impl_, "two siblings must not draw the same");
        assert!(api.chars().count() <= ARTIFACT_MAX, "{api}");
        assert!(impl_.chars().count() <= ARTIFACT_MAX, "{impl_}");
    }

    /// A name that only repeats the folder the row already shows says nothing.
    #[test]
    fn an_artifact_that_repeats_the_folder_is_dropped() {
        let unit = finish(Path::new("/p/service"), "service", vec!["JDK 21".into()], vec![])
            .expect("a unit");
        assert_eq!(unit.artifact, "");
        let named = finish(Path::new("/p/service"), "acme-service", vec!["JDK 21".into()], vec![])
            .expect("a unit");
        assert_eq!(named.artifact, "acme-service");
    }
}
