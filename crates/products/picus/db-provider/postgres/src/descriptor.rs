//! The PostgreSQL descriptor — the per-engine document the UI renders from.
//!
//! Everything the frontend used to hardcode about PostgreSQL lives here, so the
//! connection form, the schema tree and the emitter read one source.

use picus_db_api::prelude::*;

/// Build the PostgreSQL descriptor. Cheap — callers may ask per keystroke.
pub fn descriptor() -> DbProviderDescriptor {
    DbProviderDescriptor {
        kind: EngineKind::Postgres,
        label: "PostgreSQL".to_string(),
        short_label: "PostgreSQL".to_string(),
        // A theme token, never a hex literal: dialect colours belong to the theme's
        // workspace ramp, the same one Corvus identifies workspaces with.
        color_var: "--ws-color-0".to_string(),
        default_port: 5432,
        fields: connection_fields(),
        capabilities: capabilities(),
        emission: emission(),
        schema_groups: vec![
            SchemaGroup::Tables,
            SchemaGroup::Views,
            SchemaGroup::Sequences,
            SchemaGroup::Triggers,
        ],
    }
}

/// The create-connection form for PostgreSQL.
///
/// Note `database` and `schema` are separate fields: PostgreSQL namespaces objects
/// by schema *inside* a database, so pinning `search_path` is a real choice. Oracle
/// will declare a single `service name` instead — which is exactly why this is data
/// and not a component.
fn connection_fields() -> Vec<ConnectionField> {
    vec![
        ConnectionField::text("host", "Host")
            .with_default("localhost")
            .with_placeholder("db.example.com"),
        ConnectionField::text("port", "Port")
            .with_kind(FieldKind::Number { min: Some(1), max: Some(65_535) })
            .with_default("5432"),
        ConnectionField::text("database", "Database").with_placeholder("appdb"),
        ConnectionField::text("user", "User").with_placeholder("app"),
        ConnectionField::text("password", "Password")
            .with_kind(FieldKind::Secret)
            .optional()
            .with_help("Stored in Arbor's keychain, never in the project."),
        ConnectionField::text("schema", "Schema")
            .optional()
            .with_default("public")
            .with_help("Pins the session's search_path. Leave empty for the server default."),
        ConnectionField::text("tls", "Require TLS")
            .with_kind(FieldKind::Toggle)
            .optional()
            .with_default("false")
            .with_help("Managed cloud databases refuse plaintext connections."),
    ]
}

fn capabilities() -> EngineCapabilities {
    EngineCapabilities {
        connect: true,
        sequences: true,
        materialized_views: true,
        // Packages are an Oracle concept; PostgreSQL has schemas full of functions.
        packages: false,
        instead_of_triggers: true,
        bitmap_indexes: false,
        expression_indexes: true,
        cancel_query: true,
        estimated_rows: true,
        schemas: true,
        session_activity: true,
        explain: true,
        validate: true,
        bind_parameters: true,
        dependency_graph: true,
        // The one that matters on this engine: PostgreSQL's DDL is transactional,
        // so a failed install really can be undone. Oracle's is not, and says so.
        transactions: TxCapability { supported: true, transactional_ddl: true, savepoints: true },
    }
}

fn emission() -> EmissionTraits {
    EmissionTraits {
        block_open: "DO $$\nBEGIN".to_string(),
        block_close: "END $$;".to_string(),
        statement_terminator: ";".to_string(),
        now_function: "NOW()".to_string(),
        upsert_form: "INSERT … ON CONFLICT DO NOTHING".to_string(),
        object_exists_check: "to_regclass('{object}') IS NOT NULL".to_string(),
        // Unquoted identifiers fold to lower case — the opposite of Oracle, and the
        // reason the "lowercase PostgreSQL identifiers" setting exists at all.
        identifier_case: IdentifierCase::Lower,
        // PostgreSQL DDL is transactional, so a "roll back on error" target rule is
        // honest here. On Oracle it would not be.
        ddl_commits_implicitly: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_secret_field_is_declared_as_a_secret() {
        let d = descriptor();
        let pw = d.fields.iter().find(|f| f.id == "password").expect("password field");
        assert!(matches!(pw.kind, FieldKind::Secret));
        assert!(!pw.required, "peer/trust authentication needs no password");
    }

    #[test]
    fn the_descriptor_serialises_for_the_frontend() {
        let v = serde_json::to_value(descriptor()).unwrap();
        assert_eq!(v["kind"], "postgres");
        assert_eq!(v["defaultPort"], 5432);
        assert_eq!(v["capabilities"]["connect"], true);
        assert_eq!(v["emission"]["identifierCase"], "lower");
        assert_eq!(v["schemaGroups"][0], "tables");
    }
}
