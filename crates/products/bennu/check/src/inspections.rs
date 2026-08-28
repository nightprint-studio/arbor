//! The project's **inspection policy** — which checks report, and how loudly.
//!
//! A validator with seventy-odd checks and no way to turn one down is a validator you stop reading.
//! On a real legacy project some kinds are noise by construction: a codebase that predates
//! `@Override`, a generated package nobody will fix, a convention the team has decided against.
//! Until now the only options were to look at all of it or none of it, and everyone picks none.
//!
//! Two mechanisms, for two different scopes.
//!
//! ## Per project: a severity per kind
//!
//! `[inspections.severity]` in the project's config maps a check's `code` to `error`, `warning`,
//! `weak` or `off`. It is a *policy over kinds* — the right shape for "this project does not care
//! about unused imports", and the wrong shape for "this one line is fine".
//!
//! ## Per site: suppression
//!
//! `@SuppressWarnings("unused-import")` on the enclosing declaration, or a
//! `// bennu:ignore unused-import` comment on the offending line or the one above it. That is the
//! shape for "I know, and here is where I said so" — it lives beside the code, survives a rename of
//! the file, and is visible to the next reader, which a settings screen is not.
//!
//! `@SuppressWarnings` is also read with javac's own vocabulary where it overlaps (`unchecked`,
//! `deprecation`, `all`), because that is what is already written in a legacy codebase and a tool
//! that ignored it would be asking everyone to annotate twice.
//!
//! ## Order
//!
//! Severity is applied first, then suppression — so a check turned `off` costs no suppression
//! lookup, and a suppressed diagnostic of a re-severitied kind is still suppressed.

use std::collections::{HashMap, HashSet};

use bennu_proto::prelude::Diagnostic;

/// How loudly a kind reports, or that it does not.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    Error,
    Warning,
    /// A style finding: true, but not a defect. Drawn faintly and grouped below the rest.
    Weak,
    /// Not reported at all.
    Off,
}

impl Level {
    /// The wire spelling — what a [`Diagnostic::severity`] carries.
    pub const fn as_str(self) -> &'static str {
        match self {
            Level::Error => "error",
            Level::Warning => "warning",
            Level::Weak => "weak",
            Level::Off => "off",
        }
    }

    /// Parse a configured spelling. `None` for anything else, so a typo in the config file leaves
    /// the check at its default rather than silently turning it off.
    pub fn parse(s: &str) -> Option<Level> {
        match s.trim().to_ascii_lowercase().as_str() {
            "error" => Some(Level::Error),
            "warning" | "warn" => Some(Level::Warning),
            "weak" => Some(Level::Weak),
            "off" | "none" | "ignore" => Some(Level::Off),
            _ => None,
        }
    }
}

/// The policy, ready to apply to a file's diagnostics.
#[derive(Debug, Clone, Default)]
pub struct Inspections {
    /// `code` → the level it reports at. A code that isn't here keeps the check's own default.
    severity: HashMap<String, Level>,
}

impl Inspections {
    /// Build from `(code, level)` pairs — the config's `[inspections.severity]` table.
    ///
    /// An unparseable level is dropped rather than guessed at: the check then reports at its own
    /// default, which is the behaviour someone who mistyped `warining` would want over silence.
    pub fn from_pairs<'a>(pairs: impl IntoIterator<Item = (&'a str, &'a str)>) -> Self {
        let severity = pairs
            .into_iter()
            .filter_map(|(code, level)| Level::parse(level).map(|l| (code.to_string(), l)))
            .collect();
        Self { severity }
    }

    /// Whether the project configured nothing — the fast path, and the common one.
    pub fn is_default(&self) -> bool {
        self.severity.is_empty()
    }

    /// The level `code` reports at, given the check's own `default` spelling.
    pub fn level_of(&self, code: &str, default: &str) -> Level {
        match self.severity.get(code) {
            Some(l) => *l,
            None => Level::parse(default).unwrap_or(Level::Warning),
        }
    }

    /// Apply the policy to one file's diagnostics: drop what is off, restamp what was re-levelled,
    /// and drop what the source itself suppresses.
    pub fn apply(&self, source: &str, diags: Vec<Diagnostic>) -> Vec<Diagnostic> {
        let mut kept: Vec<Diagnostic> = Vec::with_capacity(diags.len());
        for mut d in diags {
            // A diagnostic with no `code` is from a check not yet on the catalog — nothing to key a
            // policy off, so it passes through untouched rather than being silently re-levelled.
            if !d.code.is_empty() {
                match self.level_of(&d.code, &d.severity) {
                    Level::Off => continue,
                    level => d.severity = level.as_str().to_string(),
                }
            }
            kept.push(d);
        }
        if kept.is_empty() {
            return kept;
        }
        let sup = Suppressions::read(source);
        if sup.is_empty() {
            return kept;
        }
        kept.retain(|d| !sup.covers(source, d));
        kept
    }
}

