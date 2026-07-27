//! `providers` domain — the per-engine descriptors the UI renders from.
//!
//! This is the handler that makes §4.3 of `docs/picus-design.md` real: instead of
//! `if (dialect === 'oracle')` scattered across components, the frontend asks once
//! and renders what it is told — connection fields, capabilities, labels, colours,
//! emission traits.
//!
//! Engines with no driver still appear here, with `capabilities.connect = false`.
//! That is the honest shape of the product: Oracle is fully supported for scripts
//! and not connectable, and the UI needs to be able to say exactly that rather than
//! pretend the engine doesn't exist.

use picus_core::prelude::PicusState;
use picus_db_api::prelude::{
    DbProviderDescriptor, EmissionTraits, EngineCapabilities, EngineKind, IdentifierCase,
    SchemaGroup,
};

/// Every engine Picus knows, connectable or not, in display order.
#[arbor_rpc::handler]
fn picus_providers(state: &PicusState) -> Result<Vec<DbProviderDescriptor>, String> {
    let registered = state.providers().descriptors();

    let mut out = Vec::new();
    for kind in EngineKind::ALL {
        match registered.iter().find(|d| d.kind == *kind) {
            Some(d) => out.push(d.clone()),
            None => out.push(script_only_descriptor(*kind)),
        }
    }
    Ok(out)
}

/// The descriptor for an engine Picus handles on the script side only.
///
/// Its emission traits are real and load-bearing — they are what the generator uses
/// to write correct Oracle — while `capabilities.connect` is false and the
/// connection form is empty, because there is nothing to connect to. An engine
/// gaining a driver replaces this with the provider's own descriptor and nothing
/// upstream changes.
fn script_only_descriptor(kind: EngineKind) -> DbProviderDescriptor {
    match kind {
        EngineKind::Oracle => DbProviderDescriptor {
            kind,
            label: "Oracle".to_string(),
            short_label: "Oracle".to_string(),
            color_var: "--ws-color-1".to_string(),
            default_port: 1521,
            fields: Vec::new(),
            capabilities: EngineCapabilities {
                sequences: true,
                packages: true,
                instead_of_triggers: true,
                bitmap_indexes: true,
                expression_indexes: true,
                ..EngineCapabilities::none()
            },
            emission: EmissionTraits {
                block_open: "DECLARE\nBEGIN".to_string(),
                block_close: "END;\n/".to_string(),
                statement_terminator: ";".to_string(),
                now_function: "SYSDATE".to_string(),
                upsert_form: "MERGE … FROM DUAL".to_string(),
                object_exists_check:
                    "(SELECT COUNT(*) FROM USER_TABLES WHERE TABLE_NAME = '{object}') > 0"
                        .to_string(),
                identifier_case: IdentifierCase::Upper,
                // Oracle commits DDL implicitly, so a "roll back on error" rule
                // cannot actually undo one. The generator warns instead of lying.
                ddl_commits_implicitly: true,
            },
            schema_groups: vec![SchemaGroup::Tables, SchemaGroup::Views, SchemaGroup::Sequences],
        },
        // Every other engine reaching this arm has no driver AND no script support
        // yet — an all-false descriptor is the honest answer.
        other => DbProviderDescriptor {
            kind: other,
            label: other.to_string(),
            short_label: other.to_string(),
            color_var: "--ws-color-2".to_string(),
            default_port: 0,
            fields: Vec::new(),
            capabilities: EngineCapabilities::none(),
            emission: EmissionTraits {
                block_open: String::new(),
                block_close: String::new(),
                statement_terminator: ";".to_string(),
                now_function: String::new(),
                upsert_form: String::new(),
                object_exists_check: String::new(),
                identifier_case: IdentifierCase::Lower,
                ddl_commits_implicitly: false,
            },
            schema_groups: Vec::new(),
        },
    }
}
