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
//!     the conservative supertype walk in [`walk`] and run only when the JDK is resolvable.
//!
//! Every check returns the wire [`Diagnostic`](bennu_proto::prelude::Diagnostic) (UTF-8 byte
//! offsets) the Problems panel + lint gutter already render, so wiring is
//! `check_file_resolved(source, &ctx, resolver, jdk_available)` → done.
//!
//! ## Public API: use the [`prelude`]

pub mod annotations;
pub mod arguments;
pub mod arity;
pub mod casts;
pub mod check;
pub mod constructors;
pub mod declarations;
pub mod duplicates;
pub mod fields;
pub mod functional;
pub mod imports;
pub mod inheritance;
pub mod lambdas;
pub mod members;
pub mod naming;
pub mod packaging;
pub mod prelude;
pub mod resolve;
pub mod returns;
pub mod special_files;
pub mod statements;
pub mod switches;
pub mod syntax;
pub mod types;
pub mod version;
pub mod walk;
