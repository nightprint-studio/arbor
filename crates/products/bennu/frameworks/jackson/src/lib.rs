//! `bennu-jackson` — what a DTO actually serialises to.
//!
//! ## The shape of the problem
//!
//! Jackson never fails loudly for the things that go wrong most often. A field with no accessor is
//! simply **absent** from the JSON; a property two members both claim throws only when that
//! particular object is serialised, which in practice is in front of a customer; a `@JsonIgnore`
//! that meant "not on the way out" silently takes the property off the way in as well.
//!
//! None of it is visible in the class. What you see is a field, spelled correctly, with an
//! annotation on it. What the caller sees is a payload without it — and the first person to notice
//! is on the other side of an HTTP call, days later, with no reason to suspect the DTO.
//!
//! ## What it will not claim
//!
//! Jackson's visibility rules are configurable, globally, at runtime: an `ObjectMapper` with a
//! changed `VisibilityChecker`, a `@JsonAutoDetect` on the class, a mix-in registered in a
//! `@Configuration`. So the accessor check runs **only** where the class says plainly what it is —
//! a class that carries Jackson annotations, has no `@JsonAutoDetect` of its own, and generates no
//! accessors through Lombok. Everywhere else it stays quiet.
//!
//! That gate is the whole design. Without it the check fires on every Lombok DTO in the project,
//! which is all of them, and a check that is wrong about all of them is a check that gets turned
//! off before it is ever right about one.

// A `@JsonCreator` whose argument names the build does not keep.
pub mod creator;
pub mod dto;
// The extension itself.
pub mod ext;
pub mod prelude;
