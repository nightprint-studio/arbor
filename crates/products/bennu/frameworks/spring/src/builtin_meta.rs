//! A curated table of the Spring Boot properties people actually type.
//!
//! ## Why this exists when the jars are better
//!
//! [`crate::metadata`] reads the real thing out of the dependency jars, and that source is
//! strictly better: version-exact, complete, and it covers third-party starters. This table
//! is what stands in **before** that source is available — a project whose Maven classpath
//! has not been resolved yet, a Gradle build we could not read, a first launch with a cold
//! cache. In those moments the choice is between a curated ~150 keys and nothing at all, and
//! nothing at all is the wrong answer for `server.port`.
//!
//! It is therefore deliberately **small and boring**: the keys that appear in nearly every
//! `application.yml`, with the type and the documented default. No attempt at coverage — the
//! jars do coverage. Entries here are overwritten by the jar-sourced ones the moment those
//! arrive ([`MetadataIndex::absorb`] keeps the first description, and the host folds the jars
//! in first).
//!
//! [`MetadataIndex::absorb`]: crate::metadata::MetadataIndex::absorb
//!
//! Descriptions are one line, written to be read in a hover card rather than transcribed
//! from the reference documentation.

/// `(name, type, default, description)`. Kept in the order Spring's own reference groups
/// them, which is also roughly the order a config file is written in.
pub const COMMON: &[(&str, &str, &str, &str)] = &[
    // ── Core ────────────────────────────────────────────────────────────────────
    ("spring.application.name", "java.lang.String", "", "Application name, used by logging, metrics and service discovery."),
    ("spring.profiles.active", "java.util.List<java.lang.String>", "", "Profiles that are active for this run."),
    ("spring.profiles.include", "java.util.List<java.lang.String>", "", "Profiles activated unconditionally on top of the active ones."),
    ("spring.config.import", "java.util.List<java.lang.String>", "", "Extra config to import (another file, a config server, a vault)."),
    ("spring.main.banner-mode", "org.springframework.boot.Banner$Mode", "console", "Where the startup banner is printed."),
    ("spring.main.web-application-type", "org.springframework.boot.WebApplicationType", "", "Force the application to run as servlet, reactive, or not web at all."),
    ("spring.main.allow-bean-definition-overriding", "java.lang.Boolean", "false", "Whether a later bean definition may replace one already registered."),
    ("spring.main.lazy-initialization", "java.lang.Boolean", "false", "Initialize every bean lazily. Faster startup, later failures."),
    ("spring.messages.basename", "java.util.List<java.lang.String>", "messages", "Base names of the resource bundles behind MessageSource."),
    ("spring.messages.encoding", "java.nio.charset.Charset", "UTF-8", "Charset the message bundles are read with."),

    // ── Server ──────────────────────────────────────────────────────────────────
    ("server.port", "java.lang.Integer", "8080", "HTTP port. 0 picks a free port, -1 disables the server."),
    ("server.address", "java.net.InetAddress", "", "Network address to bind to."),
    ("server.servlet.context-path", "java.lang.String", "", "Context path the application is served under."),
    ("server.servlet.session.timeout", "java.time.Duration", "30m", "Session timeout. A bare number is read as seconds."),
    ("server.servlet.session.cookie.name", "java.lang.String", "", "Name of the session cookie."),
    ("server.servlet.encoding.charset", "java.nio.charset.Charset", "UTF-8", "Charset of HTTP requests and responses."),
    ("server.compression.enabled", "java.lang.Boolean", "false", "Whether responses are compressed."),
    ("server.error.include-message", "java.lang.String", "never", "Whether the error response includes the exception message."),
    ("server.error.include-stacktrace", "java.lang.String", "never", "When the error response includes a stack trace."),
    ("server.error.whitelabel.enabled", "java.lang.Boolean", "true", "Whether the default error page is served."),
    ("server.tomcat.max-threads", "java.lang.Integer", "200", "Maximum worker threads."),
    ("server.tomcat.threads.max", "java.lang.Integer", "200", "Maximum worker threads."),
    ("server.tomcat.max-http-form-post-size", "org.springframework.util.unit.DataSize", "2MB", "Maximum size of a form POST body."),
    ("server.tomcat.accesslog.enabled", "java.lang.Boolean", "false", "Whether the access log is written."),
    ("server.ssl.enabled", "java.lang.Boolean", "true", "Whether SSL is enabled on this connector."),
    ("server.ssl.key-store", "java.lang.String", "", "Path to the key store holding the certificate."),
    ("server.ssl.key-store-password", "java.lang.String", "", "Password for the key store."),

    // ── Data source ─────────────────────────────────────────────────────────────
    ("spring.datasource.url", "java.lang.String", "", "JDBC URL of the database."),
    ("spring.datasource.username", "java.lang.String", "", "Login user of the database."),
    ("spring.datasource.password", "java.lang.String", "", "Login password of the database."),
    ("spring.datasource.driver-class-name", "java.lang.String", "", "JDBC driver. Usually inferred from the URL."),
    ("spring.datasource.hikari.maximum-pool-size", "java.lang.Integer", "10", "Maximum size of the connection pool."),
    ("spring.datasource.hikari.minimum-idle", "java.lang.Integer", "", "Minimum number of idle connections kept in the pool."),
    ("spring.datasource.hikari.connection-timeout", "java.lang.Long", "30000", "Milliseconds to wait for a connection from the pool."),
    ("spring.datasource.hikari.idle-timeout", "java.lang.Long", "600000", "Milliseconds an idle connection may stay in the pool."),
    ("spring.datasource.hikari.max-lifetime", "java.lang.Long", "1800000", "Maximum lifetime of a connection in the pool."),
    ("spring.sql.init.mode", "org.springframework.boot.sql.init.DatabaseInitializationMode", "embedded", "When schema.sql / data.sql are run."),
    ("spring.sql.init.schema-locations", "java.util.List<java.lang.String>", "", "Schema scripts to run at startup."),
    ("spring.sql.init.data-locations", "java.util.List<java.lang.String>", "", "Data scripts to run at startup."),

    // ── JPA / Hibernate ─────────────────────────────────────────────────────────
    ("spring.jpa.hibernate.ddl-auto", "java.lang.String", "", "Schema generation: none, validate, update, create, create-drop."),
    ("spring.jpa.show-sql", "java.lang.Boolean", "false", "Log every SQL statement to stdout. Prefer the logger in production."),
    ("spring.jpa.properties", "java.util.Map<java.lang.String,java.lang.String>", "", "Extra properties passed straight to the JPA provider."),
    ("spring.jpa.database-platform", "java.lang.String", "", "Hibernate dialect to use."),
    ("spring.jpa.open-in-view", "java.lang.Boolean", "true", "Keep the persistence context open for the whole request."),
    ("spring.jpa.defer-datasource-initialization", "java.lang.Boolean", "false", "Run data.sql after Hibernate has created the schema."),
    ("spring.jpa.generate-ddl", "java.lang.Boolean", "false", "Whether the provider generates DDL at startup."),

    // ── Web / MVC / JSON ────────────────────────────────────────────────────────
    ("spring.mvc.servlet.path", "java.lang.String", "/", "Path the dispatcher servlet is mapped to."),
    ("spring.mvc.static-path-pattern", "java.lang.String", "/**", "Pattern static resources are served under."),
    ("spring.mvc.view.prefix", "java.lang.String", "", "Prefix prepended to a view name."),
    ("spring.mvc.view.suffix", "java.lang.String", "", "Suffix appended to a view name."),
    ("spring.mvc.format.date", "java.lang.String", "", "Date format for request parameters and form fields."),
    ("spring.web.resources.static-locations", "java.util.List<java.lang.String>", "", "Where static resources are served from."),
    ("spring.jackson.default-property-inclusion", "com.fasterxml.jackson.annotation.JsonInclude$Include", "", "Which properties are serialized."),
    ("spring.jackson.date-format", "java.lang.String", "", "Date format for JSON serialization."),
    ("spring.jackson.time-zone", "java.util.TimeZone", "", "Time zone used when formatting dates."),
    ("spring.servlet.multipart.enabled", "java.lang.Boolean", "true", "Whether multipart uploads are supported."),
    ("spring.servlet.multipart.max-file-size", "org.springframework.util.unit.DataSize", "1MB", "Maximum size of a single uploaded file."),
    ("spring.servlet.multipart.max-request-size", "org.springframework.util.unit.DataSize", "10MB", "Maximum size of a whole multipart request."),

    // ── Security ────────────────────────────────────────────────────────────────
    ("spring.security.user.name", "java.lang.String", "user", "Name of the default in-memory user."),
    ("spring.security.user.password", "java.lang.String", "", "Password of the default in-memory user."),
    ("spring.security.oauth2.client.registration", "java.util.Map<java.lang.String,java.lang.String>", "", "OAuth2 client registrations, keyed by registration id."),

    // ── Logging ─────────────────────────────────────────────────────────────────
    ("logging.level", "java.util.Map<java.lang.String,java.lang.String>", "", "Log level per logger name. `logging.level.root` sets the root logger."),
    ("logging.file.name", "java.lang.String", "", "Log file to write to."),
    ("logging.file.path", "java.lang.String", "", "Directory to write spring.log into."),
    ("logging.pattern.console", "java.lang.String", "", "Appender pattern for console output."),
    ("logging.pattern.file", "java.lang.String", "", "Appender pattern for file output."),
    ("logging.config", "java.lang.String", "", "Location of the logging configuration file."),
    ("logging.charset.console", "java.nio.charset.Charset", "", "Charset of console output."),

    // ── Actuator ────────────────────────────────────────────────────────────────
    ("management.endpoints.web.exposure.include", "java.util.List<java.lang.String>", "health", "Endpoint ids exposed over HTTP."),
    ("management.endpoints.web.exposure.exclude", "java.util.List<java.lang.String>", "", "Endpoint ids hidden from HTTP."),
    ("management.endpoints.web.base-path", "java.lang.String", "/actuator", "Base path of the actuator endpoints."),
    ("management.endpoint.health.show-details", "java.lang.String", "never", "When health check details are shown."),
    ("management.server.port", "java.lang.Integer", "", "Separate port for the management endpoints."),

    // ── Caching, scheduling, mail, misc ─────────────────────────────────────────
    ("spring.cache.type", "org.springframework.boot.autoconfigure.cache.CacheType", "", "Cache implementation to auto-configure."),
    ("spring.task.execution.pool.core-size", "java.lang.Integer", "8", "Core threads of the application task executor."),
    ("spring.task.scheduling.pool.size", "java.lang.Integer", "1", "Threads available for scheduled tasks."),
    ("spring.mail.host", "java.lang.String", "", "SMTP server host."),
    ("spring.mail.port", "java.lang.Integer", "", "SMTP server port."),
    ("spring.mail.username", "java.lang.String", "", "SMTP login user."),
    ("spring.mail.password", "java.lang.String", "", "SMTP login password."),
    ("spring.redis.host", "java.lang.String", "localhost", "Redis server host."),
    ("spring.redis.port", "java.lang.Integer", "6379", "Redis server port."),
    ("spring.data.redis.host", "java.lang.String", "localhost", "Redis server host."),
    ("spring.data.redis.port", "java.lang.Integer", "6379", "Redis server port."),
    ("spring.flyway.enabled", "java.lang.Boolean", "true", "Whether Flyway migration runs at startup."),
    ("spring.flyway.locations", "java.util.List<java.lang.String>", "", "Where the migration scripts live."),
    ("spring.liquibase.enabled", "java.lang.Boolean", "true", "Whether Liquibase runs at startup."),
    ("spring.liquibase.change-log", "java.lang.String", "", "Master change log to apply."),
    ("spring.thymeleaf.cache", "java.lang.Boolean", "true", "Whether compiled templates are cached."),
    ("spring.thymeleaf.prefix", "java.lang.String", "classpath:/templates/", "Prefix prepended to a template name."),
    ("spring.thymeleaf.suffix", "java.lang.String", ".html", "Suffix appended to a template name."),
    ("spring.devtools.restart.enabled", "java.lang.Boolean", "true", "Whether the automatic restart is enabled."),
    ("spring.jmx.enabled", "java.lang.Boolean", "false", "Whether management beans are exposed over JMX."),
];

