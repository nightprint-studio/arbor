//! The preconditions for judging a **bare** call `foo(a, b)` — one written with no receiver, so it
//! binds against `this` and everything the enclosing type inherits or imports statically.
//!
//! Both [`crate::calls::arity`] and [`crate::calls::arguments`] started life reading only `recv.method(…)`,
//! because a receiver is what gives them a type to ask. That left the shape a Java file is mostly
//! made of — a class calling its own methods — unjudged: `f(1, 2)` against a `void f(int)` on the
//! same class drew nothing, from either check. The receiver is not missing there, it is `this`; what
//! was missing is the small pile of guards that make `this` safe to name, and they are the same for
//! both checks, so they live here rather than twice.
//!
//! ## Soundness — never a false positive
//!
//! WHOLE-FILE guards (any → judge no bare call in the file):
//!   * no single top-level class/enum, or its hierarchy not fully known — an un-indexed base class
//!     could declare the overload that makes the call legal.
//!
//! NAME-SET guards — they decide which NAMES a bare call may bind to, so they matter to a check that
//! asks whether a name binds at all or how many overloads it has ([`BareCalls::judgeable_across_lambdas`]),
//! and not to one that already holds a non-empty member overload set ([`BareCalls::judgeable_member_call`]):
//!   * a **method-generating annotation** on the top type or a field
//!     ([`crate::support::nodes::generated_names`]) — under Lombok the legal `getName()` is declared nowhere
//!     we can read;
//!   * an `import static X.*;` whose owner `X` is un-indexed — it can supply ANY name with any
//!     signature;
//!   * a name an `import static` supplies.
//!
//! PER-SITE guards (any → skip that call):
//!   * it must be a `method_invocation` with no `object` field;
//!   * it must sit in the top type, crossing no nested / anonymous / local class body
//!     ([`crate::support::scopes::scope_is_top_across_lambdas`]) — each of those can declare methods of its
//!     own that the top type's hierarchy knows nothing about. A lambda declares none and does not
//!     rebind `this`, so it is crossed;
//!   * its name must not be one of `java.lang.Object`'s, nor an `enum`'s compiler-generated `values` /
//!     `valueOf` — the binding exists but its signature is not something we can enumerate here.
//!
//! ## Why a member overload set makes the import guards moot
//!
//! A method a class declares or inherits SHADOWS every statically imported method of the same name,
//! whatever their arities (JLS §6.4.1, §15.12.1: the search stops at the innermost class that has a
//! member of that name). So once the top type's hierarchy has a method named `m`, no `import static`
//! can add an overload of `m` — and Lombok, which generates a method only when none of that name and
//! parameter count exists, cannot add one at an arity the index already has. The argument-type check
//! judges only such a set; refusing it the whole file for a `@Data` or an unreadable `import static
//! X.*` left every call to an own method unjudged in exactly the legacy classes that most need it.
//!
//! ## Why the file's own declarations are re-read from the CST
//!
//! The index is a snapshot and the buffer is not. A method typed one second ago is callable
//! immediately, and judging a call against an index that has not seen it yet is how a check that is
//! correct in a batch run becomes a squiggle that blinks while someone types. So the signatures the
//! FILE declares are gathered from [`FileSymbols`] as well, and count as candidates on top of
//! whatever the resolver knows. Over-collection is safe in one direction only — every extra
//! candidate can suppress a report, never cause one — which is why they are gathered file-wide,
//! nested and anonymous types included, without asking which type each belongs to.

use std::collections::{HashMap, HashSet};

use bennu_java::prelude::{
    same_binary_type, split_array_dims, static_import_targets, FileSymbols, Member, TypeRef,
    TypeResolver,
};
use tree_sitter::Node;

use crate::support::nodes::{generated_names, is_type_var};
use crate::support::resolve::type_binary;
use crate::support::scopes::{scope_is_top_across_lambdas, single_top_level_type};
use crate::support::walk::{for_each_supertype, hierarchy_fully_known};

/// `java.lang.Object`'s methods, callable bare from any class body. Their overload sets (`wait()`,
/// `wait(long)`, `wait(long, int)`) come from an index that may summarise `Object` — so rather than
/// judge a call against a partial list, we do not judge these names at all.
const OBJECT_METHODS: &[&str] = &[
    "toString", "hashCode", "equals", "getClass", "clone", "finalize", "notify", "notifyAll", "wait",
];

/// The two methods the compiler adds to every `enum` (JLS §8.9.3), present in no source file.
const ENUM_IMPLICIT_METHODS: &[&str] = &["values", "valueOf"];

