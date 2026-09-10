//! `junit-platform.properties` — the configuration parameters the JUnit Platform reads, as data.
//!
//! ## One version line, not two
//!
//! The keys come in two namespaces on two different version lines: `junit.jupiter.*` belongs to
//! Jupiter (`5.x`) and `junit.platform.*` to the Platform (`1.x`). They are not independent — the
//! release train ships Platform `1.N` with Jupiter `5.N`, always — so every `since` below is written
//! in the **Jupiter** line and a `junit.platform.*` key introduced in Platform 1.10 records `5.10`.
//!
//! Tracking both would mean resolving two coordinates to gate one file, and getting the pairing
//! wrong in a way nothing would ever surface.
//!
//! ## Where the versions stop
//!
//! As in the Lombok table: a `since` is recorded only where the introducing release is certain, and
//! a key with no `since` is offered to every project. A gate that is missing costs a key too many;
//! a gate that is wrong hides a key the project really has.

use crate::model::{Catalogue, ConfigKey};

const BOOL: &[&str] = &["true", "false"];

const KEYS: &[ConfigKey] = &[
    // ── Lifecycle and discovery ──────────────────────────────────────────────
    ConfigKey::new(
        "junit.jupiter.testinstance.lifecycle.default",
        "per_method | per_class",
        "Whether a fresh test instance is created for each test method. `per_class` keeps one \
         instance for the whole class, which is what `@BeforeAll` on a non-static method needs.",
    )
    .default_value("per_method")
    .values(&["per_method", "per_class"])
    .since("5.0"),
    ConfigKey::new(
        "junit.jupiter.extensions.autodetection.enabled",
        "boolean",
        "Register extensions found through the `ServiceLoader` automatically, instead of only \
         those named by `@ExtendWith`.",
    )
    .default_value("false")
    .values(BOOL)
    .since("5.0"),
    ConfigKey::new(
        "junit.jupiter.conditions.deactivate",
        "pattern",
        "Deactivate matching `ExecutionCondition` implementations — the switch that runs the \
         `@Disabled` tests too (`*` deactivates every condition).",
    )
    .since("5.0"),
    ConfigKey::new(
        "junit.jupiter.displayname.generator.default",
        "class name",
        "The fully-qualified `DisplayNameGenerator` used where a test declares none. \
         `org.junit.jupiter.api.DisplayNameGenerator$ReplaceUnderscores` is the usual choice.",
    )
    .since("5.4"),
    ConfigKey::new(
        "junit.jupiter.testclass.order.default",
        "class name",
        "The fully-qualified `ClassOrderer` applied to top-level test classes.",
    )
    .since("5.8"),
    ConfigKey::new(
        "junit.jupiter.testmethod.order.default",
        "class name",
        "The fully-qualified `MethodOrderer` applied where a class declares none. Ordering tests \
         globally is usually a smell; it is here because migrating a JUnit 4 suite that depended \
         on order needs it.",
    ),
    // ── Parallel execution ───────────────────────────────────────────────────
    ConfigKey::new(
        "junit.jupiter.execution.parallel.enabled",
        "boolean",
        "Turn parallel execution on. On its own it changes nothing: without a `mode.default` of \
         `concurrent`, or `@Execution(CONCURRENT)` somewhere, every node still runs in the same \
         thread.",
    )
    .default_value("false")
    .values(BOOL)
    .since("5.3"),
    ConfigKey::new(
        "junit.jupiter.execution.parallel.mode.default",
        "same_thread | concurrent",
        "The default execution mode for every node.",
    )
    .default_value("same_thread")
    .values(&["same_thread", "concurrent"])
    .since("5.3"),
    ConfigKey::new(
        "junit.jupiter.execution.parallel.mode.classes.default",
        "same_thread | concurrent",
        "The default execution mode for top-level classes — set this to `concurrent` and the \
         method mode to `same_thread` to run classes in parallel but their methods in order.",
    )
    .default_value("same_thread")
    .values(&["same_thread", "concurrent"])
    .since("5.3"),
    ConfigKey::new(
        "junit.jupiter.execution.parallel.config.strategy",
        "dynamic | fixed | custom",
        "How the parallelism is sized.",
    )
    .default_value("dynamic")
    .values(&["dynamic", "fixed", "custom"])
    .since("5.3"),
    ConfigKey::new(
        "junit.jupiter.execution.parallel.config.dynamic.factor",
        "decimal",
        "Multiplied by the number of available processors to give the parallelism, under the \
         `dynamic` strategy.",
    )
    .default_value("1.0")
    .since("5.3"),
    ConfigKey::new(
        "junit.jupiter.execution.parallel.config.fixed.parallelism",
        "integer",
        "The parallelism, under the `fixed` strategy.",
    )
    .since("5.3"),
    ConfigKey::new(
        "junit.jupiter.execution.parallel.config.fixed.max-pool-size",
        "integer",
        "The maximum pool size, under the `fixed` strategy — the ceiling the pool may grow to \
         while threads are blocked.",
    )
    .since("5.9"),
    ConfigKey::new(
        "junit.jupiter.execution.parallel.config.fixed.saturate",
        "boolean",
        "Whether the fixed-size pool may saturate — i.e. exceed its parallelism while threads are \
         blocked.",
    )
    .default_value("true")
    .values(BOOL)
    .since("5.9"),
    ConfigKey::new(
        "junit.jupiter.execution.parallel.config.custom.class",
        "class name",
        "The fully-qualified `ParallelExecutionConfigurationStrategy`, under the `custom` strategy.",
    )
    .since("5.3"),
    // ── Timeouts ─────────────────────────────────────────────────────────────
    ConfigKey::new(
        "junit.jupiter.execution.timeout.default",
        "duration",
        "A timeout applied to every testable and lifecycle method. Written as a number with a \
         unit — `10 s`, `500 ms`, `1 m` — a bare number meaning seconds.",
    )
    .since("5.5"),
    ConfigKey::new(
        "junit.jupiter.execution.timeout.testable.method.default",
        "duration",
        "A timeout for `@Test`, `@TestFactory` and `@TestTemplate` methods.",
    )
    .since("5.5"),
    ConfigKey::new(
        "junit.jupiter.execution.timeout.test.method.default",
        "duration",
        "A timeout for `@Test` methods.",
    )
    .since("5.5"),
    ConfigKey::new(
        "junit.jupiter.execution.timeout.testtemplate.method.default",
        "duration",
        "A timeout for `@TestTemplate` methods, `@ParameterizedTest` among them.",
    )
    .since("5.5"),
    ConfigKey::new(
        "junit.jupiter.execution.timeout.testfactory.method.default",
        "duration",
        "A timeout for `@TestFactory` methods.",
    )
    .since("5.5"),
    ConfigKey::new(
        "junit.jupiter.execution.timeout.lifecycle.method.default",
        "duration",
        "A timeout for every lifecycle method.",
    )
    .since("5.5"),
    ConfigKey::new(
        "junit.jupiter.execution.timeout.beforeall.method.default",
        "duration",
        "A timeout for `@BeforeAll` methods.",
    )
    .since("5.5"),
    ConfigKey::new(
        "junit.jupiter.execution.timeout.beforeeach.method.default",
        "duration",
        "A timeout for `@BeforeEach` methods.",
    )
    .since("5.5"),
    ConfigKey::new(
        "junit.jupiter.execution.timeout.aftereach.method.default",
        "duration",
        "A timeout for `@AfterEach` methods.",
    )
    .since("5.5"),
    ConfigKey::new(
        "junit.jupiter.execution.timeout.afterall.method.default",
        "duration",
        "A timeout for `@AfterAll` methods.",
    )
    .since("5.5"),
    ConfigKey::new(
        "junit.jupiter.execution.timeout.mode",
        "enabled | disabled | disabled_on_debug",
        "When timeouts apply. `disabled_on_debug` is the one worth knowing: it stops a breakpoint \
         from failing the test you stopped in.",
    )
    .default_value("enabled")
    .values(&["enabled", "disabled", "disabled_on_debug"])
    .since("5.6"),
    // ── Temp directories ─────────────────────────────────────────────────────
    ConfigKey::new(
        "junit.jupiter.tempdir.cleanup.mode.default",
        "ALWAYS | ON_SUCCESS | NEVER",
        "When a `@TempDir` is deleted. `ON_SUCCESS` keeps the directory of a failed test, which \
         is the whole point of looking at one.",
    )
    .default_value("ALWAYS")
    .values(&["ALWAYS", "ON_SUCCESS", "NEVER"])
    .since("5.9"),
    ConfigKey::new(
        "junit.jupiter.tempdir.factory.default",
        "class name",
        "The fully-qualified `TempDirFactory` used where a `@TempDir` names none — an in-memory \
         filesystem, for instance.",
    )
    .since("5.10"),
    // ── Platform ─────────────────────────────────────────────────────────────
    ConfigKey::new(
        "junit.platform.output.capture.stdout",
        "boolean",
        "Capture `System.out` per test and attach it to the report, instead of letting it go to \
         the build log.",
    )
    .default_value("false")
    .values(BOOL)
    .since("5.3"),
    ConfigKey::new(
        "junit.platform.output.capture.stderr",
        "boolean",
        "Capture `System.err` per test and attach it to the report.",
    )
    .default_value("false")
    .values(BOOL)
    .since("5.3"),
    ConfigKey::new(
        "junit.platform.output.capture.maxBuffer",
        "integer",
        "The maximum number of captured characters per test before output is dropped.",
    )
    .default_value("1048576")
    .since("5.3"),
    ConfigKey::new(
        "junit.platform.execution.listeners.deactivate",
        "pattern",
        "Deactivate matching `TestExecutionListener` implementations found on the classpath.",
    ),
    ConfigKey::new(
        "junit.platform.stacktrace.pruning.enabled",
        "boolean",
        "Prune the framework's own frames out of reported stack traces, so a failure shows your \
         code rather than twenty lines of the launcher.",
    )
    .default_value("true")
    .values(BOOL)
    .since("5.10"),
    ConfigKey::new(
        "junit.platform.reporting.open.xml.enabled",
        "boolean",
        "Write the Open Test Reporting XML alongside the legacy one.",
    )
    .default_value("false")
    .values(BOOL)
    .since("5.10"),
];