/// What a file says about the diagnostics it does not want.
#[derive(Default)]
struct Suppressions {
    /// `(0-based line, code)` from a `bennu:ignore` / `noinspection` comment. The empty code means
    /// "everything on this line".
    lines: HashSet<(usize, String)>,
    /// `(declaration span, the codes it suppresses)` from `@SuppressWarnings`.
    regions: Vec<((usize, usize), Vec<String>)>,
}

impl Suppressions {
    fn is_empty(&self) -> bool {
        self.lines.is_empty() && self.regions.is_empty()
    }

    /// Read both mechanisms out of `source`.
    ///
    /// Both are gated on a substring test first. The parse `@SuppressWarnings` needs is a whole
    /// extra parse of the file, and the overwhelming majority of files contain neither marker — so
    /// the common case costs two `contains` and nothing else.
    fn read(source: &str) -> Self {
        let mut out = Self::default();
        if source.contains("bennu:ignore") || source.contains("noinspection") {
            out.read_line_comments(source);
        }
        if source.contains("SuppressWarnings") {
            out.read_annotations(source);
        }
        out
    }

    /// `// bennu:ignore code-a code-b` — on the offending line, or on the line above it.
    fn read_line_comments(&mut self, source: &str) {
        for (i, line) in source.lines().enumerate() {
            let Some(rest) = marker_tail(line) else { continue };
            let codes: Vec<&str> = rest.split_whitespace().collect();
            if codes.is_empty() {
                // A bare marker suppresses everything on the line it governs — which is what
                // someone who wrote it without a code meant, and is why the empty code exists.
                self.lines.insert((i, String::new()));
                self.lines.insert((i + 1, String::new()));
                continue;
            }
            for code in codes {
                let code = code.trim_matches(|c: char| c == ',' || c == '"');
                if code.is_empty() {
                    continue;
                }
                // The comment governs its OWN line (a trailing comment) and the NEXT one (a comment
                // written above the code). Both spellings are in use and neither is wrong.
                self.lines.insert((i, code.to_string()));
                self.lines.insert((i + 1, code.to_string()));
            }
        }
    }

    /// `@SuppressWarnings("unused-import")` — governs the declaration it annotates.
    fn read_annotations(&mut self, source: &str) {
        let Some(tree) = bennu_java::prelude::parse_java(source) else { return };
        let bytes = source.as_bytes();
        let mut stack = vec![tree.root_node()];
        while let Some(n) = stack.pop() {
            let mut c = n.walk();
            for child in n.named_children(&mut c) {
                stack.push(child);
            }
            if !matches!(n.kind(), "annotation" | "marker_annotation") {
                continue;
            }
            let Some(name) = n.child_by_field_name("name") else { continue };
            // `@SuppressWarnings` and `@java.lang.SuppressWarnings` are the same annotation.
            let simple = name
                .utf8_text(bytes)
                .unwrap_or("")
                .rsplit('.')
                .next()
                .unwrap_or("");
            if simple != "SuppressWarnings" {
                continue;
            }
            let codes = string_literals_in(n, bytes);
            if codes.is_empty() {
                continue;
            }
            // The declaration the annotation is attached to — its parent, once past the modifiers
            // node the grammar wraps annotations in.
            let Some(decl) = annotated_declaration(n) else { continue };
            self.regions.push(((decl.start_byte(), decl.end_byte()), codes));
        }
    }

    /// Whether this file suppresses `d`.
    fn covers(&self, source: &str, d: &Diagnostic) -> bool {
        if !self.lines.is_empty() {
            let line = line_of(source, d.start);
            if self.lines.contains(&(line, String::new()))
                || self.lines.contains(&(line, d.code.clone()))
            {
                return true;
            }
        }
        self.regions.iter().any(|((s, e), codes)| {
            d.start >= *s && d.start < *e && codes.iter().any(|c| suppresses(c, &d.code))
        })
    }
}

