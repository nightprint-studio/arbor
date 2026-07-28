//! Properties asserted over **every** input in `test/corpus/`.
//!
//! The Tree-sitter corpus checks the shape of the tree; this file checks the
//! things the Rust layer promises about it, and it does so over the same 200-odd
//! inputs rather than a handful chosen by hand. The most important of them is
//! the byte-identical round trip: `picus-rewrite` may only ever splice, so if
//! statements plus gaps did not reproduce the source exactly, every rewrite
//! would be a silent corruption.

use std::fs;
use std::path::{Path, PathBuf};

use picus_parse::prelude::*;

fn corpus_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("test").join("corpus")
}

/// Every `(file, test name, input)` in the corpus.
fn corpus_cases() -> Vec<(String, String, String)> {
    let mut cases = Vec::new();
    let mut files: Vec<PathBuf> = fs::read_dir(corpus_dir())
        .expect("test/corpus must exist")
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|e| e == "txt"))
        .collect();
    files.sort();

    for path in files {
        let file = path.file_name().unwrap_or_default().to_string_lossy().to_string();
        let text = fs::read_to_string(&path).expect("corpus file must be readable");
        cases.extend(parse_corpus(&file, &text));
    }
    assert!(cases.len() > 150, "the corpus should be large; found {}", cases.len());
    cases
}

/// Split a Tree-sitter corpus file into its cases.
///
/// The format is a header fenced by lines of `=`, then the input, then a line of
/// `-`, then the expected tree.
fn parse_corpus(file: &str, text: &str) -> Vec<(String, String, String)> {
    let lines: Vec<&str> = text.lines().collect();
    let is_fence = |l: &str, c: char| l.len() >= 10 && l.chars().all(|ch| ch == c);

    let mut out = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        if !is_fence(lines[i], '=') || i + 2 >= lines.len() || !is_fence(lines[i + 2], '=') {
            i += 1;
            continue;
        }
        let name = lines[i + 1].to_string();
        let mut j = i + 3;
        let mut input: Vec<&str> = Vec::new();
        while j < lines.len() && !is_fence(lines[j], '-') {
            input.push(lines[j]);
            j += 1;
        }
        // The corpus format puts one blank line on each side of the input.
        let body = input.join("\n");
        out.push((file.to_string(), name, body.trim_matches('\n').to_string()));
        i = j + 1;
    }
    out
}

#[test]
fn every_corpus_input_round_trips_byte_for_byte() {
    let mut parser = SqlParser::new();
    for (file, name, input) in corpus_cases() {
        for engine in [EngineKind::Oracle, EngineKind::Postgres] {
            let parsed = parser.parse(&input, DialectScope::One(engine));
            assert_eq!(
                parsed.reassemble(&input),
                input,
                "{file} / {name} / {engine}: statements plus gaps must reproduce the source"
            );
        }
    }
}

#[test]
fn segments_tile_the_source_exactly_once() {
    let mut parser = SqlParser::new();
    for (file, name, input) in corpus_cases() {
        let parsed = parser.parse(&input, DialectScope::One(EngineKind::Oracle));
        let mut cursor = 0usize;
        for segment in parsed.segments() {
            let range = segment.range();
            assert_eq!(range.start, cursor, "{file} / {name}: a gap was left between segments");
            assert!(range.end >= range.start, "{file} / {name}: inverted range");
            cursor = range.end;
        }
        assert_eq!(cursor, input.len(), "{file} / {name}: the tail of the file was dropped");
    }
}

#[test]
fn statement_ranges_are_ordered_disjoint_and_in_bounds() {
    let mut parser = SqlParser::new();
    for (file, name, input) in corpus_cases() {
        let parsed = parser.parse(&input, DialectScope::One(EngineKind::Postgres));
        let mut previous_end = 0usize;
        for statement in &parsed.statements {
            assert!(
                statement.range.start >= previous_end,
                "{file} / {name}: statements overlap"
            );
            assert!(
                statement.range.end <= input.len(),
                "{file} / {name}: statement runs past the end of the source"
            );
            previous_end = statement.range.end;
        }
    }
}

#[test]
fn the_whole_corpus_parses_without_hard_errors() {
    let mut parser = SqlParser::new();
    for (file, name, input) in corpus_cases() {
        let parsed = parser.parse(&input, DialectScope::One(EngineKind::Oracle));
        let errors: Vec<String> = parsed
            .errors.iter()
            .map(|e| format!("{:?} at {:?} in {}: {}", e.kind, e.range, e.parent, e.text))
            .collect();
        assert!(errors.is_empty(), "{file} / {name}: {errors:?}");
    }
}

#[test]
fn every_corpus_input_yields_at_least_one_statement() {
    let mut parser = SqlParser::new();
    for (file, name, input) in corpus_cases() {
        let parsed = parser.parse(&input, DialectScope::One(EngineKind::Oracle));
        assert!(
            !parsed.statements.is_empty(),
            "{file} / {name}: nothing was recognised as a statement"
        );
    }
}

#[test]
fn parsing_is_independent_of_the_declared_dialect_except_for_findings() {
    // The grammar is ONE superset: switching the engine must change which
    // constructs are reported as foreign and nothing else. If the two runs ever
    // disagreed on statement boundaries, the "one grammar" premise would be
    // broken.
    let mut parser = SqlParser::new();
    for (file, name, input) in corpus_cases() {
        let oracle = parser.parse(&input, DialectScope::One(EngineKind::Oracle));
        let postgres = parser.parse(&input, DialectScope::One(EngineKind::Postgres));
        let oracle_ranges: Vec<_> = oracle.statements.iter().map(|s| s.range).collect();
        let postgres_ranges: Vec<_> = postgres.statements.iter().map(|s| s.range).collect();
        assert_eq!(oracle_ranges, postgres_ranges, "{file} / {name}");
    }
}
