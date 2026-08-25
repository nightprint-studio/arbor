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
        let host = dial_host(&spec.host, spec.port).await?;

        let mut config = tokio_postgres::Config::new();
        config
            .host(&host)
            .port(spec.port)
            .dbname(&spec.database)
            .user(&spec.user)
            .application_name("Arbor Picus");
        if let Some(s) = &secret {
            config.password(s.expose());
        }

        // Whether one was resolved at all, kept because the failure below cannot be
        // told apart from any other config error once the secret has been dropped.
        let had_secret = secret.is_some();

        let tls = TlsChoice::build(spec.tls)?;
        let client = spawn_connection(&config, &tls).await
            .map_err(|e| explain_missing_password(e, had_secret, &spec.user))?;

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

/// Turn the driver's `password missing` into a sentence about *this* connection.
///
/// ## Why this is worth a function
///
/// `tokio-postgres` raises `password missing` **client-side**, the moment the server
/// asks for one and the config has none. It is a true statement about the config and
/// a useless one about the situation: it names nothing the reader can act on, and —
/// worse — it reads like a driver fault rather than what it is, *the keychain had
/// nothing for this connection*.
///
/// That ambiguity has cost real time. A vault that failed to load and a connection
/// that genuinely has no password stored arrive here identically, because both end as
/// "no secret was resolved". A vault failure does surface on its own (the broker
/// propagates it), so reaching this point means the vault answered and answered
/// **empty** — which is exactly what the message now says.
fn explain_missing_password(error: DbError, had_secret: bool, user: &str) -> DbError {
    if had_secret || !error.to_string().contains("password missing") {
        return error;
    }
    DbError::AuthFailed(format!(
        "the server asked for a password for `{user}` and none is stored for this connection. \
         Open the connection and enter it — it goes to Arbor's keychain, not into the project."
    ))
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

/// The host to actually dial.
///
/// Normally the configured host, verbatim. There is one exception, and it is not a
/// workaround for somebody's broken machine — it is what the standard asks a client
/// to do.
///
/// **`localhost` is a reserved name.** RFC 6761 §6.3 defines it as always meaning
/// the loopback interface, and says resolvers *should* answer it themselves rather
/// than passing it to the network. Windows normally does. What breaks it is a
/// client that inserts itself into name resolution — a VPN's DNS layer is the usual
/// one — and answers "no such host" for a name that, by definition, cannot fail to
/// resolve. Every literal address still works; only the name is broken.
///
/// So when a **loopback name** does not resolve, the loopback address is used
/// instead. That is a fact, not a guess: `localhost` has exactly one correct answer
/// and this is it. Any other host that fails to resolve is still an error, because
/// there we would be inventing one.
///
/// Two deliberate limits:
///
/// * ordinary resolution is tried **first**, so a machine whose resolver works is
///   completely unaffected — including one that deliberately maps `localhost`
///   somewhere unusual;
/// * the substitution is announced on the log rather than performed silently. A
///   host that is not the one that was configured is exactly the kind of thing that
///   must not be discovered later.
///
/// The one thing it costs: with TLS, the certificate is then verified against the
/// address rather than the name. A TLS server reached by an unresolvable
/// `localhost` is not a combination that occurs in practice, and the alternative
/// there is failing outright.
async fn dial_host(host: &str, port: u16) -> DbResult<String> {
    // The lookup is consumed here rather than in a `match` guard: a binding is
    // immutable until the end of its guard, so advancing the iterator there does
    // not compile.
    let resolved = match tokio::net::lookup_host((host, port)).await {
        Ok(mut addrs) => addrs.next().is_some(),
        Err(e) => {
            if !is_loopback_name(host) {
                return Err(DbError::Connect(format!("cannot resolve the host name {host}: {e}")));
            }
            false
        }
    };

    if resolved {
        return Ok(host.to_string());
    }

    if !is_loopback_name(host) {
        return Err(DbError::Connect(format!("{host} resolved to no address")));
    }

    eprintln!(
        "picus: this machine cannot resolve {host} — a name that always means the \
         loopback interface — so 127.0.0.1 is being used instead. Something is \
         intercepting name resolution here; a VPN client is the usual cause."
    );
    Ok(LOOPBACK.to_string())
}

const LOOPBACK: &str = "127.0.0.1";

/// The names RFC 6761 reserves for the loopback interface: `localhost` itself, and
/// anything under it.
fn is_loopback_name(host: &str) -> bool {
    host.eq_ignore_ascii_case("localhost")
        || host.to_ascii_lowercase().ends_with(".localhost")
}

/// Connect-time errors get their own mapping: at this point an authentication
/// failure is the single most likely cause and deserves to be said plainly, rather
/// than arriving as a generic "connection lost".
fn connect_error(err: tokio_postgres::Error) -> DbError {
    let mapped = match map_pg(err) {
        DbError::Disconnected(m) => DbError::Connect(m),
        other => other,
    };
    // On stderr as well as in the tooltip. A failure to connect is the one error
    // whose cause the user most often has to correlate with something outside the
    // window — a service that is down, a firewall, a certificate — and a tooltip
    // is gone the moment the pointer moves.
    eprintln!("picus: {mapped}");
    mapped
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

#[cfg(test)]
mod tests {
    use super::is_loopback_name;

    /// The substitution in `dial_host` is only ever right for the names the standard
    /// reserves, so which names those are is the part worth pinning down.
    #[test]
    fn the_reserved_loopback_names_are_recognised() {
        assert!(is_loopback_name("localhost"));
        assert!(is_loopback_name("LOCALHOST"));
        assert!(is_loopback_name("LocalHost"));
        // RFC 6761 reserves everything under it too.
        assert!(is_loopback_name("db.localhost"));
        assert!(is_loopback_name("Api.LocalHost"));
    }

    #[test]
    fn anything_else_is_a_real_host() {
        assert!(!is_loopback_name("db.example.test"));
        assert!(!is_loopback_name("127.0.0.1"));
        // The suffix has to be a label of its own: a host that merely ends in those
        // letters is somebody else's machine, and answering it with the loopback
        // would be sending a query to the wrong server.
        assert!(!is_loopback_name("notlocalhost"));
        assert!(!is_loopback_name("mylocalhost"));
        assert!(!is_loopback_name(""));
    }
}
