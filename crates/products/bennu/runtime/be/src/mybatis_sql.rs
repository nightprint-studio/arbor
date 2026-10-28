//! `mybatis_sql` domain — the project's mapper statements as plain SQL, for checking against a
//! database (Picus). `bennu_mybatis_statements`.
//!
//! The rendering — dynamic tags flattened, bindings turned into `NULL` — lives in
//! [`bennu_web::prelude::mapper_statements`]; this only walks the project's mappers and gives each
//! statement its file. Nothing here talks to a database.

use bennu_core::prelude::BennuState;
use bennu_web::prelude::mapper_statements;
use serde::{Deserialize, Serialize};

use crate::index_service::IndexService;

/// Args for [`bennu_mybatis_statements`].
#[derive(Deserialize)]
pub struct MybatisStatementsArgs {
    /// Absolute path to the project root.
    pub root: String,
    /// What each `${name}` stands for, as `[name, value]` — only the project knows.
    #[serde(default)]
    pub values: Vec<(String, String)>,
}

#[derive(Serialize)]
pub struct MybatisStatementView {
    pub file: String,
    pub namespace: String,
    pub id: String,
    pub kind: String,
    pub line: usize,
    pub database_id: String,
    pub sql: String,
    pub dynamic: bool,
    pub unchecked: Option<String>,
    /// The `${name}` splices it contains, valued or not — what the window offers to give values to.
    pub splices: Vec<String>,
}

/// Args for [`bennu_mybatis_file_statements`].
#[derive(Deserialize)]
pub struct MybatisFileArgs {
    /// The live buffer of a mapper.
    pub source: String,
    #[serde(default)]
    pub values: Vec<(String, String)>,
}

/// One statement of the mapper in a live buffer, with where its opening tag sits (byte offsets).
#[derive(Serialize)]
pub struct MybatisFileStatement {
    pub id: String,
    pub kind: String,
    pub database_id: String,
    pub sql: String,
    pub unchecked: Option<String>,
    pub start: usize,
    pub end: usize,
}

/// The statements of ONE mapper, rendered from the text in the editor — what the editor asks the database
/// about as the mapper is edited. Empty for a buffer that is not a mapper or is not well-formed at the moment
/// (mid-edit), which the caller reads as "nothing to say".
#[arbor_rpc::handler]
fn bennu_mybatis_file_statements(
    _ctx: &BennuState,
    args: MybatisFileArgs,
) -> Result<Vec<MybatisFileStatement>, String> {
    Ok(mapper_statements(&args.source, &args.values)
        .into_iter()
        .map(|s| MybatisFileStatement {
            id: s.id,
            kind: s.kind,
            database_id: s.database_id,
            sql: s.sql,
            unchecked: s.unchecked,
            start: s.start,
            end: s.end,
        })
        .collect())
}

/// Every statement of every mapper the project's config graph knows. Empty while that graph is
/// still building — "cannot say yet", which the window answers with a refresh.
#[arbor_rpc::handler]
fn bennu_mybatis_statements(
    _ctx: &BennuState,
    args: MybatisStatementsArgs,
) -> Result<Vec<MybatisStatementView>, String> {
    // Any file of the project selects its index slot; the manifest is the one that always exists.
    let anchor = format!("{}/pom.xml", args.root.trim_end_matches(['/', '\\']));
    let mut out = Vec::new();
    for (namespace, file) in IndexService::global().mapper_files(&anchor) {
        let Ok(bytes) = std::fs::read(&file) else { continue };
        let source = String::from_utf8_lossy(&bytes);
        for s in mapper_statements(&source, &args.values) {
            out.push(MybatisStatementView {
                file: file.replace('\\', "/"),
                namespace: namespace.clone(),
                id: s.id,
                kind: s.kind,
                line: s.line,
                database_id: s.database_id,
                sql: s.sql,
                dynamic: s.dynamic,
                unchecked: s.unchecked,
                splices: s.splices,
            });
        }
    }
    Ok(out)
}
