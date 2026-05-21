//! Rust-source schema loader for the RON Studio modal.
//!
//! Given a `.rs` file path and a root type name, walks the entire enclosing
//! crate (discovered by climbing to the first `Cargo.toml`), parses every
//! `.rs` reachable via `mod` declarations, indexes all `struct`/`enum`/`type`
//! items by their canonical path inside the crate, and computes the
//! reachable closure of types from the chosen root.
//!
//! The result is a `Schema` the modal can use to:
//!   · annotate tree rows with accurate type badges (`u16`, not just
//!     "number"; `Server`, not just "map")
//!   · validate that field names + variants actually exist in the schema
//!   · enumerate missing fields ("port: u16 is in the schema but not in
//!     this document")
//!
//! Best-effort: types from other crates are marked `External(path)`; types
//! that don't resolve (macro-generated, behind `#[cfg(...)]` we ignored,
//! unresolvable generics at the root) are marked `Unknown(path)`. Either
//! state is surfaced to the UI so the user knows what is and isn't
//! validated, but never blocks the rest of the schema from working.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};

use serde::Serialize;
use syn::{
    AngleBracketedGenericArguments, GenericArgument, Item, ItemEnum, ItemMod,
    ItemStruct, ItemType, PathArguments, Type, TypePath, TypeTuple,
};

use crate::error::{AppError, Result};

// ── Public types ────────────────────────────────────────────────────────────

/// A resolved type expression. The `ResolvedType` is what the UI uses to
/// understand any RON node it sees. Generics are concretised at every use
/// site, so `Option<Vec<Server>>` becomes a 3-level nested `ResolvedType`,
/// not an unresolved generic.
#[derive(Debug, Clone, Serialize, PartialEq, Eq, Hash)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ResolvedType {
    /// Rust primitive: `u8..u128`, `i8..i128`, `f32`, `f64`, `bool`, `char`,
    /// `String`, `&str`, `()`.
    Primitive { name: String },
    /// `Option<T>`.
    Option { inner: Box<ResolvedType> },
    /// `Vec<T>`, `[T; N]`, `&[T]`, `VecDeque<T>`, etc. — any homogeneous list.
    Vec { inner: Box<ResolvedType> },
    /// `HashMap<K, V>`, `BTreeMap<K, V>`.
    Map { key: Box<ResolvedType>, value: Box<ResolvedType> },
    /// Tuple `(T1, T2, …)`.
    Tuple { items: Vec<ResolvedType> },
    /// A named type defined inside the current crate. The `path` is the
    /// canonical fully-qualified path (`crate::server::Server`); look it up
    /// in `Schema::types` for the definition.
    Named { path: String },
    /// A type from another crate or std (`tokio::net::TcpStream`,
    /// `std::time::Duration`, …) we don't resolve.
    External { path: String },
    /// We tried to resolve and failed. Surfaced to the UI as a yellow badge.
    Unknown { hint: String },
}

/// Definition of a user-defined type from the indexed crate.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TypeDef {
    Struct {
        /// Canonical name (`Server`).
        name: String,
        /// `Foo { a: T, b: U }` → named fields. `Foo(T, U)` → tuple fields
        /// (synthetic names "0", "1", …). `Foo;` → empty.
        fields: Vec<FieldDef>,
        /// True for tuple structs `struct Foo(T, U);` — RON renders these
        /// without field names.
        tuple_like: bool,
    },
    Enum {
        name: String,
        variants: Vec<VariantDef>,
    },
    /// `type Foo = Bar;` — flattened to the aliased type at lookup time.
    Alias {
        name: String,
        target: ResolvedType,
    },
}

