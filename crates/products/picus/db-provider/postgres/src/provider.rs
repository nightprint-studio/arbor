//! [`PostgresProvider`] — the engine, and how a session is opened.

use async_trait::async_trait;
use picus_db_api::prelude::*;
use tokio_postgres::Client;

use crate::descriptor;
use crate::error::map_pg;
use crate::session::PgSession;
use crate::sql::quote_ident;
use crate::tls::TlsChoice;

/// The PostgreSQL engine. Stateless — one instance serves every connection.
#[derive(Debug, Default, Clone, Copy)]
pub struct PostgresProvider;

impl PostgresProvider {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl DbProvider for PostgresProvider {
    fn kind(&self) -> EngineKind {
        EngineKind::Postgres
    }

    fn descriptor(&self) -> DbProviderDescriptor {
        descriptor::descriptor()
    }

    async fn connect(
        &self,
        spec: &ConnectionSpec,
        secret: Option<Secret>,
    ) -> DbResult<Box<dyn DbSession>> {
        let mut config = tokio_postgres::Config::new();
        config
            .host(&spec.host)
            .port(spec.port)
            .dbname(&spec.database)
            .user(&spec.user)
            .application_name("Arbor Picus");
        if let Some(s) = &secret {
            config.password(s.expose());
        }

        let tls = TlsChoice::build(spec.tls)?;
        let client = spawn_connection(&config, &tls).await?;

        // The secret has done its job. Dropping it here rather than at the end of
        // the function zeroes it as early as possible.
        drop(secret);

        if !spec.schema.is_empty() {
            // Not a bind parameter — `SET` takes an identifier, so it is quoted.
            let sql = format!("SET search_path TO {}", quote_ident(&spec.schema));
            client.simple_query(&sql).await.map_err(map_pg)?;
        }

        if spec.read_only {
            // THE enforcement. From here the *server* refuses every write on this
            // session, which is what makes the read-only flag hold for a pasted
            // script or a plugin — not just for the buttons the UI greys out.
            client
                .simple_query("SET SESSION CHARACTERISTICS AS TRANSACTION READ ONLY")
                .await
                .map_err(map_pg)?;
            client.simple_query("SET default_transaction_read_only = on").await.map_err(map_pg)?;
        }

        let server_version = read_server_version(&client).await;
        Ok(Box::new(PgSession::new(client, tls, spec.clone(), server_version)))
    }
}

/// Connect and drive the connection task.
///
/// tokio-postgres hands back a `(Client, Connection)` pair where the `Connection`
/// half is the actual protocol driver and must be polled for the client to work at
/// all. Spawning it is not optional — forgetting it produces a client that hangs on
/// the first query with no error.
async fn spawn_connection(
    config: &tokio_postgres::Config,
    tls: &TlsChoice,
) -> DbResult<Client> {
    match tls {
        TlsChoice::Plain(connector) => {
            let (client, connection) =
                config.connect(connector.clone()).await.map_err(connect_error)?;
            tokio::spawn(async move {
                // The connection ends when the client is dropped or the server goes
                // away; either way there is nothing to do but stop.
                let _ = connection.await;
            });
            Ok(client)
        }
        TlsChoice::Rustls(connector) => {
            let (client, connection) =
                config.connect(connector.clone()).await.map_err(connect_error)?;
            tokio::spawn(async move {
                let _ = connection.await;
            });
            Ok(client)
        }
    }
}

/// Connect-time errors get their own mapping: at this point an authentication
/// failure is the single most likely cause and deserves to be said plainly, rather
/// than arriving as a generic "connection lost".
fn connect_error(err: tokio_postgres::Error) -> DbError {
    match map_pg(err) {
        DbError::Disconnected(m) => DbError::Connect(m),
        other => other,
    }
}

/// The server banner, best-effort — a session is perfectly usable without it.
async fn read_server_version(client: &Client) -> String {
    let Ok(messages) = client.simple_query("SELECT version()").await else {
        return String::new();
    };
    messages
        .into_iter()
        .find_map(|m| match m {
            tokio_postgres::SimpleQueryMessage::Row(r) => r.get(0).map(|s| s.to_string()),
            _ => None,
        })
        // `version()` returns a whole paragraph ("PostgreSQL 16.2 on
        // x86_64-pc-linux-gnu, compiled by …"); the status bar wants the first two
        // words of it.
        .map(|v| v.split_whitespace().take(2).collect::<Vec<_>>().join(" "))
        .unwrap_or_default()
}
