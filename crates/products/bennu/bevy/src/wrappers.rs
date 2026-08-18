//! System parameters that stand for an access this crate cannot see.
//!
//! A `#[derive(SystemParam)]` struct is a parameter made of other parameters, and [`crate::build`]
//! expands the ones a **project declares itself**: its fields are in the scan, so the accesses are
//! readable. Two kinds are not.
//!
//! * One in a **dependency** — its source is not in the scan at all.
//! * One that is **generic over what it wraps**. `DomainResParam<'w, S> { resource: Res<'w,
//!   PerDomainResource<S>> }` cannot be expanded by substituting text: the field names a type
//!   parameter, and only knowing *which* `S` the call site passed makes it an access.
//!
//! Both are the same shape from the outside — `Wrapper<T>` means "the access `T`, through a layer"
//! — and both are answered here, by a table. It is a small table on purpose: a wrapper earns a row
//! when leaving it out makes a whole project's data look untouched, which is what a game engine's
//! own parameter layer does. Everything else stays shape-driven.
//!
//! Getting one wrong is a *false* access, so the entries say only what the wrapper's own definition
//! says. The `fulcrum` rows below are read off `fulcrum-domain`'s `system_param/` module: each of
//! those structs holds exactly the `Res` / `ResMut` / `MessageReader` / `MessageWriter` this claims
//! it does.

/// What a wrapper's type argument turns out to be an access to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Effect {
    /// `Wrapper<T>` reads the resource `T`.
    ResourceRead,
    /// `Wrapper<T>` writes the resource `T`.
    ResourceWrite,
    /// `Wrapper<T>` reads the message buffer of `T`.
    MessageRead,
    /// `Wrapper<T>` writes the message buffer of `T`.
    MessageWrite,
    /// `Wrapper<D, F>` is a `Query<D, F>` with a filter of its own bolted on.
    QueryLike,
}

/// The known wrappers, by the last segment of their type path.
const WRAPPERS: &[(&str, Effect)] = &[
    // ── fulcrum-domain ───────────────────────────────────────────────────────
    //
    // The engine's per-domain layer: N documents open at once, each with its own copy of every
    // resource, state and message queue. A game written on it declares `#[derive(DomainResource)]`
    // and never writes `Res` again — so without these rows every declaration in the project reads
    // as touched by nothing, which is exactly what it looked like.
    ("DomainResParam", Effect::ResourceRead),
    ("DomainResMutParam", Effect::ResourceWrite),
    // Reading a domain state is a `Res<PerDomainState<S>>`; the Mut form additionally holds the
    // writer that *requests* a transition. Recorded as a write of `S` rather than as a write of the
    // request queue: what a reader of that state contends with is this parameter, and naming the
    // queue would pair it with nothing.
    ("DomainStateParam", Effect::ResourceRead),
    ("DomainStateMutParam", Effect::ResourceWrite),
    ("DomainMessageReader", Effect::MessageRead),
    ("DomainMessageWriter", Effect::MessageWrite),
    ("DomainQuery", Effect::QueryLike),
];

/// What `head` wraps, if this crate knows it.
pub fn lookup(head: &str) -> Option<Effect> {
    WRAPPERS.iter().find(|(name, _)| *name == head).map(|(_, effect)| *effect)
}

