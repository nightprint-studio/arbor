//! `bennu-i18n` — message bundles for Bennu, as a framework extension.
//!
//! Half of what a legacy web application shows a user is not in its source. It is in a
//! `.properties` file, reached by a string, and that string is checked by nothing: not the
//! compiler, not the tests, and — because Struts renders an unresolved key as the key itself —
//! frequently not QA either. A screen that says `note.login.expiredPassword.intro` in production
//! is a typo somebody made months earlier in a page nobody reopened.
//!
//! ## What it knows
//!
//! - **Bundles** ([`bundle`], [`catalog`]) — every `.properties` file grouped by the bundle it
//!   translates, with each key's byte span so a jump lands on the line rather than the file.
//! - **References** ([`refs`]) — every place a key is read, recognised by SHAPE rather than by a
//!   list of tags: an attribute called `key` or ending in `Key`, the `name` of a `*:text` tag,
//!   the first string argument of `getText` / `getMessage` / `getString`.
//! - **What follows from both** — go-to onto the declaring line, hover showing what the key
//!   actually says in each language, the keys nothing declares, and the keys nothing uses.
//!
//! ## The rule that shapes the checks
//!
//! Under-report rather than risk a false positive. A computed value (`%{keyName}`,
//! `${row.label}`) is not treated as a key at all — it usually is one at runtime, but nothing
//! here can say which, and a check that guessed would flag every dynamic label in the project. A
//! project whose bundles did not resolve says nothing rather than declaring every key on the page
//! unknown.
//!
//! ## Public API: use the [`prelude`]
//!
//! Workspace convention: call sites reach this crate's surface through
//! `bennu_i18n::prelude::...`.

// One `.properties` file: its bundle, its locale, its keys.
pub mod bundle;
// Every bundle in the project, indexed by key.
pub mod catalog;
// The extension — what a host registers.
pub mod ext;
pub mod prelude;
// Where a key is read.
pub mod refs;
