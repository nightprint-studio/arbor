//! The naming pack against a real legacy-style file.
//!
//! `JSRxmlCompiler` is a genuine source from a user's project, kept verbatim because every
//! interesting construct in it caused a real bug: a `record` whose components are *not* parameters,
//! Lombok `val` locals, `static final` constants, a lambda, generics over a project type, and a
//! class name (`JSRxmlCompiler`) whose acronym run must survive PascalCase untouched.
//!
//! What this pins down is the **classification** — which declaration is what kind. That is the
//! input to every rename decision, including whether a fix may be applied without a preview, so
//! getting it wrong here is how a bulk fix rewrites code nobody asked it to.

use bennu_naming::prelude::{NamingConfig, Target};
use std::collections::BTreeMap;

const SOURCE: &str = r##"
import lombok.val;
import net.sf.jasperreports.engine.JasperCompileManager;

import java.nio.file.Path;
import java.util.List;
import java.util.Queue;

/**
 * Compila ricorsivamente tutti i jrxml di una cartella, in parallelo.
 */
public class JSRxmlCompiler {

    private static final String JRXML_EXTENSION = ".jrxml";
    private static final String DEFAULT_SOURCE = "src/main/resources/jasper/PA";
    private static final int DEFAULT_THREAD_COUNT = 1;
    private static final int PROGRESS_BAR_WIDTH = 30;

    /** Esito di una singola compilazione. */
    private record compilation_failure(Path source_path, String error_message) {}

    public static void main(final String[] arguments) throws Exception {
        final Path source_directory = Paths.get(argument_or_default(arguments, 0, DEFAULT_SOURCE));
        final int thread_count = 1;
        final List<Path> report_paths = collect_report_paths(source_directory);
        final Instant started_at = Instant.now();
        final List<compilation_failure> failures = compile_all(report_paths, source_directory);
    }

    private static List<Path> collect_report_paths(final Path source_directory) throws IOException {
        try (final Stream<Path> tree = Files.walk(source_directory)) {
            return tree
                .filter(candidate_path -> candidate_path.toString().endsWith(JRXML_EXTENSION))
                .collect(Collectors.toList());
        }
    }

    private static List<compilation_failure> compile_all(
        final List<Path> report_paths
        , final Path source_directory
    ) throws InterruptedException {
        final Queue<compilation_failure> failures = new ConcurrentLinkedQueue<>();
        final AtomicInteger completed_count = new AtomicInteger();
        return failures.stream()
                .sorted(Comparator.comparing(failure -> failure.source_path().toString()))
                .collect(Collectors.toList());
    }

    private static synchronized void report_progress(
        final int completed_count
        , final int total_count
        , final Throwable compilation_error
    ) {
        final int percentage = (completed_count * 100) / total_count;
        val progress_bar = "#".repeat(percentage);
        val outcome_marker = compilation_error == null ? "OK  " : "FAIL";
    }

    private static String argument_or_default(
        final String[] arguments
        , final int index
        , final String fallback_value
    ) {
        return arguments.length > index ? arguments[index] : fallback_value;
    }

    private static Throwable root_cause_of(final Throwable error) {
        return error.getCause() == null ? error : root_cause_of(error.getCause());
    }
}
"##;

/// The whole Java standard convention, switched on.
fn standard() -> NamingConfig {
    NamingConfig {
        enabled: true,
        ignore: Vec::new(),
        rules: BTreeMap::from([(
            "java".to_string(),
            bennu_naming::prelude::LanguageRules::from_pairs(
                bennu_naming::java::JAVA.standard.iter().copied(),
            ),
        )]),
        overrides: Vec::new(),
    }
}

fn violations() -> Vec<bennu_naming::prelude::Violation> {
    bennu_naming::prelude::violations("src/JSRxmlCompiler.java", SOURCE, &standard())
}

fn named(target: Target) -> Vec<(String, String)> {
    violations()
        .into_iter()
        .filter(|v| v.target == target)
        .map(|v| (v.name, v.suggested))
        .collect()
}

#[test]
fn an_acronym_class_name_is_already_pascal_case() {
    // `JSRxmlCompiler` splits to JS / Rxml / Compiler and renders back to itself. A convention that
    // "fixed" this would be rewriting a name that was never wrong.
    let types = named(Target::Type);
    assert!(
        !types.iter().any(|(name, _)| name == "JSRxmlCompiler"),
        "JSRxmlCompiler must not be flagged: {types:?}"
    );
}

