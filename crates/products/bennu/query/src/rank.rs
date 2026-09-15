//! How good a member-access completion is *here* — the relevance ordering the popup is shown in.
//!
//! ## Why this is not a detail
//!
//! Completion used to come back sorted by `(kind, label)`: alphabetical, within fields-then-methods.
//! Everything that matched was offered, in an order that knew nothing about the question. Typing
//! `list.` put `add` below `clone`, `equals`, `forEach`, `getClass` and `hashCode`; typing `Color.`
//! offered `wait(long, int)` — which is not even legal on a type — in the same breath as the enum's
//! own constants. The list was correct and useless, because being right about *what* is offered is
//! only half of it: what makes completion feel like it read your mind is that the three things you
//! might have meant are the three at the top.
//!
//! ## The signals, and why these
//!
//! Each one is something the engine already knows for certain. Nothing here is a guess, and nothing
//! here needs a model — this is the ordering an IDE gets from its index, and it is most of what
//! "good completion" turns out to be:
//!
//! - **What the receiver IS.** After `Color.` the statics are the answer and the instance methods
//!   are noise (`Color.equals` does not compile). After `color.` it is the other way round, less
//!   sharply — a static reached through an instance compiles, it is just rarely what you meant.
//! - **How far up the hierarchy it was found.** A method the receiver's own class declares beats one
//!   it inherited, and the further up, the weaker.
//! - **`java.lang.Object`.** Its members match every prefix on every receiver and are almost never
//!   what anyone is reaching for. They sink furthest, which is the single biggest change to how the
//!   list reads.
//! - **Deprecated.** Still offered — it exists, and you may be reading old code — but last.
//!   Only visible on project source: a member decoded from bytecode carries no annotations, so this
//!   sharpens as the code you own, which is the code you edit.
//! - **What this file already uses.** A member you have already written in this buffer is very
//!   likely the one you want again, and the buffer is right there. Capped, so a name used forty
//!   times cannot outrank relevance itself.
//!
//! - **What you picked last time** ([`crate::picked`]) — a habit, weighed like the rest.
//!
//! ## The expected type is not one of the terms
//!
//! Knowing that `return builder.|` in a method returning `Order` wants something `Order`-shaped is
//! the strongest signal there is, and it is a different KIND of signal: every term above says how
//! likely a candidate is, and this one says whether it can be written there at all. As a weighted
//! term it lost exactly where it mattered — `builder.customer(..)` written three times in the
//! method and picked twice this session outscored `build()`, the one member that compiles after
//! that `return`.
//!
//! So it is a [`Fit`], and the list is ordered by fit FIRST and by [`score`] within each fit
//! (IntelliJ's "smart" ordering). Nothing is hidden: what does not fit is still offered, below.

use std::cell::RefCell;
use std::collections::HashMap;

use bennu_java::prelude::{ClassMembers, Member, MemberKind, TypeRef, TypeResolver, Visibility};
use bennu_proto::prelude::MemberOrigin;

/// How well what a candidate PRODUCES answers the type the position wants — the leading sort key
/// of every completion list that has an expected type. Ordered worst to best, so `max` is the
/// better of two.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord)]
pub enum Fit {
    /// No expected type, or a candidate that does not produce it.
    #[default]
    None,
    /// A proper subtype of the expected type — `ArrayList<Order>` where a `List<Order>` is
    /// returned. It compiles, and it is ranked under the exact type because the declared type is
    /// what the author spelled.
    Subtype,
    /// The expected type itself, a primitive and its box counted as one.
    Exact,
}

/// The binary name whose members match everything and are wanted almost never.
pub(crate) const OBJECT: &str = "java/lang/Object";

/// How many uses in the current buffer can still improve an item's standing. Past this the signal
/// has said what it has to say, and letting it keep climbing would let a much-used `getClass()`
/// outrank the method you are actually looking for.
const MAX_COUNTED_USES: usize = 3;

