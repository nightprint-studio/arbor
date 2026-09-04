//! The member-access completion query (the SEAM's `completion(pos)`):
//!
//!   infer_receiver_type(source, dot_offset, resolver)
//!     → members_of(type.binary_name)  (walking superclass + interfaces)
//!     → filter by the typed prefix
//!     → Vec<CompletionItem { label, kind, detail }>
//!
//! Returns the wire [`CompletionItem`] the provider forwards unchanged.

use std::collections::{HashMap, HashSet};

use bennu_classpath::prelude::MemberIndex as CpMemberIndex;
use bennu_java::prelude::{
    enclosing_type_binary, extract_symbols, infer_receiver_type, ClassMembers, Member, MemberKind,
    TypeRef, TypeResolver, Visibility,
};
use bennu_proto::prelude::CompletionItem;

use crate::access::{same_package, same_top_level};
use crate::rank;
use crate::resolver::IndexResolver;

/// Simple type name → the binary names on the classpath that could be it.
///
/// **Completion only.** A type that is not imported is not in scope, and every other consumer must
/// go on saying exactly that — an unresolved name is an error the validator has to report, and a
/// resolver that guessed one would report nothing. But a receiver you are completing is one you are
/// in the middle of writing: `Arrays.` above a file with no `import java.util.Arrays;` is not a
/// mistake, it is the moment before the import exists. Answering nothing there is the same answer a
/// typo gets, and it is worse than unhelpful — the completion is the gesture that would have ADDED
/// the import, so refusing it leaves no way to reach the state where it would have worked.
///
/// Only an unambiguous name is taken. `List` names two importable classes and picking one would be
/// choosing the user's program for them; `Arrays` names one.
pub trait TypeNameCatalog {
    /// The importable binary names (`java/util/Arrays`) for a simple name, or empty.
    fn candidates(&self, simple: &str) -> Vec<String>;

    /// The types nested directly inside `binary`, as binary names — the LIBRARY half of the
    /// question [`IndexResolver::nested_types`](crate::resolver::IndexResolver) answers for the
    /// project. Default empty, so a catalog that cannot answer says "none, or not read".
    ///
    /// A nested type of a dependency is reached exactly as one of your own is — `AddHeader.Kind`
    /// — and the project index knows nothing about a class in a jar. Without this, `AddHeader.`
    /// offered a nested type when you had written the annotation and nothing when you had
    /// imported it.
    fn nested_types(&self, _binary: &str) -> Vec<String> {
        Vec::new()
    }
}

/// The identifier spliced in at the caret to make a `receiver.` buffer parse while the enclosing
/// type is read off it. Its name never reaches an answer — only the type declaration around it
/// does — so anything that lexes as a Java identifier would do.
const SITE_PLACEHOLDER: &str = "x";

/// Compute member-access completions at `byte_offset` in `source`.
///
/// The caret is expected to sit after a `receiver.` (optionally with a partial prefix
/// already typed, `receiver.ge|`). Returns `[]` when the receiver type can't be
/// inferred — a normal, non-fatal state (the FE shows nothing gracefully).
pub fn completion<M: CpMemberIndex>(
    source: &str,
    byte_offset: usize,
    resolver: &IndexResolver<M>,
) -> Vec<CompletionItem> {
    completion_in(source, byte_offset, resolver, None)
}

