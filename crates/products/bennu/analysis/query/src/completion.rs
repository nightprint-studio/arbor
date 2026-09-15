//! The member-access completion query (the SEAM's `completion(pos)`):
//!
//!   infer_receiver_type(source, dot_offset, resolver)
//!     → members_of(type.binary_name)  (walking superclass + interfaces)
//!     → filter by the typed prefix
//!     → Vec<CompletionItem { label, kind, detail }>
//!
//! Returns the wire [`CompletionItem`] the provider forwards unchanged.

use std::cell::OnceCell;
use std::collections::{HashMap, HashSet};

use bennu_classpath::prelude::MemberIndex as CpMemberIndex;
use bennu_java::prelude::{
    enclosing_type_binary, extract_symbols, infer_receiver_type, ClassMembers, Member, MemberKind,
    TypeRef, TypeResolver, Visibility,
};
use bennu_complete::prelude::{MatchCase, Typed};
use bennu_proto::prelude::{CompletionItem, MemberOrigin, SnippetStop};

use crate::access::{protected_visible, same_package, same_top_level};
use crate::member_text::{named_parameters, render_param, render_type, simple_of, split_top_level};
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
pub(crate) const SITE_PLACEHOLDER: &str = "x";

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
    completion_in(source, byte_offset, resolver, None, MatchCase::default())
}

/// [`completion`], with the classpath's type-name catalog — see [`TypeNameCatalog`] for why only
/// this entry point gets one. `None` behaves exactly like [`completion`].
///
/// `case` is the user's "match case" setting, which decides how strictly the typed letters have to
/// agree with the name's — see [`MatchCase`]. It is honoured HERE rather than by filtering the
/// answer afterwards: the humps and the case rule are one question, and a strict filter applied on
/// top of a lenient match is how `aah` stopped reaching `addAllowedHeader` the moment the setting
/// was turned on.
pub fn completion_in<M: CpMemberIndex>(
    source: &str,
    byte_offset: usize,
    resolver: &IndexResolver<M>,
    catalog: Option<&dyn TypeNameCatalog>,
    case: MatchCase,
) -> Vec<CompletionItem> {
    // Guard the caret before any `&source[..]` slicing below: a stale/out-of-range offset, or one
    // that (defensively) isn't a char boundary, would panic. Clamp to len, then back off to the
    // preceding boundary.
    let mut byte_offset = byte_offset.min(source.len());
    while byte_offset > 0 && !source.is_char_boundary(byte_offset) {
        byte_offset -= 1;
    }
    // `Type::|` is a member access too, and it used to reach none of this: the scan below stops at
    // the `:`, finds no receiver, and the popup fell through to answering a bare word — every local
    // and every class on the classpath, after a `::` where only a method of `Type` can be written.
    if let Some(items) = crate::method_reference::method_reference_completion(
        source,
        byte_offset,
        resolver,
        catalog,
        case,
    ) {
        return items;
    }
    let (dot_offset, prefix) = split_prefix(source, byte_offset);
    let typed = Typed::new(&prefix, case);

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
        // `recv.pre|(args)` — correcting the name of a call that is already written, which is one
        // of the ordinary reasons to open the popup at all. Excising the prefix outright leaves
        // `recv.(args)`, and that is not Java: the parse fails, the receiver is never inferred,
        // and completing an existing call came back with nothing whatsoever. A placeholder keeps
        // the call well-formed; the receiver being read is to the LEFT of the dot either way.
        if source[byte_offset..].starts_with('(') {
            s.push_str(SITE_PLACEHOLDER);
        }
        s.push_str(&source[byte_offset..]);
        s
    };

    let Some(Receiver { ty: recv, is_type: receiver_is_type, import: needs_import }) =
        resolve_receiver(&repaired, dot_offset, resolver, catalog)
    else {
        return Vec::new();
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
    // What the POSITION wants, read off the same repaired buffer the site came from — a caret in
    // the middle of `String s = order.|` sits inside a declarator whose type is three words to
    // the left, and no amount of looking at `order` finds it.
    let expected = bennu_java::prelude::expected_type(source, byte_offset, resolver);
    let ctx = rank::Context::new(source, receiver_is_type).expecting(expected);
    collect_members(resolver, &recv, typed, site.as_deref(), false, &ctx, &mut out, &mut seen);
    // A nested type is a member of its outer, named `Outer.Inner` with no import — so `Outer.`
    // offers it alongside the statics. Only when the receiver IS a type: `instance.Inner` is not
    // Java. The resolver answers for PROJECT types (the index keys them by binary name); a library
    // type reports none, which the seam reads as "not read" rather than "declares none".
    if receiver_is_type {
        collect_nested_types(resolver, catalog, &recv.binary_name, typed, &ctx, &mut out, &mut seen);
    }
    // Overloads stay separate rows here: each shows its own parameters, and accepting `wait()` or
    // `wait(long)` leaves the caret in different places. Only a method reference folds them — see
    // `collapse_overloads`.
    sort_members(&mut out);
    preselect_the_only_exact_fit(&mut out);
    if prefix.is_empty() {
        preselect_the_first_own_member(&mut out);
    }
    let mut items: Vec<CompletionItem> = out
        .into_iter()
        .map(|r| match &needs_import {
            Some(fqn) => CompletionItem { auto_import: Some(fqn.clone()), ..r.item },
            None => r.item,
        })
        .collect();
    drop_call_syntax_if_written(&mut items, source, byte_offset);
    items
}