/// One signature as the FILE writes it: the parameter type texts, plus whether the last parameter is
/// a `T...` / `T[]` (so the signature admits a varargs call).
pub(crate) struct FileSig {
    pub(crate) param_texts: Vec<String>,
    pub(crate) varargs: bool,
}

/// What a file has to be for its bare calls to be judgeable, and what it declares itself.
pub(crate) struct BareCalls<'t> {
    top_node: Node<'t>,
    /// The binary name of the single top-level type — the static type of the implicit `this`.
    pub(crate) top_binary: String,
    is_enum: bool,
    /// A method-generating annotation (Lombok & co.) sits on the top type or one of its fields.
    generates_methods: bool,
    /// An `import static X.*;` whose owner cannot be read — it may supply any name.
    unreadable_static_wildcard: bool,
    /// Names an `import static` binds; each is a candidate whose signature we cannot enumerate.
    static_names: HashSet<String>,
    /// `import static a.B.m;` — `(a/B, m)`.
    single_static: Vec<(String, String)>,
    /// `import static a.B.*;` — the owners whose whole hierarchy could be read.
    on_demand_static: Vec<String>,
    /// `java.lang.Object`'s own methods can be read, so their overload sets are exact.
    object_readable: bool,
    /// Every method the file declares, by name — the buffer's own answer, ahead of the index.
    file_sigs: HashMap<String, Vec<FileSig>>,
}

/// Where a bare call's name is looked up (JLS §15.12.1).
pub(crate) enum MemberOwner {
    /// A nested, inner or anonymous class between the call and the top type has a method of that
    /// name — this binary (the anonymous class's supertype, for one of those).
    Nested(String),
    /// Nothing between them does: the top type, then the static imports.
    Top,
}

/// Establish the whole-file preconditions, or `None` when bare calls must not be judged here.
pub(crate) fn bare_call_scope<'t>(
    root: Node<'t>,
    source: &str,
    symbols: &FileSymbols,
    resolver: &dyn TypeResolver,
) -> Option<BareCalls<'t>> {
    let bytes = source.as_bytes();
    let top = single_top_level_type(root, bytes)?;
    let generates_methods = generated_names(top.node, bytes).calls;
    let top_binary = type_binary(&top.decl_name, symbols, resolver)?;
    if !hierarchy_fully_known(resolver, &top_binary) {
        return None;
    }

    let mut unreadable_static_wildcard = false;
    let mut static_names: HashSet<String> = HashSet::new();
    let mut single_static = Vec::new();
    let mut on_demand_static = Vec::new();
    for t in static_import_targets(&symbols.imports) {
        match t.member {
            Some(m) => {
                static_names.insert(m.clone());
                single_static.push((t.owner_binary, m));
            }
            None => {
                // A wildcard whose owner we cannot read could supply ANY name.
                if !hierarchy_fully_known(resolver, &t.owner_binary) {
                    unreadable_static_wildcard = true;
                    continue;
                }
                for_each_supertype(resolver, &t.owner_binary, &mut |_bn, cm| {
                    for member in &cm.methods {
                        static_names.insert(member.name.clone());
                    }
                });
                on_demand_static.push(t.owner_binary);
            }
        }
    }
    let object_readable = resolver
        .members_of("java/lang/Object")
        .is_some_and(|cm| cm.methods.iter().any(|m| m.name == "toString"));

    let mut file_sigs: HashMap<String, Vec<FileSig>> = HashMap::new();
    for td in &symbols.types {
        for m in &td.methods {
            let param_texts: Vec<String> = m.params.iter().map(|p| p.type_text.clone()).collect();
            let varargs = param_texts
                .last()
                .is_some_and(|t| t.trim_end().ends_with("...") || t.trim_end().ends_with("[]"));
            file_sigs.entry(m.name.clone()).or_default().push(FileSig { param_texts, varargs });
        }
    }

    Some(BareCalls {
        top_node: top.node,
        top_binary,
        is_enum: top.node.kind() == "enum_declaration",
        generates_methods,
        unreadable_static_wildcard,
        static_names,
        single_static,
        on_demand_static,
        object_readable,
        file_sigs,
    })
}

impl<'t> BareCalls<'t> {
    /// The type a bare call binds its name in, reading through the nested and anonymous classes
    /// between it and the top type — see [`Self::member_owner`] — with every name-set guard applied
    /// when the lookup reaches the top type. For a check that needs the whole set of what the name
    /// may call (`throws_of`).
    pub(crate) fn owner_through_nested(
        &self,
        call: Node,
        bytes: &[u8],
        symbols: &FileSymbols,
        resolver: &dyn TypeResolver,
    ) -> Option<String> {
        if self.generates_methods || self.unreadable_static_wildcard {
            return None;
        }
        match self.member_owner(call, bytes, symbols, resolver)? {
            MemberOwner::Nested(owner) => Some(owner),
            MemberOwner::Top => {
                let name = self.judgeable_name(call, bytes)?;
                (!self.static_names.contains(name)).then(|| self.top_binary.clone())
            }
        }
    }

