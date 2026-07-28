//! One schema, shared by the suite.
//!
//! Shaped around the cases that are actually hard:
//!
//! * `LOCALSTRINGS` — the plain two-column table from the original example;
//! * `ORDINI` → `CLIENTI` **twice**, which is the ambiguity a join has to refuse;
//! * `ORDINI` → `PRODOTTI` once, which is the ambiguity's control;
//! * `CLIENTI.NOME` and `PRODOTTI.NOME`, so an unqualified `nome` over a chain
//!   containing both is ambiguous;
//! * `CODICE` as `Text` and `QUANTITA` as `Number`, which is the whole quoting
//!   story in two columns.

use crate::prelude::*;

pub fn schema() -> SchemaView {
    SchemaView::new(vec![
        TableMeta::new(
            "LOCALSTRINGS",
            vec![
                ColumnMeta::new("KEYCODE", ValueKind::Text),
                ColumnMeta::new("VALUE", ValueKind::Text),
            ],
        ),
        TableMeta::new(
            "ORDINI",
            vec![
                ColumnMeta::new("ID", ValueKind::Number),
                ColumnMeta::new("ID_CLIENTE", ValueKind::Number),
                ColumnMeta::new("ID_CLIENTE_FATTURAZIONE", ValueKind::Number),
                ColumnMeta::new("ID_PRODOTTO", ValueKind::Number),
                ColumnMeta::new("CODICE", ValueKind::Text),
                ColumnMeta::new("QUANTITA", ValueKind::Number),
                ColumnMeta::new("EVASO", ValueKind::Boolean),
                ColumnMeta::new("DATA", ValueKind::Date),
                ColumnMeta::new("ALLEGATO", ValueKind::Other),
            ],
        )
        .with_foreign_keys(vec![
            ForeignKeyMeta::new("ID_CLIENTE", "CLIENTI", "ID"),
            ForeignKeyMeta::new("ID_CLIENTE_FATTURAZIONE", "CLIENTI", "ID"),
            ForeignKeyMeta::new("ID_PRODOTTO", "PRODOTTI", "ID"),
        ]),
        TableMeta::new(
            "CLIENTI",
            vec![ColumnMeta::new("ID", ValueKind::Number), ColumnMeta::new("NOME", ValueKind::Text)],
        ),
        TableMeta::new(
            "PRODOTTI",
            vec![ColumnMeta::new("ID", ValueKind::Number), ColumnMeta::new("NOME", ValueKind::Text)],
        ),
        // No relation to anything — the "there is no foreign key" case.
        TableMeta::new("LOG", vec![ColumnMeta::new("RIGA", ValueKind::Text)]),
    ])
}

/// A lower-case schema, for the identifier-casing and alias-casing rules.
pub fn lowercase_schema() -> SchemaView {
    SchemaView::new(vec![
        TableMeta::new(
            "localstrings",
            vec![
                ColumnMeta::new("keycode", ValueKind::Text),
                ColumnMeta::new("value", ValueKind::Text),
            ],
        ),
        TableMeta::new("ordini", vec![ColumnMeta::new("id_cliente", ValueKind::Number)])
            .with_foreign_keys(vec![ForeignKeyMeta::new("id_cliente", "clienti", "id")]),
        TableMeta::new(
            "clienti",
            vec![ColumnMeta::new("id", ValueKind::Number), ColumnMeta::new("nome", ValueKind::Text)],
        ),
    ])
}

/// Expand, and render with the default style — the shape most assertions want.
pub fn sql(input: &str) -> String {
    render(&expand(input, &schema()).expect("expands"), &RenderStyle::default())
}

/// Expand and expect a refusal; the message is what is asserted on.
pub fn refusal(input: &str) -> String {
    expand(input, &schema()).expect_err("refused").to_string()
}
