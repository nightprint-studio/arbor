//! `lombok.config` — the keys Lombok reads, as data.
//!
//! ## Where the versions come from, and where they deliberately do not
//!
//! A `since` is recorded **only where the release that introduced the key is certain**. Everything
//! else is left blank and therefore always offered, which is the safe direction: see
//! [`ConfigKey::since`](crate::model::ConfigKey::since) for why a missing gate costs a key too many
//! and a wrong one costs a key that a project really has.
//!
//! The file format itself arrived in **1.14.0**; before that there is no `lombok.config` to read,
//! which is what [`Catalogue::min_version`](crate::model::Catalogue::min_version) records.
//!
//! ## `flagUsage`
//!
//! Nearly every Lombok feature has a `lombok.<feature>.flagUsage` key that makes *using* it a
//! warning or an error — the lever a team reaches for when it wants `@Data` everywhere and `var`
//! nowhere. They are generated below rather than written out one by one, because there are around
//! forty of them and they differ only in the feature's name: a hand-written list is forty chances
//! to typo a key and no chance at all that a reader notices.

use crate::model::{Catalogue, ConfigKey};

/// `WARNING` / `ERROR` / `ALLOW` — the three answers every `flagUsage` key takes.
const USAGE: &[&str] = &["WARNING", "ERROR", "ALLOW"];
const BOOL: &[&str] = &["true", "false"];

/// The features that carry a `lombok.<name>.flagUsage` key.
///
/// Grouped the way Lombok's own documentation groups them — the stable annotations first, the
/// `experimental` ones after — so the completion list reads in an order a person recognises.
const FLAGGABLE: &[&str] = &[
    "data",
    "value",
    "builder",
    "builder.default",
    "getter",
    "setter",
    "getter.lazy",
    "toString",
    "equalsAndHashCode",
    "allArgsConstructor",
    "noArgsConstructor",
    "requiredArgsConstructor",
    "nonNull",
    "cleanup",
    "sneakyThrows",
    "synchronized",
    "with",
    "val",
    "var",
    "log.apacheCommons",
    "log.flogger",
    "log.javaUtilLogging",
    "log.jbosslog",
    "log.log4j",
    "log.log4j2",
    "log.slf4j",
    "log.xslf4j",
    "log.custom",
    "experimental",
    "accessors",
    "delegate",
    "fieldDefaults",
    "fieldNameConstants",
    "helper",
    "onX",
    "standardException",
    "superBuilder",
    "utilityClass",
];

