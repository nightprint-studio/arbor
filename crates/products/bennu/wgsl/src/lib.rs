//! `bennu-wgsl` — what Bennu knows about a WGSL shader.
//!
//! Two engines, and the split between them is the whole design:
//!
//! * **[`validate`]** runs the shader through [`naga`] — the front end and validator wgpu
//!   compiles with, and therefore the one Bevy compiles with. An error reported here is an
//!   error the shader would really have hit at pipeline creation, rather than one an
//!   editor's own approximation of the grammar decided to invent. It answers the question
//!   "would this run".
//! * **[`bindings`]** reads the resources a shader declares — `@group @binding` — which is the
//!   half a Bevy material's `#[derive(AsBindGroup)]` has to agree with, and the only place the
//!   two files can be checked against each other.
//! * **[`symbols`]** is a tolerant text scanner. It answers "what is in this file" while
//!   you are still typing it — which is exactly when the compiler cannot answer anything,
//!   because half of what you have written is not valid yet. Outline, completion and
//!   find-usages ride on it, and they keep working through a syntax error.
//!
//! A single engine cannot do both jobs: a parser strict enough to be trusted about errors
//! is a parser that gives up on the file you are in the middle of writing, and one lenient
//! enough to survive that is not one whose silence means anything.
//!
//! ## Where the language server fits
//!
//! Above both. When `wgsl-analyzer` is installed it serves the file and this is not
//! consulted for diagnostics at all — see `bennu-be`'s `intel::bennu_diagnostics`. This is
//! what a project gets *without* installing anything, which for a shader in a Bevy game is
//! the overwhelmingly common case.

pub mod bindings;
pub mod builtins;
pub mod imports;
pub mod prelude;
pub mod symbols;
pub mod validate;
