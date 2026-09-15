//! `package.json`, read the way an editor needs it.
//!
//! Two questions, and the answer to both is a **span**:
//!
//! - *Which dependencies are declared, and where is each version string?* — so a version that is
//!   behind can be drawn over and replaced in place.
//! - *Which scripts are declared, and where is each name?* — so a script can be run from the line
//!   that declares it.
//!
//! Everything here is pure: text in, spans out. Nothing opens a socket and nothing touches the
//! network — [`registry`] knows the URL shape and the cache layout, and the module in `bennu-be`
//! that actually fetches is the one that needed splitting off. Same division as `bennu-cargo`, and
//! for the same reason: this half is testable without a network and the other half is not.

pub mod manifest;
pub mod prelude;
pub mod registry;
