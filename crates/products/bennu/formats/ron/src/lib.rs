//! `bennu-ron` — what Bennu says about a `.ron` file.
//!
//! Three questions, three modules, and the split follows what each needs from the text:
//!
//! * **[`syntax`]** — is it RON, and if not, where does it stop being. Brackets first, because an
//!   unclosed `(` is reported by a parser at the *end of the file*, which is where nobody is
//!   looking; this reports it at the opener. Anything else is `arbor-studio-ron`'s parser's word.
//! * **[`scan`]** — a walk of the document that keeps **byte spans**, which `arbor-studio-ron`'s
//!   tree deliberately does not (it exists to be diffed and re-serialised, not to be pointed at).
//!   Only run on text the real parser already accepted, so it can be strict and give up quietly.
//! * **[`validate`]** — the walk checked against a Rust type's [`Schema`](arbor_studio_types::prelude::Schema):
//!   a field the type does not have, a required one the document does not, a variant that is not
//!   one, a string where a number goes.
//!
//! [`directive`] finds the `// ron-studio: schema=…, root=…` comment that says *which* type, using
//! the detection Studio already uses, so a file bound for Studio is bound for the editor too.
//!
//! ## What the schema cannot say
//!
//! The loader resolves `rename`, `alias`, `default` and `flatten` on **fields**. It does not see a
//! container-level `#[serde(default)]`, a field `skip` / `with`, a variant `rename`, or an
//! `untagged` enum. So the checks are tuned to be wrong in the quiet direction: a missing field is
//! an *info*, an unknown field or variant a *warning* (serde ignores the former and a typo is
//! almost always what it is), and anything whose shape does not match what was expected is skipped
//! rather than guessed at.

pub mod directive;
pub mod prelude;
pub mod scan;
pub mod syntax;
pub mod validate;

/// How serious a finding is. Mapped onto the editor's severities by the backend.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RonSeverity {
    Error,
    Warning,
    Info,
}

/// One finding, in **UTF-8 byte offsets** into the text it was found in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RonDiagnostic {
    pub start: usize,
    pub end: usize,
    pub severity: RonSeverity,
    /// Stable kebab-case kind slug.
    pub code: &'static str,
    pub message: String,
}
