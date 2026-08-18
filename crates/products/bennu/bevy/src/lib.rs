//! Bevy ECS support for Bennu — read from the source, with no build and no running game.
//!
//! ## What it answers
//!
//! * **Which components, resources and events does this project declare, and who touches each?**
//!   A `#[derive(Component)]` is a declaration; a `Query<&mut Health>` is a *use*, and the two
//!   together are the answer to "what breaks if I change this" — the question a text search for
//!   `Health` answers with every comment that mentions the word.
//! * **Which pairs of systems can never run at the same time, and why?** Bevy derives that from
//!   system signatures at schedule-build time; so does this, from the same signatures.
//!
//! ## What it deliberately does not answer
//!
//! **The ordering graph.** Most of a Bevy app's systems come from plugins whose sources are not in
//! the project, so a picture of "what runs before what" drawn from this project's own
//! `add_systems` calls would be a fragment presented as a whole. The conflict report has the
//! opposite property and that is the reason it is here instead: a conflict cannot be undone by a
//! system nobody has read, so every claim it makes survives the parts of the app this crate cannot
//! see. See [`conflict`] for the full argument.
//!
//! ## How it reads Rust
//!
//! By **shape**, over a masked copy of the source ([`mask`]) — not with a parser. The subset that
//! matters is small and regular (an attribute before an item, a parameter list, the arguments of
//! one call), and a scan that tolerates a half-written file is worth more in an editor than one
//! that needs a valid one. The cost is real and is stated where it bites: names are not resolved,
//! so `Health` is a string and two `Health`s in two modules look like one ([`params`]), and a
//! registration made through a helper this scan cannot follow leaves a system with no schedule
//! ([`build`]). Both are visible in the UI rather than smoothed over.

pub mod build;
pub mod catalog;
pub mod conflict;
pub mod editor;
pub mod ext;
pub mod items;
pub mod mask;
pub mod model;
pub mod params;
pub mod prelude;
pub mod wrappers;