/// The keys written out by hand — everything that is not a `flagUsage`.
const EXPLICIT: &[ConfigKey] = &[
    // ── The config system itself ─────────────────────────────────────────────
    ConfigKey::new(
        "config.stopBubbling",
        "boolean",
        "Stop looking for further `lombok.config` files in parent directories. Set it in the \
         project root, or a `lombok.config` in your home directory silently applies to every \
         project on the machine.",
    )
    .default_value("false")
    .values(BOOL),
    // ── Generated-code markers ───────────────────────────────────────────────
    ConfigKey::new(
        "lombok.addLombokGeneratedAnnotation",
        "boolean",
        "Mark generated members with `@lombok.Generated`. This is the one coverage tools key on: \
         without it JaCoCo counts every generated getter as an untested branch.",
    )
    .default_value("false")
    .values(BOOL)
    .since("1.16.14"),
    ConfigKey::new(
        "lombok.addJavaxGeneratedAnnotation",
        "boolean",
        "Mark generated members with `@javax.annotation.Generated`. Off by default because the \
         annotation was removed from the JDK in 9 and the import breaks the build there.",
    )
    .default_value("false")
    .values(BOOL)
    .since("1.16.14"),
    ConfigKey::new(
        "lombok.addGeneratedAnnotation",
        "boolean",
        "Add both generated-code annotations at once.",
    )
    .values(BOOL)
    .deprecated(
        "1.16.14",
        "lombok.addLombokGeneratedAnnotation / lombok.addJavaxGeneratedAnnotation",
    ),
    ConfigKey::new(
        "lombok.addSuppressWarnings",
        "boolean",
        "Put `@SuppressWarnings(\"all\")` on generated members, so a strict compiler does not \
         report code nobody wrote.",
    )
    .default_value("true")
    .values(BOOL),
    ConfigKey::new(
        "lombok.addNullAnnotations",
        "flavour",
        "Annotate generated parameters and return values with a nullability flavour, so a static \
         analyser sees the same contract on generated code as on hand-written code.",
    )
    .values(&[
        "javax", "jakarta", "eclipse", "jetbrains", "netbeans", "androidx", "checkerframework",
        "findbugs", "spring", "jml", "none",
    ])
    .since("1.18.22"),
    ConfigKey::new(
        "lombok.extern.findbugs.addSuppressFBWarnings",
        "boolean",
        "Put `@SuppressFBWarnings` on generated members. Needs SpotBugs' annotations on the \
         classpath — without them the generated code does not compile.",
    )
    .default_value("false")
    .values(BOOL),
    // ── Accessors ────────────────────────────────────────────────────────────
    ConfigKey::new(
        "lombok.accessors.chain",
        "boolean",
        "Generated setters return `this` instead of `void`, so calls chain.",
    )
    .default_value("false")
    .values(BOOL),
    ConfigKey::new(
        "lombok.accessors.fluent",
        "boolean",
        "Drop the `get` / `set` prefixes: `name()` and `name(value)`. Implies chaining unless \
         `lombok.accessors.chain` says otherwise.",
    )
    .default_value("false")
    .values(BOOL),
    ConfigKey::new(
        "lombok.accessors.prefix",
        "list",
        "Field-name prefixes to strip before building an accessor name, so `m_name` gets \
         `getName()`. A list key: use `+=` to add one and `-=` to remove one.",
    ),
    ConfigKey::new(
        "lombok.accessors.capitalization",
        "BASIC | BEANSPEC",
        "How the first letter is capitalised after a prefix. `BASIC` uppercases it; `BEANSPEC` \
         follows the JavaBeans rule, under which a field called `uName` keeps its `u` lowercase.",
    )
    .default_value("BASIC")
    .values(&["BASIC", "BEANSPEC"]),
    ConfigKey::new(
        "lombok.accessors.makeFinal",
        "boolean",
        "Generated accessors are `final`.",
    )
    .default_value("false")
    .values(BOOL),
    // ── Constructors ─────────────────────────────────────────────────────────
    ConfigKey::new(
        "lombok.anyConstructor.addConstructorProperties",
        "boolean",
        "Put `@java.beans.ConstructorProperties` on generated constructors — what a JSON binder \
         that has no parameter names to read needs in order to bind them.",
    )
    .default_value("false")
    .values(BOOL)
    .since("1.16.20"),
    ConfigKey::new(
        "lombok.anyConstructor.suppressConstructorProperties",
        "boolean",
        "Leave `@ConstructorProperties` off generated constructors.",
    )
    .values(BOOL)
    .deprecated("1.16.20", "lombok.anyConstructor.addConstructorProperties"),
    ConfigKey::new(
        "lombok.copyableAnnotations",
        "list",
        "Fully-qualified annotations to copy from a field onto the generated constructor \
         parameter, setter parameter and getter. This is how `@Qualifier` or `@JsonProperty` \
         survives on generated code. A list key: use `+=`.",
    ),
    // ── toString / equals ────────────────────────────────────────────────────
    ConfigKey::new(
        "lombok.toString.includeFieldNames",
        "boolean",
        "Write `name=value` rather than just the value in the generated `toString()`.",
    )
    .default_value("true")
    .values(BOOL),
    ConfigKey::new(
        "lombok.toString.doNotUseGetters",
        "boolean",
        "Read the fields directly instead of calling their getters.",
    )
    .default_value("false")
    .values(BOOL),
    ConfigKey::new(
        "lombok.toString.callSuper",
        "CALL | SKIP | WARN",
        "Whether a generated `toString()` includes `super.toString()`. `WARN` generates the \
         skipping version and warns when the class has a superclass that is not `Object`.",
    )
    .default_value("WARN")
    .values(&["CALL", "SKIP", "WARN"]),
    ConfigKey::new(
        "lombok.toString.onlyExplicitlyIncluded",
        "boolean",
        "Include only fields marked `@ToString.Include`, rather than every field that is not \
         excluded.",
    )
    .default_value("false")
    .values(BOOL),
    ConfigKey::new(
        "lombok.equalsAndHashCode.doNotUseGetters",
        "boolean",
        "Read the fields directly instead of calling their getters.",
    )
    .default_value("false")
    .values(BOOL),
    ConfigKey::new(
        "lombok.equalsAndHashCode.callSuper",
        "CALL | SKIP | WARN",
        "Whether generated `equals` / `hashCode` chain to the superclass. `WARN` is the default \
         because getting this wrong on an entity hierarchy is the classic silent bug.",
    )
    .default_value("WARN")
    .values(&["CALL", "SKIP", "WARN"]),
    // ── Fields ───────────────────────────────────────────────────────────────
    ConfigKey::new(
        "lombok.fieldDefaults.defaultPrivate",
        "boolean",
        "Every field with no explicit access modifier becomes `private`. Applies to the whole \
         project, including classes with no Lombok annotation on them at all.",
    )
    .default_value("false")
    .values(BOOL),
    ConfigKey::new(
        "lombok.fieldDefaults.defaultFinal",
        "boolean",
        "Every field that is not explicitly `non_final` becomes `final`.",
    )
    .default_value("false")
    .values(BOOL),
    ConfigKey::new(
        "lombok.fieldNameConstants.uppercase",
        "boolean",
        "Generate the `@FieldNameConstants` constants in `SCREAMING_SNAKE_CASE` rather than \
         under the field's own name.",
    )
    .default_value("false")
    .values(BOOL),
    // ── Builder / singular ───────────────────────────────────────────────────
    ConfigKey::new(
        "lombok.builder.className",
        "identifier",
        "The name of the generated builder class. `*` stands for the annotated type's own name.",
    )
    .default_value("*Builder"),
    ConfigKey::new(
        "lombok.singular.useGuava",
        "boolean",
        "Build `@Singular` collections with Guava's immutable types instead of the JDK's \
         `Collections.unmodifiable*`.",
    )
    .default_value("false")
    .values(BOOL),
    ConfigKey::new(
        "lombok.singular.auto",
        "boolean",
        "Derive the singular name from the plural one automatically (`items` → `item`), so \
         `@Singular` needs no explicit name.",
    )
    .default_value("true")
    .values(BOOL),
    // ── Logging ──────────────────────────────────────────────────────────────
    ConfigKey::new(
        "lombok.log.fieldName",
        "identifier",
        "The name of the field the `@Slf4j` family generates.",
    )
    .default_value("log"),
    ConfigKey::new(
        "lombok.log.fieldIsStatic",
        "boolean",
        "Generate the logger field as `static`.",
    )
    .default_value("true")
    .values(BOOL),
    ConfigKey::new(
        "lombok.log.custom.declaration",
        "declaration",
        "The logger `@CustomLog` generates, written as a factory declaration — how an in-house \
         logging facade gets the same one-annotation treatment as slf4j.",
    ),
    // ── Misc ─────────────────────────────────────────────────────────────────
    ConfigKey::new(
        "lombok.nonNull.exceptionType",
        "exception",
        "What a `@NonNull` check throws when it fails.",
    )
    .default_value("NullPointerException")
    .values(&["NullPointerException", "IllegalArgumentException", "Assertion", "JDK"]),
];

