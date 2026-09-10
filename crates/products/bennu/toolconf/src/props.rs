//! Reading a `.properties`-shaped configuration file, and deciding what the caret is on.
//!
//! Neither file this crate serves is quite `java.util.Properties`, and the differences are the
//! whole reason this is not one `split('=')`:
//!
//! * **`lombok.config` has three separators.** `key = value` sets, `key += value` appends to a list
//!   and `key -= value` removes from one, and a reader that splits on `=` reports the key of a
//!   `lombok.copyableAnnotations += com.acme.Ann` line as `lombok.copyableAnnotations +`.
//! * **`lombok.config` has a statement.** `clear lombok.accessors.prefix` is a line with no
//!   separator at all whose second word is a key.
//! * **both accept `:`** as well as `=`, because `Properties` does.
//!
//! Everything here works in **UTF-8 byte offsets**, like the rest of the bennu contract.

/// One `key = value` line, with the spans an editor answer needs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub key: String,
    pub key_start: usize,
    pub key_end: usize,
    pub value: String,
    pub value_start: usize,
    pub value_end: usize,
}

/// What the caret is sitting on.
///
/// `start` and `end` bracket the **whole token**, not the part before the caret: `end` may sit
/// after the caret, because a caret in the middle of a key that is already written is correcting
/// that key rather than starting a new one. Handing that range to the editor is what stops
/// accepting a candidate on `clear lombok.acc|` from replacing the `clear` as well.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Caret {
    /// A key being typed. `partial` is what is there **up to the caret**.
    Key { partial: String, start: usize, end: usize },
    /// A value being typed, under `key`.
    Value { key: String, partial: String, start: usize, end: usize },
}

impl Caret {
    /// The byte range accepting a candidate should replace.
    pub fn range(&self) -> (usize, usize) {
        match self {
            Caret::Key { start, end, .. } | Caret::Value { start, end, .. } => (*start, *end),
        }
    }
}

/// The separators, longest first — `+=` has to be tried before `=` or it is read as a key ending
/// in `+`.
const SEPARATORS: [&str; 4] = ["+=", "-=", "=", ":"];

/// Whether a line carries nothing to look at: blank, or a comment.
///
/// `#` is the comment marker in both files. `!` is `java.util.Properties`' second one, and is
/// honoured because a `junit-platform.properties` really is read by that class.
fn is_ignorable(trimmed: &str) -> bool {
    trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with('!')
}

/// Split a line into its key part and its value part, returning the byte offset **within the line**
/// at which the value begins. `None` when the line has no separator.
fn split_at_separator(line: &str) -> Option<(usize, usize)> {
    SEPARATORS
        .iter()
        .filter_map(|sep| line.find(sep).map(|at| (at, at + sep.len())))
        // The earliest separator wins, and among separators starting at the same place the longest
        // does — which is exactly what `+=` needs against `=`.
        .min_by(|a, b| a.0.cmp(&b.0).then(b.1.cmp(&a.1)))
}

/// The key a `clear <key>` statement names, as an offset-into-line and the word.
///
/// Lombok's own reader is case-sensitive about the verb, so this is too: `Clear x` is not a
/// statement, and pretending it is would offer completion where lombok will complain.
fn clear_statement(line: &str) -> Option<(usize, &str)> {
    let rest = line.strip_prefix("clear ")?;
    let lead = rest.len() - rest.trim_start().len();
    Some(("clear ".len() + lead, rest.trim()))
}

/// Every key/value line of the file, comments and blanks skipped.
pub fn entries(source: &str) -> Vec<Entry> {
    let mut out = Vec::new();
    let mut line_start = 0usize;
    for line in source.split_inclusive('\n') {
        let bare = line.trim_end_matches(['\n', '\r']);
        let lead = bare.len() - bare.trim_start().len();
        let trimmed = bare.trim();
        if is_ignorable(trimmed) {
            line_start += line.len();
            continue;
        }
        if let Some((at, key)) = clear_statement(&bare[lead..]) {
            let key_start = line_start + lead + at;
            out.push(Entry {
                key: key.to_string(),
                key_start,
                key_end: key_start + key.len(),
                value: String::new(),
                value_start: key_start + key.len(),
                value_end: key_start + key.len(),
                });
            line_start += line.len();
            continue;
        }
        if let Some((sep_at, value_at)) = split_at_separator(bare) {
            let key_text = bare[..sep_at].trim();
            let key_start = line_start + lead;
            let value_raw = &bare[value_at..];
            let value_lead = value_raw.len() - value_raw.trim_start().len();
            let value_text = value_raw.trim();
            let value_start = line_start + value_at + value_lead;
            out.push(Entry {
                key: key_text.to_string(),
                key_start,
                key_end: key_start + key_text.len(),
                value: value_text.to_string(),
                value_start,
                value_end: value_start + value_text.len(),
            });
        }
        line_start += line.len();
    }
    out
}

