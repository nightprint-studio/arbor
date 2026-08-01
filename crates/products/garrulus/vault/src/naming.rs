//! From a naming pattern to a file name that is legal on every platform the
//! vault will ever be opened on.
//!
//! The vault syncs between machines, so a file name that is fine on Linux and
//! impossible on Windows is not a cosmetic problem — it is a note the other PC
//! cannot check out. The rules applied here are therefore the **intersection** of
//! what the three platforms allow, always, even when the machine writing the note
//! would have accepted more.
//!
//! What that costs is small and worth naming: `?`, `:` and `*` become `-` in a
//! title that used them. What it buys is that every note created by Garrulus can
//! be pulled anywhere.

use crate::template::{expand, TemplateCtx};

/// The extension a note has. There is one.
pub const NOTE_EXTENSION: &str = "md";

/// Characters no file name may contain on Windows, plus the separators, which no
/// file *name* may contain anywhere.
const FORBIDDEN: [char; 9] = ['<', '>', ':', '"', '/', '\\', '|', '?', '*'];

/// A generous cap on the stem, in characters.
///
/// Not a platform limit (Windows's is 255 for the whole path, which the folder
/// eats into) — a limit on how much of a very long first line ends up as a file
/// name. A title longer than this is a note whose title should be in the note.
const MAX_STEM_CHARS: usize = 120;

/// Device names MS-DOS reserved and Windows still refuses, in any casing and with
/// any extension. `CON.md` cannot be created.
const RESERVED_STEMS: [&str; 22] = [
    "con", "prn", "aux", "nul", "com1", "com2", "com3", "com4", "com5", "com6", "com7", "com8",
    "com9", "lpt1", "lpt2", "lpt3", "lpt4", "lpt5", "lpt6", "lpt7", "lpt8", "lpt9",
];

/// Expand a naming pattern into a note's file name, extension included.
///
/// The pattern is the type's `naming` line — `{{date}}-{{slug}}`, `{{title}}`,
/// `{{date}}` for a daily note. Expansion is [`crate::template::expand`], so
/// every placeholder available in a body template is available here too, and an
/// unknown one survives as literal text rather than producing a mystery file.
///
/// A pattern that expands to nothing at all yields `senza-titolo.md` rather than
/// a dotfile — the failure mode of `"".md` is a note that never appears in the
/// tree.
pub fn file_name(pattern: &str, ctx: &TemplateCtx) -> String {
    let expanded = expand(pattern, ctx);
    let stem = sanitize_file_name(&expanded);
    let stem = if stem.is_empty() { "senza-titolo".to_string() } else { stem };
    format!("{stem}.{NOTE_EXTENSION}")
}

/// Reduce any string to something usable as a file name stem on every platform.
///
/// Forbidden characters and control characters become `-`, runs collapse, and
/// leading/trailing dots and spaces are trimmed (Windows silently drops trailing
/// ones, which turns "same name" into a rename that appears to do nothing).
pub fn sanitize_file_name(source: &str) -> String {
    // 1. Every illegal character becomes the separator.
    let mapped: String = source
        .chars()
        .map(|c| if FORBIDDEN.contains(&c) || c.is_control() { '-' } else { c })
        .collect();

    // 2. Every run of separators collapses into one. A run that contained a
    //    hyphen stays a hyphen; a run of pure whitespace stays a space — which is
    //    what keeps `Crash all'avvio` readable while `a / b` becomes `a-b`.
    //    A run at either end is dropped: it never gets flushed.
    let mut out = String::with_capacity(mapped.len());
    let mut in_run = false;
    let mut run_had_hyphen = false;
    for c in mapped.chars() {
        if c == '-' || c.is_whitespace() {
            in_run = true;
            run_had_hyphen |= c == '-';
            continue;
        }
        if in_run {
            if !out.is_empty() {
                out.push(if run_had_hyphen { '-' } else { ' ' });
            }
            in_run = false;
            run_had_hyphen = false;
        }
        out.push(c);
    }

    // 3. Trim what Windows silently drops, then cap the length by characters —
    //    truncating by bytes would split a `à` in half.
    let trimmed = out.trim_matches(|c: char| c == '.' || c == '-' || c.is_whitespace());
    let mut stem: String = trimmed.chars().take(MAX_STEM_CHARS).collect();
    while stem.ends_with([' ', '.', '-']) {
        stem.pop();
    }

    if RESERVED_STEMS.contains(&stem.to_lowercase().as_str()) {
        // A trailing underscore is the least surprising escape: the name still
        // reads as what the user typed.
        stem.push('_');
    }
    stem
}

