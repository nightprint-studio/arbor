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
use bennu_java::prelude::{infer_receiver_type, ClassMembers, Member, MemberKind, TypeRef, TypeResolver};
use bennu_proto::prelude::CompletionItem;

use crate::resolver::IndexResolver;

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

    let Some(recv) = infer_receiver_type(&repaired, dot_offset, resolver) else {
        return Vec::new();
    };

    let mut out = Vec::new();
    let mut seen = HashSet::new();
    collect_members(resolver, &recv, &prefix, &mut out, &mut seen, &mut HashSet::new());
    // Deterministic order: fields then methods (kind tag), alphabetical within.
    out.sort_by(|a, b| a.kind.cmp(&b.kind).then(a.label.cmp(&b.label)));
    out
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
/// starts with `prefix`. Picks up inherited members; dedups overrides by name+kind.
fn collect_members<M: CpMemberIndex>(
    resolver: &IndexResolver<M>,
    recv: &TypeRef,
    prefix: &str,
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
    add_matching(&cm, prefix, out, seen);

    if let Some(sc) = &cm.superclass {
        collect_members(resolver, &TypeRef::simple(sc.clone()), prefix, out, seen, visited);
    }
    for iface in &cm.interfaces {
        collect_members(resolver, &TypeRef::simple(iface.clone()), prefix, out, seen, visited);
    }
}

fn add_matching(
    cm: &ClassMembers,
    prefix: &str,
    out: &mut Vec<CompletionItem>,
    seen: &mut HashSet<String>,
) {
    for m in cm.methods.iter().chain(cm.fields.iter()) {
        if !m.name.starts_with(prefix) {
            continue;
        }
        let key = format!("{}/{}", kind_tag(m.kind), m.name);
        if !seen.insert(key) {
            continue;
        }
        out.push(CompletionItem {
            label: m.name.clone(),
            kind: kind_tag(m.kind).to_string(),
            detail: Some(render_detail(m)),
        });
    }
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