/// What the ranking knows about the question being asked, gathered once per completion.
pub struct Context {
    /// The receiver is a TYPE name (`Color.`), not a value (`color.`) — so statics are the answer
    /// and instance members are not applicable.
    pub receiver_is_type: bool,
    /// Identifier → how many times it appears in the buffer being edited. See the module docs.
    pub uses: HashMap<String, usize>,
    /// The type this position **wants** — the binary name of what an assignment target, a
    /// `return` or a condition constrains the hole to. `None` when the position constrains
    /// nothing, which is most of them.
    ///
    /// The strongest signal there is, and the only one that is about the hole rather than about
    /// the candidate: `String name = order.|` has forty members to offer and a handful that can
    /// be written there at all.
    pub expected: Option<TypeRef>,
    /// The shape a METHOD REFERENCE being written has to have — set only after a `::`, and only
    /// when the slot the reference is passed to describes one. See [`ReferenceShape`].
    pub reference: Option<ReferenceShape>,
    /// Produced binary name → whether it is a subtype of [`Context::expected`], asked once per
    /// type rather than once per candidate: forty members of one receiver return a handful of
    /// distinct types, and each answer is a hierarchy walk.
    subtype_memo: RefCell<HashMap<String, bool>>,
}

/// What a method reference has to look like to compile where it is being written: the functional
/// interface's parameters and return, and what stands left of the `::`.
///
/// `opt.map(ResolvedIdentity::|)` wants a `Function<ResolvedIdentity, U>`. Through a TYPE, three
/// kinds of method can be written there, and each consumes the parameters differently:
///
/// * a **static** method takes them all — `static Token convert(ResolvedIdentity)`;
/// * an **instance** method is unbound: the first parameter IS the receiver, and the method takes
///   the rest — `String identifier()`;
/// * through a VALUE (`resolver::`), an instance method is bound and takes them all.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReferenceShape {
    /// The single abstract method's parameters, generics substituted; an unbound variable is `Object`.
    pub params: Vec<TypeRef>,
    /// What it returns — `void` as the primitive name.
    pub returns: TypeRef,
    /// The binary name of the type the qualifier denotes or has.
    pub qualifier: String,
    /// The qualifier names a TYPE (`Foo::`), not a value (`foo::`).
    pub through_type: bool,
}

/// Whether `m`, referenced through `shape`'s qualifier, compiles in `shape`'s slot.
///
/// A ranking test, deliberately loose where the loose answer is the harmless one: a parameter the
/// slot types as `Object` (a variable nothing binds yet) accepts anything, and a method parameter
/// that is itself a type variable accepts anything — `static <T> T id(T)` fits every slot of arity
/// one. What it is strict about is what does not compile at all: the wrong number of arguments, a
/// `void` where a value is wanted, a static through a value.
pub fn fits_reference(m: &Member, shape: &ReferenceShape) -> bool {
    if m.kind != MemberKind::Method {
        return false;
    }
    if shape.returns.binary_name != "void"
        && m.return_type.binary_name == "void"
        && m.return_type.dims == 0
    {
        return false;
    }
    if m.is_static {
        return shape.through_type && params_fit(&m.params, &shape.params);
    }
    if !shape.through_type {
        return params_fit(&m.params, &shape.params);
    }
    let Some((receiver, rest)) = shape.params.split_first() else {
        return false;
    };
    let receiver_fits = receiver.dims == 0
        && (receiver.binary_name == shape.qualifier || receiver.binary_name == OBJECT);
    receiver_fits && params_fit(&m.params, rest)
}

/// Whether a method declared with `declared` parameters can be handed `wanted` — see
/// [`fits_reference`] for which differences count.
pub fn params_fit(declared: &[TypeRef], wanted: &[TypeRef]) -> bool {
    declared.len() == wanted.len()
        && declared.iter().zip(wanted).all(|(d, w)| {
            let takes_anything = d.dims == 0 && (d.binary_name == OBJECT || is_type_variable_name(&d.binary_name));
            let gives_anything = w.dims == 0 && w.binary_name == OBJECT;
            takes_anything || gives_anything || (d.dims == w.dims && boxes_to(&d.binary_name, &w.binary_name))
        })
}

/// A bare capitalised name — `T`, `Source` — is a type variable: every class reaches the ranking as
/// a slashed binary name, and every primitive is lower-case.
fn is_type_variable_name(name: &str) -> bool {
    !name.contains('/') && name.starts_with(|c: char| c.is_uppercase())
}

