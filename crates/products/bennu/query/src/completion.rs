//! The member-access completion query (the SEAM's `completion(pos)`):
//!
//!   infer_receiver_type(source, dot_offset, resolver)
//!     → members_of(type.binary_name)  (walking superclass + interfaces)
//!     → filter by the typed prefix
//!     → Vec<CompletionItem { label, kind, detail }>
//!
//! Returns the wire [`CompletionItem`] the provider forwards unchanged.

use std::collections::HashSet;

use bennu_classpath::prelude::MemberIndex as CpMemberIndex;
use bennu_java::prelude::{
    enclosing_type_binary, extract_symbols, infer_receiver_type, ClassMembers, Member, MemberKind,
    TypeRef, TypeResolver, Visibility,
};
use bennu_proto::prelude::CompletionItem;

use crate::resolver::IndexResolver;

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

    let recv = match infer_receiver_type(&repaired, dot_offset, resolver) {
        Some(r) => r,
        // A **type** receiver — `Color.RED`, `Files.copy(…)`, `Config.MAX`. Inference types
        // expressions, and a type name is not one, so it answered nothing and every static access
        // completed to an empty list. Resolving the written name AS a type is the other half of
        // the same question, and the one `refs` already asks on the go-to path.
        None => match type_receiver(&repaired, dot_offset, resolver) {
            Some(r) => r,
            None => return Vec::new(),
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
    let sited = format!("{}{SITE_PLACEHOLDER}{}", &source[..byte_offset], &source[byte_offset..]);
    let site = enclosing_type_binary(&sited, byte_offset);

    let mut out = Vec::new();
    let mut seen = HashSet::new();
    collect_members(resolver, &recv, &prefix, site.as_deref(), &mut out, &mut seen, &mut HashSet::new());
    // Deterministic order: fields then methods (kind tag), alphabetical within.
    out.sort_by(|a, b| a.kind.cmp(&b.kind).then(a.label.cmp(&b.label)));
    out
}

/// Whether a `private` member declared in `declaring` is accessible from within `site`: true iff
/// they belong to the same top-level class — equal, or one nested in the other (its binary is the
/// other's with a `/`-boundary suffix). Package vs nesting is `/`-ambiguous in a binary, but two
/// *top-level* classes never prefix each other at a `/` boundary, so this only ever matches a real
/// same-class / nesting relationship.
fn same_top_level(declaring: &str, site: Option<&str>) -> bool {
    let Some(site) = site else { return false };
    declaring == site
        || site.starts_with(&format!("{declaring}/"))
        || declaring.starts_with(&format!("{site}/"))
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
    let bytes = source.as_bytes();
    // Back over `Foo`, `a.b.Foo` — but not over a `)` or a `]`, which mean the receiver was an
    // expression that inference already failed to type. Guessing a type from one of those would
    // complete the wrong thing rather than nothing.
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
    if name.contains('.') {
        // Already qualified: it names a type exactly when the classpath holds one.
        let binary = name.replace('.', "/");
        return resolver.members_of(&binary).is_some().then(|| TypeRef::simple(binary));
    }
    let imports = extract_symbols(source).imports;
    resolver.resolve_simple_name(name, &imports).map(TypeRef::simple)
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
/// overloads distinct, so an overload set is offered one entry per signature.
fn collect_members<M: CpMemberIndex>(
    resolver: &IndexResolver<M>,
    recv: &TypeRef,
    prefix: &str,
    site: Option<&str>,
    out: &mut Vec<CompletionItem>,
    seen: &mut HashSet<String>,
    visited: &mut HashSet<String>,
) {
    let bn = &recv.binary_name;
    if !visited.insert(bn.clone()) {
        return;
    }
    let Some(cm) = resolver.members_of(bn) else {
        return;
    };
    // `private` members of this level are offered only when the caret's class is the same top-level
    // class (a private is never inherited, so a supertype level's privates are simply never shown).
    let allow_private = same_top_level(bn, site);
    add_matching(&cm, prefix, allow_private, out, seen);

    if let Some(sc) = &cm.superclass {
        collect_members(resolver, &TypeRef::simple(sc.clone()), prefix, site, out, seen, visited);
    }
    for iface in &cm.interfaces {
        collect_members(resolver, &TypeRef::simple(iface.clone()), prefix, site, out, seen, visited);
    }
}

fn add_matching(
    cm: &ClassMembers,
    prefix: &str,
    allow_private: bool,
    out: &mut Vec<CompletionItem>,
    seen: &mut HashSet<String>,
) {
    for m in cm.methods.iter().chain(cm.fields.iter()) {
        if !m.name.starts_with(prefix) {
            continue;
        }
        // Hide a private member from an external / cross-class receiver (the common case: a field
        // of another object). Protected/package stay visible — hiding them would need package +
        // subclass context and risks dropping genuinely-accessible members.
        if m.visibility == Visibility::Private && !allow_private {
            continue;
        }
        if !seen.insert(dedup_key(m)) {
            continue;
        }
        out.push(CompletionItem {
            label: m.name.clone(),
            kind: kind_tag(m.kind).to_string(),
            detail: Some(render_detail(m)),
            auto_import: None, // a member has no import to add
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
            format!("{}({}) : {}", m.name, params.join(", "), render_type(&m.return_type))
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
