//! Call and type hierarchy for Java — the tree the Hierarchy panel draws, one level at a time.
//!
//! The panel and its wire shape already existed; only a language server could fill them. On a
//! `.java` buffer the two verbs were hidden, which on the one language this product exists for is
//! the wrong way round. This is the Java engine's answer to the same four questions.
//!
//! ## It is the reference index, read two ways
//!
//! Nothing here re-resolves anything. The whole-project reference index already holds
//! `declaration → its use sites`, and both call directions are that one table read differently:
//!
//! * **incoming** — the method's own bucket, each use site attributed to the declaration that
//!   encloses it;
//! * **outgoing** — every bucket, filtered to the use sites that fall inside the method's own span.
//!
//! That is deliberate rather than convenient. A call hierarchy that resolved calls its own way
//! would be free to disagree with find-usages about what calls what, and two answers to one
//! question is the bug that never gets reported because each looks plausible alone.
//!
//! The type directions read the same two structures a rename does: `SubtypeMap` downward (which is
//! why an anonymous `new Runnable() { … }` shows up as an implementor — it is an ordinary subtype
//! there, and nothing that goes by name can see it), and the resolver upward.
//!
//! ## Honest limits
//!
//! * **Project calls only.** The index records an edge only when the callee is a project type, so a
//!   call hierarchy never shows `System.out.println` — which is the useful default, and is why
//!   every method node has a file to open.
//! * **Overloads collapse.** The index keys a method by owner + name, so `process(String)` and
//!   `process(int)` are one node. A hierarchy that split them would have to claim an arity match it
//!   does not compute.
//! * **Incoming spans the override family.** Callers written against an interface and callers
//!   written against an implementation are both callers of the method, and the family is the same
//!   one a rename carries — so what the hierarchy shows and what a rename would move agree.
//! * A use site inside a field initialiser or a static block has no enclosing method; it is
//!   attributed to its **type**, and that node is a leaf. Its real callers are the constructors,
//!   which the index cannot attribute to it.

use std::collections::HashMap;

use bennu_java::prelude::{FileSymbols, MethodDecl, Span, TypeDecl, TypeResolver};
use bennu_query::prelude::PlanFile;
use serde::{Deserialize, Serialize};

use crate::refs::{DeclKey, ReferenceIndex, UsageLocation};
use crate::rename::SubtypeMap;

/// Which way a hierarchy is walked. `Incoming`/`Outgoing` are calls, the other two types.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HierarchyDirection {
    Incoming,
    Outgoing,
    Supertypes,
    Subtypes,
}

impl HierarchyDirection {
    /// The wire spelling the frontend sends. `None` for a direction this build has never heard of
    /// — answering nothing is right there; guessing which of the four was meant would hang the
    /// wrong list under an expanded node.
    pub fn from_wire(s: &str) -> Option<Self> {
        match s {
            "incoming" => Some(Self::Incoming),
            "outgoing" => Some(Self::Outgoing),
            "supertypes" => Some(Self::Supertypes),
            "subtypes" => Some(Self::Subtypes),
            _ => None,
        }
    }
}

/// What a node IS, in the terms this engine can ask a follow-up question in.
///
/// The panel treats it as opaque and sends it back verbatim, which is the point: a node's children
/// are asked for by handle, so the engine is never handed a description of an item it never offered.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "on", rename_all = "snake_case")]
pub enum HierarchyHandle {
    /// A method, by the same owner + name key the reference index buckets under.
    Method { owner: String, name: String },
    /// A type, by JVM binary name.
    Type { binary: String },
}

/// One call site inside a hierarchy node — what lets a caller row jump to the CALL rather than to
/// the head of the method that contains it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HierarchyCallSite {
    pub file: String,
    pub start: usize,
    pub end: usize,
    pub line: usize,
    pub preview: String,
}

/// One node of the tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HierarchyItem {
    pub name: String,
    /// A lowercase kind slug the frontend keys its glyphs off (`class`, `method`, `constructor`).
    pub kind: String,
    pub detail: Option<String>,
    /// The declaration's own file — empty for a library type, which has no project source.
    pub file: String,
    pub start: usize,
    pub end: usize,
    pub line: usize,
    pub col: usize,
    pub preview: String,
    pub call_sites: Vec<HierarchyCallSite>,
    pub handle: HierarchyHandle,
}

