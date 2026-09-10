//! Completion for a **bare identifier** — a name being written with nothing to its left.
//!
//! ## The half that was missing
//!
//! [`crate::completion`] answers `receiver.pre|`: it infers what the receiver is and walks its
//! members. That is the whole of what the engine offered, and it means the completion popup only
//! knew anything after a dot. Type `ord|` meaning the local `order` and the index had no answer —
//! the list came from scanning the buffer for words that start with `ord`, which is what an editor
//! with no index does.
//!
//! The names a bare identifier can mean are not a mystery. Java says exactly what they are, in
//! this order of nearness:
//!
//! 1. what the **scope chain** binds — locals, parameters, pattern variables ([`bennu_java`]'s
//!    [`visible_bindings`](bennu_java::prelude::visible_bindings));
//! 2. the **members of the enclosing type** and everything it inherits, written without `this.`;
//! 3. whatever an `import static` pulled in.
//!
//! Class names are the fourth, and they are not here: they come from the class-name index, which
//! belongs to `bennu-intel` (it spans the JDK, the dependency jars and the project at once). The
//! caller concatenates the two — see `NativeJavaProvider::complete_at`.
//!
//! ## Why nearness is the whole ordering
//!
//! These four categories do not compete on any shared axis, so ranking them against each other on
//! one would be inventing a comparison. What actually decides is distance from the caret, and the
//! bands in [`crate::rank::band`] are that distance made explicit. Inside a band, the ordinary
//! member ranking applies.

use std::collections::HashSet;

use bennu_classpath::prelude::MemberIndex as CpMemberIndex;
use bennu_java::prelude::{
    caret_is_static, enclosing_type_binary, expected_type, extract_symbols, parse_java,
    static_import_targets, visible_bindings, TypeRef,
};
use bennu_proto::prelude::CompletionItem;

use crate::completion::{
    collapse_overloads, collect_members, drop_call_syntax_if_written, split_prefix, Ranked,
};
use crate::member_text::render_type;
use bennu_complete::prelude::{MatchCase, Typed};

use crate::rank;
use crate::resolver::IndexResolver;

/// The identifier spliced in at the caret so the buffer parses — the same repair member
/// completion makes, and for the same reason: a half-written statement can flatten the enclosing
/// class into an ERROR node, and then there is no scope chain left to walk.
const SITE_PLACEHOLDER: &str = "x";

/// Completions for the bare identifier at `byte_offset`, most relevant first.
///
/// Empty when the caret is a member access (`recv.pre|` — that is [`crate::completion`]'s
/// question), when the buffer does not parse, or when nothing in scope matches the prefix. All
/// three are ordinary states, not errors.
pub fn scope_completion<M: CpMemberIndex>(
    source: &str,
    byte_offset: usize,
    resolver: &IndexResolver<M>,
    case: MatchCase,
) -> Vec<CompletionItem> {
    let mut at = byte_offset.min(source.len());
    while at > 0 && !source.is_char_boundary(at) {
        at -= 1;
    }
    let (start, prefix) = split_prefix(source, at);
    let typed = Typed::new(&prefix, case);
    // `recv.pre|` — a different question with a different answer, and answering it here would put
    // every name in scope underneath the members that are actually legal there.
    if is_member_access(source, start) {
        return Vec::new();
    }

    let sited = format!("{}{SITE_PLACEHOLDER}{}", &source[..at], &source[at..]);
    let Some(tree) = parse_java(&sited) else {
        return Vec::new();
    };
    let root = tree.root_node();
    let symbols = extract_symbols(&sited);
    let site = enclosing_type_binary(&sited, at);
    // A bare name in a static context can only be a static member — `count` inside
    // `static void main` does not compile, however visible the field is.
    let statik = caret_is_static(&root, &sited, at);

    // `receiver_is_type` says "statics are the answer here", which is exactly what a static
    // context means for an unqualified name. One flag, two spellings of the same fact.
    // The plain buffer, not `sited`: the expected-type walk makes its own repair, and it needs a
    // terminator as well as an identifier — see `expected_type`.
    let expected = expected_type(source, at, resolver);
    let ctx = rank::Context::new(source, statik).expecting(expected);
    let mut out: Vec<Ranked> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();

    for b in visible_bindings(&root, &sited, &symbols, at, resolver) {
        let Some(tier) = typed.tier(&b.name) else {
            continue;
        };
        if !seen.insert(format!("local:{}", b.name)) {
            continue;
        }
        out.push(Ranked {
            score: rank::score_binding(
                &b.name,
                b.is_parameter,
                b.depth,
                at.saturating_sub(b.decl),
                &ctx,
            ) - rank::tier_penalty(tier),
            item: CompletionItem {
                label: b.name,
                kind: if b.is_parameter { "parameter" } else { "variable" }.to_string(),
                // The declared type, which is the one thing worth saying about a local: the popup
                // is being read to decide *which* name, and the type is what tells them apart.
                detail: b.ty.as_ref().map(render_type),
                ..Default::default()
            },
        });
    }

    if let Some(site) = site.as_deref() {
        // Written without `this.`, so `site` is both the receiver and the access site: a private
        // member of your own class is exactly what a bare name most often is.
        let mut members = Vec::new();
        let mut member_seen = HashSet::new();
        collect_members(
            resolver,
            &TypeRef::simple(site),
            typed,
            Some(site),
            statik,
            &ctx,
            &mut members,
            &mut member_seen,
        );
        collapse_overloads(&mut members);
        for mut m in members {
            if !seen.insert(format!("member:{}", m.item.label)) {
                continue;
            }
            // Re-banded rather than re-scored: `score` already ordered these against each other
            // correctly, and the band only says where the whole group sits.
            m.score = rank::band::OWN_MEMBER + m.score.clamp(-90, 90);
            out.push(m);
        }
    }

    collect_static_imports(resolver, &symbols, typed, site.as_deref(), &ctx, &mut out, &mut seen);

    out.sort_by(|a, b| {
        b.score
            .cmp(&a.score)
            .then(a.item.kind.cmp(&b.item.kind))
            .then(a.item.label.cmp(&b.item.label))
    });
    let mut items: Vec<CompletionItem> = out.into_iter().map(|r| r.item).collect();
    drop_call_syntax_if_written(&mut items, source, at);
    items
}

