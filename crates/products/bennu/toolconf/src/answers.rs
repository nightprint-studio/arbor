//! The four editor answers, driven by a [`Catalogue`] and the project's resolved version.
//!
//! Everything here is written once and shared by both tools: what separates a `lombok.config` from
//! a `junit-platform.properties` is which table is passed in, and that is the whole claim this
//! crate makes.

use bennu_complete::prelude::{unique_continuation, Proposal, Proposals};
use bennu_ext::prelude::ExtHover;
use bennu_proto::prelude::{severity, CompletionItem, Diagnostic};

use crate::model::{Catalogue, ConfigKey};
use crate::props::{self, Caret};

/// Diagnostic code for a key the project's own version of the tool does not have yet.
pub const CODE_TOO_NEW: &str = "toolconf-key-too-new";
/// Diagnostic code for a key that still works and should not be written in new files.
pub const CODE_DEPRECATED: &str = "toolconf-key-deprecated";
/// Diagnostic code for a value outside a **closed** set.
pub const CODE_BAD_VALUE: &str = "toolconf-value-not-allowed";

// ── Completion ───────────────────────────────────────────────────────────────

/// Candidates at the caret.
pub fn completions(
    catalogue: &Catalogue,
    version: Option<&str>,
    source: &str,
    offset: usize,
) -> Vec<CompletionItem> {
    let mut out = Proposals::default();
    let caret = props::classify(source, offset);
    match &caret {
        Some(Caret::Key { partial, .. }) => {
            for key in catalogue.known(version) {
                if !key.key.starts_with(partial.as_str()) {
                    continue;
                }
                out.offer(Proposal::new(key.key, "property").detail(detail_of(key)));
            }
        }
        Some(Caret::Value { key, partial, .. }) => {
            // Only where the set is genuinely closed. A duration, a class name or a field name has
            // no candidates, and offering the current default as the only entry would dress a
            // guess up as a choice.
            let Some(meta) = catalogue.lookup(key) else { return Vec::new() };
            for value in meta.values {
                if !value.starts_with(partial.as_str()) {
                    continue;
                }
                out.offer(Proposal::new(*value, "value"));
            }
        }
        None => {}
    }
    // Every candidate carries the range it replaces, rather than leaving the editor to work out
    // where the token began from the buffer. The token can start where a generic rule would not
    // look (`clear lombok.acc` — the key begins after the verb) and can end *after* the caret
    // (correcting a key that is already written), and both of those corrupt the line when guessed.
    let Some((start, end)) = caret.map(|c| c.range()) else { return Vec::new() };
    out.into_items()
        .into_iter()
        .map(|item| CompletionItem {
            replace_start: Some(start),
            replace_end: Some(end),
            ..item
        })
        .collect()
}

/// The right-aligned line in the completion list: the type, then whichever of the default, the
/// deprecation and the version requirement applies.
fn detail_of(key: &ConfigKey) -> String {
    if !key.deprecated_since.is_empty() {
        return format!("{}  · deprecated", key.type_text);
    }
    if !key.default_value.is_empty() {
        return format!("{}  = {}", key.type_text, key.default_value);
    }
    key.type_text.to_string()
}

// ── Hover ────────────────────────────────────────────────────────────────────

/// The card for the key (or the value) under the caret.
///
/// Version-blind about *which* key it will explain, deliberately: the one moment a reader most
/// needs to be told that a key arrived in 1.18.22 is when it is sitting in front of them doing
/// nothing, and a hover gated the way completion is would go silent exactly there.
pub fn hover(
    catalogue: &Catalogue,
    version: Option<&str>,
    source: &str,
    offset: usize,
) -> Option<ExtHover> {
    let entry = props::key_at(source, offset).or_else(|| props::value_at(source, offset))?;
    let key = catalogue.lookup(&entry.key)?;

    let mut signature = key.type_text.to_string();
    if !key.default_value.is_empty() {
        signature.push_str("  ·  default ");
        signature.push_str(key.default_value);
    }

    let mut doc = key.doc.to_string();
    if !key.values.is_empty() {
        doc.push_str("\n\nAccepts: ");
        doc.push_str(&key.values.join(", "));
    }
    match () {
        _ if !key.deprecated_since.is_empty() => {
            doc.push_str(&format!(
                "\n\nDeprecated since {} {}",
                catalogue.display_name, key.deprecated_since
            ));
            if !key.replacement.is_empty() {
                doc.push_str(&format!(" — use `{}` instead.", key.replacement));
            }
        }
        _ if !key.since.is_empty() => {
            doc.push_str(&format!(
                "\n\nSince {} {}.",
                catalogue.display_name, key.since
            ));
            if let Some(v) = version {
                if !key.known_in(Some(v)) {
                    doc.push_str(&format!(
                        " This project resolves {} {v}, which ignores it.",
                        catalogue.display_name
                    ));
                }
            }
        }
        _ => doc.push_str(&format!(
            "\n\nAvailable since {} {}, which is as far back as `{}` goes.",
            catalogue.display_name, catalogue.min_version, catalogue.file_name
        )),
    }

    Some(ExtHover { title: key.key.to_string(), signature, doc })
}

