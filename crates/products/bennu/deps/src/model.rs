//! What a dependency is, once every question about it has been answered.
//!
//! The shape follows the questions a developer actually asks of a dependency list, in order:
//! *what is it*, *which version am I really getting*, *who decided that*, and *is it there*. The
//! third is the one every other tool drops, and it is the one that costs an afternoon: a version
//! you cannot find in the pom in front of you came from a property, from a
//! `<dependencyManagement>` block, or from a parent three directories up, and "which" is not a
//! detail.

use serde::{Deserialize, Serialize};

/// Where a dependency's presence — or its version — was decided.
///
/// Three different facts, deliberately not collapsed into "inherited":
/// - [`Origin::Declared`] — written here, version and all. Nothing to explain.
/// - [`Origin::Managed`] — the module declares the dependency but not its version; a
///   `<dependencyManagement>` entry somewhere up the chain supplies it. The module's pom shows a
///   `<dependency>` with no `<version>`, and the number in this panel is the answer to "so which
///   one".
/// - [`Origin::Inherited`] — the module does not mention the dependency at all; it is on the
///   classpath because a parent pom's own `<dependencies>` are inherited by every child.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Origin {
    Declared,
    /// `from` is the artifactId of the pom whose `<dependencyManagement>` pinned it.
    Managed { from: String },
    /// `from` is the artifactId of the parent pom that declares it.
    Inherited { from: String },
}

impl Origin {
    /// The pom this dependency's version came from, when it was not this one.
    pub fn from(&self) -> Option<&str> {
        match self {
            Origin::Declared => None,
            Origin::Managed { from } | Origin::Inherited { from } => Some(from),
        }
    }
}

/// One dependency of one module, with everything resolved that can be.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Dependency {
    pub group_id: String,
    pub artifact_id: String,
    /// The version, with `${…}` expanded and `<dependencyManagement>` applied.
    ///
    /// **Empty when nothing in the reactor answers it**, which is a real state and not an error:
    /// the version may come from an imported BOM, or from a parent that lives in the repository
    /// rather than on disk. Left empty rather than guessed — an invented version in a dependency
    /// list is worse than an absent one. A `${property}` nothing defines is left *as written*,
    /// because that one is usually a bug in the pom and hiding it would hide the bug.
    pub version: String,
    /// `compile` when the pom does not say — Maven's own default, applied here so the panel never
    /// shows a blank where the answer is known.
    pub scope: String,
    /// `<type>`, when it is not the default `jar` (`pom`, `war`, `test-jar`, …).
    pub packaging: String,
    /// `<classifier>`, when there is one.
    pub classifier: String,
    pub optional: bool,
    pub origin: Origin,
    /// The profile whose `<dependencies>` this came from, empty for the ordinary case. A profile
    /// dependency is only on the classpath when that profile is active, which this cannot know —
    /// so it is shown, and labelled.
    pub profile: String,
    /// The pom that declares it, and where in it — so a row is somewhere the editor can go.
    pub declared_in: Site,
    /// The jar in the local repository this resolved to, empty when it did not resolve.
    ///
    /// The check that makes the panel worth opening when something is broken: a dependency the
    /// project declares and Maven never resolved is exactly the shape of "cannot find symbol" in
    /// a file that looks fine.
    pub jar: String,
}

impl Dependency {
    /// `groupId:artifactId` — the coordinate a person reads.
    pub fn coord(&self) -> String {
        format!("{}:{}", self.group_id, self.artifact_id)
    }

    /// Whether this is the same artifact as `other`, version aside. Two dependencies are "the
    /// same" when their coordinate and classifier match — which is Maven's own conflict key, and
    /// what makes `spring-core` and `spring-core:tests` two entries rather than one.
    pub fn same_artifact(&self, other: &Dependency) -> bool {
        self.group_id == other.group_id
            && self.artifact_id == other.artifact_id
            && self.classifier == other.classifier
    }
}

/// A place in a file, for go-to.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Site {
    /// Absolute path, forward-slashed — the convention on the bennu wire.
    pub file: String,
    /// Byte offset of the declaration's opening tag.
    pub offset: usize,
    /// 1-based line.
    pub line: u32,
}

/// One module of the reactor and what it depends on.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Module {
    /// Display name — the pom's `<name>`, else its artifactId.
    pub name: String,
    pub artifact_id: String,
    /// Absolute path of the module's `pom.xml`, forward-slashed.
    pub pom: String,
    /// `<packaging>` — `jar` unless the pom says otherwise. Worth showing: `pom` means the module
    /// builds nothing and its dependency list is a declaration for its children.
    pub packaging: String,
    pub dependencies: Vec<Dependency>,
}

/// A jar on the resolved classpath that no module declares — i.e. something a dependency dragged
/// in.
///
/// Kept apart from the declared list rather than merged into it, because the two answer different
/// questions and mixing them is how a dependency panel becomes unreadable. This group is where
/// "why is this version of commons-collections on my classpath" is answered.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Transitive {
    pub group_id: String,
    pub artifact_id: String,
    pub version: String,
    /// Absolute path of the jar in the local repository.
    pub jar: String,
}

impl Transitive {
    pub fn coord(&self) -> String {
        format!("{}:{}", self.group_id, self.artifact_id)
    }
}

/// Everything the Dependencies panel shows.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Report {
    pub modules: Vec<Module>,
    pub transitive: Vec<Transitive>,
    /// Whether a resolved classpath was available at all.
    ///
    /// `false` means the jar column is unknown rather than empty — nothing has been resolved yet
    /// (the index resolves it in the background on open), and a panel that showed every dependency
    /// as "not resolved" there would be lying about a project that builds.
    pub classpath_known: bool,
    /// Poms that were found but could not be read, by path. Rare, and worth saying out loud: a
    /// module missing from the list is otherwise indistinguishable from a module with no
    /// dependencies.
    pub unreadable: Vec<String>,
}

impl Report {
    /// Total declared dependencies across every module.
    pub fn declared_count(&self) -> usize {
        self.modules.iter().map(|m| m.dependencies.len()).sum()
    }
}
