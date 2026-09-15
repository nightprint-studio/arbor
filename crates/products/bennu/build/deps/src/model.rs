//! What a dependency is, once every question about it has been answered.
//!
//! The shape follows the questions a developer actually asks of a dependency list, in order:
//! *what is it*, *which version am I really getting*, *who decided that*, and *is it there*. The
//! third is the one every other tool drops, and it is the one that costs an afternoon: a version
//! you cannot find in the manifest in front of you came from a property, from a
//! `<dependencyManagement>` block, from a parent three directories up, or from
//! `[workspace.dependencies]` — and "which" is not a detail.
//!
//! ## One shape, two ecosystems
//!
//! These types are **ecosystem-neutral** and read by one panel for both Maven and Cargo. That is a
//! deliberate choice and not an accident of reuse: the four questions above are the same questions,
//! and the answers line up more closely than the vocabularies suggest.
//!
//! | This field | Maven | Cargo |
//! |---|---|---|
//! | [`Dependency::group`] | `groupId` | empty — Cargo crate names are a flat namespace |
//! | [`Dependency::source`] | empty | where it comes from: `crates.io` · `path` · `git` · `workspace` |
//! | [`Dependency::name`] | `artifactId` | the crate name (the local one, when renamed) |
//! | [`Dependency::version`] | the resolved version | the requirement, or the locked version |
//! | [`Dependency::scope`] | `compile` · `test` · `provided` … | `normal` · `dev` · `build` |
//! | [`Dependency::kind`] | `<type>` when not `jar` | empty |
//! | [`Dependency::variant`] | `<classifier>` | the real crate name, when this entry renames it |
//! | [`Dependency::condition`] | the profile it came from | the `cfg(…)` of a target table |
//! | [`Dependency::origin`] | declared · managed · inherited | declared · inherited from the workspace |
//! | [`Dependency::resolved`] | the jar in `~/.m2` | the crate's source in the local registry |
//!
//! [`Origin::Managed`] carrying Cargo's `workspace = true` is the neatest of these: "the module
//! asks for the dependency and something further up chooses the version" is exactly what both
//! mechanisms do.
//!
//! The fields that do NOT generalise are named for what they are rather than smoothed over:
//! [`Dependency::features`] and [`Dependency::source`] are Cargo's alone, [`Dependency::group`] is
//! Maven's, and [`Report::ecosystem`] exists so the panel can label a column `scope` or `kind`
//! instead of pretending they are the same word.
//!
//! [`Dependency::source`] is separate from `group` deliberately, though both are "where it comes
//! from" in loose English. A groupId is part of the artifact's *identity* — it is half the
//! coordinate, and `org.springframework:spring-web` is how the dependency is named. A Cargo source
//! is *provenance*: `serde` is `serde` whether it arrives from crates.io, a path or a fork, and
//! rendering it as `crates.io:serde` would invent a coordinate nobody writes. Collapsing the two
//! into one field meant [`Dependency::coord`] had to guess which it held, which is the kind of
//! tidiness that produces a wrong label.

use serde::{Deserialize, Serialize};

/// Where a dependency's presence — or its version — was decided.
///
/// Three different facts, deliberately not collapsed into "inherited":
/// - [`Origin::Declared`] — written here, version and all. Nothing to explain.
/// - [`Origin::Managed`] — the module declares the dependency but not its version; something
///   further up supplies it. A Maven `<dependencyManagement>` entry, or Cargo's
///   `serde = { workspace = true }`. The module's manifest shows the dependency with no version,
///   and the number in this panel is the answer to "so which one".
/// - [`Origin::Inherited`] — the module does not mention the dependency at all; it is there because
///   a parent's own `<dependencies>` are inherited by every child. Maven only.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Origin {
    Declared,
    /// `from` names what pinned the version — the pom whose `<dependencyManagement>` did, or the
    /// workspace root.
    Managed { from: String },
    /// `from` is the artifactId of the parent pom that declares it.
    Inherited { from: String },
}

impl Origin {
    /// What decided this dependency's version, when it was not this manifest.
    pub fn from(&self) -> Option<&str> {
        match self {
            Origin::Declared => None,
            Origin::Managed { from } | Origin::Inherited { from } => Some(from),
        }
    }
}

