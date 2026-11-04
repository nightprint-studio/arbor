//! `hirundo-model` — what a Hirundo request *is*, and every rule that needs no network or disk.
//!
//! | Module | What it holds |
//! |---|---|
//! | [`method`] | HTTP methods, and which of them change things |
//! | [`kv`] | key/value rows (params, headers, variables) and their **bulk-edit text** form |
//! | [`auth`] | Bearer / Basic / API key, and inheritance down collection → folder → request |
//! | [`env`] | environment **types** (colour, protection, variable set), environments, the confirmation rule |
//! | [`vars`] | layered variable resolution and `{{interpolation}}` |
//! | [`request`] | the request itself, its body and settings |
//! | [`prepare`] | request + inherited auth/headers + variables → exactly what goes on the wire |
//! | [`curl`] | `curl …` → request, and request → `curl …` |
//! | [`soap`] | SOAP: protocol headers, WS-Security token, faults, XML indenting |
//! | [`format`] | the readable one-file-per-request TOML format, and the collection/environment files |
//! | [`slug`] | file-name slugs |
//!
//! ## Public API: use the [`prelude`]

pub mod assertion;
pub mod auth;
pub mod cookies;
pub mod curl;
pub mod env;
pub mod format;
pub mod httpfile;
pub mod kv;
pub mod method;
pub mod multipart;
pub mod openapi;
pub mod prelude;
pub mod prepare;
pub mod request;
pub mod slug;
pub mod soap;
pub mod sse;
pub mod vars;

mod tomlw;