    /// Where a bare call's name is looked up: the innermost class between the call and the top type
    /// that has a method of that name (JLS §15.12.1), or the top type when none does.
    ///
    /// A member class answers for itself when its hierarchy was read to the end; an anonymous class
    /// answers through its supertype, unless its body declares the name — those methods are not in
    /// the index. A local class, or an enum constant's body, answers `None`: nothing is judged.
    pub(crate) fn member_owner(
        &self,
        call: Node,
        bytes: &[u8],
        symbols: &FileSymbols,
        resolver: &dyn TypeResolver,
    ) -> Option<MemberOwner> {
        let name = self.judgeable_name(call, bytes)?;
        if scope_is_top_across_lambdas(call, self.top_node) {
            return Some(MemberOwner::Top);
        }
        let top_body = self.top_node.child_by_field_name("body").map(|b| b.id());
        let has_method = |binary: &str| {
            crate::support::walk::hierarchy_has(resolver, binary, &|cm| cm.methods.iter().any(|m| m.name == name))
        };
        let mut cur = call.parent();
        while let Some(p) = cur {
            if p.id() == self.top_node.id() {
                return Some(MemberOwner::Top);
            }
            match p.kind() {
                "class_body" | "enum_body" if Some(p.id()) == top_body => {}
                "enum_body_declarations" if p.parent().map(|b| b.id()) == top_body => {}
                "class_body" | "interface_body" | "enum_body" => {
                    let owner = p.parent()?;
                    let binary = match owner.kind() {
                        "object_creation_expression" => {
                            if body_declares_method(p, name, bytes) {
                                return None;
                            }
                            let written = owner.child_by_field_name("type")?.utf8_text(bytes).ok()?;
                            crate::support::resolve::type_binary_at(written, owner, bytes, symbols, resolver)?
                        }
                        "class_declaration" | "interface_declaration" | "enum_declaration" | "record_declaration"
                            if owner.parent().is_some_and(|q| q.kind() != "block") =>
                        {
                            bennu_java::prelude::type_decl_at(&owner, symbols)?.fqn.replace('.', "/")
                        }
                        _ => return None,
                    };
                    if !hierarchy_fully_known(resolver, &binary) {
                        return None;
                    }
                    if body_declares_method(p, name, bytes) || has_method(&binary) {
                        return Some(MemberOwner::Nested(binary));
                    }
                }
                "enum_body_declarations" => return None,
                _ => {}
            }
            cur = p.parent();
        }
        None
    }

    /// The static methods named `name` the file's static imports bring in — a single static import
    /// of the name, or else every static import on demand (JLS §6.4.1: the single one shadows the
    /// others). `None` when some owner cannot be read, when a generated or unreadable member could be
    /// the one meant, or when nothing of the name is imported.
    pub(crate) fn static_import_methods(&self, name: &str, resolver: &dyn TypeResolver) -> Option<Vec<Member>> {
        if self.generates_methods || self.unreadable_static_wildcard {
            return None;
        }
        let singles: Vec<&String> =
            self.single_static.iter().filter(|(_, member)| member == name).map(|(owner, _)| owner).collect();
        let owners: Vec<&String> = match singles.is_empty() {
            true => self.on_demand_static.iter().collect(),
            false => singles,
        };
        let mut found: Vec<Member> = Vec::new();
        for owner in owners {
            if !hierarchy_fully_known(resolver, owner) {
                return None;
            }
            for_each_supertype(resolver, owner, &mut |_bn, cm| {
                for m in cm.methods.iter().filter(|m| m.name == name && m.is_static) {
                    if !found.contains(m) {
                        found.push(m.clone());
                    }
                }
            });
        }
        (!found.is_empty()).then_some(found)
    }

    /// Whether a method-generating annotation makes the top type's member list incomplete.
    pub(crate) fn generates_methods(&self) -> bool {
        self.generates_methods
    }

    /// Whether this file declares `binary` — one whose index entry may lag behind the buffer.
    pub(crate) fn declares(&self, binary: &str, symbols: &FileSymbols) -> bool {
        symbols.types.iter().any(|t| same_binary_type(&t.fqn.replace('.', "/"), binary))
    }