impl Context {
    /// Read the buffer once, counting every identifier in it.
    ///
    /// Deliberately not a parse: this is a popularity contest, not an analysis, and a token that
    /// happened to be inside a string or a comment is still evidence of what this file is about.
    /// One pass, and the result is asked O(1) per candidate — scanning per candidate would be the
    /// completion path doing quadratic work on every keystroke.
    pub fn new(source: &str, receiver_is_type: bool) -> Self {
        let mut uses: HashMap<String, usize> = HashMap::new();
        let bytes = source.as_bytes();
        let mut i = 0;
        while i < bytes.len() {
            let c = bytes[i];
            if c.is_ascii_alphabetic() || c == b'_' || c == b'$' {
                let start = i;
                while i < bytes.len()
                    && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_' || bytes[i] == b'$')
                {
                    i += 1;
                }
                if let Some(word) = source.get(start..i) {
                    *uses.entry(word.to_string()).or_insert(0) += 1;
                }
            } else {
                i += 1;
            }
        }
        Self {
            receiver_is_type,
            uses,
            expected: None,
            reference: None,
            subtype_memo: RefCell::default(),
        }
    }

    /// Tell the ranking the shape a method reference must have here — see [`ReferenceShape`].
    pub fn for_reference(mut self, shape: Option<ReferenceShape>) -> Self {
        self.reference = shape;
        self
    }

    /// Tell the ranking what type the position wants — see [`Context::expected`].
    ///
    /// Builder-style because it arrives from a different question than the buffer does: the
    /// expected type is a walk up the tree from the caret, and half the call sites have nothing
    /// to say. `None` is not a neutral value to thread through them, it is the answer.
    pub fn expecting(mut self, expected: Option<TypeRef>) -> Self {
        self.expected = expected;
        self
    }

    /// Whether `produced` is exactly what the position wants.
    ///
    /// Compared by **name**, with the primitive/boxed pairs treated as one — Java's own
    /// autoboxing, and the difference between `int n = order.getCount()` ranking the `Integer`
    /// getter first or not at all — and by the type ARGUMENTS where both sides spell them, so a
    /// `List<String>` is not taken for the `List<Order>` a method returns. See [`arguments_agree`].
    fn wants(&self, produced: &TypeRef) -> bool {
        let Some(expected) = &self.expected else {
            return false;
        };
        expected.dims == produced.dims
            && boxes_to(&expected.binary_name, &produced.binary_name)
            && arguments_agree(&expected.type_args, &produced.type_args)
    }

    /// How well a candidate producing `produced` fits the position — see [`Fit`].
    ///
    /// A subtype is found by walking UP from what is produced, through `resolver`, and remembered
    /// per produced type. Only class types are walked: a primitive has no hierarchy, and an
    /// expected `Object` is a supertype of everything, which would make every candidate a fit and
    /// the key mean nothing.
    pub fn fit(&self, produced: &TypeRef, resolver: &dyn TypeResolver) -> Fit {
        let Some(expected) = &self.expected else {
            return Fit::None;
        };
        if self.wants(produced) {
            return Fit::Exact;
        }
        let walkable = expected.dims == produced.dims
            && expected.binary_name != OBJECT
            && expected.binary_name != produced.binary_name
            && expected.binary_name.contains('/')
            && produced.binary_name.contains('/');
        if !walkable {
            return Fit::None;
        }
        let cached = self.subtype_memo.borrow().get(&produced.binary_name).copied();
        let is_subtype = match cached {
            Some(hit) => hit,
            None => {
                let start = TypeRef::simple(produced.binary_name.clone());
                let found = bennu_java::prelude::walk_up::<()>(resolver, &start, |a| {
                    (a.ty.binary_name == expected.binary_name).then_some(())
                })
                .is_some();
                self.subtype_memo
                    .borrow_mut()
                    .insert(produced.binary_name.clone(), found);
                found
            }
        };
        if is_subtype { Fit::Subtype } else { Fit::None }
    }
}

/// Whether two type-argument lists can be the same parameterisation.
///
/// Lenient wherever leniency is the harmless answer: a side that spells no arguments (a raw type,
/// a diamond, a member decoded without its signature), a different arity, and an argument that is
/// `Object` or a bare type variable all agree. What it rejects is two class arguments that are
/// visibly different types — `List<String>` for `List<Order>`.
fn arguments_agree(expected: &[TypeRef], produced: &[TypeRef]) -> bool {
    if expected.is_empty() || produced.is_empty() || expected.len() != produced.len() {
        return true;
    }
    expected.iter().zip(produced).all(|(e, p)| {
        let open = |t: &TypeRef| t.binary_name == OBJECT || is_type_variable_name(&t.binary_name);
        open(e)
            || open(p)
            || (e.dims == p.dims
                && boxes_to(&e.binary_name, &p.binary_name)
                && arguments_agree(&e.type_args, &p.type_args))
    })
}