/// Ensure a name ends in `.md`, adding the extension when it is missing.
///
/// Case-insensitive, because a note named `Note.MD` on Windows is the same file.
pub fn ensure_note_extension(name: &str) -> String {
    let already = name
        .rsplit_once('.')
        .is_some_and(|(stem, ext)| !stem.is_empty() && ext.eq_ignore_ascii_case(NOTE_EXTENSION));
    if already {
        name.to_string()
    } else {
        format!("{name}.{NOTE_EXTENSION}")
    }
}

/// A file name that is not taken, by appending ` 2`, ` 3`, … to the stem.
///
/// The caller supplies the "is it taken" test, so this stays pure and works
/// equally against a directory listing, an index, or a pending batch of creates
/// that has not been written yet.
///
/// The suffix goes on the **stem**, never after the extension: `Crash 2.md`, not
/// `Crash.md 2`.
pub fn unique_name(name: &str, taken: impl Fn(&str) -> bool) -> String {
    if !taken(name) {
        return name.to_string();
    }
    let (stem, extension) = match name.rsplit_once('.') {
        Some((stem, ext)) if !stem.is_empty() => (stem, format!(".{ext}")),
        _ => (name, String::new()),
    };
    // Two is where a human starts counting copies; the loop is bounded by the
    // caller running out of files long before it runs out of integers.
    for n in 2u32.. {
        let candidate = format!("{stem} {n}{extension}");
        if !taken(&candidate) {
            return candidate;
        }
    }
    unreachable!("the range is unbounded")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx() -> TemplateCtx {
        TemplateCtx::new("Crash all'avvio: profilo vuoto è già rotto?", "2026-07-31", "14:22")
    }

    #[test]
    fn a_pattern_expands_and_the_slug_folds_its_accents() {
        assert_eq!(
            file_name("{{date}}-{{slug}}", &ctx()),
            "2026-07-31-crash-all-avvio-profilo-vuoto-e-gia-rotto.md"
        );
    }

    #[test]
    fn a_title_pattern_keeps_the_words_but_loses_the_illegal_characters() {
        // `:` and `?` are legal in a title and impossible in a Windows file name.
        assert_eq!(file_name("{{title}}", &ctx()), "Crash all'avvio-profilo vuoto è già rotto.md");
    }

    #[test]
    fn a_daily_note_is_named_after_its_day() {
        assert_eq!(file_name("{{date}}", &ctx()), "2026-07-31.md");
    }

    #[test]
    fn a_pattern_that_expands_to_nothing_still_produces_a_visible_note() {
        let empty = TemplateCtx::new("", "", "");
        assert_eq!(file_name("{{slug}}", &empty), "senza-titolo.md");
    }

    #[test]
    fn separators_collapse_and_edges_are_trimmed() {
        assert_eq!(sanitize_file_name("  a / b : c  "), "a-b-c");
        assert_eq!(sanitize_file_name("a - - b"), "a-b");
        assert_eq!(sanitize_file_name("Crash  all'avvio"), "Crash all'avvio");
        assert_eq!(sanitize_file_name("...note..."), "note");
        assert_eq!(sanitize_file_name("///"), "");
    }

    #[test]
    fn a_reserved_windows_device_name_is_escaped() {
        assert_eq!(file_name("{{title}}", &TemplateCtx::new("CON", "", "")), "CON_.md");
        assert_eq!(file_name("{{title}}", &TemplateCtx::new("com4", "", "")), "com4_.md");
        assert_eq!(file_name("{{title}}", &TemplateCtx::new("console", "", "")), "console.md");
    }

    #[test]
    fn a_very_long_title_is_cut_at_a_character_not_at_a_byte() {
        let long = "à".repeat(200);
        let name = file_name("{{title}}", &TemplateCtx::new(long, "", ""));
        assert_eq!(name.chars().count(), MAX_STEM_CHARS + ".md".len());
    }

    #[test]
    fn the_extension_is_added_once_and_case_insensitively() {
        assert_eq!(ensure_note_extension("Crash"), "Crash.md");
        assert_eq!(ensure_note_extension("Crash.md"), "Crash.md");
        assert_eq!(ensure_note_extension("Crash.MD"), "Crash.MD");
        assert_eq!(ensure_note_extension("Crash.txt"), "Crash.txt.md");
    }

    #[test]
    fn a_taken_name_grows_a_counter_on_its_stem() {
        let taken = ["Crash.md".to_string(), "Crash 2.md".to_string()];
        let is_taken = |candidate: &str| taken.iter().any(|t| t == candidate);
        assert_eq!(unique_name("Crash.md", is_taken), "Crash 3.md");
        assert_eq!(unique_name("Altro.md", is_taken), "Altro.md");
    }
}