/// Everything a hierarchy question is answered from, borrowed for the duration of one.
///
/// A struct rather than four parameters because every function in here needs all four, and because
/// they have to come from ONE read of the engine's live state — an index that has caught up with an
/// edit paired with source text that has not produces spans that no longer mean anything.
pub struct HierarchyCtx<'a> {
    pub index: &'a ReferenceIndex,
    pub files: &'a [PlanFile],
    pub resolver: &'a dyn TypeResolver,
    pub subtypes: &'a SubtypeMap,
}

/// The root of the tree for the declaration at the caret, or nothing when the caret is not on one
/// this hierarchy can be built from.
///
/// A **call** hierarchy needs a method the project declares: a caret on a library call has no
/// bucket to read and no source to open, and a root that can only ever say "nothing here" is worse
/// than saying so up front.
///
/// A **type** hierarchy accepts a caret anywhere in a type — on a member, it climbs to the owner.
/// Ctrl+H with the caret in the middle of a method body is asking about the class you are reading,
/// and refusing on the grounds that the caret was two lines off would be pedantry.
pub fn prepare(ctx: &HierarchyCtx, key: &DeclKey, calls: bool) -> Vec<HierarchyItem> {
    if calls {
        let DeclKey::Method { owner, name } = key else {
            return Vec::new();
        };
        method_item(ctx, owner, name, Vec::new()).into_iter().collect()
    } else {
        type_item(ctx, key.owner_binary()).into_iter().collect()
    }
}

/// One level, expanded from a node's own handle.
pub fn step(
    ctx: &HierarchyCtx,
    handle: &HierarchyHandle,
    direction: HierarchyDirection,
) -> Vec<HierarchyItem> {
    match (handle, direction) {
        (HierarchyHandle::Method { owner, name }, HierarchyDirection::Incoming) => {
            incoming(ctx, owner, name)
        }
        (HierarchyHandle::Method { owner, name }, HierarchyDirection::Outgoing) => {
            outgoing(ctx, owner, name)
        }
        (HierarchyHandle::Type { binary }, HierarchyDirection::Supertypes) => {
            supertypes(ctx, binary)
        }
        (HierarchyHandle::Type { binary }, HierarchyDirection::Subtypes) => subtypes(ctx, binary),
        // A type asked a call question, or a method asked a type one. The panel keeps its roots
        // when the direction changes, so this is reachable by an ordinary gesture — flipping a call
        // hierarchy's chips over a type node — and the honest answer is none.
        _ => Vec::new(),
    }
}

// ── calls ─────────────────────────────────────────────────────────────────────────

/// Who calls `owner.name` — one node per calling declaration, each carrying its own call sites.
fn incoming(ctx: &HierarchyCtx, owner: &str, name: &str) -> Vec<HierarchyItem> {
    // The whole override family, not just this declaration: a call written against the interface
    // and a call written against the implementation are both calls to this method, and they sit in
    // different buckets. Same family a rename carries, so the two never disagree.
    let family = crate::rename::override_family(ctx.resolver, ctx.subtypes, owner, name);

    // Grouped by the declaration the call sits in — `(file, decl start)` identifies it, and two
    // calls to the same thing inside one method are one row with a count, not two rows.
    let mut order: Vec<(String, usize)> = Vec::new();
    let mut groups: HashMap<(String, usize), (Caller, Vec<HierarchyCallSite>)> = HashMap::new();

    for family_owner in &family {
        let key = DeclKey::Method {
            owner: family_owner.clone(),
            name: name.to_string(),
        };
        for usage in ctx.index.usages_of(&key) {
            let Some(caller) = caller_of(ctx, usage) else {
                continue;
            };
            let group = (caller.file.clone(), caller.decl.start);
            let slot = groups.entry(group.clone()).or_insert_with(|| {
                order.push(group.clone());
                (caller, Vec::new())
            });
            slot.1.push(call_site_of(usage));
        }
    }

    let mut out: Vec<HierarchyItem> = Vec::new();
    for group in order {
        let Some((caller, mut sites)) = groups.remove(&group) else {
            continue;
        };
        sites.sort_by_key(|s| s.start);
        if let Some(item) = caller.into_item(ctx, sites) {
            out.push(item);
        }
    }
    // File then line: a caller list is read, and source order is the only order a reader can hold.
    out.sort_by(|a, b| a.file.cmp(&b.file).then(a.line.cmp(&b.line)));
    out
}