/// Whether two type names are the same type, counting a primitive and its box as one.
fn boxes_to(a: &str, b: &str) -> bool {
    const PAIRS: &[(&str, &str)] = &[
        ("int", "java/lang/Integer"),
        ("long", "java/lang/Long"),
        ("double", "java/lang/Double"),
        ("float", "java/lang/Float"),
        ("boolean", "java/lang/Boolean"),
        ("char", "java/lang/Character"),
        ("byte", "java/lang/Byte"),
        ("short", "java/lang/Short"),
    ];
    a == b || PAIRS.iter().any(|(p, boxed)| (a == *p && b == *boxed) || (a == *boxed && b == *p))
}

impl Context {

    /// How relevant a NESTED TYPE of the receiver is — `Outer.Inner`.
    ///
    /// Ranked as a static member, because that is what it behaves like: it is only reachable through a
    /// type receiver, and reaching for one is the same gesture as reaching for a constant. It does not
    /// get the constant's extra nudge: a type name is what you want less often than `Outer.MAX`, and
    /// the file's own usage count settles the rest.
    pub fn score_nested_type(&self, simple: &str) -> i32 {
        let mut s = 25;
        if let Some(n) = self.uses.get(simple) {
            s += 6 * (*n).min(MAX_COUNTED_USES) as i32;
        }
        s
    }
}

/// What a weaker match costs.
///
/// The tier is a ranking input as much as a filter: `s.to` should offer `toString` above
/// `toLowerCase` even though the humps reach both, because one of them is what was literally
/// typed. Weighted below the position's expected type — being *possible here* matters more than
/// being spelled the way you started — and above everything that is merely a habit.
pub fn tier_penalty(tier: u8) -> i32 {
    i32::from(tier) * 14
}

/// The relevance **bands** a bare-identifier completion is ordered in.
///
/// A member list is one kind of thing ranked against itself, and [`score`] is enough. A bare name
/// is not: the local declared two lines up, an inherited method, a statically-imported constant
/// and a class on the classpath are four different categories, and they do not compete on any
/// shared axis. What actually decides between them is how far the name is from the caret — so the
/// category is ranked first and [`score`] orders within it.
///
/// The gaps are wide enough that nothing inside a band can climb out of it. That is the point: a
/// field used forty times in this file must not outrank the variable you just declared.
pub mod band {
    /// A local, a parameter, a pattern variable — bound by the scope the caret is standing in.
    pub const BINDING: i32 = 400;
    /// A field or method of the enclosing type, its supertypes included.
    pub const OWN_MEMBER: i32 = 200;
    /// A member reached through an `import static`.
    pub const STATIC_IMPORT: i32 = 100;
}

/// How relevant a lexically-bound name is at the caret — see [`band::BINDING`].
///
/// `depth` and `distance` say the same thing at two scales and both are needed. Depth separates
/// the scopes: a variable in this `if` block beats a parameter of the method around it. Distance
/// separates names *within* one scope, where depth cannot tell them apart at all — three locals
/// declared in the same block are equally deep, and the one on the line above is the one being
/// reached for.
pub fn score_binding(
    name: &str,
    is_parameter: bool,
    depth: usize,
    distance: usize,
    ctx: &Context,
) -> i32 {
    let mut s = band::BINDING;
    s -= (depth as i32 * 4).min(60);
    // Bounded, and coarse on purpose: this is meant to order the locals of one block, not to make
    // the top of a long method unreachable.
    s -= (distance / 160).min(20) as i32;
    // What the method was handed. At the top of a body, before anything has been declared, it is
    // almost always what is being reached for — and the depth term alone puts it below every
    // local, however far above the caret that local was declared.
    if is_parameter {
        s += 6;
    }
    if let Some(n) = ctx.uses.get(name) {
        s += 6 * (*n).min(MAX_COUNTED_USES) as i32;
    }
    s
}

