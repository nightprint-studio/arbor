//! What the scan produces: the ECS types a project declares, the systems that touch them, and
//! where each was written.
//!
//! Deliberately flat and owned — no borrows into the sources, because the model outlives the scan
//! and is handed to catalog queries from another thread.
//!
//! ## What a name means here
//!
//! Types are identified by their **simple name** (`Transform`, not `bevy::prelude::Transform`).
//! Bevy's own conflict rule is over resolved `ComponentId`s, and this crate has no resolver — so
//! two distinct `Health` types in two modules look like one here. That is the single approximation
//! everything downstream inherits, and the reason a conflict row says *which* access it came from:
//! the name is the claim, the parameter is the evidence.

use std::path::PathBuf;

/// The ECS role a declaration plays. A type can hold several (a `Component` that also derives
/// `Default` holds one; a `Component` that is also a `Message` holds two).
///
/// **`Message` and `Event` are two roles, not one name for one thing.** Bevy split them: a message
/// is buffered — written with a `MessageWriter`, drained by whoever reads it — while an event is
/// *triggered*, and delivered to observers. They are declared differently, consumed differently,
/// and a catalog that merged them would answer "who reads this" wrongly for both. The older
/// spellings still map onto whichever of the two they meant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Role {
    Component,
    Resource,
    /// A buffered message: `#[derive(Message)]`, and the `BufferedEvent` it was called for one
    /// release. Read with a `MessageReader`, written with a `MessageWriter`.
    Message,
    /// An observer event: `#[derive(Event)]` / `#[derive(EntityEvent)]`, delivered to whoever
    /// observes it rather than posted to a queue.
    Event,
    Bundle,
    /// A `States` enum — the thing `OnEnter` / `in_state` key on.
    States,
    /// A `#[derive(SystemParam)]` struct: not data, but the accesses of the systems that take one.
    SystemParam,
    /// A `#[derive(Asset)]` type — loaded rather than spawned.
    ///
    /// Not an ECS role in the strict sense, and here anyway: what makes it belong in the same
    /// list is that the question is the same one. "Who touches `SpiralHoverMaterial`" is
    /// answered by the same signatures — a `ResMut<Assets<SpiralHoverMaterial>>` that creates
    /// one, a `Query<&MeshMaterial3d<…>>` that reads it — and answering it in a different panel
    /// would split one question in two. `Bundle` is here on the same argument.
    Asset,
}

impl Role {
    /// The badge a catalog row carries.
    pub fn label(self) -> &'static str {
        match self {
            Role::Component => "Component",
            Role::Resource => "Resource",
            Role::Message => "Message",
            Role::Event => "Event",
            Role::Bundle => "Bundle",
            Role::States => "States",
            Role::SystemParam => "SystemParam",
            Role::Asset => "Asset",
        }
    }

    /// The gutter icon key. Unknown keys render as a neutral dot, so adding a role never breaks
    /// the editor.
    pub fn gutter_kind(self) -> &'static str {
        match self {
            Role::Component => "component",
            Role::Resource => "resource",
            Role::Message => "message",
            Role::Event => "event",
            Role::Bundle => "bundle",
            Role::States => "states",
            Role::SystemParam => "systemparam",
            Role::Asset => "asset",
        }
    }

    /// The role a derive (or a manual `impl … for`) names, if it names one.
    ///
    /// Includes the derives of an engine built **on** Bevy: a `#[derive(DomainResource)]` is a
    /// resource as far as every question this crate answers goes — it is stored per document rather
    /// than once, which changes where it lives and not what it is.
    pub fn from_trait(name: &str) -> Option<Role> {
        match name {
            "Component" => Some(Role::Component),
            "Resource" | "DomainResource" => Some(Role::Resource),
            "Message" | "BufferedEvent" | "DomainMessage" => Some(Role::Message),
            "Event" | "EntityEvent" => Some(Role::Event),
            "Bundle" => Some(Role::Bundle),
            "States" | "SubStates" | "ComputedStates" | "DomainState" => Some(Role::States),
            "SystemParam" => Some(Role::SystemParam),
            "Asset" => Some(Role::Asset),
            _ => None,
        }
    }

    /// Whether a declaration in this role is reached through a **buffer** rather than by name.
    pub fn buffered(self) -> bool {
        matches!(self, Role::Message | Role::Event)
    }
}

