//! `bennu-generate` — code generators.
//!
//! Every generator is a unit struct implementing [`registry::Generator`]: metadata for the palette and
//! menus, the options a dialog opens with, and a **pure** function from `options + context` to
//! [`output::Output`] (new files, members for the class at the caret, snippets for other files, notes).
//! No generator reads a file or the index: the backend fills a [`context::GenContext`] and writes the
//! result, which is what keeps each of them testable with a hand-built context.
//!
//! | piece | role |
//! |---|---|
//! | [`code`] | indenting line buffer, import set, compilation-unit writer |
//! | [`names`] | case conversion and type-text helpers |
//! | [`context`] | project facts (Spring/Jakarta/Java/Lombok), class at the caret, endpoints |
//! | [`output`] | the shapes a generator returns, and option validation |
//! | [`registry`] | the list of generators and `run(id, options, ctx)` |
//! | [`generators`] | the generators themselves, one module each |

pub mod code;
pub mod context;
pub mod generators;
pub mod layout;
pub mod names;
pub mod output;
pub mod prelude;
pub mod registry;
pub mod soap;
pub mod wire;

#[cfg(test)]
pub(crate) mod testing;