/// What `owner.name` calls — one node per callee, carrying the sites inside this method that reach
/// it.
fn outgoing(ctx: &HierarchyCtx, owner: &str, name: &str) -> Vec<HierarchyItem> {
    let Some(file) = ctx.index.file_declaring(owner) else {
        return Vec::new();
    };
    let Some(symbols) = ctx.index.symbols(file) else {
        return Vec::new();
    };
    let Some(decl_type) = type_decl_for(symbols, owner) else {
        return Vec::new();
    };
    // Every overload's body: the key collapses them, so "what does `process` call" is the union.
    let bodies: Vec<Span> = decl_type
        .methods
        .iter()
        .filter(|m| m.name == name)
        .filter_map(|m| m.span)
        .collect();
    if bodies.is_empty() {
        return Vec::new();
    }

    let mut order: Vec<(String, String)> = Vec::new();
    let mut groups: HashMap<(String, String), Vec<HierarchyCallSite>> = HashMap::new();
    // A scan of every bucket, which is the whole index — but the file test is one string compare
    // per use site and rejects all but one file's worth, and this runs once per expanded node.
    for (key, usages) in ctx.index.iter() {
        let DeclKey::Method {
            owner: callee_owner,
            name: callee_name,
        } = key
        else {
            continue;
        };
        for usage in usages {
            if usage.file != file {
                continue;
            }
            if !bodies.iter().any(|b| usage.start >= b.start && usage.end <= b.end) {
                continue;
            }
            let group = (callee_owner.clone(), callee_name.clone());
            groups
                .entry(group.clone())
                .or_insert_with(|| {
                    order.push(group.clone());
                    Vec::new()
                })
                .push(call_site_of(usage));
        }
    }

    let mut out: Vec<HierarchyItem> = Vec::new();
    for group in order {
        let Some(mut sites) = groups.remove(&group) else {
            continue;
        };
        sites.sort_by_key(|s| s.start);
        if let Some(item) = method_item(ctx, &group.0, &group.1, sites) {
            out.push(item);
        }
    }
    // Source order of the FIRST call: a callee list read top-to-bottom follows the body.
    out.sort_by_key(|i| i.call_sites.first().map(|s| s.start).unwrap_or(0));
    out
}

/// The declaration a use site sits in.
struct Caller {
    file: String,
    /// The type it is declared on, by the JVM binary name the index buckets under.
    owner: String,
    /// The enclosing method's name, or `None` for a use site in a field initialiser / static block.
    member: Option<String>,
    decl: Span,
}

impl Caller {
    fn into_item(self, ctx: &HierarchyCtx, sites: Vec<HierarchyCallSite>) -> Option<HierarchyItem> {
        match &self.member {
            Some(name) => method_item(ctx, &self.owner, name, sites),
            // No enclosing method — the type is the honest attribution, and a leaf: what calls a
            // field initialiser is a constructor, and the index does not record that edge.
            None => type_item(ctx, &self.owner).map(|mut item| {
                item.call_sites = sites;
                item
            }),
        }
    }
}

/// The declaration enclosing a use site, from the file's already-extracted symbols.
fn caller_of(ctx: &HierarchyCtx, usage: &UsageLocation) -> Option<Caller> {
    let symbols = ctx.index.symbols(&usage.file)?;
    // Innermost first: a local or anonymous class inside a method is a type of its own, and a call
    // in it belongs to ITS member, not to the method the class happens to sit in.
    let decl_type = innermost(
        symbols.types.iter().filter_map(|t| t.span.map(|s| (s, t))),
        usage.start,
    )?;
    let method = innermost(
        decl_type.methods.iter().filter_map(|m| m.span.map(|s| (s, m))),
        usage.start,
    );
    Some(Caller {
        file: usage.file.clone(),
        // The project's binary-name convention is the fqn with `/` for every dot — package
        // separators and nesting alike (`p/Outer/Inner`, see `binary_of_type_at`). It has to be
        // that and not the dotted fqn, because this key is handed to the resolver.
        owner: decl_type.fqn.replace('.', "/"),
        member: method.map(|m: &MethodDecl| m.name.clone()),
        decl: method.and_then(|m| m.span).or(decl_type.span)?,
    })
}