/// How relevant `m` is, where `declaring` is the binary that declares it and `depth` is how many
/// levels up the hierarchy it was found (`0` = the receiver's own type). Higher is better; the
/// caller sorts descending and breaks ties on the stable `(kind, label)` order.
///
/// The weights are chosen so the classes of signal cannot swap places by accident: being wrong for
/// the receiver dominates hierarchy distance, which dominates familiarity. Within a class the
/// numbers are only ordering, not measurement.
pub fn score(m: &Member, declaring: &str, depth: usize, ctx: &Context) -> i32 {
    let mut s = 0i32;

    // Object's members match every prefix on every receiver. This is the term that changes how the
    // list reads more than any other.
    if declaring == OBJECT {
        s -= 60;
    }

    // Inherited is weaker than declared, and keeps weakening — bounded, so a deep framework
    // hierarchy does not end up ranked purely by shape.
    s -= (depth as i32 * 3).min(30);

    if is_deprecated(m) {
        s -= 40;
    }

    if ctx.receiver_is_type {
        // `Color.RED` and `Color.valueOf(..)` are the question. `Color.equals(..)` is not a program.
        if m.is_static {
            s += 25;
            // A constant is what a type receiver is reached for most often of all.
            if m.kind == MemberKind::Field && m.is_final {
                s += 5;
            }
        } else {
            s -= 35;
        }
    } else if m.is_static {
        // Legal through an instance, and almost always a mistake in the reading — worth demoting,
        // not worth hiding.
        s -= 8;
    }

    // What the POSITION wants. The list is ordered by [`Fit`] before it is ordered by this score
    // (see the module docs), so this term does not decide between a fit and a miss. It stays for
    // what the score is still asked alone: which overload of a folded row is its face, and how a
    // `void` sinks among the misses.
    if ctx.wants(&m.return_type) {
        s += 40;
    } else if ctx.expected.is_some() && m.return_type.binary_name == "void" {
        // `String s = list.clear()` does not compile. Nothing else about `clear` says so.
        s -= 20;
    }

    // After a `::`, the methods the slot can actually take. As strong as the expected type, for the
    // same reason: `map(ResolvedIdentity::|)` has a handful of methods that compile and a class full
    // that do not. Ranked, not filtered — the fit is by name, and a subtype parameter is a miss.
    // A static reached through a VALUE never compiles (`resolver::staticMethod`), so it sinks hard.
    if let Some(shape) = &ctx.reference {
        if fits_reference(m, shape) {
            s += 45;
        } else if m.is_static && !shape.through_type {
            s -= 60;
        }
    }

    // Something this file already says. Weak on its own, decisive between equals.
    if let Some(n) = ctx.uses.get(&m.name) {
        s += 6 * (*n).min(MAX_COUNTED_USES) as i32;
    }

    // What you picked here last time. Deliberately weaker than the expected type — the position
    // constrains what compiles, and a habit only says what is likely — and stronger than the
    // buffer's own use count, which is the same signal read off a worse source.
    s += crate::picked::weight(declaring, &m.name);

    // A member you cannot see from anywhere else is, where it IS offered, usually your own.
    if m.visibility == Visibility::Private {
        s += 2;
    }

    s
}

/// Where `m`, found `depth` levels up the walk in `declaring` (whose members are `declared`), stands
/// relative to the receiver — see [`MemberOrigin`].
///
/// The one judgement call is a record: its `equals`, `hashCode` and `toString` are declared ON the
/// record (so they resolve without `java.lang.Object` indexed), and yet nobody wrote them. They read
/// as what they are — `Object`'s contract, implemented for you — so they rank with the inherited
/// members, after the record's components, and are not drawn as the record's own. A record that
/// writes one itself cannot be told apart from here and is treated the same way; that costs a row's
/// position, not its presence.
pub fn origin(m: &Member, declaring: &str, depth: usize, declared: &ClassMembers) -> MemberOrigin {
    if declaring == OBJECT {
        MemberOrigin::Object
    } else if depth > 0 || (declared.flags.is_record && is_object_contract(m)) {
        MemberOrigin::Inherited
    } else {
        MemberOrigin::Own
    }
}

/// `equals(Object)`, `hashCode()`, `toString()` — the three methods of `Object` a record implements.
fn is_object_contract(m: &Member) -> bool {
    m.kind == MemberKind::Method
        && matches!((m.name.as_str(), m.params.len()), ("equals", 1) | ("hashCode", 0) | ("toString", 0))
}

