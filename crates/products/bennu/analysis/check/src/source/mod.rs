//! Compilation-unit rules: parse errors, statements, generics syntax, imports, file and package agreement, language-version gates.

pub mod generics_syntax;
pub mod import_ambiguity;
pub mod import_clash;
pub mod imports;
pub mod naming;
pub mod packaging;
pub mod special_files;
pub mod statements;
pub mod syntax;
pub mod var_target;
pub mod version;