/// [`completion`], with the classpath's type-name catalog — see [`TypeNameCatalog`] for why only
/// this entry point gets one. `None` behaves exactly like [`completion`].
pub fn completion_in<M: CpMemberIndex>(
    source: &str,
    byte_offset: usize,
    resolver: &IndexResolver<M>,
    catalog: Option<&dyn TypeNameCatalog>,
) -> Vec<CompletionItem> {
    // Guard the caret before any `&source[..]` slicing below: a stale/out-of-range offset, or one
    // that (defensively) isn't a char boundary, would panic. Clamp to len, then back off to the
    // preceding boundary.
    let mut byte_offset = byte_offset.min(source.len());
    while byte_offset > 0 && !source.is_char_boundary(byte_offset) {
        byte_offset -= 1;
    }
    let (dot_offset, prefix) = split_prefix(source, byte_offset);

    // `infer_receiver_type` wants the caret immediately after the `.` (it splices a
    // parse-repair stub only when the byte there is whitespace/`}`/`)`/`;`). With a
    // partial prefix already typed (`s.to|`), the byte after the dot is `t`, so the
    // stub never fires and the receiver mis-parses. Excise the typed prefix first so
    // the receiver ends cleanly at the dot — the empty-prefix case inference expects.
    let repaired: String = if prefix.is_empty() {
        source.to_string()
    } else {
        let mut s = String::with_capacity(source.len().saturating_sub(prefix.len()));
        s.push_str(&source[..dot_offset]);
        s.push_str(&source[byte_offset..]);
        s
    };

    // Whether the receiver names a TYPE rather than a value — the ranking's strongest term, since
    // after `Color.` an instance member is not merely unlikely, it does not compile.
    let mut receiver_is_type = false;
    // Set when the receiver was found ONLY through the catalog — i.e. it is not in scope yet. Every
    // item then carries it, so accepting any member adds the receiver's import in the same gesture.
    let mut needs_import: Option<String> = None;
    let recv = match infer_receiver_type(&repaired, dot_offset, resolver) {
        Some(r) => r,
        // A **type** receiver — `Color.RED`, `Files.copy(…)`, `Config.MAX`. Inference types
        // expressions, and a type name is not one, so it answered nothing and every static access
        // completed to an empty list. Resolving the written name AS a type is the other half of
        // the same question, and the one `refs` already asks on the go-to path.
        None => match type_receiver(&repaired, dot_offset, resolver) {
            Some(r) => {
                receiver_is_type = true;
                r
            }
            None => match unimported_type_receiver(&repaired, dot_offset, resolver, catalog) {
                Some((r, fqn)) => {
                    receiver_is_type = true;
                    needs_import = Some(fqn);
                    r
                }
                None => return Vec::new(),
            },
        },
    };

    // The class the caret sits inside. A `private` member is offered only when its declaring type
    // shares this top-level class (JLS §6.6.1); `None` (caret outside any type) → no private is
    // accessible.
    //
    // Asked of a buffer with a placeholder identifier spliced in AT the caret. `receiver.` is a
    // syntax error, and tree-sitter's recovery for one can swallow the enclosing class whole —
    // `return this.` leaves a parse with no `class_declaration` in it at all. So the site came
    // back `None` in the one state completion ever runs in, and a class could not see its own
    // private members. `receiver.x` parses, and nothing before the caret moves.
    //
    // A SEPARATE repair from `repaired`, deliberately: `infer_receiver_type` splices its own stub
    // only when the byte after the dot is whitespace or a closer, and handing it this buffer
    // suppresses that — the receiver then mis-parses and every completion goes empty.
    let sited = format!(
        "{}{SITE_PLACEHOLDER}{}",
        &source[..byte_offset],
        &source[byte_offset..]
    );
    let site = enclosing_type_binary(&sited, byte_offset);

    let mut out = Vec::new();
    let mut seen = HashSet::new();
    let ctx = rank::Context::new(source, receiver_is_type);
    collect_members(resolver, &recv, &prefix, site.as_deref(), &ctx, &mut out, &mut seen);
    // A nested type is a member of its outer, named `Outer.Inner` with no import — so `Outer.`
    // offers it alongside the statics. Only when the receiver IS a type: `instance.Inner` is not
    // Java. The resolver answers for PROJECT types (the index keys them by binary name); a library
    // type reports none, which the seam reads as "not read" rather than "declares none".
    if receiver_is_type {
        collect_nested_types(resolver, catalog, &recv.binary_name, &prefix, &ctx, &mut out, &mut seen);
    }
    collapse_overloads(&mut out);
    // Most relevant first (see `rank`), and — because relevance ties are common and a popup that
    // reshuffles between keystrokes is unusable — the old deterministic order underneath it:
    // fields then methods, alphabetical within.
    out.sort_by(|a, b| {
        b.score
            .cmp(&a.score)
            .then(a.item.kind.cmp(&b.item.kind))
            .then(a.item.label.cmp(&b.item.label))
    });
    out.into_iter()
        .map(|r| match &needs_import {
            Some(fqn) => CompletionItem { auto_import: Some(fqn.clone()), ..r.item },
            None => r.item,
        })
        .collect()
}