/// What stands left of a member access, read three ways in Java's own order.
pub(crate) struct Receiver {
    /// The type whose members are offered.
    pub(crate) ty: TypeRef,
    /// Whether the receiver names a TYPE rather than a value — the ranking's strongest term, since
    /// after `Color.` an instance member is not merely unlikely, it does not compile.
    pub(crate) is_type: bool,
    /// Set when the receiver was found ONLY through the catalog — i.e. it is not in scope yet. Every
    /// item then carries it, so accepting any member adds the receiver's import in the same gesture.
    pub(crate) import: Option<String>,
}

/// Read the receiver whose member access ends at `dot_offset` in `repaired` — the one question the
/// `.` and the `::` completions share, answered once.
///
/// A value first; then a **type** receiver — `Color.RED`, `Files.copy(…)`, `Config.MAX`. Inference
/// types expressions, and a type name is not one, so it answered nothing and every static access
/// completed to an empty list. Resolving the written name AS a type is the other half of the same
/// question, and the one `refs` already asks on the go-to path. Last, a type the file has not
/// imported yet.
pub(crate) fn resolve_receiver<M: CpMemberIndex>(
    repaired: &str,
    dot_offset: usize,
    resolver: &IndexResolver<M>,
    catalog: Option<&dyn TypeNameCatalog>,
) -> Option<Receiver> {
    if let Some(ty) = infer_receiver_type(repaired, dot_offset, resolver) {
        return Some(Receiver { ty, is_type: false, import: None });
    }
    if let Some(ty) = type_receiver(repaired, dot_offset, resolver) {
        return Some(Receiver { ty, is_type: true, import: None });
    }
    let (ty, fqn) = unimported_type_receiver(repaired, dot_offset, resolver, catalog)?;
    Some(Receiver { ty, is_type: true, import: Some(fqn) })
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
    typed: Typed<'_>,
    ctx: &rank::Context,
    out: &mut Vec<Ranked>,
    seen: &mut HashSet<String>,
) {
    // The project tier answers from the index, the classpath tier from the name enumeration. Both,
    // because a receiver is a project type or a library one and the caller does not know which.
    let library = catalog.map(|c| c.nested_types(owner)).unwrap_or_default();
    for binary in resolver.nested_types(owner).into_iter().chain(library) {
        let Some(simple) = binary.rsplit(['/', '$']).next() else { continue };
        if !typed.matches(simple) || !seen.insert(format!("type:{simple}")) {
            continue;
        }
        let item = CompletionItem {
            label: simple.to_string(),
            kind: "class".to_string(),
            detail: Some(binary.replace('/', ".")),
            owner: Some(binary.clone()),
            ..Default::default()
        };
        let score = ctx.score_nested_type(simple);
        // A type name is not a value, so it produces nothing a position could want.
        out.push(Ranked { score, fit: rank::Fit::None, tier: typed.tier(simple).unwrap_or(0), item });
    }
}