/// Whether the identifier starting at `start` is preceded by a `.` — i.e. it is a member access.
///
/// Whitespace is crossed, because `receiver\n    .method` is one expression written over two
/// lines and the dot is still what governs the name.
fn is_member_access(source: &str, start: usize) -> bool {
    source[..start]
        .trim_end()
        .ends_with('.')
}

/// The members an `import static` made writable without a receiver.
///
/// A named import (`import static java.util.Arrays.asList;`) offers that one member; a wildcard
/// (`import static Foo.*;`) offers every static the type has. Both go through the same member
/// walk as anything else, so visibility and the `<init>` filter are decided in one place.
#[allow(clippy::too_many_arguments)]
fn collect_static_imports<M: CpMemberIndex>(
    resolver: &IndexResolver<M>,
    symbols: &bennu_java::prelude::FileSymbols,
    typed: Typed<'_>,
    site: Option<&str>,
    ctx: &rank::Context,
    out: &mut Vec<Ranked>,
    seen: &mut HashSet<String>,
) {
    for target in static_import_targets(&symbols.imports) {
        // A named import is its own filter: `import static X.max;` makes `max` writable and
        // nothing else, so the member name is what the walk must match on — not the typed prefix,
        // which would offer every static of `X`.
        let (walk_typed, wanted) = match &target.member {
            Some(m) if !typed.matches(m) => continue,
            // A named import is its own filter, so the walk looks for THAT member exactly rather
            // than for what was typed — which would offer every static of the owner.
            Some(m) => (Typed::new(m.as_str(), MatchCase::All), Some(m.as_str())),
            None => (typed, None),
        };
        let mut found = Vec::new();
        let mut found_seen = HashSet::new();
        collect_members(
            resolver,
            &TypeRef::simple(&target.owner_binary),
            walk_typed,
            site,
            true,
            ctx,
            &mut found,
            &mut found_seen,
        );
        collapse_overloads(&mut found);
        for mut m in found {
            if wanted.is_some_and(|w| m.item.label != w) {
                continue;
            }
            if !seen.insert(format!("member:{}", m.item.label)) {
                continue;
            }
            m.score = rank::band::STATIC_IMPORT + m.score.clamp(-90, 90);
            // Where it came from, because a statically-imported name is the one case where the
            // reader of the popup has no other way to tell.
            m.item.detail = Some(match m.item.detail.take() {
                Some(d) => format!("{d} — {}", dotted(&target.owner_binary)),
                None => dotted(&target.owner_binary),
            });
            out.push(m);
        }
    }
}

fn dotted(binary: &str) -> String {
    binary.replace('/', ".").replace('$', ".")
}
