//! `restructure` domain — structural search and replace across the repository.
//!
//! The migration this exists for: *portale 1 → portale 2*. Every `INSERT` on one
//! table becomes an `INSERT` on another, with its columns renamed and its values
//! reordered — across four hundred scripts, in two dialects, without a regex
//! anywhere near it.
//!
//! ## Why the pattern is SQL and not a form
//!
//! A form would have to enumerate the transformations Picus is willing to make, and
//! the next migration would need a field the form does not have. A pattern is the
//! statement itself with holes in it, so what it can express is bounded by SQL
//! rather than by this module — see `arbor-syntax`, which owns the matching and
//! knows no SQL at all.
//!
//! ## The same three steps as a generation
//!
//! Find → preview → apply, with the digests carried through, because this writes
//! into the same scripts and deserves the same guarantee: **nothing is written that
//! the user has not seen**, and a file that moved between the preview and the write
//! stops the write rather than being overwritten. The rewrite goes through
//! `picus-rewrite` for the same reason the generator does — encoding, line endings
//! and the round-trip check are not things to re-implement per feature.

use std::collections::BTreeMap;

use arbor_fs::prelude::encoding::EncodingContext;
use arbor_syntax::prelude::{render_with, ByteRange, Pattern, SyntaxError};
use picus_core::prelude::{digest, PicusState, ScriptSnapshot};
use picus_parse::prelude::EngineKind;
use picus_project::prelude::{label_to_encoding, parent_of, FolderRole, LineEnding};
use picus_rewrite::prelude::{commit, prepare_one, PreparedFile, SourceText, Splice};
use serde::{Deserialize, Serialize};

use crate::apply::{line_ending_of, Digests};
use crate::scripts::snapshot_for;

/// Which scripts a transformation is allowed to touch.
///
/// Defaulting to "all of them" would be the wrong default for the one operation in
/// Picus that rewrites four hundred files at once, so every field narrows and the
/// interface is expected to show what is left.
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Scope {
    /// Only scripts under this project-relative folder.
    #[serde(default)]
    pub folder: Option<String>,
    /// Only scripts of this engine. A portable script belongs to both.
    #[serde(default)]
    pub engine: Option<EngineKind>,
    /// Only folders with this role.
    #[serde(default)]
    pub role: Option<FolderRole>,
    /// Explicit list, when the user has picked files by hand. Wins over the rest.
    #[serde(default)]
    pub paths: Vec<String>,
}

impl Scope {
    fn admits(&self, snapshot: &ScriptSnapshot, path: &str) -> bool {
        if !self.paths.is_empty() {
            return self.paths.iter().any(|p| p == path);
        }
        if let Some(folder) = &self.folder {
            if !path.starts_with(folder.as_str()) {
                return false;
            }
        }
        let Some(file) = snapshot.project.file_at(path) else { return false };
        if file.effective_excluded {
            return false;
        }
        if let Some(engine) = self.engine {
            // `covers`, not equality: a **portable** script belongs to both engines
            // and must not fall out of a scope that names one of them.
            if !file.effective_engine.is_some_and(|e| e.covers(engine)) {
                return false;
            }
        }
        if let Some(role) = self.role {
            // The role lives on the folder, never on the file — asking the file
            // for one would be a second answer that could disagree with the tree.
            let folder = snapshot.project.folder_at(parent_of(path));
            if folder.map(|f| f.effective_role) != Some(role) {
                return false;
            }
        }
        true
    }
}

/// One place a pattern matched, in whatever text it was matched against.
///
/// Split out from [`FoundMatch`] so that searching a repository and searching the
/// buffer in front of the user produce the *same* row: same captures, same rendered
/// replacement, same failure when a template cannot be applied here. Two builders
/// for one row is how a feature ends up meaning two subtly different things
/// depending on where it was invoked from.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Hit {
    pub range: ByteRange,
    /// 1-based, for the row the user clicks.
    pub line: usize,
    /// The matched text, so the list reads without a second round trip per row.
    pub text: String,
    /// What each placeholder caught here — the column that tells the user whether
    /// the pattern caught what they meant before anything is rewritten.
    pub captures: BTreeMap<String, String>,
    /// The replacement, when one was asked for. `None` on a plain search, and
    /// `Some` with the reason on a template that could not be rendered here.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub replacement: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub problem: Option<String>,
}