/// Every key, explicit ones first and the generated `flagUsage` family after.
///
/// Built at load rather than `const`, because the `flagUsage` half is derived. It is built **once**
/// — see [`catalogue`].
fn all_keys() -> Vec<ConfigKey> {
    let mut keys = EXPLICIT.to_vec();
    for feature in FLAGGABLE {
        keys.push(
            ConfigKey::new(
                Box::leak(format!("lombok.{feature}.flagUsage").into_boxed_str()),
                "WARNING | ERROR | ALLOW",
                Box::leak(
                    format!(
                        "Report any use of Lombok's `{feature}` feature in this project. `ERROR` \
                         fails the build on it — the lever for banning a feature a team has \
                         decided against."
                    )
                    .into_boxed_str(),
                ),
            )
            .values(USAGE),
        );
    }
    keys
}

/// Lombok's vocabulary. Built once on first use — the tables are `&'static` from there on, so
/// every answer borrows rather than allocates.
pub fn catalogue() -> &'static Catalogue {
    static CATALOGUE: std::sync::OnceLock<Catalogue> = std::sync::OnceLock::new();
    CATALOGUE.get_or_init(|| Catalogue {
        tool: "lombok",
        display_name: "Lombok",
        file_name: "lombok.config",
        artifacts: &[("org.projectlombok", "lombok")],
        min_version: "1.14.0",
        keys: Box::leak(all_keys().into_boxed_slice()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_flag_usage_family_is_generated_for_every_feature() {
        let c = catalogue();
        assert!(c.lookup("lombok.data.flagUsage").is_some());
        assert!(c.lookup("lombok.experimental.flagUsage").is_some());
        assert!(c.lookup("lombok.log.slf4j.flagUsage").is_some());
        assert_eq!(c.lookup("lombok.val.flagUsage").unwrap().values, USAGE);
    }

    #[test]
    fn the_catalogue_is_one_object_however_often_it_is_asked_for() {
        assert!(std::ptr::eq(catalogue(), catalogue()));
    }

    #[test]
    fn a_1_16_project_is_not_offered_the_1_18_null_annotations_key() {
        let c = catalogue();
        let offered = |v: &str| c.known(Some(v)).any(|k| k.key == "lombok.addNullAnnotations");
        assert!(!offered("1.16.20"));
        assert!(offered("1.18.30"));
    }

    #[test]
    fn a_deprecated_key_is_still_offered_because_it_is_in_the_file_being_read() {
        let c = catalogue();
        let k = c.lookup("lombok.addGeneratedAnnotation").unwrap();
        assert!(!k.deprecated_since.is_empty());
        assert!(k.known_in(Some("1.18.30")));
    }

    #[test]
    fn every_key_carries_prose() {
        for k in catalogue().keys {
            assert!(!k.doc.is_empty(), "{} has no documentation", k.key);
        }
    }
}