/// The receiver read as a type name the file has NOT imported: `Arrays.` with no
/// `import java.util.Arrays;`. Returns the type and the FQN to import on accept.
///
/// Asked only after both inference and the in-scope type reading have declined, so a name that IS
/// in scope never reaches here and nothing already resolvable changes meaning.
fn unimported_type_receiver<M: CpMemberIndex>(
    source: &str,
    dot_offset: usize,
    resolver: &IndexResolver<M>,
    catalog: Option<&dyn TypeNameCatalog>,
) -> Option<(TypeRef, String)> {
    let catalog = catalog?;
    let name = written_receiver_name(source, dot_offset)?;
    // A qualified name is already unambiguous and needs no import — `type_receiver` handles it, and
    // if it declined, the type is simply not on the classpath.
    if name.contains('.') {
        return None;
    }
    let candidates = catalog.candidates(&name);
    let [only] = candidates.as_slice() else { return None };
    let binary = only.replace('.', "/");
    resolver.members_of(&binary)?;
    Some((TypeRef::simple(binary), only.replace('/', ".")))
}

/// Offer the types nested directly inside `owner` — `Outer.Inner`, which is a member access like
/// any other and was the one kind of member the walk never listed.
#[allow(clippy::too_many_arguments)]
fn collect_nested_types<M: CpMemberIndex>(
    resolver: &IndexResolver<M>,
    catalog: Option<&dyn TypeNameCatalog>,
    owner: &str,
    prefix: &str,
    ctx: &rank::Context,
    out: &mut Vec<Ranked>,
    seen: &mut HashSet<String>,
) {
    // The project tier answers from the index, the classpath tier from the name enumeration. Both,
    // because a receiver is a project type or a library one and the caller does not know which.
    let library = catalog.map(|c| c.nested_types(owner)).unwrap_or_default();
    for binary in resolver.nested_types(owner).into_iter().chain(library) {
        let Some(simple) = binary.rsplit(['/', '$']).next() else { continue };
        if !simple.starts_with(prefix) || !seen.insert(format!("type:{simple}")) {
            continue;
        }
        let item = CompletionItem {
            label: simple.to_string(),
            kind: "class".to_string(),
            detail: Some(binary.replace('/', ".")),
            ..Default::default()
        };
        let score = ctx.score_nested_type(simple);
        out.push(Ranked { score, item });
    }
}

/// Fold a method's overloads into ONE row, counted in its detail.
///
/// They are collected separately — an override has to be told from an overload, and the parameters
/// are what tells them apart — but a *list* of them is a list of rows that all insert the same
/// text. Accepting a completion here writes the method's name; it does not write arguments, so
/// there is no sense in which you can pick "the `Integer` one". Three `fallback` rows are three
/// chances to choose and one outcome, and they push the members you were looking for off the popup.
///
/// Nothing is hidden by the fold: the detail says `+2 overloads`, and the **parameter hints** strip
/// shows the whole set the moment you type `(` — which is when knowing them starts to matter and
/// when the editor can show them properly, one at a time, with the argument you are on marked.
///
/// (This is why the fluent-accessor case the per-parameter dedup exists for is safe: a Lombok
/// `name()` and its `name(String)` still both reach here, and the row says there are two.)
fn collapse_overloads(out: &mut Vec<Ranked>) {
    let mut kept: Vec<Ranked> = Vec::with_capacity(out.len());
    // `(kind, label)` → where its row is in `kept`, and how many have folded into it so far.
    let mut at: HashMap<(String, String), (usize, usize)> = HashMap::new();
    for r in out.drain(..) {
        if r.item.kind != "method" {
            kept.push(r);
            continue;
        }
        let key = (r.item.kind.clone(), r.item.label.clone());
        match at.get_mut(&key) {
            Some((idx, extra)) => {
                *extra += 1;
                // The most relevant of the set is the one whose signature is shown — a deprecated
                // overload should not become the face of a method that also has a current one.
                if r.score > kept[*idx].score {
                    kept[*idx].score = r.score;
                    kept[*idx].item.detail = r.item.detail.clone();
                }
                let n = *extra;
                let base = kept[*idx]
                    .item
                    .detail
                    .as_deref()
                    .map(|d| d.split("  +").next().unwrap_or(d).to_string());
                kept[*idx].item.detail = Some(match base {
                    Some(d) => format!("{d}  +{n} overload{}", if n == 1 { "" } else { "s" }),
                    None => format!("+{n} overload{}", if n == 1 { "" } else { "s" }),
                });
            }
            None => {
                at.insert(key, (kept.len(), 0));
                kept.push(r);
            }
        }
    }
    *out = kept;
}