/// The access targets a declaration can turn up under.
///
/// Its own name covers a component, a resource and an observer event — a system names those
/// directly. A **message** does not: a `MessageReader<HudCommand>` is a read of the *buffer*
/// `Messages<HudCommand>`, which is the resource two systems actually contend over, and is the
/// name this crate keys such an access by. Looking a declaration up by its own name alone is why
/// every message in a project read as touched by nothing.
pub fn access_keys(name: &str, roles: &[Role]) -> Vec<String> {
    let mut keys = vec![name.to_string()];
    if roles.iter().any(|r| r.buffered()) {
        keys.push(format!("Messages<{name}>"));
    }
    // An asset is never named directly by a system: it is reached through the `Assets<T>`
    // resource that stores it. Keying an asset by its own name alone is why every material in a
    // project read as touched by nothing — the same mistake, and the same fix, as a message.
    if roles.contains(&Role::Asset) {
        keys.push(format!("Assets<{name}>"));
    }
    keys
}

/// One declared type — where it is, and what it is to the ECS.
#[derive(Debug, Clone)]
pub struct TypeDecl {
    pub name: String,
    /// Sorted and deduplicated, so two files agreeing on a type produce one row.
    pub roles: Vec<Role>,
    pub file: PathBuf,
    /// Byte offset of the type's **name** — where a jump lands.
    pub offset: usize,
    pub line: u32,
    /// Field types, verbatim. Populated for a `Bundle` (what it inserts) and for a `SystemParam`
    /// (whose accesses a system taking one inherits); empty otherwise. Reduce with
    /// [`crate::params::type_key`] to compare one against a declaration.
    pub fields: Vec<String>,
}

/// How a system touches one type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccessKind {
    ComponentRead,
    ComponentWrite,
    ResourceRead,
    ResourceWrite,
}

impl AccessKind {
    pub fn writes(self) -> bool {
        matches!(self, AccessKind::ComponentWrite | AccessKind::ResourceWrite)
    }

    pub fn label(self) -> &'static str {
        match self {
            AccessKind::ComponentRead => "read",
            AccessKind::ComponentWrite => "write",
            AccessKind::ResourceRead => "read (resource)",
            AccessKind::ResourceWrite => "write (resource)",
        }
    }
}

/// One `With<T>` / `Without<T>` a query carries. The only thing that can make two conflicting
/// accesses provably harmless — and the only disjointness rule Bevy itself applies.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Filter {
    With(String),
    Without(String),
}

/// One type a system reads or writes, with the query filters that qualify it.
#[derive(Debug, Clone)]
pub struct Access {
    pub target: String,
    pub kind: AccessKind,
    /// The filters of the query this came from. Empty for a resource, and for an unfiltered query.
    pub filters: Vec<Filter>,
    /// Whether the filter expression contained an `Or<…>` this crate did not try to reason about.
    /// Such an access is kept (it is real) but never used to *claim* a conflict — see
    /// [`crate::conflict`].
    pub opaque_filter: bool,
    /// The parameter it was read from, verbatim (`q: Query<&mut Transform, With<Player>>`). The
    /// evidence behind every row that names this access.
    pub param: String,
}

