//! Canonical entry point for `bennu-wgsl`'s public API.
//!
//! Workspace convention: call sites reach this crate's surface through
//! `bennu_wgsl::prelude::...`. The submodules stay `pub` for rustdoc navigation, but the
//! prelude is the canonical call-site path.

pub use crate::bindings::{scan as scan_bindings, Binding};
pub use crate::builtins::{
    completions_for, Builtin, ATTRIBUTES, BUILTIN_FUNCTIONS, BUILTIN_TYPES, BUILTIN_VALUES,
    KEYWORDS,
};
pub use crate::imports::{
    context_at as import_context_at, defined_path, parse_imports, ImportContext, ImportPath,
    ImportedPath, BEVY_IMPORTS,
};
pub use crate::library::{LibrarySymbol, ShaderLibrary, ShaderModule};
pub use crate::symbols::{
    doc_above, signature_at,
    occurrences_of, scan as scan_symbols, symbol_at, WgslSymbol, WgslSymbolKind,
};
pub use crate::preview_hints::{hints_before, PreviewHint};
pub use crate::preview_layout::{
    has_vertex_entry, image_for_key, preview_plan, preview_plan_with, texture_key, PlacedBinding, PreviewCaps,
    PreviewLayout, PreviewPlan, Rejected, SlotFamily, EXTENSION_BASE, IMAGES, SAMPLER_SLOTS,
    TEXTURE_2D_ARRAY_SLOTS, TEXTURE_2D_SLOTS, TEXTURE_CUBE_SLOTS, UNIFORM_SLOTS,
    VIEWPORT_SAMPLER_OWNING_SLOTS, VIEWPORT_SAMPLER_SLOTS,
    VIEWPORT_TEXTURE_2D_OWNING_SLOTS, VIEWPORT_TEXTURE_2D_SLOTS,
};
pub use crate::uniforms::{
    material_bind_group, material_uniform, MaterialBindGroup, MaterialResource, ResourceKind,
    UniformBlock, UniformField,
};
pub use crate::validate::{
    preprocessor_reason, validate, WgslDiagnostic, WgslReport, WgslSeverity,
};