#[derive(Debug, Clone, Serialize)]
pub struct FieldDef {
    /// SERIALIZED field name — the string the FE walker matches against
    /// the in-source key. This is the result of applying the field's
    /// `#[serde(rename = "...")]` (highest priority) or the struct
    /// container's `#[serde(rename_all = "...")]` (case-converted from
    /// the Rust identifier) to the original Rust ident. Falls back to
    /// the bare Rust ident when neither is set.
    pub name:     String,
    pub ty:       ResolvedType,
    /// Additional accepted names from `#[serde(alias = "...")]` (may
    /// repeat) **plus** the Rust source identifier when it differs
    /// from `name` (so a doc that hand-typed the Rust name still
    /// resolves correctly).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub aliases:  Vec<String>,
    /// `#[serde(default)]`, `#[serde(default = "...")]` → the field has a
    /// default and is therefore allowed to be absent.
    pub has_default: bool,
    /// `#[serde(skip_serializing_if = "...")]` → may be absent in serialised
    /// output for optional-like values.
    pub skip_if_default: bool,
    /// `#[serde(flatten)]` — fields are inlined into the parent.
    pub flatten: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct VariantDef {
    pub name:       String,
    /// `Foo` → Unit; `Foo(T, U)` → Tuple; `Foo { a: T, b: U }` → Struct.
    pub shape:      VariantShape,
    pub fields:     Vec<FieldDef>,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum VariantShape {
    Unit,
    Tuple,
    Struct,
}

/// The complete schema returned to the frontend after a successful load.
#[derive(Debug, Clone, Serialize)]
pub struct Schema {
    /// Canonical path of the root type the user selected (`crate::Config`).
    pub root_type:    String,
    /// `Config` — last segment of `root_type`.
    pub root_name:    String,
    /// Absolute path to the `Cargo.toml` we discovered.
    pub crate_manifest: String,
    /// Crate name as declared in the manifest (`[package].name`). Used only
    /// for display.
    pub crate_name:   String,
    /// All resolved type definitions reachable from the root, keyed by
    /// canonical path (`crate::server::Server`). Includes the root.
    pub types:        BTreeMap<String, TypeDef>,
    /// Counts surfaced in the schema panel of the modal.
    pub stats:        SchemaStats,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct SchemaStats {
    pub resolved:   usize,
    pub external:   usize,
    pub unknown:    usize,
}

/// Items that can be picked as a "root type". One per public/private
/// struct/enum in the file the user opened. Stable order = source order.
#[derive(Debug, Clone, Serialize)]
pub struct RootCandidate {
    pub name:          String,
    /// Canonical crate-relative path of this type (e.g.
    /// `crate::server::Server`).
    pub canonical_path: String,
    pub kind:          CandidateKind,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CandidateKind {
    Struct,
    Enum,
}

#[derive(Debug, Clone, Serialize)]
pub struct CrateProbe {
    pub crate_manifest:    String,
    pub crate_name:        String,
    /// All struct/enum items defined in the FILE the user picked. Either of
    /// them is a valid root; the dropdown is populated from this list.
    pub root_candidates:   Vec<RootCandidate>,
}

// ── Public entry points ─────────────────────────────────────────────────────

/// Stage 1 — discover the crate around `rs_file_path` and list every
/// struct/enum defined in that file. Cheap: parses just one file plus the
/// crate manifest.
pub fn probe(rs_file_path: &str) -> Result<CrateProbe> {
    let rs_path = PathBuf::from(rs_file_path);
    if !rs_path.is_file() {
        return Err(AppError::Other(format!(
            "Not a file: {rs_file_path}"
        )));
    }
    let manifest = find_cargo_toml(&rs_path).ok_or_else(|| {
        AppError::Other(format!(
            "Could not find Cargo.toml above {rs_file_path}"
        ))
    })?;
    let crate_name = read_crate_name(&manifest).unwrap_or_else(|| "(unknown)".to_string());

    let module_path_of_file = guess_module_path(&manifest, &rs_path);

    let src = std::fs::read_to_string(&rs_path).map_err(|e| {
        AppError::Other(format!("Cannot read {}: {e}", rs_path.display()))
    })?;
    let file = syn::parse_file(&src).map_err(|e| {
        AppError::Other(format!("Cannot parse {}: {e}", rs_path.display()))
    })?;

    let mut candidates = Vec::<RootCandidate>::new();
    collect_file_candidates(&file.items, &module_path_of_file, &mut candidates);

    Ok(CrateProbe {
        crate_manifest:  manifest.to_string_lossy().into_owned(),
        crate_name,
        root_candidates: candidates,
    })
}

/// Re-emit the Rust source for a single named type that the schema has
/// already indexed. Used by RON Studio's "View implementation" action so
/// the user can read a struct/enum definition from inside the modal
/// without leaving for their IDE.
///
/// Resolves re-exports (`crate::prelude::Foo` → `crate::deeper::Foo`)
/// before lookup so any path the UI surfaces works. The output is the
/// `prettyplease`-formatted form of the syn item, not the original text
/// (we don't keep byte spans — see comment on `RawTypeDef`). Comments
/// and rustfmt-specific whitespace are dropped; the user gets a clean,
/// canonical view of *what the type is*, which is what they were
/// asking for.
pub fn get_type_source(rs_file_path: &str, canonical_path: &str) -> Result<TypeSource> {
    use quote::ToTokens;

    let rs_path = PathBuf::from(rs_file_path);
    let manifest = find_cargo_toml(&rs_path).ok_or_else(|| {
        AppError::Other(format!("Could not find Cargo.toml above {rs_file_path}"))
    })?;
    let crate_src_root = manifest.parent().unwrap().join("src");
    let entry = find_crate_entry(&crate_src_root)?;

    let mut index = TypeIndex::default();
    walk_module_file(&entry, "crate".to_string(), &mut index, &mut BTreeSet::new())?;
    expand_glob_reexports(&mut index);

    let resolved = index.canonicalize(canonical_path)
        .unwrap_or_else(|| canonical_path.to_string());
    let (raw, _module) = index.types.get(&resolved).ok_or_else(|| {
        AppError::Other(format!("Type not found in indexed crate: {canonical_path}"))
    })?;

    // Wrap the single item in a syn::File so prettyplease can format it.
    let item: Item = match &raw.body {
        RawTypeBody::Struct(s) => Item::Struct(s.clone()),
        RawTypeBody::Enum(e)   => Item::Enum(e.clone()),
        RawTypeBody::Alias(t)  => Item::Type(t.clone()),
    };
    let file = syn::File {
        shebang: None,
        attrs:   Vec::new(),
        items:   vec![item.clone()],
    };
    let text = prettyplease::unparse(&file);

    let kind = match &raw.body {
        RawTypeBody::Struct(_) => CandidateKind::Struct,
        RawTypeBody::Enum(_)   => CandidateKind::Enum,
        // Alias is reported as Struct for the UI's purposes — the modal
        // doesn't distinguish further.
        RawTypeBody::Alias(_)  => CandidateKind::Struct,
    };

    // We discarded ToTokens once already; keep the type unused-warning quiet.
    let _ = item.to_token_stream();

    Ok(TypeSource {
        canonical_path: resolved,
        name:           raw.name.clone(),
        kind,
        source:         text,
    })
}

#[derive(Debug, Clone, Serialize)]
pub struct TypeSource {
    pub canonical_path: String,
    pub name:           String,
    pub kind:           CandidateKind,
    pub source:         String,
}

/// Stage 2 — load the full schema rooted at `root_canonical_path` (one of
/// the canonical paths returned by `probe`). Parses every `.rs` reachable
/// via `mod` declarations from the crate root, indexes all types, and walks
/// the reachability closure from the root.
pub fn load(rs_file_path: &str, root_canonical_path: &str) -> Result<Schema> {
    let rs_path = PathBuf::from(rs_file_path);
    let manifest = find_cargo_toml(&rs_path).ok_or_else(|| {
        AppError::Other(format!(
            "Could not find Cargo.toml above {rs_file_path}"
        ))
    })?;
    let crate_name = read_crate_name(&manifest).unwrap_or_else(|| "(unknown)".to_string());

    let crate_src_root = manifest.parent().unwrap().join("src");
    let entry = find_crate_entry(&crate_src_root)?;

    // Build the module map by walking `mod foo;` declarations starting from
    // the entry file. `mod foo { ... }` inline blocks are processed in the
    // same pass.
    let mut index = TypeIndex::default();
    walk_module_file(
        &entry,
        "crate".to_string(),
        &mut index,
        &mut BTreeSet::new(),
    )?;
    // Now that every type + every named `pub use` has been collected,
    // expand the queued glob re-exports against the populated index.
    // This lets `pub use crate::elements::*;` in a prelude module become
    // transparent — every type defined under `crate::elements::*` becomes
    // reachable via `crate::prelude::*` aliases.
    expand_glob_reexports(&mut index);

    // Reachability BFS from the root, allowing the chosen root path to
    // be specified as a re-export alias too (e.g. `crate::prelude::Foo`).
    let canonical_root = index.canonicalize(root_canonical_path)
        .unwrap_or_else(|| root_canonical_path.to_string());
    let root = index.types.get(&canonical_root).ok_or_else(|| {
        AppError::Other(format!(
            "Root type not found in indexed crate: {root_canonical_path}"
        ))
    })?;
    let root_name = root.0.short_name();

    let mut reachable = BTreeMap::<String, TypeDef>::new();
    let mut stats = SchemaStats::default();
    let mut queue: VecDeque<String> = VecDeque::new();
    queue.push_back(canonical_root.clone());

    while let Some(path) = queue.pop_front() {
        if reachable.contains_key(&path) { continue; }
        let Some((raw, module_path)) = index.types.get(&path) else { continue; };
        let resolved = resolve_typedef(raw, module_path, &index);
        // Schedule referenced types — the referenced path may itself be a
        // re-export alias that hasn't been promoted to a canonical entry
        // yet, so we canonicalise here too.
        for referenced in collect_referenced_paths(&resolved) {
            let canon = index.canonicalize(&referenced).unwrap_or(referenced);
            if !reachable.contains_key(&canon) && !queue.contains(&canon) {
                if index.types.contains_key(&canon) {
                    queue.push_back(canon);
                }
            }
        }
        stats.resolved += 1;
        for rt in walk_resolved(&resolved) {
            match rt {
                ResolvedType::External { .. } => stats.external += 1,
                ResolvedType::Unknown { .. }  => stats.unknown  += 1,
                _ => {}
            }
        }
        reachable.insert(path, resolved);
    }

    Ok(Schema {
        root_type:      canonical_root,
        root_name,
        crate_manifest: manifest.to_string_lossy().into_owned(),
        crate_name,
        types:          reachable,
        stats,
    })
}

// ── Internals ───────────────────────────────────────────────────────────────

/// Raw (unresolved) type definition straight out of `syn`. `module_path` is
/// the canonical module path the definition lives in (e.g. `crate::server`),
/// used later to resolve `Bar` referenced inside the same module.
#[derive(Debug, Clone)]
struct RawTypeDef {
    name:  String,
    body:  RawTypeBody,
    /// File-level `use` map for resolving simple paths used inside this type.
    file_uses: HashMap<String, String>,
}

impl RawTypeDef {
    fn short_name(&self) -> String { self.name.clone() }
}

#[derive(Debug, Clone)]
enum RawTypeBody {
    Struct(ItemStruct),
    Enum(ItemEnum),
    Alias(ItemType),
}

#[derive(Default)]
struct TypeIndex {
    /// Canonical path (`crate::server::Server`) → (raw, module of the def).
    types: BTreeMap<String, (RawTypeDef, String)>,
    /// Named re-exports: `crate::prelude::Element` → `crate::elements::Element`.
    /// Populated during the initial walk and then resolved transitively in
    /// `expand_reexports` so that `prelude → prelude2 → real` chains work.
    /// Glob re-exports (`pub use foo::*;`) are queued in `pending_globs` and
    /// expanded into this map after every type is indexed.
    reexports: BTreeMap<String, String>,
    /// `(target_module, source_module_path)` pairs from `pub use mod::*`
    /// statements. Processed in a second pass once all types are known.
    pending_globs: Vec<(String, String)>,
}

impl TypeIndex {
    /// Follow re-export chains to a path that actually appears in
    /// `self.types`. Returns the original input if it's already canonical
    /// or no re-export covers it; returns `None` only if a chain dead-ends
    /// before reaching a real definition.
    fn canonicalize(&self, path: &str) -> Option<String> {
        let mut cur = path.to_string();
        // Bounded by the size of the re-export map — chains can't be longer
        // than the number of entries without a cycle, which we break on.
        let max_hops = self.reexports.len() + 1;
        let mut seen = BTreeSet::<String>::new();
        for _ in 0..max_hops {
            if self.types.contains_key(&cur) { return Some(cur); }
            if !seen.insert(cur.clone()) { return None; }
            match self.reexports.get(&cur) {
                Some(next) => cur = next.clone(),
                None       => return None,
            }
        }
        None
    }
}

fn collect_file_candidates(
    items:        &[Item],
    module_path:  &str,
    out:          &mut Vec<RootCandidate>,
) {
    for item in items {
        match item {
            Item::Struct(s) => {
                let name = s.ident.to_string();
                out.push(RootCandidate {
                    name: name.clone(),
                    canonical_path: format!("{module_path}::{name}"),
                    kind: CandidateKind::Struct,
                });
            }
            Item::Enum(e) => {
                let name = e.ident.to_string();
                out.push(RootCandidate {
                    name: name.clone(),
                    canonical_path: format!("{module_path}::{name}"),
                    kind: CandidateKind::Enum,
                });
            }
            // Inline `mod foo { struct Bar {} }` — recurse so users who
            // organise types under a private module aren't surprised.
            Item::Mod(ItemMod { ident, content: Some((_, sub_items)), .. }) => {
                let sub_path = format!("{module_path}::{}", ident);
                collect_file_candidates(sub_items, &sub_path, out);
            }
            _ => {}
        }
    }
}

fn walk_module_file(
    file_path:       &Path,
    module_path:     String,
    index:           &mut TypeIndex,
    visited_files:   &mut BTreeSet<PathBuf>,
) -> Result<()> {
    let canon = file_path.canonicalize().unwrap_or_else(|_| file_path.to_path_buf());
    if !visited_files.insert(canon.clone()) {
        return Ok(()); // already parsed via a different path
    }
    let src = match std::fs::read_to_string(file_path) {
        Ok(s) => s,
        Err(_) => return Ok(()), // missing file = silent skip
    };
    let parsed = match syn::parse_file(&src) {
        Ok(p) => p,
        Err(_) => return Ok(()), // unparseable = silent skip
    };

    // Submodule names at this file's top level — drives the relative-vs-
    // external use disambiguation when building the file's use-map.
    let submodules: HashSet<String> = parsed.items.iter().filter_map(|item| {
        if let Item::Mod(m) = item { Some(m.ident.to_string()) } else { None }
    }).collect();
    let file_uses = collect_use_map(&parsed.items, &module_path, &submodules);

    walk_items_in_file(
        &parsed.items,
        &module_path,
        file_path,
        &file_uses,
        index,
        visited_files,
    )?;
    Ok(())
}

fn walk_items_in_file(
    items:           &[Item],
    module_path:     &str,
    file_path:       &Path,
    file_uses:       &HashMap<String, String>,
    index:           &mut TypeIndex,
    visited_files:   &mut BTreeSet<PathBuf>,
) -> Result<()> {
    // Step 1 — collect the names of submodules declared at this level
    // (`mod foo;` / `pub mod foo;` / `mod foo { ... }`). They're needed by
    // the use-resolver to distinguish a relative path (`pub use foo::*;`
    // referring to a sibling submodule) from an extern-crate path (`pub
    // use serde::Serialize;`), which canonicalise into very different
    // crate-relative names.
    let submodules: HashSet<String> = items.iter().filter_map(|item| {
        if let Item::Mod(m) = item { Some(m.ident.to_string()) } else { None }
    }).collect();
    // Step 2 — register `pub use` re-exports defined in this module so
    // patterns like `crate::prelude::*` become transparent.
    collect_pub_use_reexports(items, module_path, &submodules, &mut index.reexports, &mut index.pending_globs);
    for item in items {
        match item {
            Item::Struct(s) => {
                let canonical = format!("{module_path}::{}", s.ident);
                index.types.insert(
                    canonical,
                    (
                        RawTypeDef {
                            name:      s.ident.to_string(),
                            body:      RawTypeBody::Struct(s.clone()),
                            file_uses: file_uses.clone(),
                        },
                        module_path.to_string(),
                    ),
                );
            }
            Item::Enum(e) => {
                let canonical = format!("{module_path}::{}", e.ident);
                index.types.insert(
                    canonical,
                    (
                        RawTypeDef {
                            name:      e.ident.to_string(),
                            body:      RawTypeBody::Enum(e.clone()),
                            file_uses: file_uses.clone(),
                        },
                        module_path.to_string(),
                    ),
                );
            }
            Item::Type(t) => {
                let canonical = format!("{module_path}::{}", t.ident);
                index.types.insert(
                    canonical,
                    (
                        RawTypeDef {
                            name:      t.ident.to_string(),
                            body:      RawTypeBody::Alias(t.clone()),
                            file_uses: file_uses.clone(),
                        },
                        module_path.to_string(),
                    ),
                );
            }
            Item::Mod(m) => {
                let sub_path = format!("{module_path}::{}", m.ident);
                if let Some((_, sub_items)) = &m.content {
                    // Inline mod — re-collect uses inside it because nested
                    // modules can have their own `use` items, then recurse.
                    // Submodule list for the inline mod is built from ITS items.
                    let sub_submodules: HashSet<String> = sub_items.iter().filter_map(|it| {
                        if let Item::Mod(mm) = it { Some(mm.ident.to_string()) } else { None }
                    }).collect();
                    let mut sub_uses = file_uses.clone();
                    sub_uses.extend(collect_use_map(sub_items, &sub_path, &sub_submodules));
                    walk_items_in_file(
                        sub_items,
                        &sub_path,
                        file_path,
                        &sub_uses,
                        index,
                        visited_files,
                    )?;
                } else if let Some(child_file) = resolve_mod_file(file_path, &m.ident.to_string(), m) {
                    walk_module_file(
                        &child_file,
                        sub_path,
                        index,
                        visited_files,
                    )?;
                }
            }
            _ => {}
        }
    }
    Ok(())
}

/// Collects `pub use …` re-exports defined in `items` (one module's items)
/// and registers them in the crate-wide re-export map. Named re-exports
/// (`pub use foo::Bar`, `pub use foo::Bar as Baz`, groups) land directly
/// in `reexports`. Glob re-exports (`pub use foo::*`) are queued in
/// `pending_globs` so they can be expanded once every type has been
/// indexed (only after that does it make sense to enumerate "everything
/// public under foo").
fn collect_pub_use_reexports(
    items:           &[Item],
    current_module:  &str,
    submodules:      &HashSet<String>,
    reexports:       &mut BTreeMap<String, String>,
    pending_globs:   &mut Vec<(String, String)>,
) {
    for item in items {
        if let Item::Use(u) = item {
            if !matches!(u.vis, syn::Visibility::Public(_)) { continue; }
            collect_pub_use_tree(&u.tree, current_module, submodules, &[], reexports, pending_globs);
        }
    }
}

fn collect_pub_use_tree(
    tree:           &syn::UseTree,
    current_module: &str,
    submodules:     &HashSet<String>,
    prefix:         &[String],
    reexports:      &mut BTreeMap<String, String>,
    pending_globs:  &mut Vec<(String, String)>,
) {
    use syn::UseTree;
    match tree {
        UseTree::Path(p) => {
            let mut next = prefix.to_vec();
            next.push(p.ident.to_string());
            collect_pub_use_tree(&p.tree, current_module, submodules, &next, reexports, pending_globs);
        }
        UseTree::Name(n) => {
            let mut full = prefix.to_vec();
            full.push(n.ident.to_string());
            let target = canonicalise_use_path(&full, current_module, submodules);
            let alias_path = format!("{current_module}::{}", n.ident);
            if alias_path != target {
                reexports.insert(alias_path, target);
            }
        }
        UseTree::Rename(r) => {
            let mut full = prefix.to_vec();
            full.push(r.ident.to_string());
            let target = canonicalise_use_path(&full, current_module, submodules);
            let alias_path = format!("{current_module}::{}", r.rename);
            if alias_path != target {
                reexports.insert(alias_path, target);
            }
        }
        UseTree::Group(g) => {
            for item in &g.items {
                collect_pub_use_tree(item, current_module, submodules, prefix, reexports, pending_globs);
            }
        }
        UseTree::Glob(_) => {
            if prefix.is_empty() { return; }
            let source = canonicalise_use_path(prefix, current_module, submodules);
            pending_globs.push((current_module.to_string(), source));
        }
    }
}

/// Resolve queued `pub use foo::*;` statements. For each pending glob,
/// look at every entry in `index.types` (and the already-resolved
/// re-exports) whose path starts with `source::`. Register a re-export
/// `target_module::short_name → real_path` for each. Repeat the pass
/// until a fixed point — chained globs like `pub use submod::*;` inside
/// the source module only become visible after the first pass settles.
fn expand_glob_reexports(index: &mut TypeIndex) {
    if index.pending_globs.is_empty() { return; }
    let globs = std::mem::take(&mut index.pending_globs);
    let max_passes = (globs.len() + index.types.len()).max(8);
    for _ in 0..max_passes {
        let mut added = false;
        // Snapshot candidate sources for THIS pass — the union of real
        // types and re-export keys, so we can chain globs through preludes.
        let candidates: Vec<(String, String)> = index.types.keys()
            .cloned()
            .map(|k| (k.clone(), k))
            .chain(index.reexports.iter().map(|(k, v)| (k.clone(), v.clone())))
            .collect();
        for (target_module, source) in &globs {
            let prefix_with_sep = format!("{source}::");
            for (candidate_path, real_path) in &candidates {
                if !candidate_path.starts_with(&prefix_with_sep) { continue; }
                // The "short name" the glob brings into scope is the very
                // next segment after `source::` (we don't want to flatten
                // nested submodules — that would mis-register their inner
                // items as direct members of target_module).
                let rest = &candidate_path[prefix_with_sep.len()..];
                let short = match rest.find("::") {
                    Some(_) => continue, // skip transitive sub-paths; the user would `pub use foo::sub::*` separately if wanted
                    None    => rest,
                };
                let alias = format!("{target_module}::{short}");
                if &alias == real_path { continue; }
                if index.types.contains_key(&alias) { continue; }
                // Skip when the same alias→real mapping already exists.
                // Clone the existing pointer first so the immutable borrow
                // on `reexports` is released before we may `insert` below.
                let existing = index.reexports.get(&alias).cloned();
                if existing.as_deref() == Some(real_path.as_str()) { continue; }
                index.reexports.insert(alias, real_path.clone());
                added = true;
            }
        }
        if !added { break; }
    }
}

/// Builds a per-file `use` map: short name → fully qualified canonical path.
/// Resolves `crate::`, `super::`, `self::`, and external (`tokio::…`).
fn collect_use_map(items: &[Item], current_module: &str, submodules: &HashSet<String>) -> HashMap<String, String> {
    let mut map = HashMap::new();
    for item in items {
        if let Item::Use(u) = item {
            collect_use_tree(&u.tree, current_module, submodules, &[], &mut map);
        }
    }
    map
}

fn collect_use_tree(
    tree:           &syn::UseTree,
    current_module: &str,
    submodules:     &HashSet<String>,
    prefix:         &[String],
    out:            &mut HashMap<String, String>,
) {
    use syn::UseTree;
    match tree {
        UseTree::Path(p) => {
            let mut next = prefix.to_vec();
            next.push(p.ident.to_string());
            collect_use_tree(&p.tree, current_module, submodules, &next, out);
        }
        UseTree::Name(n) => {
            let mut full = prefix.to_vec();
            full.push(n.ident.to_string());
            let resolved = canonicalise_use_path(&full, current_module, submodules);
            out.insert(n.ident.to_string(), resolved);
        }
        UseTree::Rename(r) => {
            let mut full = prefix.to_vec();
            full.push(r.ident.to_string());
            let resolved = canonicalise_use_path(&full, current_module, submodules);
            out.insert(r.rename.to_string(), resolved);
        }
        UseTree::Group(g) => {
            for item in &g.items {
                collect_use_tree(item, current_module, submodules, prefix, out);
            }
        }
        UseTree::Glob(_) => {
            // `use foo::*;` — we can't track each glob-imported name without
            // also parsing `foo`. Skipped; the resolver falls back to the
            // crate-wide search as a last resort.
        }
    }
}

/// Resolve a `use`-style path (`crate::foo::Bar`, `self::Bar`, `super::Baz`,
/// `sibling_mod::Bar`, or `extern_crate::Bar`) to a canonical form.
///
///   · `crate::…`  → absolute crate-relative path, used verbatim
///   · `self::…`   → rebased onto `current_module`
///   · `super::…`  → rebased onto the parent of `current_module`
///   · `<sub>::…`  → if `sub` is a submodule of `current_module`, treated
///                   as `self::sub::…`; this is what makes Rust 2018's
///                   `pub use sibling::*;` resolve to the right place
///   · anything else → returned verbatim, e.g. `serde::Serialize` stays
///                   as `serde::Serialize` and ends up classified as
///                   External by the resolver.
///
/// Without the submodule-aware branch, every `pub use ability::*;` style
/// re-export inside a `mod.rs` is incorrectly treated as referring to an
/// external crate named `ability`, and the entire re-export chain for
/// `crate::prelude::*` collapses.
fn canonicalise_use_path(segments: &[String], current_module: &str, submodules: &HashSet<String>) -> String {
    if segments.is_empty() { return String::new(); }
    let head = segments[0].as_str();
    match head {
        "crate" => segments.join("::"),
        "self"  => {
            let tail: Vec<&str> = segments[1..].iter().map(|s| s.as_str()).collect();
            if tail.is_empty() { current_module.to_string() }
            else { format!("{current_module}::{}", tail.join("::")) }
        }
        "super" => {
            let mut parts: Vec<&str> = current_module.split("::").collect();
            for seg in segments.iter() {
                if seg == "super" {
                    if parts.len() > 1 { parts.pop(); }
                } else {
                    parts.push(seg);
                }
            }
            parts.join("::")
        }
        _ if submodules.contains(head) => {
            // Relative submodule reference — equivalent to `self::head::…`.
            format!("{current_module}::{}", segments.join("::"))
        }
        // External crates (tokio, std, serde, …) keep their absolute form.
        _ => segments.join("::"),
    }
}

fn resolve_mod_file(parent_file: &Path, mod_name: &str, m: &ItemMod) -> Option<PathBuf> {
    // Honour `#[path = "..."]` first.
    for attr in &m.attrs {
        if attr.path().is_ident("path") {
            if let Ok(lit) = attr.parse_args::<syn::LitStr>() {
                let candidate = parent_file.parent()?.join(lit.value());
                if candidate.is_file() { return Some(candidate); }
            }
        }
    }
    let parent_dir = parent_file.parent()?;
    let stem = parent_file.file_stem()?.to_string_lossy();
    // `lib.rs` / `main.rs` / `mod.rs` look up siblings in the same dir;
    // `foo.rs` looks under a `foo/` directory.
    let search_root = if stem == "lib" || stem == "main" || stem == "mod" {
        parent_dir.to_path_buf()
    } else {
        parent_dir.join(stem.as_ref())
    };
    let a = search_root.join(format!("{mod_name}.rs"));
    if a.is_file() { return Some(a); }
    let b = search_root.join(mod_name).join("mod.rs");
    if b.is_file() { return Some(b); }
    None
}

fn resolve_typedef(
    def:         &RawTypeDef,
    module_path: &str,
    index:       &TypeIndex,
) -> TypeDef {
    match &def.body {
        RawTypeBody::Struct(s) => {
            let container_rename = serde_container_rename_all(&s.attrs);
            let (fields, tuple_like) = match &s.fields {
                syn::Fields::Named(n) => {
                    let fs: Vec<FieldDef> = n.named.iter()
                        .map(|f| build_named_field(f, module_path, &def.file_uses, index, container_rename))
                        .collect();
                    (fs, false)
                }
                syn::Fields::Unnamed(u) => {
                    let fs: Vec<FieldDef> = u.unnamed.iter().enumerate().map(|(i, f)| FieldDef {
                        name: i.to_string(),
                        ty:   resolve_type_expr(&f.ty, module_path, &def.file_uses, index),
                        aliases:          Vec::new(),
                        has_default:      serde_field_default(&f.attrs),
                        skip_if_default:  serde_field_skip(&f.attrs),
                        flatten:          serde_field_flatten(&f.attrs),
                    }).collect();
                    (fs, true)
                }
                syn::Fields::Unit => (Vec::new(), false),
            };
            TypeDef::Struct {
                name: def.name.clone(),
                fields,
                tuple_like,
            }
        }
        RawTypeBody::Enum(e) => {
            // Enum container `rename_all` applies to FIELD names of
            // struct-shaped variants (not to variant names — those are
            // governed by `rename_all_fields` which serde gates behind
            // a separate attribute, not in scope here).
            let container_rename = serde_container_rename_all(&e.attrs);
            let variants = e.variants.iter().map(|v| {
                let (fields, shape) = match &v.fields {
                    syn::Fields::Named(n) => {
                        let fs: Vec<FieldDef> = n.named.iter()
                            .map(|f| build_named_field(f, module_path, &def.file_uses, index, container_rename))
                            .collect();
                        (fs, VariantShape::Struct)
                    }
                    syn::Fields::Unnamed(u) => {
                        let fs: Vec<FieldDef> = u.unnamed.iter().enumerate().map(|(i, f)| FieldDef {
                            name: i.to_string(),
                            ty:   resolve_type_expr(&f.ty, module_path, &def.file_uses, index),
                            aliases:         Vec::new(),
                            has_default:     serde_field_default(&f.attrs),
                            skip_if_default: serde_field_skip(&f.attrs),
                            flatten:         serde_field_flatten(&f.attrs),
                        }).collect();
                        (fs, VariantShape::Tuple)
                    }
                    syn::Fields::Unit => (Vec::new(), VariantShape::Unit),
                };
                VariantDef { name: v.ident.to_string(), shape, fields }
            }).collect();
            TypeDef::Enum { name: def.name.clone(), variants }
        }
        RawTypeBody::Alias(t) => TypeDef::Alias {
            name:   def.name.clone(),
            target: resolve_type_expr(&t.ty, module_path, &def.file_uses, index),
        },
    }
}

/// Construct a `FieldDef` from a syn-parsed named field, applying the
/// serde rename/alias/rename_all rules:
///   1. `name`     = `#[serde(rename = "x")]` if present, else
///                   container `rename_all` applied to the Rust ident,
///                   else the bare Rust ident.
///   2. `aliases`  = `#[serde(alias = "...")]` entries (may repeat),
///                   PLUS the Rust ident itself when the resolved
///                   `name` differs from it (so a doc that hand-typed
///                   the Rust name still resolves).
fn build_named_field(
    f:                &syn::Field,
    module_path:      &str,
    file_uses:        &HashMap<String, String>,
    index:            &TypeIndex,
    container_rename: Option<RenameAll>,
) -> FieldDef {
    let source_ident = f.ident.as_ref().map(|i| i.to_string()).unwrap_or_default();
    let explicit     = serde_field_rename(&f.attrs);
    let name = if let Some(r) = explicit {
        r
    } else if let Some(policy) = container_rename {
        policy.apply(&source_ident)
    } else {
        source_ident.clone()
    };
    let mut aliases = serde_field_aliases(&f.attrs);
    if !source_ident.is_empty() && source_ident != name && !aliases.iter().any(|a| a == &source_ident) {
        aliases.push(source_ident);
    }
    FieldDef {
        name,
        ty:              resolve_type_expr(&f.ty, module_path, file_uses, index),
        aliases,
        has_default:     serde_field_default(&f.attrs),
        skip_if_default: serde_field_skip(&f.attrs),
        flatten:         serde_field_flatten(&f.attrs),
    }
}

fn resolve_type_expr(
    ty:          &Type,
    module_path: &str,
    file_uses:   &HashMap<String, String>,
    index:       &TypeIndex,
) -> ResolvedType {
    match ty {
        Type::Path(TypePath { qself: None, path }) => resolve_path_type(path, module_path, file_uses, index),
        Type::Tuple(TypeTuple { elems, .. }) => {
            if elems.is_empty() {
                return ResolvedType::Primitive { name: "()".into() };
            }
            ResolvedType::Tuple {
                items: elems.iter().map(|e| resolve_type_expr(e, module_path, file_uses, index)).collect(),
            }
        }
        Type::Array(arr) => ResolvedType::Vec {
            inner: Box::new(resolve_type_expr(&arr.elem, module_path, file_uses, index)),
        },
        Type::Slice(sl) => ResolvedType::Vec {
            inner: Box::new(resolve_type_expr(&sl.elem, module_path, file_uses, index)),
        },
        Type::Reference(r) => resolve_type_expr(&r.elem, module_path, file_uses, index),
        Type::Group(g) => resolve_type_expr(&g.elem, module_path, file_uses, index),
        Type::Paren(p) => resolve_type_expr(&p.elem, module_path, file_uses, index),
        _ => ResolvedType::Unknown {
            hint: "complex type expression".into(),
        },
    }
}

fn resolve_path_type(
    path:        &syn::Path,
    module_path: &str,
    file_uses:   &HashMap<String, String>,
    index:       &TypeIndex,
) -> ResolvedType {
    let segments: Vec<&syn::PathSegment> = path.segments.iter().collect();
    if segments.is_empty() {
        return ResolvedType::Unknown { hint: "empty path".into() };
    }
    let last = segments.last().unwrap();
    let name = last.ident.to_string();

    // Generic container shortcuts.
    if let PathArguments::AngleBracketed(args) = &last.arguments {
        match name.as_str() {
            "Option" => {
                if let Some(t) = first_type_arg(args) {
                    return ResolvedType::Option {
                        inner: Box::new(resolve_type_expr(t, module_path, file_uses, index)),
                    };
                }
            }
            "Vec" | "VecDeque" | "LinkedList" | "HashSet" | "BTreeSet" | "IndexSet" => {
                if let Some(t) = first_type_arg(args) {
                    return ResolvedType::Vec {
                        inner: Box::new(resolve_type_expr(t, module_path, file_uses, index)),
                    };
                }
            }
            "HashMap" | "BTreeMap" | "IndexMap" => {
                let mut types = args.args.iter().filter_map(|a| if let GenericArgument::Type(t) = a { Some(t) } else { None });
                let k = types.next();
                let v = types.next();
                if let (Some(k), Some(v)) = (k, v) {
                    return ResolvedType::Map {
                        key:   Box::new(resolve_type_expr(k, module_path, file_uses, index)),
                        value: Box::new(resolve_type_expr(v, module_path, file_uses, index)),
                    };
                }
            }
            "Box" | "Rc" | "Arc" | "Cell" | "RefCell" | "Mutex" | "RwLock" => {
                // Transparent wrappers — unwrap to the inner type.
                if let Some(t) = first_type_arg(args) {
                    return resolve_type_expr(t, module_path, file_uses, index);
                }
            }
            _ => {}
        }
    }

    // Primitives.
    if is_primitive(&name) && path.segments.len() == 1 {
        return ResolvedType::Primitive { name };
    }

    // Build the canonical path:
    //   `crate::foo::Bar` if path starts with `crate`
    //   `module_path::Bar`  if single ident and we find it in the index
    //   `external::path::Bar` otherwise
    let raw_str: String = segments
        .iter()
        .map(|s| s.ident.to_string())
        .collect::<Vec<_>>()
        .join("::");

    if raw_str.starts_with("crate::") {
        return classify_named(&raw_str, index);
    }
    if segments.len() == 1 {
        // Use the file-level use map first.
        if let Some(full) = file_uses.get(&name) {
            return classify_named(full, index);
        }
        // Try the current module.
        let local = format!("{module_path}::{name}");
        if let Some(canon) = index.canonicalize(&local) {
            return ResolvedType::Named { path: canon };
        }
        // Try crate root.
        let root = format!("crate::{name}");
        if let Some(canon) = index.canonicalize(&root) {
            return ResolvedType::Named { path: canon };
        }
        // Last resort: search every module for a type with this short name.
        // Useful when `pub use foo::Bar;` re-exported it without an explicit
        // `use` in this file. Both real types and re-export aliases are
        // candidates — re-exports get canonicalised on hit.
        let mut found: Option<String> = None;
        for k in index.types.keys().chain(index.reexports.keys()) {
            if k.rsplit("::").next() == Some(name.as_str()) {
                let canon = index.canonicalize(k).unwrap_or_else(|| k.clone());
                match &found {
                    Some(prev) if prev == &canon => { /* same dest, keep */ }
                    Some(_) => { found = None; break; } // ambiguous → bail
                    None => found = Some(canon),
                }
            }
        }
        if let Some(f) = found {
            return ResolvedType::Named { path: f };
        }
        return ResolvedType::External { path: name };
    }
    // Multi-segment external path (`std::time::Duration`, `tokio::net::Foo`).
    classify_named(&raw_str, index)
}

fn classify_named(path: &str, index: &TypeIndex) -> ResolvedType {
    if let Some(canon) = index.canonicalize(path) {
        return ResolvedType::Named { path: canon };
    }
    if path.starts_with("crate::") {
        ResolvedType::Unknown { hint: format!("not found in indexed crate: {path}") }
    } else {
        ResolvedType::External { path: path.to_string() }
    }
}

fn first_type_arg(args: &AngleBracketedGenericArguments) -> Option<&Type> {
    for a in &args.args {
        if let GenericArgument::Type(t) = a { return Some(t); }
    }
    None
}

fn is_primitive(name: &str) -> bool {
    matches!(name,
        "u8" | "u16" | "u32" | "u64" | "u128" | "usize" |
        "i8" | "i16" | "i32" | "i64" | "i128" | "isize" |
        "f32" | "f64" |
        "bool" | "char" | "String" | "str"
    )
}

fn collect_referenced_paths(td: &TypeDef) -> Vec<String> {
    let mut out = Vec::new();
    match td {
        TypeDef::Struct { fields, .. } => for f in fields { walk_collect(&f.ty, &mut out); },
        TypeDef::Enum { variants, .. } => for v in variants { for f in &v.fields { walk_collect(&f.ty, &mut out); } },
        TypeDef::Alias { target, .. } => walk_collect(target, &mut out),
    }
    out
}

fn walk_collect(rt: &ResolvedType, out: &mut Vec<String>) {
    match rt {
        ResolvedType::Named { path } => out.push(path.clone()),
        ResolvedType::Option { inner } => walk_collect(inner, out),
        ResolvedType::Vec { inner }    => walk_collect(inner, out),
        ResolvedType::Map { key, value } => { walk_collect(key, out); walk_collect(value, out); }
        ResolvedType::Tuple { items } => for it in items { walk_collect(it, out); },
        _ => {}
    }
}

fn walk_resolved(td: &TypeDef) -> Vec<&ResolvedType> {
    let mut out = Vec::new();
    fn visit<'a>(rt: &'a ResolvedType, out: &mut Vec<&'a ResolvedType>) {
        out.push(rt);
        match rt {
            ResolvedType::Option { inner } | ResolvedType::Vec { inner } => visit(inner, out),
            ResolvedType::Map { key, value } => { visit(key, out); visit(value, out); }
            ResolvedType::Tuple { items } => for it in items { visit(it, out); },
            _ => {}
        }
    }
    match td {
        TypeDef::Struct { fields, .. } => for f in fields { visit(&f.ty, &mut out); },
        TypeDef::Enum { variants, .. } => for v in variants { for f in &v.fields { visit(&f.ty, &mut out); } },
        TypeDef::Alias { target, .. } => visit(target, &mut out),
    }
    out
}

// ── #[serde(...)] attribute parsing (minimal) ───────────────────────────────

fn serde_field_default(attrs: &[syn::Attribute]) -> bool {
    serde_flag_present(attrs, "default")
}
fn serde_field_skip(attrs: &[syn::Attribute]) -> bool {
    serde_flag_present(attrs, "skip_serializing_if")
        || serde_flag_present(attrs, "skip_serializing")
}
fn serde_field_flatten(attrs: &[syn::Attribute]) -> bool {
    serde_flag_present(attrs, "flatten")
}

/// Field-level `#[serde(rename = "x")]` — the serialised name override.
/// Returns `None` when no rename is present.
fn serde_field_rename(attrs: &[syn::Attribute]) -> Option<String> {
    serde_string_value(attrs, "rename")
}

/// Field-level `#[serde(alias = "x")]` — additional accepted names
/// (may repeat across multiple attribute calls).
fn serde_field_aliases(attrs: &[syn::Attribute]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for a in attrs {
        if !a.path().is_ident("serde") { continue; }
        let _ = a.parse_nested_meta(|m| {
            if m.path.is_ident("alias") {
                if m.input.peek(syn::Token![=]) {
                    let _: syn::Token![=] = m.input.parse()?;
                    if let Ok(lit) = m.input.parse::<syn::LitStr>() {
                        out.push(lit.value());
                        return Ok(());
                    }
                    // Fallthrough — consume the value either way.
                    let _: syn::Expr = m.input.parse()?;
                }
            } else if m.input.peek(syn::Token![=]) {
                let _: syn::Token![=] = m.input.parse()?;
                let _: syn::Expr = m.input.parse()?;
            }
            Ok(())
        });
    }
    out
}

/// Container-level `#[serde(rename_all = "snake_case")]` — drives the
/// default case conversion applied to every field that doesn't carry
/// an explicit `#[serde(rename = "...")]`.
fn serde_container_rename_all(attrs: &[syn::Attribute]) -> Option<RenameAll> {
    serde_string_value(attrs, "rename_all").and_then(|s| RenameAll::parse(&s))
}

/// Generic helper — first `#[serde(key = "...")]` value found, or `None`.
fn serde_string_value(attrs: &[syn::Attribute], key: &str) -> Option<String> {
    let mut found: Option<String> = None;
    for a in attrs {
        if !a.path().is_ident("serde") { continue; }
        let _ = a.parse_nested_meta(|m| {
            if m.path.is_ident(key) && m.input.peek(syn::Token![=]) {
                let _: syn::Token![=] = m.input.parse()?;
                if let Ok(lit) = m.input.parse::<syn::LitStr>() {
                    if found.is_none() { found = Some(lit.value()); }
                    return Ok(());
                }
                let _: syn::Expr = m.input.parse()?;
            } else if m.input.peek(syn::Token![=]) {
                let _: syn::Token![=] = m.input.parse()?;
                let _: syn::Expr = m.input.parse()?;
            }
            Ok(())
        });
        if found.is_some() { break; }
    }
    found
}

fn serde_flag_present(attrs: &[syn::Attribute], flag: &str) -> bool {
    for a in attrs {
        if !a.path().is_ident("serde") { continue; }
        let mut found = false;
        let _ = a.parse_nested_meta(|m| {
            if m.path.is_ident(flag) { found = true; }
            // Eat the optional `= "..."` value without complaining.
            if m.input.peek(syn::Token![=]) {
                let _: syn::Token![=] = m.input.parse()?;
                let _: syn::Expr = m.input.parse()?;
            }
            Ok(())
        });
        if found { return true; }
    }
    false
}

/// The serde `rename_all` policy. The variants mirror serde's accepted
/// values verbatim (PR #1) so a `.rs` file using the canonical names
/// resolves straight away.
#[derive(Debug, Clone, Copy)]
enum RenameAll {
    Lowercase,
    Uppercase,
    PascalCase,
    CamelCase,
    SnakeCase,
    ScreamingSnakeCase,
    KebabCase,
    ScreamingKebabCase,
}

impl RenameAll {
    fn parse(s: &str) -> Option<Self> {
        match s {
            "lowercase"             => Some(Self::Lowercase),
            "UPPERCASE"             => Some(Self::Uppercase),
            "PascalCase"            => Some(Self::PascalCase),
            "camelCase"             => Some(Self::CamelCase),
            "snake_case"            => Some(Self::SnakeCase),
            "SCREAMING_SNAKE_CASE"  => Some(Self::ScreamingSnakeCase),
            "kebab-case"            => Some(Self::KebabCase),
            "SCREAMING-KEBAB-CASE"  => Some(Self::ScreamingKebabCase),
            _ => None,
        }
    }

    /// Apply this policy to a Rust identifier (`snake_case` by convention).
    /// Identifiers that already happen to be in the target case round-trip
    /// unchanged.
    fn apply(self, ident: &str) -> String {
        // Always start from the canonical word-boundary list — split the
        // identifier on `_` first (Rust convention), then on case
        // transitions for incoming camelCase / PascalCase residues.
        let words: Vec<String> = split_into_words(ident);
        match self {
            Self::Lowercase             => words.join("").to_lowercase(),
            Self::Uppercase             => words.join("").to_uppercase(),
            Self::SnakeCase             => words.iter().map(|w| w.to_lowercase()).collect::<Vec<_>>().join("_"),
            Self::ScreamingSnakeCase    => words.iter().map(|w| w.to_uppercase()).collect::<Vec<_>>().join("_"),
            Self::KebabCase             => words.iter().map(|w| w.to_lowercase()).collect::<Vec<_>>().join("-"),
            Self::ScreamingKebabCase    => words.iter().map(|w| w.to_uppercase()).collect::<Vec<_>>().join("-"),
            Self::PascalCase            => words.iter().map(|w| capitalize(w)).collect::<Vec<_>>().join(""),
            Self::CamelCase => {
                let mut it = words.iter();
                let head = it.next().map(|w| w.to_lowercase()).unwrap_or_default();
                let tail: String = it.map(|w| capitalize(w)).collect();
                format!("{head}{tail}")
            }
        }
    }
}

/// Split a Rust identifier into lowercased words. Handles `snake_case`,
/// `kebab-case`, `PascalCase` and `camelCase` input so the converter
/// behaves correctly even when a field is hand-written in a non-Rust
/// case (e.g. someone declares `let myField: T` in a generated file).
fn split_into_words(ident: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut cur = String::new();
    let mut prev_lower = false;
    for c in ident.chars() {
        if c == '_' || c == '-' {
            if !cur.is_empty() { out.push(std::mem::take(&mut cur)); }
            prev_lower = false;
            continue;
        }
        if c.is_uppercase() && prev_lower {
            if !cur.is_empty() { out.push(std::mem::take(&mut cur)); }
        }
        cur.push(c.to_ascii_lowercase());
        prev_lower = c.is_lowercase() || c.is_ascii_digit();
    }
    if !cur.is_empty() { out.push(cur); }
    out
}

fn capitalize(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        None => String::new(),
        Some(c) => c.to_ascii_uppercase().to_string() + chars.as_str(),
    }
}

// ── Crate discovery + module-path inference ─────────────────────────────────

fn find_cargo_toml(starting_from: &Path) -> Option<PathBuf> {
    let mut cur = if starting_from.is_dir() { Some(starting_from.to_path_buf()) } else { starting_from.parent().map(|p| p.to_path_buf()) };
    while let Some(dir) = cur {
        let candidate = dir.join("Cargo.toml");
        if candidate.is_file() { return Some(candidate); }
        cur = dir.parent().map(|p| p.to_path_buf());
    }
    None
}

fn read_crate_name(manifest: &Path) -> Option<String> {
    let raw = std::fs::read_to_string(manifest).ok()?;
    let parsed: toml::Value = toml::from_str(&raw).ok()?;
    let name = parsed.get("package")?.get("name")?.as_str()?.to_string();
    Some(name)
}

fn find_crate_entry(src_dir: &Path) -> Result<PathBuf> {
    let lib = src_dir.join("lib.rs");
    if lib.is_file() { return Ok(lib); }
    let main = src_dir.join("main.rs");
    if main.is_file() { return Ok(main); }
    Err(AppError::Other(format!(
        "Neither lib.rs nor main.rs found in {}",
        src_dir.display()
    )))
}

/// Heuristic: derive the canonical module path of `file` based on its path
/// relative to `src/`. Inline modules can't be inferred this way; the
/// `walk_module_file` pass figures out their canonical names properly. This
/// is only used by `probe()` to label root candidates when we haven't yet
/// done a full crate walk.
fn guess_module_path(manifest: &Path, file: &Path) -> String {
    let src = match manifest.parent() {
        Some(p) => p.join("src"),
        None => return "crate".into(),
    };
    let rel = match file.strip_prefix(&src) {
        Ok(r) => r,
        Err(_) => return "crate".into(),
    };
    let mut parts: Vec<String> = Vec::new();
    parts.push("crate".into());
    let comps: Vec<&std::ffi::OsStr> = rel.iter().collect();
    for (i, c) in comps.iter().enumerate() {
        let s = c.to_string_lossy();
        let is_last = i == comps.len() - 1;
        if is_last {
            // lib.rs / main.rs → crate; mod.rs → drop, the dir name is
            // already in `parts`; foo.rs → add `foo`.
            if s == "lib.rs" || s == "main.rs" || s == "mod.rs" { /* nothing */ }
            else if let Some(stem) = s.strip_suffix(".rs") { parts.push(stem.to_string()); }
        } else {
            parts.push(s.to_string());
        }
    }
    parts.join("::")
}