/// A function whose parameters make it a system.
#[derive(Debug, Clone)]
pub struct SystemDecl {
    pub name: String,
    pub file: PathBuf,
    pub offset: usize,
    pub line: u32,
    pub accesses: Vec<Access>,
    /// Takes `&mut World` — it runs alone, whatever else is in the schedule.
    pub exclusive: bool,
    /// The schedules it was seen registered in (`Update`, `FixedUpdate`, `OnEnter(Menu)`), in
    /// registration order. Empty means no `add_systems` call in this project named it: a system
    /// registered by a helper this scan could not follow, or a plain function that merely looks
    /// like one.
    pub schedules: Vec<String>,
    /// The sets it was put in with `.in_set(…)`.
    pub sets: Vec<String>,
}

impl SystemDecl {
    /// A one-line summary of what it touches — the catalog's secondary column.
    pub fn access_summary(&self) -> String {
        if self.exclusive {
            return "&mut World — runs alone".to_string();
        }
        if self.accesses.is_empty() {
            return "no component or resource access".to_string();
        }
        let mut parts: Vec<String> = Vec::new();
        for a in &self.accesses {
            let part = if a.kind.writes() {
                format!("&mut {}", a.target)
            } else {
                format!("&{}", a.target)
            };
            if !parts.contains(&part) {
                parts.push(part);
            }
        }
        parts.join(", ")
    }
}

/// One `fn fragment_shader()` — a material naming a shader, and where it said so.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShaderRefDecl {
    /// `fragment`, `vertex`, `prepass_fragment`, …
    pub stage: String,
    /// The asset path as written.
    pub path: String,
    /// Byte offsets of the path INSIDE its quotes — a go-to lands on the path, not the quote.
    pub offset: usize,
    pub end: usize,
    pub line: u32,
}

/// One binding a material's `#[derive(AsBindGroup)]` declares.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BindingDecl {
    pub index: u32,
    pub kind: crate::shader::BindingKind,
    pub field: String,
    pub ty: String,
    pub offset: usize,
    pub line: u32,
}

/// A `#[derive(AsBindGroup)]` type — what a material supplies the pipeline, and which shaders
/// it runs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MaterialDecl {
    pub name: String,
    pub file: PathBuf,
    pub offset: usize,
    pub line: u32,
    pub bindings: Vec<BindingDecl>,
    pub shaders: Vec<ShaderRefDecl>,
}

/// A `#[derive(ShaderType)]` struct: the layout a uniform is written in, which a `struct` in the
/// shader has to match byte for byte.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UniformStruct {
    pub name: String,
    pub file: PathBuf,
    pub offset: usize,
    pub line: u32,
    pub fields: Vec<UniformField>,
}

/// One field of a [`UniformStruct`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UniformField {
    pub name: String,
    /// Verbatim Rust type.
    pub ty: String,
    pub offset: usize,
    pub line: u32,
}

/// One material naming one shader — the row under a shader in the catalog.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShaderUse {
    pub material: String,
    pub stage: String,
    /// The `.rs` that said so.
    pub file: PathBuf,
    pub offset: usize,
    pub end: usize,
    pub line: u32,
}

/// One site where a declaration is put into the world.
///
/// The answer to "who creates one of these", which no signature carries: a `Query<&Health>` says
/// who reads it, and until something spawns a `Health` there is nothing to read. It is also the
/// first place to look when a component is not behaving — not who consumes it, but who put it
/// there and with what.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InsertSite {
    pub type_name: String,
    pub kind: crate::items::InsertKind,
    pub file: PathBuf,
    pub offset: usize,
    pub line: u32,
    /// The argument as written — `Health(100.0)`, `Transform::from_xyz(x, y, 0.0)`.
    pub arg: String,
    /// The function it happens in, when the site is inside one. Empty for a call at module scope.
    ///
    /// Approximate by construction: the enclosing function is taken to be the last one declared
    /// above the site, which is right for everything but a call inside a nested item. Wrong only
    /// in the label, never in the jump.
    pub in_fn: String,
}

