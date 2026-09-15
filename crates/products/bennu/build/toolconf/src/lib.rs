//! `bennu-toolconf` — the build-tool configuration files that have a **documented, versioned
//! vocabulary** behind them.
//!
//! Today that is three files: `lombok.config`, `junit-platform.properties` and
//! `struts.properties`. All three look like every other `.properties` in a Java tree and none is: their keys are not names somebody chose, they
//! are an API, published per release, with defaults and legal values written down. An editor that
//! knows that turns the file from something you keep a browser tab open beside into something you
//! can read.
//!
//! ## Shape
//!
//! One engine, two tables:
//!
//! * [`props`] reads the syntax — which is `Properties` plus Lombok's `+=` / `-=` list operators and
//!   its `clear` statement, and those extras are the whole reason it is not one `split('=')`;
//! * [`model`] is what a documented key *is*, and the rule that decides whether this project's
//!   version of the tool understands it;
//! * [`answers`] turns a table plus a version into the four editor answers — completion, hover,
//!   ghost text, diagnostics — and knows nothing about which tool it is serving;
//! * [`lombok`], [`junit`] and [`struts`] are the tables;
//! * [`version`] answers "which version is this project actually on", from the poms;
//! * [`ext`] is the [`FrameworkExtension`](bennu_ext::prelude::FrameworkExtension) a host registers.
//!
//! A third file of this shape costs a table and one line in the extension.
//!
//! ## The standard the tables are held to
//!
//! Two rules, both of them the "under-report rather than risk a false positive" rule from bennu's
//! own docs §7, applied to data:
//!
//! 1. **a `since` is recorded only where the introducing release is certain.** A key with no `since`
//!    is offered to every project — so not knowing costs a key too many, never a key missing from a
//!    project that has it;
//! 2. **there is no "unknown key" diagnostic.** The tables are the keys worth documenting, not a
//!    transcription of every constant the tool defines, so a key that is not in one means *not
//!    written down here* and never *not understood*.
//!
//! ## Public API: use the [`prelude`]

pub mod answers;
pub mod ext;
pub mod junit;
pub mod lombok;
pub mod model;
pub mod prelude;
pub mod props;
pub mod struts;
pub mod version;
