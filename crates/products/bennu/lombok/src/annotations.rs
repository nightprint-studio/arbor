//! The catalogue: which annotations Lombok defines, and what each one puts in the compiled class.
//!
//! Every entry is matched on the annotation's **simple name** — the last segment — so `@Data` and
//! `@lombok.Data` read the same. Whether that simple name really resolves to Lombok in a given file
//! is a separate question, answered by [`crate::imports`]; keeping the two apart is what lets a
//! consumer gate once per file instead of once per annotation.

/// Lombok's root package. The only string in the crate that is not an annotation name.
pub const PACKAGE: &str = "lombok";

/// The annotations that generate **accessors or other members on the annotated type**: the reason a
/// method can be called with no declaration anywhere in the source.
pub const MEMBER_GENERATING: &[&str] = &[
    "Data",
    "Value",
    "Getter",
    "Setter",
    "Builder",
    "SuperBuilder",
    "Accessors",
    "With",
    "EqualsAndHashCode",
    "ToString",
    "UtilityClass",
    "FieldNameConstants",
];

/// The logger annotations — each injects one `log` field, of a type that depends on the backend.
pub const LOGGERS: &[&str] = &[
    "Slf4j",
    "XSlf4j",
    "Log",
    "Log4j",
    "Log4j2",
    "CommonsLog",
    "JBossLog",
    "Flogger",
    "CustomLog",
];

/// The annotations that make Lombok emit a **constructor**.
///
/// The three explicit ones, the two bundles that imply one (`@Data` → `@RequiredArgsConstructor`,
/// `@Value` → `@AllArgsConstructor`), and the two builders: `@Builder` on a *type* is
/// `@AllArgsConstructor(access = PACKAGE)` plus the builder class, and `@SuperBuilder` the same with
/// a protected constructor taking the builder. The builders are the entry the blank-final check was
/// missing — they are as much a constructor as `@AllArgsConstructor` is.
pub const CONSTRUCTOR_GENERATING: &[&str] = &[
    "NoArgsConstructor",
    "RequiredArgsConstructor",
    "AllArgsConstructor",
    "Data",
    "Value",
    "Builder",
    "SuperBuilder",
];

/// The locals-inference keywords Lombok adds to the language: `val` (an inferred `final` local) and
/// `var` (its pre-Java-10 stand-in). Both parse as an ordinary type name, so a consumer must know
/// they are keywords before deciding a type called `val` failed to resolve.
pub const INFERENCE_KEYWORDS: &[&str] = &["val", "var"];

/// Whether `simple` names an annotation that adds members to the annotated type — accessors,
/// builders, a logger field. A `true` means "this type's member list is partly invisible to the
/// index", which is a reason to stay silent, never a reason to report.
pub fn generates_members(simple: &str) -> bool {
    MEMBER_GENERATING.contains(&simple)
        || LOGGERS.contains(&simple)
        || CONSTRUCTOR_GENERATING.contains(&simple)
}

/// Whether `simple` names an annotation that makes Lombok emit a constructor — regardless of what
/// that constructor assigns. This is the "the type HAS a constructor, it is just not in the source"
/// question: an enum whose constants pass arguments to one, a class whose `new` call has a target.
pub fn generates_constructor(simple: &str) -> bool {
    CONSTRUCTOR_GENERATING.contains(&simple)
}

/// Whether the annotation `simple`, written with the arguments `args` (the text between its
/// parentheses, `None` for a marker annotation), emits a constructor that **assigns the type's
/// blank `final` fields**.
///
/// Narrower than [`generates_constructor`], and the difference is `@NoArgsConstructor`: it assigns
/// nothing unless `force = true`, which initialises the finals to their default value. Without
/// `force` the class does not compile at all — a different error from "this field is never
/// initialised", so the blank-final report stands.
pub fn initializes_blank_finals(simple: &str, args: Option<&str>) -> bool {
    match simple {
        "NoArgsConstructor" => args.is_some_and(|a| flag_is_true(a, "force")),
        other => generates_constructor(other),
    }
}

/// Whether `simple` is one of Lombok's [`INFERENCE_KEYWORDS`].
pub fn is_inference_keyword(simple: &str) -> bool {
    INFERENCE_KEYWORDS.contains(&simple)
}

/// Whether an annotation's argument text sets `key` to `true` — `force = true`, `fluent=true`.
///
/// Whitespace is compacted first, so every spelling of the same pair matches. Deliberately textual:
/// the alternative is a parser for annotation arguments, and the only thing anyone asks of them
/// here is whether one boolean flag is on.
pub fn flag_is_true(args: &str, key: &str) -> bool {
    let compact: String = args.chars().filter(|c| !c.is_whitespace()).collect();
    compact.contains(&format!("{key}=true"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_builders_generate_a_constructor() {
        // The regression this crate exists for: `@Builder` on a type carries the all-args
        // constructor its builder calls, so the type's blank finals ARE initialised.
        for ann in ["Builder", "SuperBuilder"] {
            assert!(generates_constructor(ann), "{ann}");
            assert!(initializes_blank_finals(ann, None), "{ann}");
        }
    }

    #[test]
    fn no_args_constructor_needs_force_to_initialize_finals() {
        assert!(generates_constructor("NoArgsConstructor"));
        assert!(!initializes_blank_finals("NoArgsConstructor", None));
        assert!(!initializes_blank_finals("NoArgsConstructor", Some("access = AccessLevel.PRIVATE")));
        assert!(initializes_blank_finals("NoArgsConstructor", Some("force = true")));
        assert!(initializes_blank_finals("NoArgsConstructor", Some("force=true")));
        assert!(initializes_blank_finals(
            "NoArgsConstructor",
            Some("access = AccessLevel.PRIVATE, force = true")
        ));
    }

    #[test]
    fn accessor_only_annotations_generate_no_constructor() {
        for ann in ["Getter", "Setter", "Accessors", "NonNull", "Slf4j"] {
            assert!(!generates_constructor(ann), "{ann}");
            assert!(!initializes_blank_finals(ann, None), "{ann}");
        }
    }

    #[test]
    fn every_constructor_annotation_also_generates_members() {
        for ann in CONSTRUCTOR_GENERATING {
            assert!(generates_members(ann), "{ann}");
        }
        for ann in LOGGERS {
            assert!(generates_members(ann), "{ann}");
        }
    }

    #[test]
    fn a_flag_that_is_not_set_is_not_true() {
        assert!(!flag_is_true("force = false", "force"));
        assert!(!flag_is_true("fluent = true", "force"));
        assert!(flag_is_true("fluent = true", "fluent"));
    }
}