    /// The per-site guards that do not depend on where the call sits.
    fn judgeable_name<'a>(&self, call: Node, bytes: &'a [u8]) -> Option<&'a str> {
        if call.child_by_field_name("object").is_some() {
            return None;
        }
        let name_node = call.child_by_field_name("name")?;
        let args = call.child_by_field_name("arguments")?;
        if name_node.has_error() || args.has_error() {
            return None;
        }
        let name = name_node.utf8_text(bytes).ok()?;
        // Judged once `Object` itself was read: its overload sets are then exact, and a `toString(x)`
        // written where the class inherits only `toString()` is javac's to reject.
        if OBJECT_METHODS.contains(&name) && !self.object_readable {
            return None;
        }
        if self.is_enum && ENUM_IMPLICIT_METHODS.contains(&name) {
            return None;
        }
        Some(name)
    }

    /// The signatures the FILE declares under `name` — empty when it declares none.
    pub(crate) fn file_sigs(&self, name: &str) -> &[FileSig] {
        self.file_sigs.get(name).map(Vec::as_slice).unwrap_or(&[])
    }
}

/// Whether a class body declares a method named `name` directly.
fn body_declares_method(body: Node, name: &str, bytes: &[u8]) -> bool {
    let mut c = body.walk();
    let found = body.named_children(&mut c).any(|m| {
        m.kind() == "method_declaration"
            && m.child_by_field_name("name").and_then(|n| n.utf8_text(bytes).ok()) == Some(name)
    });
    found
}

/// Whether `candidates` (what the index knows) already covers every signature the FILE declares
/// under `name` — on any of its types, constructors as `<init>` — parameter type for parameter type.
///
/// This is the gate for any judgement that needs the overload set to be **exact** rather than
/// merely non-empty — argument types, where committing to a lone candidate is the whole method.
/// Matching on arity alone is not enough and was the first thing tried: a buffer that adds
/// `own(String)` beside an indexed `own(int)` has an arity-1 candidate either way, so the stale
/// single candidate stood, and a legal `own("x")` came out as a wrong argument type. Anything
/// that will not resolve is a reason to say no, not to guess. The signatures gathered are those of
/// `binary` and of every type of the file it may inherit from — never a constructor of anything but
/// `binary` itself, since constructors are not inherited. Gathering the whole file refused every
/// call to a type sharing it with another that declares the same name: `new Pair("a", 1)` went
/// unjudged because a sibling `Inner(int)` is not one of `Pair`'s constructors.
///
/// Two shapes of parameter text are matched structurally rather than by resolved name, because
/// neither HAS a resolved name to compare and both used to make every own method carrying one
/// uncoverable — and so every call to it unjudged: a type variable (`<T> void put(T value)`),
/// matched against an index parameter that is an unresolved variable too; and an array or
/// varargs parameter, whose depth the index keeps beside the name (`dims`) and the text spells
/// with brackets.
pub(crate) fn index_covers_file_sigs(
    binary: &str,
    name: &str,
    candidates: &[Member],
    symbols: &FileSymbols,
    resolver: &dyn TypeResolver,
) -> bool {
    let judged = TypeRef::simple(binary);
    let declared = symbols
        .types
        .iter()
        .filter(|t| {
            let decl = t.fqn.replace('.', "/");
            same_binary_type(&decl, binary)
                || (name != "<init>"
                    && bennu_java::prelude::subtype_verdict(resolver, &judged, &TypeRef::simple(decl))
                        != Some(false))
        })
        .flat_map(|t| &t.methods)
        .filter(|m| m.name == name);
    declared.into_iter().all(|fs| {
        candidates.iter().any(|m| {
            m.params.len() == fs.params.len()
                && m.params
                    .iter()
                    .zip(&fs.params)
                    .all(|(p, written)| param_matches_text(p, &written.type_text, symbols, resolver))
        })
    })
}

/// Whether the index parameter `p` is the parameter the file writes as `text`.
fn param_matches_text(
    p: &TypeRef,
    text: &str,
    symbols: &FileSymbols,
    resolver: &dyn TypeResolver,
) -> bool {
    let written = text.trim();
    let (written, varargs) = match written.strip_suffix("...") {
        Some(base) => (base.trim_end(), 1),
        None => (written, 0),
    };
    let (element, dims) = split_array_dims(written);
    let p_element = p.binary_name.trim_end_matches("[]");
    let p_dims = usize::from(p.dims) + p.binary_name.matches("[]").count();
    if p_dims != dims + varargs {
        return false;
    }
    match type_binary(element, symbols, resolver) {
        Some(binary) => same_binary_type(p_element, &binary),
        // Nothing binds the written name: a type variable. It is the same parameter only when the
        // index could not bind it either — never a resolved class that happens to sit there.
        None => {
            !p_element.contains('/')
                && resolver.members_of(p_element).is_none()
                && (p_element == element || (is_type_var(element) && is_type_var(p_element)))
        }
    }
}