/// A candidate and how relevant it is here, before the sort turns the pair back into a list.
struct Ranked {
    score: i32,
    item: CompletionItem,
}

/// The receiver read as a TYPE name — the other half of "what is before this dot".
///
/// `Color.` and `color.` are the same shape and different programs: one names a type and offers
/// its constants and statics, the other is a variable. Inference answers the second; this answers
/// the first, and only after it has declined — so a name that is both stays a value, which is what
/// Java's own rule says.
///
/// `None` when the text before the dot is not a plain (possibly dotted) name, or when nothing on
/// the classpath is called that.
fn type_receiver<M: CpMemberIndex>(
    source: &str,
    dot_offset: usize,
    resolver: &IndexResolver<M>,
) -> Option<TypeRef> {
    let name = written_receiver_name(source, dot_offset)?;
    let name = name.as_str();
    if name.contains('.') {
        // Already qualified: it names a type exactly when the classpath holds one.
        let binary = name.replace('.', "/");
        return resolver
            .members_of(&binary)
            .is_some()
            .then(|| TypeRef::simple(binary));
    }
    let imports = extract_symbols(source).imports;
    resolver
        .resolve_simple_name(name, &imports)
        .map(TypeRef::simple)
}

/// The plain (possibly dotted) NAME written just left of the dot: `Foo`, `a.b.Foo`.
///
/// `None` when what is there is not a name — a `)` or a `]` means the receiver was an expression
/// that inference has already failed to type, and guessing a type from one of those would complete
/// the wrong thing rather than nothing.
fn written_receiver_name(source: &str, dot_offset: usize) -> Option<String> {
    let bytes = source.as_bytes();
    let mut start = dot_offset.checked_sub(1)?; // the dot itself
    while start > 0 {
        let c = bytes[start - 1];
        if c.is_ascii_alphanumeric() || c == b'_' || c == b'$' || c == b'.' {
            start -= 1;
        } else {
            break;
        }
    }
    let name = source.get(start..dot_offset - 1)?.trim();
    if name.is_empty() || !name.starts_with(|c: char| c.is_alphabetic() || c == '_') {
        return None;
    }
    Some(name.to_string())
}

/// Split the caret into `(dot_offset, typed_prefix)`: scan back over identifier chars;
/// `dot_offset` is just past the `.` (or, absent a receiver, the identifier start).
fn split_prefix(source: &str, caret: usize) -> (usize, String) {
    let bytes = source.as_bytes();
    let mut start = caret.min(source.len());
    while start > 0 {
        let c = bytes[start - 1];
        if c == b'_' || c.is_ascii_alphanumeric() {
            start -= 1;
        } else {
            break;
        }
    }
    (start, source[start..caret.min(source.len())].to_string())
}