/// `(key, legal values)` — the enumerated properties worth completing by value. Same
/// standard as the rest of the file: only where the set is closed and well known, so a
/// suggestion is never a guess.
pub const VALUE_HINTS: &[(&str, &[&str])] = &[
    ("logging.level", &["trace", "debug", "info", "warn", "error", "fatal", "off"]),
    ("spring.jpa.hibernate.ddl-auto", &["none", "validate", "update", "create", "create-drop"]),
    ("spring.main.banner-mode", &["console", "log", "off"]),
    ("spring.main.web-application-type", &["servlet", "reactive", "none"]),
    ("spring.sql.init.mode", &["always", "embedded", "never"]),
    ("management.endpoint.health.show-details", &["never", "when-authorized", "always"]),
    ("server.error.include-message", &["never", "always", "on-param"]),
    ("server.error.include-stacktrace", &["never", "always", "on-param"]),
    ("spring.jackson.default-property-inclusion", &["always", "non_null", "non_absent", "non_default", "non_empty"]),
    ("spring.cache.type", &["caffeine", "ehcache", "hazelcast", "infinispan", "jcache", "none", "redis", "simple"]),
];

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    #[test]
    fn the_table_is_free_of_duplicate_keys() {
        let mut seen = BTreeSet::new();
        for (name, ..) in COMMON {
            assert!(seen.insert(*name), "`{name}` is listed twice");
        }
    }

    /// Every hinted key must be a key the table describes, or the value completion fires on
    /// something hover knows nothing about.
    #[test]
    fn every_value_hint_belongs_to_a_described_property() {
        for (key, values) in VALUE_HINTS {
            assert!(COMMON.iter().any(|(n, ..)| n == key), "`{key}` is hinted but not described");
            assert!(!values.is_empty(), "`{key}` has an empty hint list");
        }
    }

    #[test]
    fn keys_are_written_in_canonical_kebab_case() {
        for (name, ..) in COMMON {
            assert!(
                !name.chars().any(|c| c.is_uppercase() || c == '_'),
                "`{name}` is not canonical — relaxed binding would still find it, but the \
                 table is the reference spelling and should read like one",
            );
        }
    }
}
