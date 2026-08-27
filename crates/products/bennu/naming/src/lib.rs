//! `bennu-naming` — a declaration whose name breaks the project's convention, and the name that
//! would not.
//!
//! ## The one idea
//!
//! A convention is a **function from words to a spelling**, not a pattern that accepts or refuses
//! one. `get_user_name` and `getUserName` are the same three words; a rule that can render those
//! words as camelCase can both *detect* the violation and *repair* it, and it is the same code
//! doing both. That is why [`convention::Convention`] is an enum and not a regex: a regex can
//! refuse a name, it cannot build one, and a naming check that cannot build one is a list of
//! complaints.
//!
//! Everything else follows from it — the quick-fix is free, it is idempotent by construction, and
//! the rule shown in the message is provably the rule that was applied.
//!
//! ## Shape: a feature pack, with two ways to see a declaration
//!
//! The crate is a leaf — declarations in, [`Diagnostic`](bennu_proto::prelude::Diagnostic) out. It
//! knows nothing about projects, the index, the resolver or the filesystem, which is what lets it
//! be switched off for nothing (the default) and, later, be something a user installs rather than
//! something the editor always carries.
//!
//! A language supplies its declarations one of two ways ([`pack::DeclSource`]):
//!
//! * from a **grammar** parsed here — exact and complete, locals and parameters included. Java,
//!   because Bennu's Java engine is its own and there is no server to ask;
//! * from a **language server's outline** — no new grammar, a whole family of languages at once,
//!   but an outline holds types and their members and *no server reports locals or parameters*.
//!   TypeScript, JavaScript and Rust, whose servers Bennu already speaks to.
//!
//! [`pack::Pack::supports`] is how that limit reaches the user: a settings screen greys out a
//! target the pack can never report, rather than offering a rule that would silently do nothing.
//!
//! ## Safety of the fix is a property of the declaration *and where it came from*
//!
//! [`target::Target::is_file_local`] says a local or a parameter cannot be referred to from
//! outside its file, so renaming one is exact. That holds for a declaration a **grammar** found.
//! It does not hold for one an **outline** reported — an outline contains top-level and member
//! declarations, so something it calls a variable is exactly what another file imports.
//! [`pack::Pack::fix_is_file_local`] combines the two, and every caller goes through it: getting
//! this wrong would rename an exported symbol across a project with no preview.
//!
//! Everything else — a method, a field, a type — can be reached by a caller, by reflection, or by
//! a framework binding a name out of an XML or JSP file that no grammar here reads. Those are
//! offered one at a time, through the project's real semantic engine, never rewritten in place.
//!
//! ## Public API: use the [`prelude`]

pub mod config;
pub mod convention;
pub mod java;
pub mod pack;
pub mod prelude;
pub mod scan;
pub mod skip;
pub mod symbols;
pub mod target;
pub mod words;