/// One place a pattern matched, and which script it was in.
///
/// Flattened on the wire, so a repository match is one flat object exactly as it
/// always was — the split is an arrangement of this code, not a change of contract.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FoundMatch {
    pub path: String,
    #[serde(flatten)]
    pub hit: Hit,
}

/// Every place `pattern` matches `text`, each carrying what it would become.
///
/// The one builder both search entry points go through.
fn hits_in(compiled: &Pattern, text: &str, replacement: Option<&str>) -> Result<Vec<Hit>, String> {
    let mut hits = Vec::new();
    for found in compiled.find_all(&language(), text).map_err(|e| e.to_string())? {
        let mut captures = BTreeMap::new();
        for capture in &found.captures {
            captures.insert(
                capture.name.clone(),
                capture.range.slice(text).unwrap_or("").to_string(),
            );
        }
        let (rendered, problem) = match replacement {
            None => (None, None),
            Some(template) => match render_with(template, &found, text, true) {
                Ok(text) => (Some(text), None),
                Err(e) => (None, Some(e.to_string())),
            },
        };
        hits.push(Hit {
            line: line_of(text, found.range.start),
            text: found.range.slice(text).unwrap_or("").to_string(),
            range: found.range,
            captures,
            replacement: rendered,
            problem,
        });
    }
    Ok(hits)
}

/// What a search answers.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FindResult {
    pub matches: Vec<FoundMatch>,
    /// Scripts actually looked at — the denominator the count means nothing
    /// without.
    pub scanned: usize,
    /// The placeholder names the pattern declares, for the template editor.
    pub placeholders: Vec<String>,
}

/// Find every place a pattern matches.
///
/// `replacement` is optional: with one, each match carries what it would become,
/// which is how a template is checked *before* a preview is asked for.
#[arbor_rpc::handler]
fn picus_structural_find(
    state: &PicusState,
    root: String,
    pattern: String,
    replacement: Option<String>,
    scope: Option<Scope>,
) -> Result<FindResult, String> {
    let snapshot = snapshot_for(state, &root)?;
    let scope = scope.unwrap_or_default();
    let compiled = compile(&pattern)?;

    let mut matches = vec![];
    let mut scanned = 0usize;
    for (path, text) in scripts_in(&snapshot, &scope) {
        scanned += 1;
        matches.extend(
            hits_in(&compiled, text, replacement.as_deref())?
                .into_iter()
                .map(|hit| FoundMatch { path: path.to_string(), hit }),
        );
    }

    Ok(FindResult {
        placeholders: compiled.names().into_iter().map(str::to_string).collect(),
        matches,
        scanned,
    })
}

/// What a scan of one piece of text answers.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanResult {
    pub matches: Vec<Hit>,
    /// The placeholder names the pattern declares, for the template editor.
    pub placeholders: Vec<String>,
}

/// Find every place a pattern matches **one buffer** — the document in front of the
/// user, not the repository.
///
/// ## Why this is a separate verb rather than a scope of the other one
///
/// The repository search is a *migration*: it reads scripts off disk, it is bounded
/// by a scope, its results are grouped and exported, and writing them goes through
/// a preview and a digest check because it rewrites files nobody has open. None of
/// that applies to the tab in front of you, which has no path, may never have been
/// saved, and whose edits belong to the editor's own undo history.
///
/// So this half does exactly one thing: it says where the pattern matches and what
/// each match would become. **It writes nothing.** The frontend splices the ranges
/// into the buffer as an ordinary edit, which is what makes Ctrl+Z work on it and
/// what keeps a structural replace from being the one edit in the editor that
/// cannot be undone.
#[arbor_rpc::handler]
fn picus_structural_scan(
    _state: &PicusState,
    text: String,
    pattern: String,
    replacement: Option<String>,
) -> Result<ScanResult, String> {
    let compiled = compile(&pattern)?;
    Ok(ScanResult {
        matches: hits_in(&compiled, &text, replacement.as_deref())?,
        placeholders: compiled.names().into_iter().map(str::to_string).collect(),
    })
}