#[test]
fn the_record_type_is_flagged_as_a_type() {
    let types = named(Target::Type);
    assert_eq!(
        types,
        [("compilation_failure".to_string(), "CompilationFailure".to_string())],
        "the record is the only misnamed type here"
    );
}

#[test]
fn record_components_are_fields_not_parameters() {
    // The regression this test exists for: a record component is a private final field PLUS a
    // generated accessor (`failure.source_path()`), so it is NOT file-local and its fix must go
    // through the rename preview. Classified as a parameter, the bulk fix would rewrite the
    // component in place and leave every accessor call behind.
    let fields = named(Target::Field);
    let field_names: Vec<&str> = fields.iter().map(|(n, _)| n.as_str()).collect();
    assert!(field_names.contains(&"source_path"), "record components must be fields: {fields:?}");
    assert!(field_names.contains(&"error_message"), "record components must be fields: {fields:?}");

    let parameters = named(Target::Parameter);
    for component in ["source_path", "error_message"] {
        assert!(
            !parameters.iter().any(|(name, _)| name == component),
            "`{component}` is a record component, not a parameter: {parameters:?}"
        );
    }

    // …and therefore never applied without showing the user what it touches.
    for violation in violations() {
        if violation.name == "source_path" || violation.name == "error_message" {
            assert!(
                !violation.file_local,
                "`{}` must not be treated as safe to rename unseen",
                violation.name
            );
        }
    }
}

#[test]
fn constants_are_already_upper_snake() {
    let constants = named(Target::Constant);
    assert!(constants.is_empty(), "every constant here is already correct: {constants:?}");
}

#[test]
fn methods_are_flagged_with_their_camel_case_spelling() {
    let methods = named(Target::Method);
    let by_name: BTreeMap<&str, &str> =
        methods.iter().map(|(n, s)| (n.as_str(), s.as_str())).collect();
    assert_eq!(by_name.get("collect_report_paths"), Some(&"collectReportPaths"));
    assert_eq!(by_name.get("compile_all"), Some(&"compileAll"));
    assert_eq!(by_name.get("report_progress"), Some(&"reportProgress"));
    assert_eq!(by_name.get("argument_or_default"), Some(&"argumentOrDefault"));
    assert_eq!(by_name.get("root_cause_of"), Some(&"rootCauseOf"));
    // `main` is already camelCase and must not appear at all.
    assert!(!by_name.contains_key("main"), "{methods:?}");
}

#[test]
fn lombok_val_locals_are_locals() {
    // `val x = …` parses as a local declaration whose type happens to be `val`. If the pack missed
    // it, two of this file's names would silently never be checked.
    let locals = named(Target::Local);
    let names: Vec<&str> = locals.iter().map(|(n, _)| n.as_str()).collect();
    assert!(names.contains(&"progress_bar"), "{locals:?}");
    assert!(names.contains(&"outcome_marker"), "{locals:?}");
}

#[test]
fn a_lambda_parameter_is_a_parameter() {
    let parameters = named(Target::Parameter);
    let names: Vec<&str> = parameters.iter().map(|(n, _)| n.as_str()).collect();
    assert!(names.contains(&"candidate_path"), "{parameters:?}");
}

#[test]
fn locals_and_parameters_are_the_only_ones_safe_to_rename_unseen() {
    for violation in violations() {
        let expected = matches!(violation.target, Target::Local | Target::Parameter);
        assert_eq!(
            violation.file_local, expected,
            "`{}` ({}) has the wrong safety classification",
            violation.name, violation.target
        );
    }
}

#[test]
fn nothing_is_flagged_when_the_project_did_not_opt_in() {
    let mut off = standard();
    off.enabled = false;
    assert!(bennu_naming::prelude::violations("src/JSRxmlCompiler.java", SOURCE, &off).is_empty());
}

#[test]
fn every_suggestion_is_a_fixed_point() {
    // Applying a fix must never produce something the check flags again — the property the whole
    // design rests on, asserted here against real code rather than a synthetic name.
    for violation in violations() {
        assert!(
            violation.convention.accepts(&violation.suggested),
            "{} -> {} would be flagged again",
            violation.name,
            violation.suggested
        );
        assert_ne!(violation.name, violation.suggested);
    }
}
