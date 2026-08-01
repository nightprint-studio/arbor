//! `garrulus-ast` — what a note *is*, with no syntax attached.
//!
//! The hinge of the product. A note is described once — headings, paragraphs,
//! lists, tasks, links, tags, callouts, code, tables, frontmatter — in a form that
//! mentions no markdown. Reading turns a source format into that description;
//! writing turns the description back into a target format. Everything in between
//! (the index, backlinks, search, the outline, refactors, HTML and PDF export)
//! works on the description and therefore works for every format at once.
//!
//! Three rules this crate exists to keep:
//!
//! * **The model mentions no syntax.** There is no `Asterisk`, no `FenceChar`, no
//!   `[[`. [`block::Block::Callout`] carries a [`block::CalloutKind`], not the
//!   string `"> [!WARNING]"`. The day a second [`io::Reader`] appears — org-mode,
//!   AsciiDoc, a Notion export — everything downstream keeps working untouched.
//! * **Frontmatter round-trips byte-stable when untouched.** Reformatting YAML on
//!   every save would turn the whole vault into a diff the first time it is opened
//!   and make the sync history worthless. [`frontmatter::Frontmatter`] therefore
//!   keeps the raw source alongside the parsed entries and hands it back verbatim
//!   until somebody actually edits a field. This is a hard invariant with a test.
//! * **Spans are UTF-8 byte offsets into the source.** Not char indices, not
//!   line/column: the editor, the index and every "jump to this link" affordance
//!   slice the original string, and byte offsets are the only unit that survives
//!   the trip through the grammar unambiguously.
//!
//! No parser lives here. `garrulus-parse` supplies `MarkdownReader` /
//! `MarkdownWriter`; this crate must stay buildable with no grammar in sight.
//!
//! ## Public API: use the [`prelude`]

pub mod block;
pub mod document;
pub mod frontmatter;
pub mod inline;
pub mod io;
pub mod prelude;
pub mod span;
pub mod walk;
