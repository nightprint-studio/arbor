//! # arbor-bevy-remote-derive
//!
//! `#[derive(RemoteInspect)]` — field attributes that tell Arbor's Bevy Remote inspector how to draw a value:
//!
//! ```ignore
//! use arbor_bevy_remote::prelude::*;
//!
//! #[derive(Component, Reflect, RemoteInspect)]
//! struct Lamp {
//!     #[inspect(color)]
//!     tint: [f32; 3],
//!     #[inspect(range = 0.0..=1.0, step = 0.01)]
//!     intensity: f32,
//!     #[inspect(angle)]
//!     beam: f32,
//!     #[inspect(label = "speed", unit = "m/s", readonly)]
//!     measured: f32,
//! }
//!
//! app.register_inspect::<Lamp>();
//! ```
//!
//! The derive only implements `arbor_bevy_remote::inspect::RemoteInspect` (a list of per-field hints); the
//! companion crate's `register_inspect` stores them and answers `arbor.inspect_hints` with them.
//!
//! This is a proc-macro crate, so it exports nothing but the macro: depend on `arbor-bevy-remote`, whose
//! prelude re-exports it next to the trait of the same name.

mod expand;
mod parse;

use proc_macro::TokenStream;
use syn::{DeriveInput, parse_macro_input};

/// See the crate docs for the `#[inspect(...)]` options.
#[proc_macro_derive(RemoteInspect, attributes(inspect))]
pub fn derive_remote_inspect(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    expand::expand(&input).unwrap_or_else(syn::Error::into_compile_error).into()
}
