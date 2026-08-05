//! The two lines of Maven's console output worth reading structurally.
//!
//! The XML reports are the truth, but they only exist once a class has **finished**. On a
//! class that takes forty seconds — or hangs — the reports directory says nothing at all,
//! and a panel driven by it alone looks frozen. Surefire announces each class as it starts
//! it, and that one line is what lets the tree show a spinner on the row that is running
//! right now.
//!
//! The totals line is read for the opposite reason: as a cross-check. If Maven says twelve
//! tests ran and the reports account for eight, something swallowed four of them, and a
//! panel that shows only its own eight would be quietly wrong.

/// The class name from Surefire's `Running com.acme.FooTest` announcement, with or without
/// Maven's `[INFO]` prefix.
///
/// Deliberately strict about what follows: `Running` also begins ordinary log lines from the
/// code under test ("Running migration 3"), and mistaking one for a class start would put a
/// phantom row in the tree.
pub fn running_class(line: &str) -> Option<&str> {
    let body = strip_level(line.trim());
    let rest = body.strip_prefix("Running ")?.trim();
    is_class_name(rest).then_some(rest)
}

/// Totals as Surefire's summary line reports them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RunTotals {
    pub run: u32,
    pub failures: u32,
    pub errors: u32,
    pub skipped: u32,
}

/// The `Tests run: 12, Failures: 1, Errors: 0, Skipped: 2` summary.
///
/// Surefire prints this **twice**: once per class (suffixed `- in com.acme.FooTest`) and
/// once for the whole run. Only the whole-run one is returned — a per-class line would
/// otherwise be read as the final tally of a run that had barely started.
pub fn run_totals(line: &str) -> Option<RunTotals> {
    let body = strip_level(line.trim());
    if !body.starts_with("Tests run:") || body.contains(" - in ") {
        return None;
    }
    let mut totals = RunTotals::default();
    for part in body.split(',') {
        let Some((key, value)) = part.split_once(':') else { continue };
        let Ok(n) = value.trim().split_whitespace().next().unwrap_or("").parse::<u32>() else {
            continue;
        };
        match key.trim() {
            "Tests run" => totals.run = n,
            "Failures" => totals.failures = n,
            "Errors" => totals.errors = n,
            "Skipped" => totals.skipped = n,
            _ => {}
        }
    }
    Some(totals)
}

/// Strip Maven's `[INFO]` / `[WARNING]` / `[ERROR]` level prefix.
fn strip_level(line: &str) -> &str {
    for tag in ["[INFO]", "[WARNING]", "[ERROR]", "[DEBUG]"] {
        if let Some(rest) = line.strip_prefix(tag) {
            return rest.trim_start();
        }
    }
    line
}

/// Whether `s` looks like a Java class name and nothing else: dot-separated identifiers,
/// no spaces, at least one uppercase-initial segment.
fn is_class_name(s: &str) -> bool {
    if s.is_empty() || s.contains(char::is_whitespace) {
        return false;
    }
    s.split('.').all(|seg| {
        !seg.is_empty()
            && seg.starts_with(|c: char| c.is_ascii_alphabetic() || c == '_')
            && seg.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '$')
    }) && s.split('.').any(|seg| seg.starts_with(char::is_uppercase))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_class_being_run() {
        assert_eq!(running_class("[INFO] Running com.acme.OrderTest"), Some("com.acme.OrderTest"));
        assert_eq!(running_class("Running com.acme.OrderTest"), Some("com.acme.OrderTest"));
        assert_eq!(running_class("  Running OrderTest  "), Some("OrderTest"));
    }

    #[test]
    fn reads_a_nested_class() {
        assert_eq!(running_class("[INFO] Running com.acme.OuterTest$Inner"), Some("com.acme.OuterTest$Inner"));
    }

    /// The reason `running_class` checks the shape: the code under test logs too.
    #[test]
    fn does_not_mistake_application_logging_for_a_class_start() {
        assert_eq!(running_class("[INFO] Running migration 3 of 7"), None);
        assert_eq!(running_class("Running the import job"), None);
        assert_eq!(running_class("[INFO] Running"), None);
    }

    #[test]
    fn reads_the_run_totals() {
        let t = run_totals("[INFO] Tests run: 12, Failures: 1, Errors: 0, Skipped: 2").expect("parses");
        assert_eq!(t, RunTotals { run: 12, failures: 1, errors: 0, skipped: 2 });
    }

    /// The per-class line looks identical apart from its suffix; reading it as the final
    /// tally would report a whole run's result after its first class.
    #[test]
    fn ignores_the_per_class_totals_line() {
        assert!(run_totals(
            "[INFO] Tests run: 3, Failures: 0, Errors: 0, Skipped: 0, Time elapsed: 0.2 s - in com.acme.OrderTest"
        )
        .is_none());
    }

    #[test]
    fn tolerates_a_time_elapsed_field_on_the_summary() {
        let t = run_totals("Tests run: 4, Failures: 0, Errors: 0, Skipped: 1, Time elapsed: 1.5 s")
            .expect("parses");
        assert_eq!(t.run, 4);
        assert_eq!(t.skipped, 1);
    }

    #[test]
    fn other_lines_are_not_totals() {
        assert!(run_totals("[INFO] BUILD SUCCESS").is_none());
        assert!(run_totals("Tests are great").is_none());
    }
}