/// Walk `recv`'s class + its superclass/interfaces, collecting members whose name
/// starts with `prefix`. Picks up inherited members; dedups overrides by [`dedup_key`] — which keeps
/// overloads distinct, so every signature survives the walk. They are folded into one row later, by
/// [`collapse_overloads`]; keeping them apart HERE is what lets that row count them.
///
/// The rank's depth term — how far up the hierarchy a member was declared, `0` for the receiver's
/// own type — comes from the walk, which is the only thing that knows it. It is what puts a class's
/// own methods above the ones it inherited.
#[allow(clippy::too_many_arguments)]
fn collect_members<M: CpMemberIndex>(
    resolver: &IndexResolver<M>,
    recv: &TypeRef,
    prefix: &str,
    site: Option<&str>,
    ctx: &rank::Context,
    out: &mut Vec<Ranked>,
    seen: &mut HashSet<String>,
) {
    // The shared supertype walk — breadth-first, so a member declared nearer the receiver reaches
    // `seen` before the one it hides, which is what the depth-based rank means to say.
    bennu_java::prelude::walk_up::<()>(resolver, recv, |a| {
        let bn = &a.ty.binary_name;
        // `private` members of this level are offered only when the caret's class is the same
        // top-level class (a private is never inherited, so a supertype level's privates are simply
        // never shown).
        let allow_private = same_top_level(bn, site);
        add_matching(&a.members, bn, prefix, allow_private, site, a.depth, ctx, out, seen);
        None
    });
}

#[allow(clippy::too_many_arguments)]
fn add_matching(
    cm: &ClassMembers,
    declaring: &str,
    prefix: &str,
    allow_private: bool,
    site: Option<&str>,
    depth: usize,
    ctx: &rank::Context,
    out: &mut Vec<Ranked>,
    seen: &mut HashSet<String>,
) {
    for m in cm.methods.iter().chain(cm.fields.iter()) {
        if !m.name.starts_with(prefix) {
            continue;
        }
        // A constructor and a static initialiser are members of the class file, not things you can
        // reach through a dot. `s.` used to open on eight `<init>` entries — they sort before every
        // letter, so they were the first thing the popup showed on any String.
        if m.name == "<init>" || m.name == "<clinit>" {
            continue;
        }
        // Hide a private member from an external / cross-class receiver (the common case: a field
        // of another object). Protected stays visible — hiding it would need subclass context and
        // risks dropping a genuinely-accessible member.
        if m.visibility == Visibility::Private && !allow_private {
            continue;
        }
        // Package-private is visible only from the same package, and the JDK's own internals are
        // full of it: `String.` opened on `COMPACT_STRINGS`, `LATIN1`, `UTF16` and
        // `checkBoundsBeginEnd` before it reached anything you could write. See `same_package` for
        // why this hides only when it is sure.
        if m.visibility == Visibility::Package && !same_package(declaring, site) {
            continue;
        }
        if !seen.insert(dedup_key(m)) {
            continue;
        }
        out.push(Ranked {
            score: rank::score(m, declaring, depth, ctx),
            item: CompletionItem {
                label: m.name.clone(),
                kind: kind_tag(m.kind).to_string(),
                detail: Some(render_detail(m)),
                auto_import: None, // a member has no import to add
                // Carried on the wire for whoever draws it (the Java popup does not yet); the
                // ranking is what puts it last today. One answer, asked once, so the two can
                // never disagree about which member is meant.
                deprecated: rank::is_deprecated(m),
                ..Default::default()
            },
        });
    }
}

/// The identity a member is deduplicated on while walking a hierarchy: kind, name **and parameter
/// types**.
///
/// The parameters are the load-bearing part. The dedup exists so an override doesn't appear twice —
/// once from the subclass, once from the supertype — and an override has the same parameter types as
/// the method it overrides *by definition*, so including them costs that nothing. Keying on the name
/// alone also collapsed every **overload**: `substring(int)` and `substring(int, int)` offered as one
/// entry, nine `valueOf`s as one, and — the way this surfaced — a Lombok `@Accessors(fluent = true)`
/// getter `name()` hiding its own setter `name(String)`, since fluent accessors share the field's
/// name and differ only in arity.
fn dedup_key(m: &Member) -> String {
    let mut key = String::with_capacity(32);
    key.push_str(kind_tag(m.kind));
    key.push('/');
    key.push_str(&m.name);
    for p in &m.params {
        key.push('(');
        key.push_str(&p.binary_name);
    }
    key
}

fn kind_tag(k: MemberKind) -> &'static str {
    match k {
        MemberKind::Method => "method",
        MemberKind::Field => "field",
    }
}