/// Whether a `@SuppressWarnings` value covers a check `code`.
///
/// Bennu's own codes match exactly. `"all"` matches everything, which is what it means everywhere
/// else. Javac's vocabulary is honoured where it overlaps, because a legacy file already carries it
/// and asking someone to annotate the same line twice is asking them not to bother.
fn suppresses(value: &str, code: &str) -> bool {
    if value == code || value == "all" {
        return true;
    }
    match value {
        "unused" => code.starts_with("unused-") || code == "redundant-import",
        "deprecation" => code == "deprecation",
        "fallthrough" => code == "switch-fallthrough",
        "serial" => code.starts_with("serial"),
        _ => false,
    }
}

/// The text after a suppression marker in a comment line, if it has one.
fn marker_tail(line: &str) -> Option<&str> {
    let comment = line.find("//").map(|i| &line[i + 2..]).or_else(|| {
        line.find("/*").map(|i| line[i + 2..].trim_end_matches("*/"))
    })?;
    for marker in ["bennu:ignore", "noinspection"] {
        if let Some(i) = comment.find(marker) {
            return Some(comment[i + marker.len()..].trim());
        }
    }
    None
}

/// Every string-literal value inside an annotation — `@SuppressWarnings({"a", "b"})` → `["a", "b"]`.
fn string_literals_in(node: tree_sitter::Node, bytes: &[u8]) -> Vec<String> {
    let mut out = Vec::new();
    let mut stack = vec![node];
    while let Some(n) = stack.pop() {
        let mut c = n.walk();
        for child in n.named_children(&mut c) {
            stack.push(child);
        }
        if n.kind() == "string_literal" {
            if let Ok(t) = n.utf8_text(bytes) {
                out.push(t.trim_matches('"').to_string());
            }
        }
    }
    out
}

/// The declaration an annotation is attached to.
///
/// The grammar puts annotations inside a `modifiers` node whose parent is the declaration, so the
/// climb is at most two steps — but it is a climb rather than a fixed `parent().parent()` because a
/// package annotation and a parameter annotation are shaped differently and neither should crash it.
fn annotated_declaration(annotation: tree_sitter::Node) -> Option<tree_sitter::Node> {
    let mut cur = annotation.parent();
    while let Some(n) = cur {
        if n.kind().ends_with("_declaration") || n.kind() == "field_declaration" {
            return Some(n);
        }
        if n.kind() == "program" {
            return None;
        }
        cur = n.parent();
    }
    None
}