/// The smallest span containing `at`, from an iterator of `(span, item)`.
fn innermost<'a, T>(items: impl Iterator<Item = (Span, &'a T)>, at: usize) -> Option<&'a T> {
    items
        .filter(|(s, _)| at >= s.start && at < s.end)
        .min_by_key(|(s, _)| s.end - s.start)
        .map(|(_, t)| t)
}

// ── types ─────────────────────────────────────────────────────────────────────────

/// What `binary` is built on — its superclass and its interfaces, as the resolver sees them.
///
/// Library supertypes are kept. `extends HttpServlet` is half the answer to "what is this class",
/// and dropping it because the jar has no source to open would leave the tree saying the class is
/// built on nothing. They carry no file, so the panel does not offer to jump to one.
fn supertypes(ctx: &HierarchyCtx, binary: &str) -> Vec<HierarchyItem> {
    let Some(members) = ctx.resolver.members_of(binary) else {
        return Vec::new();
    };
    members
        .superclass
        .iter()
        .chain(members.interfaces.iter())
        // Everything extends it, and a parent that is always the same node says nothing.
        .filter(|s| s.as_str() != "java/lang/Object")
        .filter_map(|s| type_item(ctx, s))
        .collect()
}

/// What is built on `binary` — the same map an override family descends, so an anonymous class is
/// here for the same reason it is there: it is an ordinary subtype, with no name to look up.
fn subtypes(ctx: &HierarchyCtx, binary: &str) -> Vec<HierarchyItem> {
    ctx.subtypes
        .children(binary)
        .iter()
        .filter_map(|s| type_item(ctx, s))
        .collect()
}

// ── nodes ─────────────────────────────────────────────────────────────────────────

/// A node for the method `owner.name`, or nothing when no project source declares it.
fn method_item(
    ctx: &HierarchyCtx,
    owner: &str,
    name: &str,
    call_sites: Vec<HierarchyCallSite>,
) -> Option<HierarchyItem> {
    let file = ctx.index.file_declaring(owner)?.to_string();
    let symbols = ctx.index.symbols(&file)?;
    let decl_type = type_decl_for(symbols, owner)?;
    let source = source_of(ctx.files, &file)?;
    // The first overload that is actually written: the key collapses them, so one of them is where
    // the row points, and the label says which arity it took.
    let method = decl_type
        .methods
        .iter()
        .filter(|m| m.name == name)
        .find(|m| m.span.is_some())?;
    let span = method.span?;

    // A constructor is written under the class's name, not under the `<init>` the index files it
    // as — so that is what the source scan looks for and what the row shows.
    let is_ctor = method.name == "<init>";
    let written = if is_ctor {
        decl_type.name.as_str()
    } else {
        method.name.as_str()
    };
    let kind = if is_ctor { "constructor" } else { "method" };
    let (start, end) = name_span(source, span, written, true).unwrap_or((span.start, span.start));
    let (line, col, preview) = locate(source, start);

    Some(HierarchyItem {
        name: format!("{written}{}", signature(method)),
        kind: kind.to_string(),
        detail: Some(decl_type.name.clone()),
        file,
        start,
        end,
        line,
        col,
        preview,
        call_sites,
        handle: HierarchyHandle::Method {
            owner: owner.to_string(),
            name: name.to_string(),
        },
    })
}

/// A node for the type `binary` — from the project source that declares it, or, for a library type,
/// from what the resolver knows about it.
fn type_item(ctx: &HierarchyCtx, binary: &str) -> Option<HierarchyItem> {
    let handle = HierarchyHandle::Type {
        binary: binary.to_string(),
    };
    let declared = ctx
        .index
        .file_declaring(binary)
        .map(str::to_string)
        .and_then(|file| {
            let symbols = ctx.index.symbols(&file)?;
            let decl = type_decl_for(symbols, binary)?;
            let source = source_of(ctx.files, &file)?;
            Some((file, decl, source))
        });

    if let Some((file, decl, source)) = declared {
        let span = decl.span?;
        let (start, end) =
            name_span(source, span, &decl.name, false).unwrap_or((span.start, span.start));
        let (line, col, preview) = locate(source, start);
        return Some(HierarchyItem {
            name: decl.name.clone(),
            kind: decl.kind.slug().to_string(),
            detail: package_of(&decl.fqn),
            file,
            start,
            end,
            line,
            col,
            preview,
            call_sites: Vec::new(),
            handle,
        });
    }

    // A library type: no source, so no file and no preview — the row names it and can still be
    // expanded upward, because the resolver reads a jar's supertype links as readily as a source's.
    let members = ctx.resolver.members_of(binary)?;
    let dotted = binary.replace('/', ".").replace('$', ".");
    let kind = if members.flags.is_annotation {
        "annotation"
    } else if members.flags.is_enum {
        "enum"
    } else if members.flags.is_record {
        "record"
    } else if members.flags.is_interface {
        "interface"
    } else {
        "class"
    };
    Some(HierarchyItem {
        name: dotted.rsplit('.').next().unwrap_or(dotted.as_str()).to_string(),
        kind: kind.to_string(),
        detail: package_of(&dotted),
        file: String::new(),
        start: 0,
        end: 0,
        line: 0,
        col: 0,
        preview: String::new(),
        call_sites: Vec::new(),
        handle,
    })
}

