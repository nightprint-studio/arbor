//! The preconditions for judging a **bare** call `foo(a, b)` — one written with no receiver, so it
//! binds against `this` and everything the enclosing type inherits or imports statically.
//!
//! Both [`crate::arity`] and [`crate::arguments`] started life reading only `recv.method(…)`,
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
//!     ([`crate::nodes::generated_names`]) — under Lombok the legal `getName()` is declared nowhere
//!     we can read;
//!   * an `import static X.*;` whose owner `X` is un-indexed — it can supply ANY name with any
//!     signature;
//!   * a name an `import static` supplies.
//!
//! PER-SITE guards (any → skip that call):
//!   * it must be a `method_invocation` with no `object` field;
//!   * it must sit in the top type, crossing no nested / anonymous / local class body
//!     ([`crate::scopes::scope_is_top_across_lambdas`]) — each of those can declare methods of its
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

use crate::nodes::{generated_names, is_type_var};
use crate::resolve::type_binary;
use crate::scopes::{scope_is_top_across_lambdas, single_top_level_type};
use crate::walk::{for_each_supertype, hierarchy_fully_known};

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
    /// Every method the file declares, by name — the buffer's own answer, ahead of the index.
    file_sigs: HashMap<String, Vec<FileSig>>,
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
    for t in static_import_targets(&symbols.imports) {
        match t.member {
            Some(m) => {
                static_names.insert(m);
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
            }
        }
    }

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
        file_sigs,
    })
}

impl<'t> BareCalls<'t> {
    /// The method name a bare call names, for a check that asks WHICH names bind or how many
    /// overloads a name has (arity, `throws_of`): every guard applies, the name-set ones included.
    pub(crate) fn judgeable_across_lambdas<'a>(
        &self,
        call: Node,
        bytes: &'a [u8],
    ) -> Option<&'a str> {
        if self.generates_methods || self.unreadable_static_wildcard {
            return None;
        }
        let name = self.judgeable_site(call, bytes)?;
        (!self.static_names.contains(name)).then_some(name)
    }

    /// The method name a bare call names, for a check that goes on only with a NON-EMPTY member
    /// overload set of that name on the top type (argument types). Such a set shadows every static
    /// import of the name, and Lombok never generates beside it at an arity it already has — see the
    /// module doc — so the name-set guards do not apply. The caller must not act on an empty set.
    pub(crate) fn judgeable_member_call<'a>(&self, call: Node, bytes: &'a [u8]) -> Option<&'a str> {
        self.judgeable_site(call, bytes)
    }

    /// The per-site guards shared by both entry points.
    fn judgeable_site<'a>(&self, call: Node, bytes: &'a [u8]) -> Option<&'a str> {
        if call.child_by_field_name("object").is_some() {
            return None;
        }
        let name_node = call.child_by_field_name("name")?;
        let args = call.child_by_field_name("arguments")?;
        if name_node.has_error() || args.has_error() {
            return None;
        }
        if !scope_is_top_across_lambdas(call, self.top_node) {
            return None;
        }
        let name = name_node.utf8_text(bytes).ok()?;
        if OBJECT_METHODS.contains(&name) {
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

/// Whether `candidates` (what the index knows) already covers every signature the FILE declares
/// under `name` — on any of its types, constructors as `<init>` — parameter type for parameter type.
///
/// This is the gate for any judgement that needs the overload set to be **exact** rather than
/// merely non-empty — argument types, where committing to a lone candidate is the whole method.
/// Matching on arity alone is not enough and was the first thing tried: a buffer that adds
/// `own(String)` beside an indexed `own(int)` has an arity-1 candidate either way, so the stale
/// single candidate stood, and a legal `own("x")` came out as a wrong argument type. Anything
/// that will not resolve is a reason to say no, not to guess. Signatures of every type in the file
/// are gathered, nested ones included: an extra one can only make the answer "no".
///
/// Two shapes of parameter text are matched structurally rather than by resolved name, because
/// neither HAS a resolved name to compare and both used to make every own method carrying one
/// uncoverable — and so every call to it unjudged: a type variable (`<T> void put(T value)`),
/// matched against an index parameter that is an unresolved variable too; and an array or
/// varargs parameter, whose depth the index keeps beside the name (`dims`) and the text spells
/// with brackets.
pub(crate) fn index_covers_file_sigs(
    name: &str,
    candidates: &[Member],
    symbols: &FileSymbols,
    resolver: &dyn TypeResolver,
) -> bool {
    let declared = symbols.types.iter().flat_map(|t| &t.methods).filter(|m| m.name == name);
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