/// The JUnit Platform's vocabulary.
pub const CATALOGUE: Catalogue = Catalogue {
    tool: "junit",
    display_name: "JUnit",
    file_name: "junit-platform.properties",
    // Most authoritative first. A project that declares `junit-jupiter` has said which version it
    // is on; one that only imports the BOM has said it a level up, and that is still an answer.
    artifacts: &[
        ("org.junit.jupiter", "junit-jupiter"),
        ("org.junit.jupiter", "junit-jupiter-api"),
        ("org.junit.jupiter", "junit-jupiter-engine"),
        ("org.junit", "junit-bom"),
    ],
    min_version: "5.0",
    keys: KEYS,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_5_5_project_has_timeouts_but_not_the_5_9_pool_ceiling() {
        let offered = |v: &str, key: &str| CATALOGUE.known(Some(v)).any(|k| k.key == key);
        assert!(offered("5.5", "junit.jupiter.execution.timeout.default"));
        assert!(!offered(
            "5.5",
            "junit.jupiter.execution.parallel.config.fixed.max-pool-size"
        ));
        assert!(offered("5.9", "junit.jupiter.execution.parallel.config.fixed.max-pool-size"));
    }

    #[test]
    fn a_5_2_project_has_no_parallel_block_at_all() {
        assert!(!CATALOGUE
            .known(Some("5.2"))
            .any(|k| k.key.starts_with("junit.jupiter.execution.parallel.")));
    }

    #[test]
    fn platform_keys_are_dated_on_the_jupiter_line() {
        // Platform 1.10 ships with Jupiter 5.10 — the table records the Jupiter number, and this
        // is the test that says so out loud.
        let k = CATALOGUE.lookup("junit.platform.stacktrace.pruning.enabled").unwrap();
        assert_eq!(k.since, "5.10");
    }

    #[test]
    fn every_key_carries_prose() {
        for k in CATALOGUE.keys {
            assert!(!k.doc.is_empty(), "{} has no documentation", k.key);
        }
    }
}