/// `(String, int)` — the parameter TYPES, which is what tells two overloads apart in a list. Names
/// are left out on purpose: a hierarchy row is read at a glance, and the arity is the question.
fn signature(method: &MethodDecl) -> String {
    let params: Vec<&str> = method.params.iter().map(|p| p.type_text.as_str()).collect();
    format!("({})", params.join(", "))
}

/// The package part of a dotted name, or `None` for the default package.
fn package_of(fqn: &str) -> Option<String> {
    let at = fqn.rfind('.')?;
    Some(fqn[..at].to_string())
}

// ── source helpers ────────────────────────────────────────────────────────────────

/// The `TypeDecl` in `symbols` whose binary name is `binary`.
///
/// The symbol model spells a nested type `p.Outer.Inner` and the JVM spells it `p/Outer$Inner`; a
/// binary that came back from bytecode carries the `$`. The exact spelling is tried first, so a
/// top-level class genuinely named `A$B` still wins over a nested reading of it — the same order
/// `ReferenceIndex::file_declaring` uses, and for the same reason.
fn type_decl_for<'a>(symbols: &'a FileSymbols, binary: &str) -> Option<&'a TypeDecl> {
    let dotted = binary.replace('/', ".");
    if let Some(hit) = symbols.types.iter().find(|t| t.fqn == dotted) {
        return Some(hit);
    }
    if !dotted.contains('$') {
        return None;
    }
    let alt = dotted.replace('$', ".");
    symbols.types.iter().find(|t| t.fqn == alt)
}

/// The project source of `file`.
fn source_of<'a>(files: &'a [PlanFile], file: &str) -> Option<&'a str> {
    files.iter().find(|f| f.path == file).map(|f| f.source.as_str())
}

/// The span of the identifier `name` as written in the header of the declaration at `decl`.
///
/// A scan of the declaration's own text, not a re-parse of the file. The symbols are already
/// extracted, and one level of a hierarchy can hold thirty nodes from thirty files — thirty parses
/// to find thirty tokens that are already in hand.
///
/// `callable` demands a `(` after the name, which is what separates a method's own header from its
/// return type and from an annotation argument that happens to spell the same word.
fn name_span(source: &str, decl: Span, name: &str, callable: bool) -> Option<(usize, usize)> {
    let end = decl.end.min(source.len());
    if decl.start >= end || name.is_empty() {
        return None;
    }
    let text = source.get(decl.start..end)?;
    let bytes = text.as_bytes();
    for (at, _) in text.match_indices(name) {
        let after = at + name.len();
        // A whole word, and never the name of an annotation — `@Foo public Foo(int x)` spells the
        // class name twice and only the second one is the declaration.
        if at > 0 && (is_ident_byte(bytes[at - 1]) || bytes[at - 1] == b'@') {
            continue;
        }
        if bytes.get(after).is_some_and(|b| is_ident_byte(*b)) {
            continue;
        }
        if callable {
            let mut i = after;
            while bytes.get(i).is_some_and(|b| b.is_ascii_whitespace()) {
                i += 1;
            }
            if bytes.get(i) != Some(&b'(') {
                continue;
            }
        }
        return Some((decl.start + at, decl.start + after));
    }
    None
}

fn is_ident_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b == b'$' || b >= 0x80
}