/// One dependency of one module, with everything resolved that can be.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Dependency {
    /// Maven: the `groupId` — half the coordinate. Empty for Cargo, whose crate names are a single
    /// flat namespace.
    pub group: String,
    /// Cargo: where the crate comes from — `crates.io`, `path`, `git`, `workspace`, or a named
    /// registry. Empty for Maven, where the equivalent question is answered by the repository the
    /// jar was found in.
    ///
    /// Provenance, not identity: see the module doc for why this is not folded into
    /// [`Dependency::group`].
    pub source: String,
    /// Maven: the `artifactId`. Cargo: the crate name as this manifest refers to it, which for a
    /// renamed dependency is the local name — the one `use` statements and feature references see.
    pub name: String,
    /// The version, with `${…}` expanded and management applied (Maven), or the requirement as
    /// written, replaced by the locked version when one is known (Cargo).
    ///
    /// **Empty when nothing answers it**, which is a real state and not an error: the version may
    /// come from an imported BOM, from a parent that lives in the repository rather than on disk, or
    /// from a `[workspace.dependencies]` entry that is not there. Left empty rather than guessed —
    /// an invented version in a dependency list is worse than an absent one. A `${property}` nothing
    /// defines is left *as written*, because that one is usually a bug in the pom and hiding it
    /// would hide the bug.
    pub version: String,
    /// When it is needed. Maven: `compile` when the pom does not say — Maven's own default, applied
    /// here so the panel never shows a blank where the answer is known. Cargo: `normal` · `dev` ·
    /// `build`.
    pub scope: String,
    /// Maven: `<type>`, when it is not the default `jar` (`pom`, `war`, `test-jar`, …). Empty for
    /// Cargo.
    pub kind: String,
    /// Maven: the `<classifier>`, when there is one. Cargo: the real crate name when this entry
    /// renames it (`json = { package = "serde_json" }`).
    ///
    /// Both are "the same coordinate, a different artifact behind it", which is why they share a
    /// field and why it is called `variant` rather than either ecosystem's word.
    pub variant: String,
    pub optional: bool,
    pub origin: Origin,
    /// What has to be true for this dependency to be on the graph at all, empty for the ordinary
    /// case. Maven: the profile whose `<dependencies>` it came from. Cargo: the `cfg(…)` of a
    /// `[target.'…'.dependencies]` table.
    ///
    /// Neither can be evaluated here — whether a profile is active and which platform you are
    /// building for are not facts about the manifest — so it is shown, and labelled.
    pub condition: String,
    /// Cargo only: the features this manifest turns on for the dependency. Empty for Maven, which
    /// has no equivalent.
    pub features: Vec<String>,
    /// The manifest that declares it, and where in it — so a row is somewhere the editor can go.
    pub declared_in: Site,
    /// Where the artifact actually is: the jar in the local Maven repository, or the crate's
    /// unpacked source in the local Cargo registry. Empty when it did not resolve.
    ///
    /// The check that makes the panel worth opening when something is broken: a dependency the
    /// project declares and the build tool never resolved is exactly the shape of "cannot find
    /// symbol" in a file that looks fine.
    pub resolved: String,
}

impl Default for Dependency {
    fn default() -> Self {
        Self {
            group: String::new(),
            source: String::new(),
            name: String::new(),
            version: String::new(),
            scope: String::new(),
            kind: String::new(),
            variant: String::new(),
            optional: false,
            origin: Origin::Declared,
            condition: String::new(),
            features: Vec::new(),
            declared_in: Site::default(),
            resolved: String::new(),
        }
    }
}

impl Dependency {
    /// The coordinate a person reads: `group:name` when there is a group, else just the name.
    ///
    /// One function serves both ecosystems without a discriminator, because `group` is only ever a
    /// namespace — a Maven dependency reads `org.springframework:spring-web` and a Cargo one reads
    /// `serde`.
    pub fn coord(&self) -> String {
        if self.group.is_empty() {
            self.name.clone()
        } else {
            format!("{}:{}", self.group, self.name)
        }
    }

    /// Whether this is the same artifact as `other`, version aside. Two dependencies are "the same"
    /// when their coordinate and variant match — which is Maven's own conflict key, and what makes
    /// `spring-core` and `spring-core:tests` two entries rather than one.
    pub fn same_artifact(&self, other: &Dependency) -> bool {
        self.group == other.group && self.name == other.name && self.variant == other.variant
    }
}

/// A place in a file, for go-to.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Site {
    /// Absolute path, forward-slashed — the convention on the bennu wire.
    pub file: String,
    /// Byte offset of the declaration.
    pub offset: usize,
    /// 1-based line.
    pub line: u32,
}