/// A readable signature line for the completion `detail`.
fn render_detail(m: &Member) -> String {
    match m.kind {
        MemberKind::Field => render_type(&m.return_type),
        MemberKind::Method => {
            let params: Vec<String> = m.params.iter().map(render_type).collect();
            format!(
                "{}({}) : {}",
                m.name,
                params.join(", "),
                render_type(&m.return_type)
            )
        }
    }
}

/// Render a `TypeRef` to a readable simple form: `java/util/List<Foo>` → `List<Foo>`.
fn render_type(t: &TypeRef) -> String {
    let simple = t.binary_name.rsplit('/').next().unwrap_or(&t.binary_name);
    if t.type_args.is_empty() {
        simple.to_string()
    } else {
        let args: Vec<String> = t.type_args.iter().map(render_type).collect();
        format!("{}<{}>", simple, args.join(", "))
    }
}


#[cfg(test)]
mod overload_collapse_tests {
    use super::*;

    fn item(kind: &str, label: &str, detail: &str, score: i32) -> Ranked {
        Ranked {
            score,
            item: CompletionItem {
                label: label.to_string(),
                kind: kind.to_string(),
                detail: Some(detail.to_string()),
                ..Default::default()
            },
        }
    }

    fn labels(v: &[Ranked]) -> Vec<String> {
        v.iter().map(|r| r.item.label.clone()).collect()
    }

    /// The reported shape: three rows that all insert `fallback`, becoming one that says so.
    #[test]
    fn overloads_of_one_method_become_one_row() {
        let mut v = vec![
            item("method", "fallback", "fallback() : Integer", 10),
            item("method", "fallback", "fallback(Integer) : void", 10),
            item("method", "fallback", "fallback(Object) : void", 10),
        ];
        collapse_overloads(&mut v);
        assert_eq!(labels(&v), vec!["fallback".to_string()]);
        assert_eq!(
            v[0].item.detail.as_deref(),
            Some("fallback() : Integer  +2 overloads")
        );
    }

    /// A single method keeps its detail untouched — no counter on something with nothing to count.
    #[test]
    fn a_lone_method_is_left_exactly_as_it_was() {
        let mut v = vec![item("method", "solo", "solo() : void", 5)];
        collapse_overloads(&mut v);
        assert_eq!(v[0].item.detail.as_deref(), Some("solo() : void"));
    }

    #[test]
    fn one_extra_overload_reads_singular() {
        let mut v = vec![
            item("method", "of", "of() : X", 1),
            item("method", "of", "of(int) : X", 1),
        ];
        collapse_overloads(&mut v);
        assert_eq!(v[0].item.detail.as_deref(), Some("of() : X  +1 overload"));
    }

    /// A field and a method of the same name are two different things you can write, so they stay
    /// two rows — the fold is keyed on kind as well as name.
    #[test]
    fn a_field_and_a_method_of_the_same_name_stay_apart() {
        let mut v = vec![
            item("field", "name", "String name", 3),
            item("method", "name", "name() : String", 3),
        ];
        collapse_overloads(&mut v);
        assert_eq!(v.len(), 2, "{:?}", labels(&v));
    }

    /// The row shows the signature of the most relevant overload, not of whichever came first.
    #[test]
    fn the_most_relevant_overload_supplies_the_signature() {
        let mut v = vec![
            item("method", "run", "run(Object) : void", 1),
            item("method", "run", "run() : void", 9),
        ];
        collapse_overloads(&mut v);
        assert_eq!(v[0].item.detail.as_deref(), Some("run() : void  +1 overload"));
        assert_eq!(v[0].score, 9, "and its score, so it ranks as the best of the set");
    }

    /// Different methods are not overloads of each other.
    #[test]
    fn different_names_are_not_folded() {
        let mut v = vec![
            item("method", "a", "a() : void", 1),
            item("method", "b", "b() : void", 1),
        ];
        collapse_overloads(&mut v);
        assert_eq!(labels(&v), vec!["a".to_string(), "b".to_string()]);
    }
}
