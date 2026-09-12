//! `bennu-templates` — code generated from templates the user owns.
//!
//! A template is a Jinja file of a **kind**, and the kind decides three things: what it is rendered
//! with, where its output goes, and which templates Bennu ships for it.
//!
//! | kind | rendered with | output |
//! |---|---|---|
//! | `new-file` | the name typed and the package its folder implies | a new file |
//! | `class` | the class at the caret | members inside it, a new file beside it, or text |
//! | `config-properties` | the keys a `@ConfigurationProperties` class binds | text for a property file |
//! | `config-class` | the keys under a prefix of a configuration file ([`config_tree`], [`config_class`]) | a class file |
//! | `validation-tests` | the DTO Lab's cases (context in `bennu-dtolab`) | a test file |
//! | `live` | the file an abbreviation is typed in | a snippet at the caret |
//!
//! The pieces every kind shares live here: the engine and its filters ([`engine`]), the directives a
//! template writes about itself (`{# bennu.output: file #}`), where templates are kept ([`store`]),
//! the class model most kinds read ([`model`]), inserting members into a class ([`placement`]), which
//! template line wrote each line of a render ([`line_map`]), `style`'s declarations ([`style`]), the project's
//! naming ([`naming`]) and the imports an output needs ([`imports`]).
//!
//! ## Where templates live
//!
//! In the profile, one directory per kind: `bennu/templates/<kind>/<name>.<extension>.jinja`. A
//! template describes how **a person** writes code, which does not change from one of their projects
//! to the next; a project only chooses which one it uses. Built-in templates are compiled in and
//! cannot be overwritten, so every kind always has one that works — or, for a kind where a built-in
//! would only duplicate what Bennu already does, a *starter* that exists to be copied.

pub mod config_class;
pub mod config_keys;
pub mod config_props;
pub mod config_tree;
pub mod config_types;
pub mod config_yaml;
pub mod contexts;
pub mod dates;
pub mod engine;
pub mod facts;
pub mod imports;
pub mod kind;
pub mod line_map;
pub mod model;
pub mod names;
pub mod naming;
pub mod placement;
pub mod requires;
pub mod prelude;
pub mod store;
pub mod style;
