//! `bennu-jsp` — the JSP tag-library model.
//!
//! A legacy page is mostly not HTML and mostly not Java: it is `<s:…>`, `<c:…>`,
//! `<wp:…>` — a vocabulary defined in files that ship inside the framework's own jars
//! and that no editor was reading. This crate reads them, and answers from them.
//!
//! | File | Holds |
//! |---|---|
//! | `tld.rs` | the TLD model + its parser, one shape for the 1.2 and 2.1 generations |
//! | `directives.rs` | the page's `<%@ taglib %>` declarations, with spans |
//! | `catalog.rs` | which library a `uri="…"` means, and where its TLD is |
//! | `intel.rs` | completion, checks, hover and go-to over the two |
//! | `model.rs` | the page as Bennu understood it — the syntax-tree panel's model tab |
//! | `ext.rs` | the [`FrameworkExtension`] impl the backend registers |
//!
//! Almost nothing here parses a page: the tag scan is `bennu-xml`'s tolerant one — a
//! taglib tag is an XML tag, and a `<%` is not a tag name, so a scriptlet is invisible to
//! it, which is the wanted behaviour, and it costs nothing on a buffer mid-keystroke.
//! `model.rs` is the exception, and the reason is in its own doc: a panel whose subject is
//! the parse cannot be drawn from an approximation of it. It reads the real grammar
//! (`bennu-jsp-grammar` — the same generate the editor loads as wasm).
//!
//! Not here yet, and named so the boundary is clear: the EL / OGNL AST and the
//! value-stack resolver (`bennu-web` carries the Struts half of that today).
//!
//! ## Public API: use the [`prelude`]
//!
//! Workspace convention: call sites reach this crate's surface through
//! `bennu_jsp::prelude::...`.
//!
//! [`FrameworkExtension`]: bennu_ext::prelude::FrameworkExtension

pub mod catalog;
pub mod directives;
pub mod ext;
pub mod intel;
pub mod model;
pub mod prelude;
pub mod tld;