/// The 0-based line `offset` falls on.
fn line_of(source: &str, offset: usize) -> usize {
    source[..offset.min(source.len())].bytes().filter(|b| *b == b'\n').count()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::check_id::CheckId;

    fn diag(code: &str, severity: &str, start: usize, end: usize) -> Diagnostic {
        Diagnostic {
            message: "m".into(),
            severity: severity.into(),
            code: code.into(),
            start,
            end,
        }
    }

    fn codes(v: &[Diagnostic]) -> Vec<String> {
        v.iter().map(|d| d.code.clone()).collect()
    }

    #[test]
    fn a_check_turned_off_is_dropped() {
        let ins = Inspections::from_pairs([("unused-import", "off")]);
        let out = ins.apply("class C {}", vec![diag("unused-import", "warning", 0, 5)]);
        assert!(out.is_empty(), "{:?}", codes(&out));
    }

    #[test]
    fn a_re_levelled_check_keeps_reporting_at_the_new_level() {
        let ins = Inspections::from_pairs([("unused-import", "error")]);
        let out = ins.apply("class C {}", vec![diag("unused-import", "warning", 0, 5)]);
        assert_eq!(out[0].severity, "error");
    }

    #[test]
    fn an_unconfigured_check_keeps_its_own_default() {
        let ins = Inspections::default();
        let out = ins.apply("class C {}", vec![diag("unknown-member", "error", 0, 5)]);
        assert_eq!(out[0].severity, "error");
    }

    /// A typo in the config must not silence a check — the risk is all one way.
    #[test]
    fn an_unparseable_level_is_ignored() {
        let ins = Inspections::from_pairs([("unused-import", "warining")]);
        let out = ins.apply("class C {}", vec![diag("unused-import", "warning", 0, 5)]);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].severity, "warning");
    }

    /// A diagnostic from a check not yet on the catalog carries no code, so no policy can key off
    /// it — it must pass through rather than be re-levelled by accident.
    #[test]
    fn a_diagnostic_without_a_code_passes_through() {
        let ins = Inspections::from_pairs([("", "off")]);
        let out = ins.apply("class C {}", vec![diag("", "warning", 0, 5)]);
        assert_eq!(out.len(), 1);
    }

    // ── suppression ──────────────────────────────────────────────────────────

    #[test]
    fn a_comment_above_the_line_suppresses_it() {
        let src = "class C {\n    // bennu:ignore unused-import\n    int x;\n}\n";
        let at = src.find("int x").unwrap();
        let out = Inspections::default().apply(src, vec![diag("unused-import", "warning", at, at + 3)]);
        assert!(out.is_empty(), "{:?}", codes(&out));
    }

    #[test]
    fn a_trailing_comment_on_the_line_suppresses_it() {
        let src = "class C {\n    int x; // bennu:ignore unused-import\n}\n";
        let at = src.find("int x").unwrap();
        let out = Inspections::default().apply(src, vec![diag("unused-import", "warning", at, at + 3)]);
        assert!(out.is_empty(), "{:?}", codes(&out));
    }

    /// A marker names the kinds it silences; a different kind on the same line still reports.
    #[test]
    fn a_marker_silences_only_the_codes_it_names() {
        let src = "class C {\n    // bennu:ignore unused-import\n    int x;\n}\n";
        let at = src.find("int x").unwrap();
        let out = Inspections::default().apply(src, vec![diag("unknown-member", "error", at, at + 3)]);
        assert_eq!(out.len(), 1, "{:?}", codes(&out));
    }

    #[test]
    fn a_bare_marker_silences_the_whole_line() {
        let src = "class C {\n    // bennu:ignore\n    int x;\n}\n";
        let at = src.find("int x").unwrap();
        let out = Inspections::default().apply(src, vec![diag("anything", "error", at, at + 3)]);
        assert!(out.is_empty(), "{:?}", codes(&out));
    }

    #[test]
    fn suppress_warnings_covers_the_declaration_it_annotates() {
        let src = "class C {\n    @SuppressWarnings(\"unused-import\")\n    void m() {\n        int x = 1;\n    }\n    void n() {\n        int y = 2;\n    }\n}\n";
        let inside = src.find("int x").unwrap();
        let outside = src.find("int y").unwrap();
        let out = Inspections::default().apply(
            src,
            vec![
                diag("unused-import", "warning", inside, inside + 3),
                diag("unused-import", "warning", outside, outside + 3),
            ],
        );
        assert_eq!(out.len(), 1, "only the annotated method is covered: {:?}", out);
        assert_eq!(out[0].start, outside);
    }

    #[test]
    fn suppress_warnings_all_covers_everything_in_it() {
        let src = "class C {\n    @SuppressWarnings(\"all\")\n    void m() {\n        int x = 1;\n    }\n}\n";
        let at = src.find("int x").unwrap();
        let out = Inspections::default().apply(src, vec![diag("whatever", "error", at, at + 3)]);
        assert!(out.is_empty());
    }

    /// A legacy file already says `@SuppressWarnings("unused")`; making it say Bennu's spelling too
    /// would be asking for the same statement twice.
    #[test]
    fn javacs_own_vocabulary_is_honoured_where_it_overlaps() {
        assert!(suppresses("unused", "unused-import"));
        assert!(suppresses("fallthrough", "switch-fallthrough"));
        assert!(!suppresses("unused", "unknown-member"));
    }

    /// A file with no marker anywhere pays nothing but two substring tests.
    #[test]
    fn a_file_with_no_markers_suppresses_nothing() {
        let src = "class C {\n    int x = 1;\n}\n";
        let out = Inspections::default().apply(src, vec![diag("unknown-member", "error", 0, 3)]);
        assert_eq!(out.len(), 1);
    }

    // ── the catalog a settings screen renders ────────────────────────────────

    #[test]
    fn every_catalogued_kind_has_a_unique_kebab_case_code() {
        let mut seen = HashSet::new();
        for id in CheckId::ALL {
            let code = id.code();
            assert!(!code.is_empty(), "{id:?} has no code");
            assert!(
                code.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-'),
                "{code} is not kebab-case"
            );
            assert!(seen.insert(code), "two kinds share the code {code}");
        }
    }

    /// A tripwire, and an honest one: adding a variant to `code()` without adding it to `ALL` leaves
    /// a check nobody can configure, and the count is the only thing that can notice.
    #[test]
    fn the_catalog_lists_every_kind() {
        assert_eq!(
            CheckId::ALL.len(),
            88,
            "a new check kind must be added to `CheckId::ALL` (and this count bumped)"
        );
    }

    /// Every catalogued kind's default level must be one the policy understands, or configuring it
    /// back to its default would be impossible.
    #[test]
    fn every_default_severity_parses() {
        for id in CheckId::ALL {
            assert!(
                Level::parse(id.severity()).is_some(),
                "{:?} defaults to an unparseable severity {:?}",
                id,
                id.severity()
            );
        }
    }
}