// ── Ghost text ───────────────────────────────────────────────────────────────

/// The continuation that **certainly** follows the caret, or `None`.
///
/// Two cases qualify, and they are the same two the Spring property files use, for the same
/// reason: this is drawn ahead of the caret as if it were already typed, so anything less than
/// certain belongs in the popup where the alternatives are visible.
///
/// 1. an empty value whose key has a documented default — the tool will use that value anyway, so
///    writing it changes nothing and makes it visible;
/// 2. a key prefix exactly one known key can continue.
pub fn inline_hint(
    catalogue: &Catalogue,
    version: Option<&str>,
    source: &str,
    offset: usize,
) -> Option<String> {
    // Ghost text is committed *at* the caret, so text still ahead of it on the line would be
    // doubled: `server.po|rt` would become `server.portrt`.
    let offset = offset.min(source.len());
    let rest_of_line = source[offset..].split('\n').next().unwrap_or_default();
    if !rest_of_line.trim().is_empty() {
        return None;
    }
    match props::classify(source, offset)? {
        Caret::Key { partial, .. } => {
            unique_continuation(&partial, catalogue.known(version).map(|k| k.key))
        }
        Caret::Value { key, partial, .. } if partial.is_empty() => {
            let meta = catalogue.lookup(&key)?;
            (!meta.default_value.is_empty()).then(|| meta.default_value.to_string())
        }
        Caret::Value { .. } => None,
    }
}

// ── Diagnostics ──────────────────────────────────────────────────────────────

