//! `bennu-lombok` — what Lombok generates, as data.
//!
//! Lombok moves declarations out of the source and into the compiled class: `@Data` writes the
//! getters, `@RequiredArgsConstructor` writes the constructor that assigns the `final` fields,
//! `@Slf4j` writes the `log` field. Every part of Bennu that reads Java therefore has to answer the
//! same two questions before it can say anything about such a file — *is this annotation really
//! Lombok's?* and *what does it generate?* — and each part used to answer them with its own hand-
//! rolled list.
//!
//! That is exactly how they drifted: the blank-final check knew `@Data` and `@Value` but not
//! `@Builder`, so a `@Builder` class had every one of its `final` fields reported as never
//! initialised — while the index, three crates away, already modelled `@Builder` perfectly well.
//! One catalogue, one gate, one answer.
//!
//! ## Shape
//!
//! The crate knows nothing about *how* a file was read. It takes names and import paths and returns
//! verdicts, so the tree-sitter checks in `bennu-check` and the symbol-model index in `bennu-intel`
//! feed the same functions from their own representations:
//!
//!   * [`annotations`] — the catalogue. Which simple names Lombok defines, and what each one emits:
//!     [`generates_constructor`](annotations::generates_constructor),
//!     [`initializes_blank_finals`](annotations::initializes_blank_finals),
//!     [`generates_members`](annotations::generates_members).
//!   * [`imports`] — the capability gate. Lombok only *does* anything when the annotation resolves
//!     to it, which requires the import — so a project's own `@Data` in another package must never
//!     silence a check. [`ImportPath`](imports::ImportPath) is the borrowed shape both consumers
//!     map onto; [`parse_import_declaration`](imports::parse_import_declaration) builds one from an
//!     `import …;` written in source.
//!
//! Zero dependencies, by construction: it is `str` in and `bool` out.
//!
//! ## Public API: use the [`prelude`]

pub mod annotations;
pub mod imports;
pub mod prelude;
