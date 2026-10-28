//! Chain completion — members of members that produce the type the position wants.
//!
//! `String name = order.|` has forty members on `order`, and none of them returns a `String` that is
//! the NAME: the one the author means is `getCustomer().getName()`, a hop away. IntelliJ answers it
//! with a second press of the smart-completion chord. Here it is a few rows added to the list the
//! first press already shows, below the members that fit directly.
//!
//! ## What qualifies, and why so little
//!
//! A chain is a first hop and a second hop, each written as a zero-argument call (or a field) —
//! nothing that needs an argument can be offered without inventing one. The second hop must produce
//! exactly the expected type; a subtype or a near miss in a chain is noise a direct member already
//! covers better. The first hop must not already fit (a direct fit needs no help) and must return a
//! PROJECT class: those are read from the memory-mapped index, which keeps the whole pass inside the
//! budget of a keystroke, while walking JDK types here would mean reading class files for every
//! member of the receiver.
//!
//! The caps (first hops considered, rows per hop, rows in total) are what keep the feature from
//! turning into a second member list; they are generous enough that the right chain is nearly
//! always among them and small enough that no receiver can flood the popup.

use std::collections::HashSet;

use bennu_complete::prelude::Typed;
use bennu_java::prelude::{walk_up, Member, MemberKind, TypeRef, TypeResolver, Visibility};
use bennu_proto::prelude::CompletionItem;

use crate::affinity::name_affinity;
use crate::completion::Ranked;
use crate::member_text::render_type;
use crate::rank::{self, Fit, OBJECT};

/// How many first hops are followed. The popup is read, not scrolled for an answer that is the
/// fortieth member of a type.
const MAX_FIRST_HOPS: usize = 24;
/// How many second hops one first hop contributes.
const MAX_PER_HOP: usize = 3;
/// How many chain rows the list gains at most.
const MAX_CHAINS: usize = 12;

/// A first hop: the member of the receiver, written as it would be, and what it produces.
struct Hop {
    written: String,
    produces: TypeRef,
}

/// The chain rows for the receiver being completed, best first — empty when the position expects
/// nothing, when the setting is off, or when no chain fits.
///
/// `receiver_is_type` makes the first hop a static (`Config.instance().getName()`); the second hop
/// is always an instance member.
pub(crate) fn chain_members(
    resolver: &dyn TypeResolver,
    receiver: &TypeRef,
    receiver_is_type: bool,
    typed: Typed<'_>,
    ctx: &rank::Context,
) -> Vec<Ranked> {
    if !ctx.tuning.chain || ctx.expected.is_none() {
        return Vec::new();
    }
    let hops = first_hops(resolver, receiver, receiver_is_type, typed, ctx);
    let mut out: Vec<Ranked> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    for hop in &hops {
        out.extend(second_hops(resolver, hop, typed, ctx, &mut seen));
    }
    out.sort_by(|a, b| b.score.cmp(&a.score).then_with(|| a.item.label.cmp(&b.item.label)));
    out.truncate(MAX_CHAINS);
    out
}

fn first_hops(
    resolver: &dyn TypeResolver,
    receiver: &TypeRef,
    receiver_is_type: bool,
    typed: Typed<'_>,
    ctx: &rank::Context,
) -> Vec<Hop> {
    let mut hops: Vec<Hop> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    walk_up::<()>(resolver, receiver, |ancestor| {
        if ancestor.ty.binary_name == OBJECT {
            return None;
        }
        for m in ancestor.members.methods.iter().chain(ancestor.members.fields.iter()) {
            if hops.len() >= MAX_FIRST_HOPS {
                break;
            }
            let usable = typed.tier(&m.name).is_some()
                && is_zero_arg_public(m)
                && m.is_static == receiver_is_type
                && walkable(&m.return_type, resolver)
                // A member that already fits the position is a row of its own; a chain through it
                // would only repeat it longer.
                && ctx.fit(&m.return_type, resolver) == Fit::None;
            if usable && seen.insert(member_key(m)) {
                hops.push(Hop { written: written(m), produces: m.return_type.clone() });
            }
        }
        None
    });
    hops
}

fn second_hops(
    resolver: &dyn TypeResolver,
    hop: &Hop,
    typed: Typed<'_>,
    ctx: &rank::Context,
    seen: &mut HashSet<String>,
) -> Vec<Ranked> {
    let mut found: Vec<Ranked> = Vec::new();
    walk_up::<()>(resolver, &hop.produces, |ancestor| {
        if ancestor.ty.binary_name == OBJECT {
            return None;
        }
        for m in ancestor.members.methods.iter().chain(ancestor.members.fields.iter()) {
            if found.len() >= MAX_PER_HOP {
                break;
            }
            if m.is_static || !is_zero_arg_public(m) || ctx.fit(&m.return_type, resolver) != Fit::Exact {
                continue;
            }
            let text = format!("{}.{}", hop.written, written(m));
            if !seen.insert(text.clone()) {
                continue;
            }
            found.push(chain_row(text, m, ancestor.depth, typed, ctx));
        }
        None
    });
    found
}