/// What is wrong in the file — held to the standard the rest of bennu is: **under-report rather
/// than risk a false positive**.
///
/// Note what is deliberately *not* here: an "unknown key" warning. These tables are the keys worth
/// documenting, not a transcription of every constant in `ConfigurationKeys`, so a key missing from
/// a table means "not written down here" and never "not understood by the tool". Reporting on that
/// difference would put a warning under a line that is perfectly correct — which is exactly the
/// finding a user learns to ignore, taking the true ones with it.
///
/// The three below are each true by construction: the version gate compares against a version the
/// project itself resolves, the deprecation is the tool's own, and a closed value set is closed.
pub fn diagnostics(catalogue: &Catalogue, version: Option<&str>, source: &str) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    for entry in props::entries(source) {
        let Some(key) = catalogue.lookup(&entry.key) else { continue };

        if let Some(v) = version {
            if !key.known_in(Some(v)) {
                out.push(Diagnostic {
                    message: format!(
                        "`{}` arrived in {} {} — this project resolves {v}, which reads the line \
                         and ignores it.",
                        key.key, catalogue.display_name, key.since
                    ),
                    severity: severity::WARNING.to_string(),
                    code: CODE_TOO_NEW.to_string(),
                    start: entry.key_start,
                    end: entry.key_end,
                });
                // One finding per line: the value of a key the tool ignores is not worth a second.
                continue;
            }
        }

        if !key.deprecated_since.is_empty() {
            let replacement = if key.replacement.is_empty() {
                String::new()
            } else {
                format!(" Use `{}` instead.", key.replacement)
            };
            out.push(Diagnostic {
                message: format!(
                    "`{}` is deprecated since {} {}.{replacement}",
                    key.key, catalogue.display_name, key.deprecated_since
                ),
                severity: severity::WEAK.to_string(),
                code: CODE_DEPRECATED.to_string(),
                start: entry.key_start,
                end: entry.key_end,
            });
        }

        // A closed set, a written value, and no `${…}` in it — a placeholder is somebody else's
        // substitution and this crate has no business ruling on what it will expand to.
        if key.values.is_empty() || entry.value.is_empty() || entry.value.contains("${") {
            continue;
        }
        if key.values.iter().any(|v| v.eq_ignore_ascii_case(&entry.value)) {
            continue;
        }
        out.push(Diagnostic {
            message: format!(
                "`{}` accepts {}. `{}` is not one of them.",
                key.key,
                key.values.join(", "),
                entry.value
            ),
            severity: severity::WARNING.to_string(),
            code: CODE_BAD_VALUE.to_string(),
            start: entry.value_start,
            end: entry.value_end,
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::junit::CATALOGUE as JUNIT;

    fn lombok() -> &'static Catalogue {
        crate::lombok::catalogue()
    }

    #[test]
    fn a_key_prefix_completes_from_the_catalogue() {
        let src = "lombok.accessors.";
        let items = completions(lombok(), None, src, src.len());
        assert!(items.iter().any(|i| i.label == "lombok.accessors.chain"));
        assert!(items.iter().any(|i| i.label == "lombok.accessors.fluent"));
        assert!(items.iter().all(|i| i.kind == "property"));
    }

    #[test]
    fn the_version_gate_reaches_the_completion_list() {
        let src = "junit.jupiter.execution.parallel.";
        let old = completions(&JUNIT, Some("5.2"), src, src.len());
        assert!(old.is_empty());
        let new = completions(&JUNIT, Some("5.9"), src, src.len());
        assert!(new.iter().any(|i| i.label.ends_with("parallel.enabled")));
    }

    #[test]
    fn a_closed_value_set_completes_and_an_open_one_does_not() {
        let closed = "lombok.accessors.chain = ";
        let items = completions(lombok(), None, closed, closed.len());
        assert_eq!(
            items.iter().map(|i| i.label.as_str()).collect::<Vec<_>>(),
            vec!["true", "false"]
        );
        let open = "lombok.log.fieldName = ";
        assert!(completions(lombok(), None, open, open.len()).is_empty());
    }

    #[test]
    fn the_hover_explains_a_key_the_project_is_too_old_for() {
        let src = "lombok.addNullAnnotations = javax\n";
        let card = hover(lombok(), Some("1.16.20"), src, 3).unwrap();
        assert_eq!(card.title, "lombok.addNullAnnotations");
        assert!(card.doc.contains("1.18.22"));
        assert!(card.doc.contains("ignores it"), "{}", card.doc);
    }

    #[test]
    fn the_hover_answers_on_the_value_too() {
        let src = "lombok.accessors.chain = true\n";
        let at = src.find("true").unwrap() + 1;
        assert_eq!(hover(lombok(), None, src, at).unwrap().title, "lombok.accessors.chain");
    }

    #[test]
    fn ghost_text_completes_a_unique_key_prefix() {
        let src = "lombok.singular.useGu";
        assert_eq!(inline_hint(lombok(), None, src, src.len()).as_deref(), Some("ava"));
    }

    #[test]
    fn ghost_text_offers_a_documented_default_for_an_empty_value() {
        let src = "lombok.log.fieldName = ";
        assert_eq!(inline_hint(lombok(), None, src, src.len()).as_deref(), Some("log"));
    }

    #[test]
    fn ghost_text_stays_quiet_with_text_ahead_of_the_caret() {
        let src = "lombok.singular.useGuava = true";
        let at = src.find(" = ").unwrap();
        assert!(inline_hint(lombok(), None, src, at).is_none());
    }

    #[test]
    fn a_key_the_project_version_ignores_is_reported_where_it_is_written() {
        let src = "lombok.addNullAnnotations = javax\n";
        let d = &diagnostics(lombok(), Some("1.16.20"), src)[0];
        assert_eq!(d.code, CODE_TOO_NEW);
        assert_eq!(&src[d.start..d.end], "lombok.addNullAnnotations");
        assert!(diagnostics(lombok(), Some("1.18.30"), src).is_empty());
    }

    #[test]
    fn an_unresolved_version_reports_nothing_about_versions() {
        let src = "lombok.addNullAnnotations = javax\n";
        assert!(diagnostics(lombok(), None, src).is_empty());
    }

    #[test]
    fn a_value_outside_a_closed_set_is_reported_on_the_value() {
        let src = "lombok.equalsAndHashCode.callSuper = MAYBE\n";
        let d = &diagnostics(lombok(), None, src)[0];
        assert_eq!(d.code, CODE_BAD_VALUE);
        assert_eq!(&src[d.start..d.end], "MAYBE");
    }

    #[test]
    fn a_placeholder_value_is_left_alone() {
        // Somebody else's substitution: what `${…}` expands to is not this crate's to rule on.
        let src = "lombok.accessors.chain = ${chain}\n";
        assert!(diagnostics(lombok(), None, src).is_empty());
    }

    #[test]
    fn an_undocumented_key_is_never_reported() {
        // The tables are what is worth documenting, not a transcription of every constant the tool
        // defines — so a key that is not in one means "not written down here".
        let src = "lombok.something.nobody.wrote.down = 1\n";
        assert!(diagnostics(lombok(), Some("1.18.30"), src).is_empty());
    }

    #[test]
    fn a_deprecated_key_is_flagged_weakly_and_names_its_replacement() {
        let src = "lombok.addGeneratedAnnotation = true\n";
        let d = &diagnostics(lombok(), Some("1.18.30"), src)[0];
        assert_eq!(d.code, CODE_DEPRECATED);
        assert_eq!(d.severity, severity::WEAK);
        assert!(d.message.contains("lombok.addLombokGeneratedAnnotation"));
    }

    #[test]
    fn nothing_is_answered_inside_a_comment() {
        let src = "# lombok.accessors.";
        assert!(completions(lombok(), None, src, src.len()).is_empty());
        assert!(inline_hint(lombok(), None, src, src.len()).is_none());
    }
}