/// 1-based line and column of `at`, plus the trimmed source line it is on — one scan for the three
/// things every node needs.
fn locate(source: &str, at: usize) -> (usize, usize, String) {
    let mut at = at.min(source.len());
    while at > 0 && !source.is_char_boundary(at) {
        at -= 1;
    }
    let head = &source[..at];
    let line = head.bytes().filter(|b| *b == b'\n').count() + 1;
    let line_start = head.rfind('\n').map(|i| i + 1).unwrap_or(0);
    let col = at - line_start + 1;
    let line_end = source[line_start..]
        .find('\n')
        .map(|i| line_start + i)
        .unwrap_or(source.len());
    (line, col, source[line_start..line_end].trim().to_string())
}

/// A use site as a call site on its node.
fn call_site_of(usage: &UsageLocation) -> HierarchyCallSite {
    HierarchyCallSite {
        file: usage.file.clone(),
        start: usage.start,
        end: usage.end,
        line: usage.line,
        preview: usage.preview.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bennu_java::prelude::{ParamDecl, Visibility};

    fn method_named(name: &str, params: &[(&str, &str)]) -> MethodDecl {
        MethodDecl {
            span: None,
            name: name.to_string(),
            return_type_text: "void".into(),
            params: params
                .iter()
                .map(|(ty, nm)| ParamDecl {
                    name: nm.to_string(),
                    type_text: ty.to_string(),
                    annotations: Vec::new(),
                })
                .collect(),
            is_static: false,
            visibility: Visibility::Public,
            is_abstract: false,
            is_default: false,
            is_final: false,
            throws: Vec::new(),
            annotations: Vec::new(),
        }
    }

    #[test]
    fn a_method_name_span_skips_the_return_type_and_the_annotations() {
        let src =
            "class C {\n    @Deprecated\n    public String value(int value) { return null; }\n}\n";
        let decl = Span {
            start: src.find("@Deprecated").unwrap(),
            end: src.rfind('}').unwrap(),
        };
        let (start, end) = name_span(src, decl, "value", true).expect("the header name");
        assert_eq!(&src[start..end], "value");
        // The one followed by `(` — not the parameter, and not a word inside the annotation.
        assert_eq!(&src[end..end + 1], "(");
    }

    #[test]
    fn a_constructor_annotated_with_its_own_class_name_takes_the_declaration() {
        let src = "class Foo {\n    @Foo\n    Foo(int x) {}\n}\n";
        let decl = Span {
            start: src.find("@Foo").unwrap(),
            end: src.rfind('}').unwrap(),
        };
        let (start, end) = name_span(src, decl, "Foo", true).expect("the constructor name");
        assert_eq!(&src[start..end], "Foo");
        assert_eq!(&src[end..end + 1], "(");
    }

    #[test]
    fn a_type_name_span_is_the_declared_name_not_the_annotation() {
        let src = "@Service\npublic class Service extends Base {}\n";
        let decl = Span { start: 0, end: src.len() };
        let (start, end) = name_span(src, decl, "Service", false).expect("the class name");
        assert_eq!(&src[start..end], "Service");
        assert!(src[..start].ends_with("class "));
    }

    #[test]
    fn a_name_that_is_only_ever_a_substring_has_no_span() {
        let src = "class C { void processAll() {} }\n";
        let decl = Span { start: 0, end: src.len() };
        assert_eq!(name_span(src, decl, "process", true), None);
    }

    #[test]
    fn locate_reports_one_based_line_and_column_with_the_trimmed_line() {
        let src = "a\n    bcd\n";
        let (line, col, preview) = locate(src, src.find("bcd").unwrap());
        assert_eq!((line, col), (2, 5));
        assert_eq!(preview, "bcd");
    }

    #[test]
    fn a_signature_is_the_parameter_types_in_order() {
        assert_eq!(
            signature(&method_named("process", &[("String", "a"), ("int", "b")])),
            "(String, int)"
        );
        assert_eq!(signature(&method_named("run", &[])), "()");
    }

    #[test]
    fn a_package_is_everything_before_the_last_dot() {
        assert_eq!(package_of("p.q.Order"), Some("p.q".to_string()));
        assert_eq!(package_of("Order"), None);
    }

    #[test]
    fn an_unknown_direction_is_not_guessed_at() {
        assert_eq!(
            HierarchyDirection::from_wire("incoming"),
            Some(HierarchyDirection::Incoming)
        );
        assert_eq!(HierarchyDirection::from_wire("sideways"), None);
    }
}
