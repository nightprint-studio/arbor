//! `bennu-check` — AST-level Java validation **without compiling**.
//!
//! The goal (docs: `bennu-indexing-validation-analysis.md`): surface the "red" errors a
//! legacy-dev wants *before* running Maven/javac, computed from the tree-sitter-java AST alone. This
//! crate holds the checks that need **no resolver** — pure syntax-tree scans, so they are
//! exhaustively unit-testable and can never produce a false "cannot resolve" (the resolver-backed
//! unresolved-symbol pass is a separate, conservative phase that plugs into the same
//! [`check_file`] aggregator later).
//!
//! Two tiers:
//!   * **pure-AST** ([`check_file`]) — syntax errors, invalid statements, missing/void returns,
//!     declaration & annotation legality, lambda capture, imports, naming, `package-info` /
//!     `module-info` shape, package-vs-location, and version-gated language features. No resolver, so
//!     exhaustively unit-tested and never a false "cannot resolve".
//!   * **resolver-backed** ([`check_file_resolved`]) — unknown members / fields, argument arity,
//!     unresolved types, cast / assignment / return-type compatibility, `extends`/`implements`
//!     legality, unimplemented abstract methods, and functional-interface / lambda arity. All share
//!     the conservative supertype walk in [`support::walk`] and run only when the JDK is resolvable.
//!
//! Every check returns the wire [`Diagnostic`](bennu_proto::prelude::Diagnostic) (UTF-8 byte
//! offsets) the Problems panel + lint gutter already render, so wiring is
//! `check_file_resolved(source, &ctx, resolver, jdk_available)` → done.
//!
//! ## Public API: use the [`prelude`]

pub mod annotation;
pub mod calls;
pub mod decls;
pub mod engine;
pub mod flow;
pub mod hierarchy;
pub mod source;
pub mod support;
pub mod switching;
pub mod throwing;
pub mod typing;
pub mod prelude;