/// One module (Maven) or crate (Cargo) of the project, and what it depends on.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Module {
    /// Display name — the pom's `<name>`, else its artifactId; the crate's name for Cargo.
    pub name: String,
    /// The identifier the build tool knows it by: the artifactId, or the crate name.
    pub id: String,
    /// Absolute path of the module's manifest (`pom.xml` / `Cargo.toml`), forward-slashed.
    pub manifest: String,
    /// What the module builds. Maven: `<packaging>` — `jar` unless the pom says otherwise, and
    /// `pom` means the module builds nothing and its dependency list is a declaration for its
    /// children. Cargo: the target kinds it has (`lib`, `bin`, `lib+bin`).
    pub kind: String,
    pub dependencies: Vec<Dependency>,
}

/// An artifact on the resolved graph that no module declares — i.e. something a dependency dragged
/// in.
///
/// Kept apart from the declared list rather than merged into it, because the two answer different
/// questions and mixing them is how a dependency panel becomes unreadable. This group is where "why
/// is this version of commons-collections on my classpath" is answered.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Transitive {
    /// Maven's `groupId`. Empty for Cargo — see [`Dependency::group`].
    pub group: String,
    pub name: String,
    pub version: String,
    /// Absolute path of the artifact: the jar, or the crate's unpacked source.
    pub resolved: String,
}

impl Transitive {
    /// The coordinate a person reads — see [`Dependency::coord`].
    pub fn coord(&self) -> String {
        if self.group.is_empty() {
            self.name.clone()
        } else {
            format!("{}:{}", self.group, self.name)
        }
    }
}

/// Everything the Dependencies panel shows.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Report {
    /// Which build tool this report describes: `maven` or `cargo`.
    ///
    /// The panel needs it for the words, not the shape: `scope` and `kind` are the same column and
    /// not the same term, and a Maven `groupId:artifactId` and a Cargo crate name are laid out
    /// differently. Empty for a project that is neither, which is a report with nothing in it.
    pub ecosystem: String,
    pub modules: Vec<Module>,
    pub transitive: Vec<Transitive>,
    /// Whether resolved artifacts were available at all.
    ///
    /// `false` means the [`Dependency::resolved`] column is **unknown** rather than empty — nothing
    /// has been resolved yet (Maven resolves in the background as the project indexes; Cargo has no
    /// `Cargo.lock`) — and a panel that showed every dependency of a project that builds as "not
    /// resolved" would be lying.
    pub resolved_known: bool,
    /// Manifests that were found but could not be read, by path. Rare, and worth saying out loud: a
    /// module missing from the list is otherwise indistinguishable from a module with no
    /// dependencies.
    pub unreadable: Vec<String>,
}

impl Report {
    /// Total declared dependencies across every module.
    pub fn declared_count(&self) -> usize {
        self.modules.iter().map(|m| m.dependencies.len()).sum()
    }

    /// Declared dependencies that did not resolve — the number worth surfacing without being asked.
    /// Always zero while [`Report::resolved_known`] is false, since nothing is known either way.
    pub fn unresolved_count(&self) -> usize {
        if !self.resolved_known {
            return 0;
        }
        self.modules
            .iter()
            .flat_map(|m| &m.dependencies)
            .filter(|d| d.resolved.is_empty())
            .count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_coordinate_reads_the_way_its_ecosystem_writes_it() {
        let maven = Dependency {
            group: "org.springframework".into(),
            name: "spring-web".into(),
            ..Dependency::default()
        };
        assert_eq!(maven.coord(), "org.springframework:spring-web");

        // Cargo has no group: where the crate comes from is `source`, which is provenance and not
        // part of the name. Folding it in would read as `crates.io:serde`.
        let cargo = Dependency {
            source: "crates.io".into(),
            name: "serde".into(),
            ..Dependency::default()
        };
        assert_eq!(cargo.coord(), "serde");
    }

    #[test]
    fn unresolved_is_zero_while_nothing_is_known() {
        let mut report = Report {
            modules: vec![Module {
                dependencies: vec![Dependency::default(), Dependency::default()],
                ..Module::default()
            }],
            ..Report::default()
        };
        assert_eq!(report.declared_count(), 2);
        // Nothing resolved yet: the answer is "unknown", and counting them as missing would be a
        // warning about a project that builds.
        assert_eq!(report.unresolved_count(), 0);
        report.resolved_known = true;
        assert_eq!(report.unresolved_count(), 2);
    }

    #[test]
    fn the_same_artifact_ignores_the_version_and_not_the_variant() {
        let a = Dependency {
            group: "g".into(),
            name: "core".into(),
            version: "1".into(),
            ..Dependency::default()
        };
        let newer = Dependency { version: "2".into(), ..a.clone() };
        let tests = Dependency { variant: "tests".into(), ..a.clone() };
        assert!(a.same_artifact(&newer));
        assert!(!a.same_artifact(&tests));
    }
}