fn chain_row(text: String, last: &Member, depth: usize, typed: Typed<'_>, ctx: &rank::Context) -> Ranked {
    let affinity = ctx
        .hole_name
        .as_deref()
        .filter(|_| ctx.tuning.name_affinity)
        .map_or(0, |hole| ctx.tuning.scaled(name_affinity(&last.name, hole)));
    let first_name = text.split(|c| c == '(' || c == '.').next().unwrap_or("");
    Ranked {
        score: affinity - (depth as i32 * 3),
        // `None`, not `Exact`: the row is placed by hand below the direct fits, and the list's
        // "only exact fit is preselected" rule must not count it.
        fit: Fit::None,
        tier: typed.tier(first_name).unwrap_or(0),
        item: CompletionItem {
            label: text.clone(),
            kind: "method".to_string(),
            detail: Some(render_type(&last.return_type)),
            insert_text: Some(text),
            ..Default::default()
        },
    }
}

/// A public instance-or-static member that can be written with no arguments: a field, or a method
/// taking none. Constructors and initialisers are not members you reach through a dot.
fn is_zero_arg_public(m: &Member) -> bool {
    m.visibility == Visibility::Public
        && m.name != "<init>"
        && m.name != "<clinit>"
        && (m.kind == MemberKind::Field || m.params.is_empty())
}

/// A type worth reading the members of: a class of this project, with no array dimensions. Anything
/// under `java/` is left alone — see the module docs.
fn walkable(ty: &TypeRef, resolver: &dyn TypeResolver) -> bool {
    ty.dims == 0
        && ty.binary_name.contains('/')
        && !ty.binary_name.starts_with("java/")
        && resolver.is_project_type(&ty.binary_name)
}

fn written(m: &Member) -> String {
    match m.kind {
        MemberKind::Method => format!("{}()", m.name),
        MemberKind::Field => m.name.clone(),
    }
}