/// Fold a method's overloads into ONE row, counted in its detail.
///
/// **Method references only.** After `::` accepting a row writes the bare name, so three overloads
/// are three rows with one outcome. After a `.` it is different — the call's parentheses and the
/// caret inside them depend on the overload, and each row shows its own parameter list — so the
/// member list keeps them apart, as IntelliJ does.
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
pub(crate) fn collapse_overloads(out: &mut Vec<Ranked>) {
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
                // The row fits if any overload does: accepting it writes the name, and the
                // parameter hints then offer the overload that returns what is wanted.
                kept[*idx].fit = kept[*idx].fit.max(r.fit);
                // The most relevant of the set is the one whose signature is shown — a deprecated
                // overload should not become the face of a method that also has a current one.
                if r.score > kept[*idx].score {
                    kept[*idx].score = r.score;
                    kept[*idx].item.detail = r.item.detail.clone();
                    kept[*idx].item.signature = r.item.signature.clone();
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
pub(crate) struct Ranked {
    pub(crate) score: i32,
    /// Whether it produces what the position wants — the key ordered BEFORE `score`. See
    /// [`rank::Fit`] for why it is not one more term in it.
    pub(crate) fit: rank::Fit,
    /// How the typed letters reached the name (`0` = an exact prefix) — the key a member list
    /// orders by right after [`Ranked::fit`]. See [`sort_members`].
    pub(crate) tier: u8,
    pub(crate) item: CompletionItem,
}

/// Order a candidate list: what fits the position first, the most relevant first within that, and
/// — because relevance ties are common and a popup that reshuffles between keystrokes is unusable —
/// fields then methods, alphabetical, underneath it all.
///
/// One sort for every list built out of [`Ranked`], so a member after a dot, a bare name and a
/// method reference cannot disagree about what "first" means.
pub(crate) fn sort_ranked(out: &mut [Ranked]) {
    out.sort_by(|a, b| {
        b.fit
            .cmp(&a.fit)
            .then(b.score.cmp(&a.score))
            .then(a.item.kind.cmp(&b.item.kind))
            .then(a.item.label.cmp(&b.item.label))
    });
}

/// Mark the row `preselect` when it is the ONLY one producing exactly the expected type.
///
/// Only then: `return builder.|` has one member returning `Order`, and that is an answer. Two
/// locals of the right type are a choice, and preselecting either would make it for the user —
/// the fit ordering already puts both on top.
pub(crate) fn preselect_the_only_exact_fit(out: &mut [Ranked]) {
    let mut exact = out.iter_mut().filter(|r| r.fit == rank::Fit::Exact);
    if let (Some(only), None) = (exact.next(), exact.next()) {
        only.item.preselect = true;
    }
}

/// Order a MEMBER list — what follows a `.` — IntelliJ's way: what fits the position, then how well
/// the typed letters matched, then **where the member stands** ([`MemberOrigin`]), then relevance.
///
/// The origin is a key and not a term of the score on purpose. After `route.` the members
/// `ServiceRoute` declares are what is reached for nine times in ten; as a weighted term, a use count
/// or a habit could still lift `hashCode` above them, and a record's implicit `equals` sorted between
/// its components alphabetically. Within one origin the score still decides — an inherited member of
/// the nearer supertype first, statics after instance members on a value.
///
/// An item that is not a member (a nested type) ranks with the receiver's own.
pub(crate) fn sort_members(out: &mut [Ranked]) {
    let origin = |r: &Ranked| r.item.member_origin.unwrap_or(MemberOrigin::Own);
    out.sort_by(|a, b| {
        b.fit
            .cmp(&a.fit)
            .then(a.tier.cmp(&b.tier))
            .then(origin(b).cmp(&origin(a)))
            .then(b.score.cmp(&a.score))
            .then(a.item.kind.cmp(&b.item.kind))
            .then(a.item.label.cmp(&b.item.label))
            // Overloads of one name: fewest parameters first, `wait()` before `wait(long)`.
            .then(arity(&a.item).cmp(&arity(&b.item)))
            .then(a.item.signature.cmp(&b.item.signature))
    });
}

/// How many parameters a row's [`CompletionItem::signature`] lists — `0` for `()`, and for a row
/// that has no signature at all.
fn arity(item: &CompletionItem) -> usize {
    let inner = item
        .signature
        .as_deref()
        .map(|s| s.trim_start_matches('(').trim_end_matches(')').trim());
    match inner {
        Some(params) if !params.is_empty() => split_top_level(params).len(),
        _ => 0,
    }
}

/// Preselect the first row when it is a member the receiver itself declares and nothing else has
/// claimed the selection.
///
/// Only the FIRST row, and only an own one: a preselected row is lifted above everything in the
/// editor, so marking an own member further down would put it over an inherited one that fits the
/// position better. The caller asks only with nothing typed yet — once letters are typed, how well
/// they match is the editor's judgement to make, not a selection to pin.
pub(crate) fn preselect_the_first_own_member(out: &mut [Ranked]) {
    if out.iter().any(|r| r.item.preselect) {
        return;
    }
    if let Some(first) = out.first_mut() {
        if first.item.member_origin == Some(MemberOrigin::Own) {
            first.item.preselect = true;
        }
    }
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
pub fn split_prefix(source: &str, caret: usize) -> (usize, String) {
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
pub(crate) fn collect_members<M: CpMemberIndex>(
    resolver: &IndexResolver<M>,
    recv: &TypeRef,
    typed: Typed<'_>,
    site: Option<&str>,
    statics_only: bool,
    ctx: &rank::Context,
    out: &mut Vec<Ranked>,
    seen: &mut HashSet<String>,
) {
    let query = MemberQuery {
        resolver,
        receiver: recv,
        typed,
        site,
        statics_only,
        ctx,
        receiver_in_site: OnceCell::new(),
    };
    // The shared supertype walk — breadth-first, so a member declared nearer the receiver reaches
    // `seen` before the one it hides, which is what the depth-based rank means to say.
    bennu_java::prelude::walk_up::<()>(resolver, recv, |a| {
        query.add_matching(&a.members, &a.ty.binary_name, a.depth, out, seen);
        None
    });
}

/// One member walk's question, asked of every level of the hierarchy: what is being completed,
/// through what, from where.
struct MemberQuery<'a> {
    resolver: &'a dyn TypeResolver,
    receiver: &'a TypeRef,
    typed: Typed<'a>,
    site: Option<&'a str>,
    /// A bare name written in a static context — only statics can be meant.
    statics_only: bool,
    ctx: &'a rank::Context,
    /// Whether an instance `protected` member outside its package may be reached through this
    /// receiver from this site. The same for every such member, and a hierarchy walk to answer, so
    /// it is asked once — see [`protected_visible`].
    receiver_in_site: OnceCell<bool>,
}

impl MemberQuery<'_> {
    fn add_matching(
        &self,
        cm: &ClassMembers,
        declaring: &str,
        depth: usize,
        out: &mut Vec<Ranked>,
        seen: &mut HashSet<String>,
    ) {
        for m in cm.methods.iter().chain(cm.fields.iter()) {
            let Some(tier) = self.typed.tier(&m.name) else {
                continue;
            };
            // A constructor and a static initialiser are members of the class file, not things you
            // can reach through a dot. `s.` used to open on eight `<init>` entries — they sort before
            // every letter, so they were the first thing the popup showed on any String.
            if m.name == "<init>" || m.name == "<clinit>" {
                continue;
            }
            // A bare name written in a static context can only be a static member: `count` inside
            // `static void main` does not compile, however visible the field is. The receiver paths
            // never set this — through a receiver an instance member is exactly what is wanted.
            if self.statics_only && !m.is_static {
                continue;
            }
            if !self.visible(m, declaring) {
                continue;
            }
            if !seen.insert(dedup_key(m)) {
                continue;
            }
            out.push(self.ranked(m, declaring, depth, cm, tier));
        }
    }

    /// Whether `m`, declared in `declaring`, can be written here at all.
    fn visible(&self, m: &Member, declaring: &str) -> bool {
        match m.visibility {
            Visibility::Public => true,
            // Only from the same top-level class (a private is never inherited, so a supertype
            // level's privates are simply never shown).
            Visibility::Private => same_top_level(declaring, self.site),
            // Package-private is visible only from the same package, and the JDK's own internals
            // are full of it: `String.` opened on `COMPACT_STRINGS`, `LATIN1`, `UTF16` and
            // `checkBoundsBeginEnd` before it reached anything you could write. See `same_package`
            // for why this hides only when it is sure.
            Visibility::Package => same_package(declaring, self.site),
            // `route.clone()` and `route.finalize()` from another class are not Java, and every
            // receiver used to offer them from `Object`. The instance answer does not depend on
            // which protected member is asked about, so it is remembered for the walk.
            Visibility::Protected if m.is_static || same_package(declaring, self.site) => {
                protected_visible(self.resolver, declaring, self.receiver, m.is_static, self.site)
            }
            Visibility::Protected => *self.receiver_in_site.get_or_init(|| {
                protected_visible(self.resolver, declaring, self.receiver, false, self.site)
            }),
        }
    }

    fn ranked(&self, m: &Member, declaring: &str, depth: usize, cm: &ClassMembers, tier: u8) -> Ranked {
        let origin = rank::origin(m, declaring, depth, cm);
        // A record's implicit `equals` sits on the record itself; scored as the inherited member it
        // reads as, so the lists ordered by score alone (a bare name, a `::`) agree with this one.
        let depth = if origin == MemberOrigin::Inherited { depth.max(1) } else { depth };
        let (insert, stops) = call_syntax(m);
        // A `void` method produces nothing, and `Fit::None` is what the walk answers for it.
        let fit = if m.return_type.binary_name == "void" && m.return_type.dims == 0 {
            rank::Fit::None
        } else {
            self.ctx.fit(&m.return_type, self.resolver)
        };
        Ranked {
            score: rank::score(m, declaring, depth, self.ctx) - rank::tier_penalty(tier),
            fit,
            tier,
            item: CompletionItem {
                label: m.name.clone(),
                kind: kind_tag(m.kind).to_string(),
                detail: Some(self.detail(m, declaring)),
                signature: parameter_list(m),
                insert_text: insert,
                snippet_stops: stops,
                auto_import: None, // a member has no import to add
                // Drawn struck through, and ranked last by the same answer — one question, asked
                // once, so the two can never disagree about which member is meant.
                deprecated: rank::is_deprecated(m),
                owner: Some(declaring.to_string()),
                member_origin: Some(origin),
                modifiers: modifiers_of(m),
                ..Default::default()
            },
        }
    }

    /// The right-hand column of a member row — [`render_detail`], except for the one member whose
    /// declared type is a lie at every call site.
    ///
    /// `Object.getClass()` is declared `Class<?>` and typed by the compiler as
    /// `Class<? extends |T|>`, the erasure of the receiver's static type. That is what IntelliJ
    /// shows, and it is free to show: the receiver is right here.
    fn detail(&self, m: &Member, declaring: &str) -> String {
        if declaring == rank::OBJECT && m.name == "getClass" && m.params.is_empty() {
            let dims = "[]".repeat(usize::from(self.receiver.dims));
            return format!("Class<? extends {}{dims}>", simple_of(&self.receiver.binary_name));
        }
        render_detail(m)
    }
}

/// What accepting a method actually writes: `name()`, with the caret between the parentheses when
/// there is something to pass.
///
/// A method is a call, and every completion that inserted only its name left the user to type the
/// two characters that make it one — every time, on every method, which is most of what typing in
/// an IDE is. The parentheses are the IntelliJ shape and not the LSP one on purpose: a template
/// that fills in `name(${1:arg0}, ${2:arg1})` writes parameter names a class file does not carry,
/// and tabbing through placeholders to delete them is slower than typing the argument.
///
/// A field gets nothing — `(None, empty)` means "insert the label", which is what it was.
fn call_syntax(m: &Member) -> (Option<String>, Vec<SnippetStop>) {
    if m.kind != MemberKind::Method {
        return (None, Vec::new());
    }
    let insert = format!("{}()", m.name);
    // One stop, between the parens, and only when there is an argument to write there. A no-arg
    // call is finished the moment it lands, and a stop inside `()` would park the caret where
    // nothing may be typed.
    let stops = if m.params.is_empty() {
        Vec::new()
    } else {
        let at = m.name.len() + 1;
        vec![SnippetStop { start: at, end: at, group: 0 }]
    };
    (Some(insert), stops)
}

/// Undo [`call_syntax`] for the candidates at a caret that **already has** a call around it.
///
/// `list.ad|()` and `list.ad|(x, y)` are the ordinary shape of adding an argument to a call you
/// already wrote, or of correcting the name of one. Inserting `add()` there produces `add()()`,
/// which is the completion breaking working code — so the parentheses come off, and the item goes
/// back to inserting its own name.
///
/// The test is the buffer to the right of the identifier being replaced, not of the caret: with
/// `ad|d()` the caret is mid-word and the `(` is three characters further on. And it stops at the
/// end of the LINE — a `(` on the next line opens a different expression, and reading across the
/// newline would leave the parentheses off every candidate whose statement happens to be followed
/// by a parenthesised one.
pub(crate) fn drop_call_syntax_if_written(items: &mut [CompletionItem], source: &str, caret: usize) {
    let rest = &source[caret.min(source.len())..];
    let line = rest.split('\n').next().unwrap_or(rest);
    let after = line.trim_start_matches(|c: char| c.is_alphanumeric() || c == '_' || c == '$');
    if !after.trim_start().starts_with('(') {
        return;
    }
    for item in items.iter_mut() {
        if item.kind == "method" {
            item.insert_text = None;
            item.snippet_stops.clear();
        }
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

pub(crate) fn kind_tag(k: MemberKind) -> &'static str {
    match k {
        MemberKind::Method => "method",
        MemberKind::Field => "field",
    }
}

/// What the popup shows on the RIGHT of a member row: the type a field holds, or a method returns.
///
/// The parameters are not in it: they sit beside the name, where IntelliJ draws them — see
/// [`parameter_list`] — so the right-hand column holds the one thing a reader compares down the
/// list, what each candidate produces.
pub(crate) fn render_detail(m: &Member) -> String {
    render_type(&m.return_type)
}

/// The parameter list a method row shows beside its name — `(String prefix, int limit)`.
///
/// With a name where the member carries one: a project method always does, a class file compiled
/// without `-parameters` does not, and then the type stands alone rather than `arg0` stating a name
/// that is not true (see [`named_parameters`]). `None` for a field.
pub(crate) fn parameter_list(m: &Member) -> Option<String> {
    if m.kind != MemberKind::Method {
        return None;
    }
    let params: Vec<String> = named_parameters(m)
        .into_iter()
        .map(|(ty, name)| render_param(&ty, name.as_deref().unwrap_or("")))
        .collect();
    Some(format!("({})", params.join(", ")))
}

/// The modifiers the popup marks on a member's icon, in the wire's fixed vocabulary.
pub(crate) fn modifiers_of(m: &Member) -> Vec<String> {
    [(m.is_static, "static"), (m.is_abstract, "abstract"), (m.is_final, "final")]
        .into_iter()
        .filter(|(on, _)| *on)
        .map(|(_, word)| word.to_string())
        .collect()
}



#[cfg(test)]
mod overload_collapse_tests {
    use super::*;

    fn item(kind: &str, label: &str, detail: &str, score: i32) -> Ranked {
        Ranked {
            score,
            fit: rank::Fit::None,
            tier: 0,
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

    fn fitting(label: &str, score: i32, fit: rank::Fit) -> Ranked {
        Ranked { fit, ..item("method", label, "", score) }
    }

    fn sorted(mut v: Vec<Ranked>) -> Vec<Ranked> {
        sort_ranked(&mut v);
        v
    }

    /// The reported case: `builder.customer(..)` written three times and picked twice this session
    /// outscores `build()` on habit, and `build()` is the only member a `return` in a method
    /// returning `Order` can take. The fit leads, whatever the score says.
    #[test]
    fn what_fits_the_position_comes_before_what_scores_higher() {
        let v = sorted(vec![
            fitting("customer", 64, rank::Fit::None),
            fitting("id", 40, rank::Fit::None),
            fitting("build", 0, rank::Fit::Exact),
        ]);
        assert_eq!(labels(&v), ["build", "customer", "id"]);
    }

    /// The exact type first, then a subtype, then everything else — nothing dropped.
    #[test]
    fn an_exact_fit_leads_a_subtype_which_leads_a_miss() {
        let v = sorted(vec![
            fitting("label", 50, rank::Fit::None),
            fitting("listed", 10, rank::Fit::Subtype),
            fitting("orders", 5, rank::Fit::Exact),
        ]);
        assert_eq!(labels(&v), ["orders", "listed", "label"]);
    }

    #[test]
    fn the_only_exact_fit_is_preselected() {
        let mut v = vec![fitting("build", 0, rank::Fit::Exact), fitting("id", 9, rank::Fit::Subtype)];
        preselect_the_only_exact_fit(&mut v);
        assert!(v[0].item.preselect);
        assert!(!v[1].item.preselect);
    }

    /// Two locals of the right type are a choice, not an answer.
    #[test]
    fn two_exact_fits_preselect_neither() {
        let mut v = vec![fitting("order", 0, rank::Fit::Exact), fitting("other", 0, rank::Fit::Exact)];
        preselect_the_only_exact_fit(&mut v);
        assert!(v.iter().all(|r| !r.item.preselect));
    }

    /// A folded row fits when any of its overloads does.
    #[test]
    fn a_folded_row_keeps_the_best_fit_of_its_overloads() {
        let mut v = vec![fitting("of", 9, rank::Fit::None), fitting("of", 1, rank::Fit::Exact)];
        collapse_overloads(&mut v);
        assert_eq!(v[0].fit, rank::Fit::Exact);
    }

    /// A member row as the walk produces it: where it stands, how it matched, how it scored.
    fn member(label: &str, origin: MemberOrigin, tier: u8, score: i32) -> Ranked {
        let mut r = item("method", label, "", score);
        r.tier = tier;
        r.item.member_origin = Some(origin);
        r
    }

    fn sorted_members(mut v: Vec<Ranked>) -> Vec<String> {
        sort_members(&mut v);
        labels(&v)
    }

    /// The reported popup: `route.` opened on the record's implicit `equals` and `hashCode` before
    /// its own `prefix()`, and `Object`'s members came last only by score.
    #[test]
    fn own_members_come_before_inherited_ones_which_come_before_objects() {
        let v = sorted_members(vec![
            member("getClass", MemberOrigin::Object, 0, 90),
            member("equals", MemberOrigin::Inherited, 0, 50),
            member("target_uri", MemberOrigin::Own, 0, 0),
            member("prefix", MemberOrigin::Own, 0, 0),
        ]);
        assert_eq!(v, ["prefix", "target_uri", "equals", "getClass"]);
    }

    /// A habit or a use count reorders WITHIN an origin, never across it.
    #[test]
    fn the_score_orders_within_one_origin() {
        let v = sorted_members(vec![
            member("legs", MemberOrigin::Inherited, 0, 1),
            member("add", MemberOrigin::Inherited, 0, 30),
            member("fetch", MemberOrigin::Own, 0, -40),
        ]);
        assert_eq!(v, ["fetch", "add", "legs"]);
    }

    /// `s.to` — what was literally typed matters more than who declares it.
    #[test]
    fn the_match_tier_comes_before_the_origin() {
        let v = sorted_members(vec![
            member("targetOrigin", MemberOrigin::Own, 2, 0),
            member("toString", MemberOrigin::Inherited, 0, 0),
        ]);
        assert_eq!(v, ["toString", "targetOrigin"]);
    }

    /// And what the position wants comes before either.
    #[test]
    fn the_fit_comes_before_the_origin() {
        let mut fits = member("toString", MemberOrigin::Object, 0, 0);
        fits.fit = rank::Fit::Exact;
        let v = sorted_members(vec![member("prefix", MemberOrigin::Own, 0, 0), fits]);
        assert_eq!(v, ["toString", "prefix"]);
    }

    /// `wait()`, `wait(long)`, `wait(long, int)` — three rows, fewest parameters first.
    #[test]
    fn overloads_stay_apart_in_parameter_order() {
        let overload = |sig: &str| {
            let mut r = member("wait", MemberOrigin::Object, 0, 0);
            r.item.signature = Some(sig.to_string());
            r
        };
        let mut v = vec![overload("(long, int)"), overload("()"), overload("(long)")];
        sort_members(&mut v);
        let sigs: Vec<_> = v.iter().map(|r| r.item.signature.clone().unwrap()).collect();
        assert_eq!(sigs, ["()", "(long)", "(long, int)"]);
    }

    #[test]
    fn the_first_own_member_is_preselected() {
        let mut v = vec![member("prefix", MemberOrigin::Own, 0, 0), member("equals", MemberOrigin::Inherited, 0, 0)];
        preselect_the_first_own_member(&mut v);
        assert!(v[0].item.preselect && !v[1].item.preselect);
    }

    /// An own member further down is not pulled over the inherited one the order put first.
    #[test]
    fn nothing_is_preselected_when_the_first_row_is_not_own() {
        let mut v = vec![member("toString", MemberOrigin::Object, 0, 0), member("prefix", MemberOrigin::Own, 0, 0)];
        preselect_the_first_own_member(&mut v);
        assert!(v.iter().all(|r| !r.item.preselect));
    }

    #[test]
    fn an_existing_preselection_is_left_alone() {
        let mut v = vec![member("prefix", MemberOrigin::Own, 0, 0), member("build", MemberOrigin::Own, 0, 0)];
        v[1].item.preselect = true;
        preselect_the_first_own_member(&mut v);
        assert!(!v[0].item.preselect && v[1].item.preselect);
    }

    /// The folded `::` row shows the signature of the overload whose detail it shows.
    #[test]
    fn a_folded_row_takes_the_signature_of_its_face() {
        let mut weak = item("method", "run", "void", 1);
        weak.item.signature = Some("(Object o)".to_string());
        let mut strong = item("method", "run", "void", 9);
        strong.item.signature = Some("()".to_string());
        let mut v = vec![weak, strong];
        collapse_overloads(&mut v);
        assert_eq!(v[0].item.signature.as_deref(), Some("()"));
    }

    /// Names from a source signature; a class file's types stand alone.
    #[test]
    fn the_parameter_list_names_what_the_member_names() {
        let source = Member::method("equals", TypeRef::simple("boolean"), vec![TypeRef::simple("java/lang/Object")])
            .sig("boolean equals(Object obj)");
        assert_eq!(parameter_list(&source).as_deref(), Some("(Object obj)"));
        let bytecode = Member::method("wait", TypeRef::simple("void"), vec![TypeRef::simple("long"), TypeRef::simple("int")])
            .sig("(JI)V");
        assert_eq!(parameter_list(&bytecode).as_deref(), Some("(long, int)"));
        assert_eq!(render_detail(&source), "boolean");
        assert_eq!(parameter_list(&Member::field("name", TypeRef::simple("java/lang/String"))), None);
    }

    #[test]
    fn the_modifiers_are_the_wire_vocabulary() {
        let constant = Member::field("MAX", TypeRef::simple("int")).stat().final_();
        assert_eq!(modifiers_of(&constant), ["static", "final"]);
        assert!(modifiers_of(&Member::method("run", TypeRef::simple("void"), Vec::new())).is_empty());
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