/// Whether the member is marked `@Deprecated`.
///
/// Source-only, and knowingly: a member decoded from a class file carries no annotations through
/// the seam, so this never fires for a JDK or dependency member. The effect is that the signal is
/// sharpest on the code you own — which is the code you are editing.
///
/// Public because the completion item is flagged with the same answer that demoted it — the popup
/// draws it struck through, and the two must never disagree about which member that is.
pub fn is_deprecated(m: &Member) -> bool {
    m.annotations.iter().any(|a| a.name == "Deprecated")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn method(name: &str) -> Member {
        Member::method(name, TypeRef::simple("void"), Vec::new())
    }

    fn ctx(receiver_is_type: bool) -> Context {
        Context::new("", receiver_is_type)
    }

    /// A resolver over a tiny hand-written hierarchy: `ArrayList implements List`,
    /// `Order extends Object`.
    struct Hierarchy;
    impl TypeResolver for Hierarchy {
        fn members_of(&self, binary: &str) -> Option<std::sync::Arc<bennu_java::prelude::ClassMembers>> {
            let (superclass, interfaces) = match binary {
                "java/util/ArrayList" => (Some(TypeRef::simple(OBJECT)), vec![TypeRef::simple("java/util/List")]),
                "java/util/List" | OBJECT => (None, Vec::new()),
                "shop/Order" => (Some(TypeRef::simple(OBJECT)), Vec::new()),
                _ => return None,
            };
            Some(std::sync::Arc::new(bennu_java::prelude::ClassMembers {
                superclass,
                interfaces,
                methods: Vec::new(),
                fields: Vec::new(),
                flags: Default::default(),
                type_params: Vec::new(),
            }))
        }
        fn resolve_simple_name(&self, _n: &str, _i: &[bennu_java::prelude::Import]) -> Option<String> {
            None
        }
    }

    fn generic(name: &str, args: &[&str]) -> TypeRef {
        TypeRef { type_args: args.iter().map(|a| TypeRef::simple(*a)).collect(), ..TypeRef::simple(name) }
    }

    fn expecting(t: TypeRef) -> Context {
        ctx(false).expecting(Some(t))
    }

    #[test]
    fn the_expected_type_itself_is_an_exact_fit() {
        let c = expecting(TypeRef::simple("shop/Order"));
        assert_eq!(c.fit(&TypeRef::simple("shop/Order"), &Hierarchy), Fit::Exact);
        assert_eq!(c.fit(&TypeRef::simple("java/lang/String"), &Hierarchy), Fit::None);
    }

    /// `return size()` in a method returning `int` — and the boxed getter fits it too.
    #[test]
    fn a_primitive_and_its_box_are_one_fit() {
        let c = expecting(TypeRef::simple("int"));
        assert_eq!(c.fit(&TypeRef::simple("int"), &Hierarchy), Fit::Exact);
        assert_eq!(c.fit(&TypeRef::simple("java/lang/Integer"), &Hierarchy), Fit::Exact);
        assert_eq!(c.fit(&TypeRef::simple("long"), &Hierarchy), Fit::None);
    }

    /// `List<Order>` is wanted: `List<Order>` and a raw `List` fit, `List<String>` does not.
    #[test]
    fn type_arguments_that_visibly_differ_are_not_a_fit() {
        let c = expecting(generic("java/util/List", &["shop/Order"]));
        assert_eq!(c.fit(&generic("java/util/List", &["shop/Order"]), &Hierarchy), Fit::Exact);
        assert_eq!(c.fit(&TypeRef::simple("java/util/List"), &Hierarchy), Fit::Exact);
        assert_eq!(c.fit(&generic("java/util/List", &["E"]), &Hierarchy), Fit::Exact);
        assert_eq!(c.fit(&generic("java/util/List", &["java/lang/String"]), &Hierarchy), Fit::None);
    }

    #[test]
    fn a_subtype_fits_below_the_exact_type() {
        let c = expecting(generic("java/util/List", &["shop/Order"]));
        assert_eq!(c.fit(&generic("java/util/ArrayList", &["shop/Order"]), &Hierarchy), Fit::Subtype);
        assert!(Fit::Exact > Fit::Subtype && Fit::Subtype > Fit::None);
    }

    /// Everything is an `Object`, so an expected `Object` walks nothing and lifts nothing.
    #[test]
    fn an_expected_object_makes_no_subtype_a_fit() {
        let c = expecting(TypeRef::simple(OBJECT));
        assert_eq!(c.fit(&TypeRef::simple("shop/Order"), &Hierarchy), Fit::None);
    }

    #[test]
    fn with_nothing_expected_nothing_fits() {
        assert_eq!(ctx(false).fit(&TypeRef::simple("shop/Order"), &Hierarchy), Fit::None);
    }

    /// `map(ResolvedIdentity::|)` — a `Function<ResolvedIdentity, U>` written through the type.
    fn function_of_identity() -> ReferenceShape {
        ReferenceShape {
            params: vec![TypeRef::simple("acme/ResolvedIdentity")],
            returns: TypeRef::simple(OBJECT),
            qualifier: "acme/ResolvedIdentity".to_string(),
            through_type: true,
        }
    }

    fn returning(name: &str, ret: &str, params: &[&str]) -> Member {
        Member::method(name, TypeRef::simple(ret), params.iter().map(|p| TypeRef::simple(*p)).collect())
    }

    /// Unbound: the element IS the receiver, so a no-argument instance method fits.
    #[test]
    fn a_no_argument_instance_method_fits_a_function_of_its_own_type() {
        let shape = function_of_identity();
        assert!(fits_reference(&returning("identifier", "java/lang/String", &[]), &shape));
        assert!(!fits_reference(&returning("rename", "java/lang/String", &["java/lang/String"]), &shape));
    }

    /// A static takes the element as its argument.
    #[test]
    fn a_static_taking_the_element_fits_and_one_taking_nothing_does_not() {
        let shape = function_of_identity();
        let convert = returning("convert", "acme/Token", &["acme/ResolvedIdentity"]).stat();
        let create = returning("create", "acme/ResolvedIdentity", &[]).stat();
        assert!(fits_reference(&convert, &shape));
        assert!(!fits_reference(&create, &shape));
    }

    /// A `Function` returns a value, and a `void` method has none to give.
    #[test]
    fn a_void_method_does_not_fit_a_slot_that_wants_a_value() {
        assert!(!fits_reference(&returning("touch", "void", &[]), &function_of_identity()));
    }

    /// Through a VALUE the method is bound, and a static cannot be reached at all.
    #[test]
    fn through_a_value_the_method_takes_every_parameter_and_statics_never_fit() {
        let shape = ReferenceShape { through_type: false, ..function_of_identity() };
        assert!(fits_reference(&returning("describe", "java/lang/String", &["acme/ResolvedIdentity"]), &shape));
        assert!(!fits_reference(&returning("identifier", "java/lang/String", &[]), &shape));
        let convert = returning("convert", "acme/Token", &["acme/ResolvedIdentity"]).stat();
        assert!(!fits_reference(&convert, &shape));
    }

    /// The fit is worth a lot: it lifts `identifier` above an alphabetically-earlier `hashCode`-like
    /// neighbour declared on the same class that takes an argument.
    #[test]
    fn a_fitting_method_outranks_one_that_does_not_fit() {
        let c = ctx(false).for_reference(Some(function_of_identity()));
        let fits = score(&returning("identifier", "java/lang/String", &[]), "acme/ResolvedIdentity", 0, &c);
        let misses = score(&returning("equalsIgnoring", "boolean", &["java/lang/String"]), "acme/ResolvedIdentity", 0, &c);
        assert!(fits > misses, "{fits} should beat {misses}");
    }

    /// The headline case: `list.` should not open on `clone`, `equals` and `getClass`.
    #[test]
    fn an_objects_own_method_beats_an_inherited_object_one() {
        let own = score(&method("add"), "java/util/ArrayList", 0, &ctx(false));
        let inherited = score(&method("getClass"), OBJECT, 3, &ctx(false));
        assert!(own > inherited, "{own} should beat {inherited}");
    }

    /// `Color.wait(..)` is not a program, and it used to be offered beside the constants.
    #[test]
    fn a_type_receiver_puts_statics_above_instance_members() {
        let c = ctx(true);
        let constant = score(&Member::field("RED", TypeRef::simple("Color")).stat().final_(), "Color", 0, &c);
        let value_of = score(&method("valueOf").stat(), "Color", 0, &c);
        let name = score(&method("name"), "java/lang/Enum", 1, &c);
        assert!(constant > value_of, "a constant is what a type receiver is reached for");
        assert!(value_of > name, "{value_of} (static) should beat {name} (instance)");
    }

    /// The other way round: through a value, a static is unusual rather than wrong.
    #[test]
    fn a_value_receiver_demotes_statics_only_mildly() {
        let c = ctx(false);
        let instance = score(&method("size"), "java/util/ArrayList", 0, &c);
        let static_one = score(&method("copyOf").stat(), "java/util/ArrayList", 0, &c);
        assert!(instance > static_one);
        // Mildly: still far above the inherited Object noise.
        assert!(static_one > score(&method("hashCode"), OBJECT, 3, &c));
    }

    #[test]
    fn deprecated_sinks_but_is_still_offered() {
        let mut old = method("legacyName");
        old.annotations.push(bennu_java::prelude::Annotation {
            name: "Deprecated".into(),
            qualified: "Deprecated".into(),
            start: 0,
            end: 0,
            strings: Vec::new(),
            args: Vec::new(),
            positional: Vec::new(),
        });
        let c = ctx(false);
        assert!(score(&old, "com/acme/Order", 0, &c) < score(&method("name"), "com/acme/Order", 0, &c));
    }

    /// Familiarity decides between members that are otherwise equal, and cannot do more than that.
    #[test]
    fn use_in_this_file_breaks_a_tie_without_overturning_relevance() {
        let mut c = ctx(false);
        c.uses.insert("getStatus".to_string(), 9);
        let used = score(&method("getStatus"), "com/acme/Order", 0, &c);
        let unused = score(&method("getState"), "com/acme/Order", 0, &c);
        assert!(used > unused, "a name this file already uses comes first");
        // But not enough to lift an Object method over a declared one.
        c.uses.insert("toString".to_string(), 9);
        let familiar_noise = score(&method("toString"), OBJECT, 2, &c);
        assert!(unused > familiar_noise, "familiarity must not outrank relevance");
    }

    fn declared(is_record: bool) -> ClassMembers {
        ClassMembers {
            superclass: None,
            interfaces: Vec::new(),
            methods: Vec::new(),
            fields: Vec::new(),
            flags: bennu_java::prelude::ClassFlags { is_record, ..Default::default() },
            type_params: Vec::new(),
        }
    }

    #[test]
    fn a_member_is_own_inherited_or_objects() {
        let plain = declared(false);
        assert_eq!(origin(&method("prefix"), "acme/ServiceRoute", 0, &plain), MemberOrigin::Own);
        assert_eq!(origin(&method("legs"), "zoo/Animal", 1, &plain), MemberOrigin::Inherited);
        assert_eq!(origin(&method("getClass"), OBJECT, 1, &plain), MemberOrigin::Object);
        // A class that writes its own `toString` owns it.
        assert_eq!(origin(&method("toString"), "acme/Order", 0, &plain), MemberOrigin::Own);
    }

    /// A record's `equals`, `hashCode` and `toString` are declared on it and written by nobody.
    #[test]
    fn a_records_implicit_object_methods_read_as_inherited() {
        let record = declared(true);
        let equals = Member::method("equals", TypeRef::simple("boolean"), vec![TypeRef::simple(OBJECT)]);
        assert_eq!(origin(&equals, "acme/ServiceRoute", 0, &record), MemberOrigin::Inherited);
        assert_eq!(origin(&method("hashCode"), "acme/ServiceRoute", 0, &record), MemberOrigin::Inherited);
        // Its component accessors are its own.
        assert_eq!(origin(&method("prefix"), "acme/ServiceRoute", 0, &record), MemberOrigin::Own);
    }

    /// The counter is a scan, not a parse — but it must at least agree with itself.
    #[test]
    fn identifiers_are_counted_once_each_occurrence() {
        let c = Context::new("order.getStatus(); if (getStatus() == 1) { order.x; }", false);
        assert_eq!(c.uses.get("getStatus"), Some(&2));
        assert_eq!(c.uses.get("order"), Some(&2));
        assert_eq!(c.uses.get("nothing"), None);
    }
}