/// Everything the project's Rust sources said, ready to be queried.
#[derive(Debug, Clone, Default)]
pub struct BevyModel {
    pub types: Vec<TypeDecl>,
    pub systems: Vec<SystemDecl>,
    pub conflicts: Vec<crate::conflict::Conflict>,
    /// The `#[derive(AsBindGroup)]` types.
    pub materials: Vec<MaterialDecl>,
    /// The `#[derive(ShaderType)]` layouts, by which the uniforms are checked.
    pub uniforms: Vec<UniformStruct>,
    /// One entry per shader a material names, with what the two disagree about.
    pub shaders: Vec<crate::shader_link::ShaderLink>,
    /// Every `spawn` / `insert` / `insert_resource` site, by the type it names.
    pub inserts: Vec<InsertSite>,
}

impl BevyModel {
    /// Systems that touch a declaration, with how. The question behind "who touches `Health`" — a
    /// query over signatures rather than a text search, which is the whole point.
    ///
    /// Takes the declaration's **keys** rather than its name; see [`access_keys`] for why a message
    /// has two.
    pub fn touching<'a>(&'a self, keys: &[String]) -> Vec<(&'a SystemDecl, &'a Access)> {
        self.systems
            .iter()
            .flat_map(|s| s.accesses.iter().map(move |a| (s, a)))
            .filter(|(_, a)| keys.iter().any(|k| k == &a.target))
            .collect()
    }

    /// Systems that use `name` as a query **filter** — which for a marker component is the whole
    /// of its job.
    ///
    /// A `With<Player>` reads no data, so it is not an access and never will be: two systems
    /// filtering on the same marker do not contend, and folding these into [`Self::touching`] would
    /// invent conflicts. But a marker that nothing *reads* is not a marker that nothing uses, and a
    /// row saying "no system touches it" beside a component twenty queries filter on is a row that
    /// is technically true and practically a lie.
    ///
    /// One entry per system, however many of its queries name the marker.
    pub fn filtering<'a>(&'a self, name: &str) -> Vec<(&'a SystemDecl, &'a Access)> {
        let mut out: Vec<(&'a SystemDecl, &'a Access)> = Vec::new();
        for s in &self.systems {
            if let Some(a) = s.accesses.iter().find(|a| {
                a.filters.iter().any(|f| match f {
                    Filter::With(t) | Filter::Without(t) => t == name,
                })
            }) {
                out.push((s, a));
            }
        }
        out
    }

    /// Bundles that carry `type_name` as a field.
    pub fn bundles_with<'a>(&'a self, type_name: &str) -> Vec<&'a TypeDecl> {
        self.types
            .iter()
            .filter(|t| {
                t.roles.contains(&Role::Bundle)
                    && t.fields.iter().any(|f| crate::params::type_key(f) == type_name)
            })
            .collect()
    }

    /// The declarations in one file, for the gutter.
    pub fn types_in<'a>(&'a self, file: &std::path::Path) -> Vec<&'a TypeDecl> {
        self.types.iter().filter(|t| t.file == file).collect()
    }

    /// Where `name` is put into the world.
    pub fn inserted<'a>(&'a self, name: &str) -> Vec<&'a InsertSite> {
        self.inserts.iter().filter(|i| i.type_name == name).collect()
    }

    /// The materials declared in one file.
    pub fn materials_in<'a>(&'a self, file: &std::path::Path) -> Vec<&'a MaterialDecl> {
        self.materials.iter().filter(|m| m.file == file).collect()
    }

    /// Every problem this file is the place to report — from either side of the seam, because a
    /// layout mismatch is reported where the layout is written and a missing asset where the
    /// path is.
    pub fn shader_problems_in<'a>(
        &'a self,
        file: &std::path::Path,
    ) -> Vec<&'a crate::shader_link::ShaderProblem> {
        self.shaders.iter().flat_map(|l| l.problems.iter()).filter(|p| p.file == file).collect()
    }

    /// The shader entry for one asset path.
    pub fn shader<'a>(&'a self, asset_path: &str) -> Option<&'a crate::shader_link::ShaderLink> {
        self.shaders.iter().find(|l| l.asset_path == asset_path)
    }
}