/// The entry whose **key** covers `offset`, for a hover.
///
/// Inclusive of the key's end so that a caret sitting just after the last character still answers —
/// that is where a caret is after typing the key, and a hover that went silent there would look
/// broken rather than deliberate.
pub fn key_at(source: &str, offset: usize) -> Option<Entry> {
    entries(source).into_iter().find(|e| offset >= e.key_start && offset <= e.key_end)
}

/// The entry whose **value** covers `offset`.
pub fn value_at(source: &str, offset: usize) -> Option<Entry> {
    entries(source)
        .into_iter()
        .find(|e| !e.value.is_empty() && offset >= e.value_start && offset <= e.value_end)
}

/// What the caret is typing, or `None` on a line where nothing completes (a comment, a blank).
///
/// The one thing this decides that the frontend also decides is *where the token starts*, and the
/// rule is kept deliberately shallow on both sides — before the separator you are typing a key,
/// after it a value — so the two cannot drift in any way that matters.
pub fn classify(source: &str, offset: usize) -> Option<Caret> {
    let offset = offset.min(source.len());
    let line_start = source[..offset].rfind('\n').map_or(0, |at| at + 1);
    let before = &source[line_start..offset];
    let trimmed_lead = before.len() - before.trim_start().len();
    if is_ignorable(before.trim_start()) && !before.trim().is_empty() {
        return None;
    }

    // Whatever is still ahead of the caret on this line — the half of the token `before` cannot see.
    let ahead = source[offset..].split('\n').next().unwrap_or_default();

    if let Some((at, _)) = clear_statement(&before[trimmed_lead..]) {
        let start = line_start + trimmed_lead + at;
        let end = offset + ahead.trim_end().len();
        return Some(Caret::Key { partial: source[start..offset].to_string(), start, end });
    }

    match split_at_separator(before) {
        None => {
            let start = line_start + trimmed_lead;
            // A key token ends at the separator when the line already has one ahead of the caret,
            // and at the end of the line when it does not.
            let end = match split_at_separator(ahead) {
                Some((sep_at, _)) => offset + ahead[..sep_at].trim_end().len(),
                None => offset + ahead.trim_end().len(),
            };
            Some(Caret::Key { partial: before.trim_start().to_string(), start, end })
        }
        Some((sep_at, value_at)) => {
            let key = before[..sep_at].trim().to_string();
            let after = &before[value_at..];
            let lead = after.len() - after.trim_start().len();
            Some(Caret::Value {
                key,
                partial: after.trim_start().to_string(),
                start: line_start + value_at + lead,
                end: offset + ahead.trim_end().len(),
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_plain_assignment_reads_as_key_and_value() {
        let e = &entries("lombok.log.fieldName = LOG\n")[0];
        assert_eq!(e.key, "lombok.log.fieldName");
        assert_eq!(e.value, "LOG");
        assert_eq!(&"lombok.log.fieldName = LOG\n"[e.key_start..e.key_end], "lombok.log.fieldName");
        assert_eq!(&"lombok.log.fieldName = LOG\n"[e.value_start..e.value_end], "LOG");
    }

    #[test]
    fn a_list_append_is_not_a_key_ending_in_plus() {
        // The bug this test exists for: `find('=')` lands after the `+`, so the key came out as
        // `lombok.copyableAnnotations +` and matched nothing in the catalogue — which reads to a
        // user as "the key I copied from the documentation is unknown".
        let e = &entries("lombok.copyableAnnotations += com.acme.Ann\n")[0];
        assert_eq!(e.key, "lombok.copyableAnnotations");
        assert_eq!(e.value, "com.acme.Ann");
    }

    #[test]
    fn a_list_removal_reads_the_same_way() {
        let e = &entries("lombok.accessors.prefix -= m_\n")[0];
        assert_eq!(e.key, "lombok.accessors.prefix");
        assert_eq!(e.value, "m_");
    }

    #[test]
    fn a_colon_separates_too_because_properties_says_so() {
        let e = &entries("junit.jupiter.execution.parallel.enabled: true\n")[0];
        assert_eq!(e.key, "junit.jupiter.execution.parallel.enabled");
        assert_eq!(e.value, "true");
    }

    #[test]
    fn a_clear_statement_names_a_key_with_no_value() {
        let src = "clear lombok.accessors.prefix\n";
        let e = &entries(src)[0];
        assert_eq!(e.key, "lombok.accessors.prefix");
        assert_eq!(&src[e.key_start..e.key_end], "lombok.accessors.prefix");
        assert!(e.value.is_empty());
    }

    #[test]
    fn comments_and_blanks_carry_nothing() {
        assert!(entries("# lombok.x = 1\n\n! also a comment\n").is_empty());
    }

    #[test]
    fn offsets_survive_the_lines_above_them() {
        let src = "# a comment\n\nconfig.stopBubbling = true\n";
        let e = &entries(src)[0];
        assert_eq!(&src[e.key_start..e.key_end], "config.stopBubbling");
        assert_eq!(&src[e.value_start..e.value_end], "true");
    }

    #[test]
    fn the_caret_before_the_separator_is_typing_a_key() {
        let src = "lombok.acc";
        let Some(Caret::Key { partial, start, end }) = classify(src, src.len()) else {
            panic!("expected a key")
        };
        assert_eq!(partial, "lombok.acc");
        assert_eq!((start, end), (0, src.len()));
    }

    #[test]
    fn the_caret_after_the_separator_is_typing_a_value() {
        let src = "lombok.accessors.chain = tr";
        let Some(Caret::Value { key, partial, start, .. }) = classify(src, src.len()) else {
            panic!("expected a value")
        };
        assert_eq!(key, "lombok.accessors.chain");
        assert_eq!(partial, "tr");
        assert_eq!(&src[start..], "tr");
    }

    #[test]
    fn a_caret_inside_a_written_key_replaces_the_whole_key() {
        // Correcting a key in place: the token runs past the caret, and accepting a candidate has
        // to take the tail with it or the line ends up `lombok.accessors.chainessors.chain`.
        let src = "lombok.acc = true\n";
        let caret = classify(src, "lombok.acc".len() - 3).unwrap();
        let (start, end) = caret.range();
        assert_eq!(&src[start..end], "lombok.acc");
    }

    #[test]
    fn a_clear_statement_replaces_only_the_key_after_it() {
        // The one that would corrupt the line: `from` at the start of `clear` would swallow the
        // verb, leaving a bare key where a statement was.
        let src = "clear lombok.acc";
        let (start, end) = classify(src, src.len()).unwrap().range();
        assert_eq!(&src[start..end], "lombok.acc");
    }

    #[test]
    fn a_value_caret_replaces_the_value_that_is_already_there() {
        let src = "lombok.accessors.chain = fal\n";
        let at = src.find("fal").unwrap() + 1;
        let (start, end) = classify(src, at).unwrap().range();
        assert_eq!(&src[start..end], "fal");
    }

    #[test]
    fn nothing_completes_inside_a_comment() {
        let src = "# lombok.acc";
        assert!(classify(src, src.len()).is_none());
    }

    #[test]
    fn a_clear_statement_completes_keys() {
        let src = "clear lombok.acc";
        let Some(Caret::Key { partial, .. }) = classify(src, src.len()) else {
            panic!("expected a key")
        };
        assert_eq!(partial, "lombok.acc");
    }

    #[test]
    fn a_key_hover_answers_at_the_end_of_the_key_too() {
        let src = "config.stopBubbling = true\n";
        let at = src.find(" =").unwrap();
        assert_eq!(key_at(src, at).unwrap().key, "config.stopBubbling");
        assert_eq!(key_at(src, 0).unwrap().key, "config.stopBubbling");
        // Past the separator is the value, not the key.
        assert!(key_at(src, at + 4).is_none());
    }

    #[test]
    fn an_offset_past_the_end_does_not_panic() {
        // The editor's buffer runs a keystroke ahead of the backend's copy, so this is the normal
        // state rather than a corner: a panic in a query handler poisons the model's lock and every
        // framework answer for the project goes empty from then on.
        assert!(classify("a = b", 9_999).is_some());
    }
}
