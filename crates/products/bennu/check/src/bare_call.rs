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
//!     could declare the overload that makes the call legal;
//!   * a **member-generating annotation** on the top type ([`crate::nodes::has_generated_members`]) —
//!     under Lombok the legal `getName()` is declared nowhere we can read;
//!   * an `import static X.*;` whose owner `X` is un-indexed — it can supply ANY name with any
//!     signature.
//!
//! PER-SITE guards (any → skip that call):
//!   * it must be a `method_invocation` with no `object` field;
//!   * it must sit directly in the top type, crossing no lambda and no nested / anonymous / local
//!     class body ([`crate::scopes::scope_is_directly_top`]) — each of those can declare methods of
//!     its own that the top type's hierarchy knows nothing about;
//!   * its name must not be one an `import static` supplies, nor one of `java.lang.Object`'s, nor an
//!     `enum`'s compiler-generated `values` / `valueOf` — for all of those the binding exists but its
//!     signature is not something we can enumerate here.
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

use bennu_java::prelude::{static_import_targets, FileSymbols, Member, TypeResolver};
use tree_sitter::Node;

use crate::nodes::has_generated_members;
use crate::resolve::type_binary;
use crate::scopes::{scope_is_directly_top, single_top_level_type};
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
    if has_generated_members(top.node, bytes) {
        return None;
    }
    let top_binary = type_binary(&top.decl_name, symbols, resolver)?;
    if !hierarchy_fully_known(resolver, &top_binary) {
        return None;
    }

    let mut static_names: HashSet<String> = HashSet::new();
    for t in static_import_targets(&symbols.imports) {
        match t.member {
            Some(m) => {
                static_names.insert(m);
            }
            None => {
                // A wildcard whose owner we cannot read could supply ANY name → judge nothing.
                if !hierarchy_fully_known(resolver, &t.owner_binary) {
                    return None;
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
        static_names,
        file_sigs,
    })
}

impl<'t> BareCalls<'t> {
    /// The method name this call names, when it is a bare call in a position we may judge.
    pub(crate) fn judgeable<'a>(&self, call: Node, bytes: &'a [u8]) -> Option<&'a str> {
        if call.child_by_field_name("object").is_some() {
            return None;
        }
        let name_node = call.child_by_field_name("name")?;
        let args = call.child_by_field_name("arguments")?;
        if name_node.has_error() || args.has_error() {
            return None;
        }
        if !scope_is_directly_top(call, self.top_node) {
            return None;
        }
        let name = name_node.utf8_text(bytes).ok()?;
        if self.static_names.contains(name) || OBJECT_METHODS.contains(&name) {
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

    /// Whether `candidates` (what the index knows) already covers every signature the FILE declares
    /// under `name`, parameter type for parameter type.
    ///
    /// This is the gate for any judgement that needs the overload set to be **exact** rather than
    /// merely non-empty — argument types, where committing to a lone candidate is the whole method.
    /// Matching on arity alone is not enough and was the first thing tried: a buffer that adds
    /// `own(String)` beside an indexed `own(int)` has an arity-1 candidate either way, so the stale
    /// single candidate stood, and a legal `own("x")` came out as a wrong argument type. Anything
    /// that will not resolve is a reason to say no, not to guess.
    pub(crate) fn index_covers_file_sigs(
        &self,
        name: &str,
        candidates: &[Member],
        symbols: &FileSymbols,
        resolver: &dyn TypeResolver,
    ) -> bool {
        for fs in self.file_sigs(name) {
            let mut binaries = Vec::with_capacity(fs.param_texts.len());
            for text in &fs.param_texts {
                // A `T...` parameter is an array of `T` once resolved — the form the index carries.
                let (text, varargs) = match text.trim().strip_suffix("...") {
                    Some(base) => (base.trim().to_string(), true),
                    None => (text.trim().to_string(), false),
                };
                let Some(mut binary) = type_binary(&text, symbols, resolver) else {
                    return false;
                };
                if varargs {
                    binary.push_str("[]");
                }
                binaries.push(binary);
            }
            let covered = candidates.iter().any(|m| {
                m.params.len() == binaries.len()
                    && m.params.iter().zip(&binaries).all(|(p, b)| p.binary_name == *b)
            });
            if !covered {
                return false;
            }
        }
        true
    }
}