fn member_key(m: &Member) -> String {
    format!("{}:{}", written(m), m.is_static)
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::Arc;

    use bennu_complete::prelude::Typed;
    use bennu_java::prelude::{ClassMembers, Import};

    use super::*;
    use crate::tuning::Tuning;

    /// `Order { getCustomer(): Customer; getTotal(): Money; getNote(): String; id: long }`,
    /// `Customer { getName(): String; getAddress(): Address; getLabel(): String }`,
    /// `Address { getCity(): String }`.
    struct Shop(HashMap<&'static str, Arc<ClassMembers>>);

    fn class(methods: Vec<Member>, fields: Vec<Member>) -> Arc<ClassMembers> {
        Arc::new(ClassMembers {
            superclass: Some(TypeRef::simple(OBJECT)),
            interfaces: Vec::new(),
            methods,
            fields,
            flags: Default::default(),
            type_params: Vec::new(),
        })
    }

    fn getter(name: &str, returns: &str) -> Member {
        Member::method(name, TypeRef::simple(returns), Vec::new())
    }

    fn shop() -> Shop {
        let mut classes = HashMap::new();
        classes.insert(
            "shop/Order",
            class(
                vec![
                    getter("getCustomer", "shop/Customer"),
                    getter("getTotal", "shop/Money"),
                    getter("getNote", "java/lang/String"),
                ],
                vec![Member::field("id", TypeRef::simple("long"))],
            ),
        );
        classes.insert(
            "shop/Customer",
            class(
                vec![
                    getter("getName", "java/lang/String"),
                    getter("getAddress", "shop/Address"),
                    getter("getLabel", "java/lang/String"),
                ],
                Vec::new(),
            ),
        );
        classes.insert("shop/Address", class(vec![getter("getCity", "java/lang/String")], Vec::new()));
        classes.insert("shop/Money", class(vec![getter("getCurrency", "java/lang/String")], Vec::new()));
        classes.insert(OBJECT, Arc::new(ClassMembers {
            superclass: None,
            interfaces: Vec::new(),
            methods: vec![getter("toString", "java/lang/String")],
            fields: Vec::new(),
            flags: Default::default(),
            type_params: Vec::new(),
        }));
        Shop(classes)
    }

    impl TypeResolver for Shop {
        fn members_of(&self, binary: &str) -> Option<Arc<ClassMembers>> {
            self.0.get(binary).cloned()
        }
        fn resolve_simple_name(&self, _n: &str, _i: &[Import]) -> Option<String> {
            None
        }
    }

    fn wanting_string() -> rank::Context {
        rank::Context::new("", false)
            .with_tuning(Tuning::DEFAULT)
            .expecting(Some(TypeRef::simple("java/lang/String")))
    }

    fn labels(rows: &[Ranked]) -> Vec<&str> {
        rows.iter().map(|r| r.item.label.as_str()).collect()
    }

    fn chains(ctx: &rank::Context, typed: &str) -> Vec<Ranked> {
        chain_members(&shop(), &TypeRef::simple("shop/Order"), false, Typed::lenient(typed), ctx)
    }

    #[test]
    fn a_member_of_a_member_that_produces_the_expected_type_is_offered() {
        let rows = chains(&wanting_string(), "");
        let all = labels(&rows);
        assert!(all.contains(&"getCustomer().getName()"), "{all:?}");
        assert!(all.contains(&"getCustomer().getLabel()"), "{all:?}");
    }

    #[test]
    fn the_chain_is_inserted_as_written_and_typed_by_its_last_hop() {
        let rows = chains(&wanting_string(), "");
        let row = rows.iter().find(|r| r.item.label == "getCustomer().getName()").expect("a chain");
        assert_eq!(row.item.insert_text.as_deref(), Some("getCustomer().getName()"));
        assert_eq!(row.item.kind, "method");
        assert_eq!(row.item.detail.as_deref(), Some("String"));
        assert!(row.item.snippet_stops.is_empty(), "a zero-argument chain parks the caret after it");
    }

    /// `getNote()` returns a `String` on its own, so a chain THROUGH it would only repeat it longer —
    /// and a hop that already fits is not followed at all.
    #[test]
    fn a_member_that_already_fits_is_not_chained_through() {
        let all = chains(&wanting_string(), "");
        assert!(!labels(&all).iter().any(|l| l.starts_with("getNote()")));
    }

    #[test]
    fn nothing_is_offered_without_an_expected_type() {
        let ctx = rank::Context::new("", false).with_tuning(Tuning::DEFAULT);
        assert!(chains(&ctx, "").is_empty());
    }

    #[test]
    fn the_setting_turns_chains_off() {
        let ctx = rank::Context::new("", false)
            .with_tuning(Tuning { chain: false, ..Tuning::DEFAULT })
            .expecting(Some(TypeRef::simple("java/lang/String")));
        assert!(chains(&ctx, "").is_empty());
    }

    #[test]
    fn the_typed_prefix_filters_by_the_first_hop() {
        let rows = chains(&wanting_string(), "getCust");
        assert!(!rows.is_empty());
        assert!(labels(&rows).iter().all(|l| l.starts_with("getCustomer()")));
        assert!(chains(&wanting_string(), "getTot").iter().all(|r| r.item.label.starts_with("getTotal()")));
        assert!(chains(&wanting_string(), "zzz").is_empty());
    }

    #[test]
    fn only_one_level_of_nesting_is_followed() {
        // Address.getCity() is two hops below a Customer, three below an Order.
        let all = chains(&wanting_string(), "");
        assert!(!labels(&all).iter().any(|l| l.contains("getCity")));
    }

    #[test]
    fn each_first_hop_contributes_a_bounded_number_of_rows() {
        let rows = chains(&wanting_string(), "");
        let through_customer = labels(&rows).into_iter().filter(|l| l.starts_with("getCustomer()")).count();
        assert!(through_customer <= MAX_PER_HOP);
        assert!(rows.len() <= MAX_CHAINS);
    }

    /// The row whose last hop echoes the name being filled leads: `String name = order.|`.
    #[test]
    fn a_last_hop_echoing_the_name_being_filled_ranks_first() {
        let mut ctx = wanting_string();
        ctx.hole_name = Some("label".to_string());
        let rows = chains(&ctx, "");
        assert_eq!(rows.first().map(|r| r.item.label.as_str()), Some("getCustomer().getLabel()"));
    }

    #[test]
    fn only_project_classes_without_dimensions_are_followed() {
        assert!(!walkable(&TypeRef::simple("java/lang/String"), &shop()));
        assert!(!walkable(&TypeRef::simple("long"), &shop()));
        assert!(walkable(&TypeRef::simple("shop/Customer"), &shop()));
        assert!(!walkable(&TypeRef { dims: 1, ..TypeRef::simple("shop/Customer") }, &shop()));
    }

    #[test]
    fn rows_are_never_marked_as_an_exact_fit() {
        assert!(chains(&wanting_string(), "").iter().all(|r| r.fit == Fit::None));
    }
}