/// One file as a transformation would leave it.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RestructuredFile {
    pub path: String,
    pub encoding: String,
    pub eol: LineEnding,
    pub before: String,
    pub after: String,
    pub matches: usize,
    /// Hand back to `picus_structural_apply` unchanged — how the write knows
    /// nothing moved underneath it.
    pub digest: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RestructurePreview {
    pub files: Vec<RestructuredFile>,
    /// Files that matched but could not be prepared, with the reason. Reported
    /// rather than dropped: a migration missing a file is worse than one that says
    /// which file it cannot do.
    pub refused: Vec<Refusal>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Refusal {
    pub path: String,
    pub reason: String,
}

/// Exactly what a transformation would write. Touches nothing.
#[arbor_rpc::handler]
fn picus_structural_preview(
    state: &PicusState,
    root: String,
    pattern: String,
    replacement: String,
    scope: Option<Scope>,
) -> Result<RestructurePreview, String> {
    let snapshot = snapshot_for(state, &root)?;
    let planned = plan(&snapshot, &pattern, &replacement, &scope.unwrap_or_default())?;
    Ok(RestructurePreview {
        files: planned
            .files
            .into_iter()
            .map(|file| RestructuredFile {
                path: file.path,
                encoding: file.prepared.encoding.clone(),
                eol: line_ending_of(file.prepared.eol),
                before: file.prepared.before,
                after: file.prepared.after,
                matches: file.matches,
                digest: file.digest,
            })
            .collect(),
        refused: planned.refused,
    })
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RestructureApplied {
    pub written: Vec<String>,
    pub unchanged: Vec<String>,
}

/// Write the transformation, or refuse because something moved since the preview.
#[arbor_rpc::handler]
fn picus_structural_apply(
    state: &PicusState,
    root: String,
    pattern: String,
    replacement: String,
    scope: Option<Scope>,
    digests: Option<Digests>,
) -> Result<RestructureApplied, String> {
    let approved = digests
        .ok_or_else(|| {
            "a structural rewrite has to carry the digests its preview returned — nothing was \
             written"
                .to_string()
        })?
        .into_map();

    let snapshot = snapshot_for(state, &root)?;
    let planned = plan(&snapshot, &pattern, &replacement, &scope.unwrap_or_default())?;

    let current: Vec<(String, String)> =
        planned.files.iter().map(|f| (f.path.clone(), f.digest.clone())).collect();
    crate::apply::unchanged_since_preview(&current, &approved)?;

    let prepared: Vec<PreparedFile> = planned.files.into_iter().map(|f| f.prepared).collect();
    let applied = commit(&prepared).map_err(|e| e.to_string())?;

    // The files on disk are no longer the files that were read.
    state.scripts().invalidate(&snapshot.root);

    let relative = |paths: Vec<std::path::PathBuf>| -> Vec<String> {
        paths.iter().map(|p| crate::apply::relative_to(&snapshot.root, p)).collect()
    };
    Ok(RestructureApplied {
        written: relative(applied.written),
        unchanged: relative(applied.unchanged),
    })
}

// ── Planning ───────────────────────────────────────────────────────────────────

struct Planned {
    files: Vec<PlannedFile>,
    refused: Vec<Refusal>,
}

struct PlannedFile {
    path: String,
    prepared: PreparedFile,
    matches: usize,
    digest: String,
}

/// Resolve a transformation into the exact bytes it would write.
///
/// One pass, so the preview and the write cannot describe different things: the
/// apply calls this and compares digests, it never re-derives the edits by another
/// route.
fn plan(
    snapshot: &ScriptSnapshot,
    pattern: &str,
    replacement: &str,
    scope: &Scope,
) -> Result<Planned, String> {
    let compiled = compile(pattern)?;
    if replacement.trim().is_empty() {
        return Err("the replacement is empty — a rewrite that deletes every match is not \
                    something Picus does by accident; write the statement it should become"
            .to_string());
    }

    let mut files = vec![];
    let mut refused = vec![];

    for (path, text) in scripts_in(snapshot, scope) {
        let found = compiled.find_all(&language(), text).map_err(|e| e.to_string())?;
        if found.is_empty() {
            continue;
        }

        let mut splices = Vec::with_capacity(found.len());
        let mut failed = None;
        for one in &found {
            match render_with(replacement, one, text, true) {
                Ok(rendered) => splices.push(Splice {
                    range: one.range.start..one.range.end,
                    replacement: rendered,
                    reason: format!("structural rewrite at line {}", line_of(text, one.range.start)),
                }),
                Err(e) => {
                    failed = Some(e.to_string());
                    break;
                }
            }
        }
        if let Some(reason) = failed {
            refused.push(Refusal { path: path.to_string(), reason });
            continue;
        }

        // Read from disk rather than reusing the decoded text: `SourceText` also
        // carries the encoder and the round-trip check, and a rewrite must not be
        // prepared against text whose bytes it cannot reproduce.
        let full = match crate::apply::destination(&snapshot.root, path) {
            Ok(full) => full,
            Err(reason) => {
                refused.push(Refusal { path: path.to_string(), reason });
                continue;
            }
        };
        let (label, eol) = crate::apply::conventions(snapshot, path);
        let encoding = label_to_encoding(&label);
        let context = EncodingContext::new().with_legacy(encoding).with_dominant(encoding);
        let source = match SourceText::read(&full, &context, encoding, eol) {
            Ok(source) => source,
            Err(e) => {
                refused.push(Refusal { path: path.to_string(), reason: e.to_string() });
                continue;
            }
        };
        // The digest of what is on disk **now**, not the one the snapshot was read
        // with: it is the value the write compares against, so it has to describe
        // the bytes the write is about to replace.
        let digest = digest(&source.bytes);
        match prepare_one(&source, &splices) {
            Ok(prepared) => files.push(PlannedFile {
                path: path.to_string(),
                prepared,
                matches: found.len(),
                digest,
            }),
            Err(e) => refused.push(Refusal { path: path.to_string(), reason: e.to_string() }),
        }
    }

    Ok(Planned { files, refused })
}

fn compile(pattern: &str) -> Result<Pattern, String> {
    if pattern.trim().is_empty() {
        return Err("write the statement to look for, with $name$ where it may differ".to_string());
    }
    Pattern::compile(&language(), pattern)
        // SQL keywords fold and these repositories are not consistent about it —
        // `INSERT` and `insert` are one keyword, and a pattern that only matched
        // the casing it was typed in would miss half the repository.
        .map(|p| p.case_insensitive(true))
        .map_err(|e: SyntaxError| e.to_string())
}

fn language() -> tree_sitter::Language {
    picus_parse::prelude::language()
}

/// The scripts a scope admits, in tree order, with the text already decoded.
fn scripts_in<'a>(snapshot: &'a ScriptSnapshot, scope: &Scope) -> Vec<(&'a str, &'a str)> {
    snapshot
        .sources
        .iter()
        .filter(|(path, _)| scope.admits(snapshot, path))
        .map(|(path, source)| (path.as_str(), source.text.as_str()))
        .collect()
}

fn line_of(text: &str, at: usize) -> usize {
    text.get(..at).map(|head| head.lines().count().max(1)).unwrap_or(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn matches_of(pattern: &str, sql: &str) -> Vec<String> {
        let compiled = compile(pattern).expect("compiles");
        compiled
            .find_all(&language(), sql)
            .expect("searches")
            .into_iter()
            .map(|m| m.range.slice(sql).unwrap_or("").to_string())
            .collect()
    }

    #[test]
    fn a_pattern_finds_the_statements_on_one_table_and_leaves_the_others() {
        let sql = "INSERT INTO CATALOGO_WIDGET (CHIAVE, ETICHETTA) VALUES ('A', 'Alfa');\n\
                   INSERT INTO STAGING_IMPORT (CHIAVE) VALUES ('B');\n";
        let found = matches_of("INSERT INTO CATALOGO_WIDGET ($cols...$) VALUES ($vals...$)", sql);
        assert_eq!(found.len(), 1, "{found:?}");
        assert!(found[0].contains("CATALOGO_WIDGET"));
    }

    #[test]
    fn the_keyword_casing_of_the_repository_does_not_have_to_match_the_pattern() {
        // These repositories are not consistent about it, and a pattern that only
        // matched the casing it was typed in would miss half of them.
        let sql = "insert into catalogo_widget (chiave) values ('A');";
        assert_eq!(matches_of("INSERT INTO CATALOGO_WIDGET ($c...$) VALUES ($v...$)", sql).len(), 1);
    }

    #[test]
    fn a_rename_with_a_reorder_renders_the_statement_it_should_become() {
        let sql = "INSERT INTO CATALOGO_WIDGET (CHIAVE, ETICHETTA) VALUES ('A', 'Alfa');";
        let compiled =
            compile("INSERT INTO CATALOGO_WIDGET ($cols...$) VALUES ($vals...$)").expect("compiles");
        let found = compiled.find_all(&language(), sql).expect("searches");
        assert_eq!(found.len(), 1);

        // Portale 1 → portale 2, in one line: new table, columns renamed, values
        // swapped.
        let out = render_with(
            "INSERT INTO CATALOGO_WIDGET_V2 (ETICHETTA_NUOVA, CHIAVE_NUOVA) VALUES ($vals.1$, $vals.0$)",
            &found[0],
            sql,
            true,
        )
        .expect("renders");
        assert_eq!(
            out,
            "INSERT INTO CATALOGO_WIDGET_V2 (ETICHETTA_NUOVA, CHIAVE_NUOVA) VALUES ('Alfa', 'A')"
        );
    }

    #[test]
    fn a_statement_split_over_lines_with_a_comment_in_it_still_matches() {
        // The shape these files are actually in.
        let sql = "INSERT INTO CATALOGO_WIDGET\n  (CHIAVE, ETICHETTA)\n  -- la riga di default\n  \
                   VALUES ('A', 'Alfa');";
        assert_eq!(
            matches_of("INSERT INTO CATALOGO_WIDGET ($cols...$) VALUES ($vals...$)", sql).len(),
            1
        );
    }

    #[test]
    fn a_value_can_be_addressed_by_the_column_it_belongs_to() {
        // The real problem, in its real shape: some statements list the columns in
        // one order and some in another, so a positional `$vals.0$` means a
        // different thing in each — which is the bug, not the fix. Addressed
        // through the column list, one template normalises both.
        let compiled = compile("INSERT INTO LOCALSTRINGS ($cols...$) VALUES ($vals...$)")
            .expect("compiles");
        let template = "INSERT INTO LOCALSTRINGS (CHIAVE, LINGUA, TESTO) VALUES \
                        ($vals[cols=chiave]$, $vals[cols=lingua]$, $vals[cols=testo]$)";

        for sql in [
            "INSERT INTO LOCALSTRINGS (CHIAVE, LINGUA, TESTO) VALUES ('K', 'it', 'Ciao');",
            // The deviation: language before key, and its values follow.
            "INSERT INTO LOCALSTRINGS (LINGUA, CHIAVE, TESTO) VALUES ('it', 'K', 'Ciao');",
            // …and a third order, lower-cased, because these repositories are like
            // that.
            "insert into localstrings (testo, chiave, lingua) values ('Ciao', 'K', 'it');",
        ] {
            let found = compiled.find_all(&language(), sql).expect("searches");
            assert_eq!(found.len(), 1, "{sql}");
            assert_eq!(
                render_with(template, &found[0], sql, true).expect("renders"),
                "INSERT INTO LOCALSTRINGS (CHIAVE, LINGUA, TESTO) VALUES ('K', 'it', 'Ciao')",
                "{sql}"
            );
        }
    }

    #[test]
    fn a_statement_missing_a_column_is_reported_rather_than_shifted() {
        // The failure mode a positional template hides: three columns addressed,
        // two present. Writing 'it' into TESTO would be silent and wrong.
        let compiled = compile("INSERT INTO LOCALSTRINGS ($cols...$) VALUES ($vals...$)")
            .expect("compiles");
        let sql = "INSERT INTO LOCALSTRINGS (CHIAVE, LINGUA) VALUES ('K', 'it');";
        let found = compiled.find_all(&language(), sql).expect("searches");
        let err = render_with("$vals[cols=testo]$", &found[0], sql, true).expect_err("refused");
        assert!(err.to_string().contains("does not hold testo"), "{err}");
    }

    #[test]
    fn an_empty_pattern_says_what_to_type_instead_of_matching_everything() {
        let err = compile("   ").expect_err("refused");
        assert!(err.contains("$name$"), "{err}");
    }

    // ── Scanning one buffer ────────────────────────────────────────────────────

    #[test]
    fn a_buffer_scan_returns_the_ranges_the_editor_splices() {
        // The frontend applies these itself, so the ranges have to address the text
        // it sent — including with an accent above them, since every offset here is
        // in UTF-8 bytes and the editor's are in UTF-16 units.
        let sql = "-- perché no\nINSERT INTO CATALOGO_WIDGET (CHIAVE) VALUES ('A');\n\
                   INSERT INTO CATALOGO_WIDGET (CHIAVE) VALUES ('B');\n";
        let compiled =
            compile("INSERT INTO CATALOGO_WIDGET ($cols...$) VALUES ($vals...$)").expect("compiles");
        // `$cols$` in a *template* is the whole capture; the `...` belongs to the
        // pattern, where it is what makes the capture a list in the first place.
        let hits = hits_in(&compiled, sql, Some("INSERT INTO CATALOGO_V2 ($cols$) VALUES ($vals$)"))
            .expect("scans");

        assert_eq!(hits.len(), 2);
        for hit in &hits {
            assert_eq!(&sql[hit.range.start..hit.range.end], hit.text, "the range must be the text");
            assert!(hit.replacement.as_deref().is_some_and(|r| r.contains("CATALOGO_V2")));
            assert!(hit.problem.is_none());
        }
        assert!(hits[0].line < hits[1].line, "and they come back in document order");
    }

    #[test]
    fn a_buffer_scan_reports_a_template_that_cannot_be_applied_on_the_row_it_fails() {
        let sql = "INSERT INTO LOCALSTRINGS (CHIAVE, LINGUA) VALUES ('K', 'it');";
        let compiled =
            compile("INSERT INTO LOCALSTRINGS ($cols...$) VALUES ($vals...$)").expect("compiles");
        let hits = hits_in(&compiled, sql, Some("$vals[cols=testo]$")).expect("scans");
        assert_eq!(hits.len(), 1);
        assert!(hits[0].replacement.is_none());
        assert!(hits[0].problem.as_deref().is_some_and(|p| p.contains("does not hold testo")));
    }

    #[test]
    fn a_buffer_scan_with_no_template_is_a_search() {
        let sql = "INSERT INTO CATALOGO_WIDGET (CHIAVE) VALUES ('A');";
        let compiled =
            compile("INSERT INTO CATALOGO_WIDGET ($cols...$) VALUES ($vals...$)").expect("compiles");
        let hits = hits_in(&compiled, sql, None).expect("scans");
        assert_eq!(hits.len(), 1);
        assert!(hits[0].replacement.is_none(), "nothing was asked to be rendered");
        assert!(hits[0].problem.is_none(), "and nothing failed");
        assert_eq!(hits[0].captures.get("cols").map(String::as_str), Some("CHIAVE"));
    }
}
