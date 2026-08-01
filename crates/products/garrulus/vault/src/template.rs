//! `{{placeholder}}` expansion — for a note body, for a filename, and for the
//! frontmatter a type fills in.
//!
//! One expander, three consumers, so a placeholder added here works in all three
//! at once. The grammar is deliberately tiny: `{{name}}` and nothing else. No
//! conditionals, no loops, no filters — a template language grows a parser, a
//! parser grows errors, and a note template does not need either.
//!
//! **An unknown placeholder is left exactly as written.** A typo degrades to
//! literal text in the new note, where the user sees it and fixes it, rather than
//! failing the creation of the note or — worse — silently expanding to nothing.
//! This is the same rule Picus's marker template follows and for the same reason.
//!
//! ## The clock is an argument
//!
//! [`TemplateCtx`] carries `date` and `time` as strings the caller supplied.
//! Nothing in this crate reads a clock on its own, which is what lets every test
//! below assert an exact string. [`civil_from_unix`] is offered for the caller
//! that has a timestamp and wants the conventional formatting, and it is UTC:
//! a local-time offset needs either a dependency or the shell to pass one in.

use std::collections::BTreeMap;

use crate::note_type::NoteType;

/// The marker for where the caret goes in a freshly created note.
///
/// It survives [`expand`] untouched — the caller strips it and keeps the offset,
/// via [`render_template_with_cursor`] — because a body template wants the caret
/// after "1." under *Passi per riprodurre*, not at the top of the file.
pub const CURSOR: &str = "{{cursor}}";

/// What a template is expanded against.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TemplateCtx {
    /// The note's title as the user typed it.
    pub title: String,
    /// `yyyy-MM-dd`, supplied by the caller.
    pub date: String,
    /// `HH:mm`, supplied by the caller.
    pub time: String,
    /// The title reduced to a filename-safe form. Derived from `title` by
    /// [`TemplateCtx::new`], and overridable for the caller that wants its own.
    pub slug: String,
    /// Anything else the call site knows — the type id, the device name, a field
    /// value prefilled from the last note of this type.
    pub extra: BTreeMap<String, String>,
}

impl TemplateCtx {
    /// A context for a note called `title`, created on `date` at `time`.
    ///
    /// `slug` is derived; override it afterwards if the call site has a better
    /// one.
    pub fn new(title: impl Into<String>, date: impl Into<String>, time: impl Into<String>) -> Self {
        let title = title.into();
        let slug = slugify(&title);
        TemplateCtx { title, date: date.into(), time: time.into(), slug, extra: BTreeMap::new() }
    }

    /// Add a placeholder the call site knows about. Chainable.
    pub fn with(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.extra.insert(key.into(), value.into());
        self
    }

    /// What `{{key}}` expands to, or `None` when nothing here answers to it.
    pub fn lookup(&self, key: &str) -> Option<&str> {
        match key {
            "title" => Some(&self.title),
            "date" => Some(&self.date),
            "time" => Some(&self.time),
            "slug" => Some(&self.slug),
            // The caret marker is resolved by the caller, not here.
            "cursor" => None,
            other => self.extra.get(other).map(String::as_str),
        }
    }
}

/// Replace every `{{name}}` this context answers to; leave the rest verbatim.
pub fn expand(source: &str, ctx: &TemplateCtx) -> String {
    let mut out = String::with_capacity(source.len());
    let mut rest = source;
    while let Some(start) = rest.find("{{") {
        out.push_str(&rest[..start]);
        let after = &rest[start + 2..];
        let Some(end) = after.find("}}") else {
            // An unterminated `{{` is literal text, not a failure.
            out.push_str(&rest[start..]);
            return out;
        };
        let key = after[..end].trim();
        match ctx.lookup(key) {
            Some(value) => out.push_str(value),
            None => out.push_str(&rest[start..start + 2 + end + 2]),
        }
        rest = &after[end + 2..];
    }
    out.push_str(rest);
    out
}

/// The type's body template, expanded. `{{cursor}}` is removed.
pub fn render_template(t: &NoteType, ctx: &TemplateCtx) -> String {
    render_template_with_cursor(t, ctx).0
}

/// The type's body template, expanded, plus the byte offset where `{{cursor}}`
/// stood — what the editor needs to put the caret in the right place.
///
/// Only the **first** marker counts: a template with two carets is a template
/// with a typo, and picking the first is the answer that surprises least.
pub fn render_template_with_cursor(t: &NoteType, ctx: &TemplateCtx) -> (String, Option<usize>) {
    let expanded = expand(&t.template, ctx);
    match expanded.find(CURSOR) {
        Some(at) => (expanded.replacen(CURSOR, "", 1), Some(at)),
        None => (expanded, None),
    }
}

/// The YAML frontmatter a new note of this type starts with: `type`, then every
/// field that has a default or is required, in the order the type declares them.
///
/// Written by hand rather than through a YAML serialiser on purpose — this is the
/// one place Garrulus *authors* frontmatter, and the output has to look like what
/// a person would have typed, because a person is going to read it in Obsidian.
pub fn render_frontmatter(t: &NoteType, ctx: &TemplateCtx) -> String {
    let mut out = String::from("---\n");
    out.push_str(&format!("type: {}\n", t.id));
    if !ctx.title.is_empty() {
        out.push_str(&format!("title: {}\n", yaml_scalar(&ctx.title)));
    }
    for field in &t.fields {
        let value = ctx
            .extra
            .get(&field.key)
            .map(String::as_str)
            .or(field.default.as_deref())
            .unwrap_or("");
        if value.is_empty() && !field.required {
            continue;
        }
        out.push_str(&format!("{}: {}\n", field.key, yaml_scalar(value)));
    }
    out.push_str("---\n");
    out
}

/// The whole file a "new note of type X" produces: frontmatter, a blank line,
/// then the body.
pub fn render_note(t: &NoteType, ctx: &TemplateCtx) -> (String, Option<usize>) {
    let head = render_frontmatter(t, ctx);
    let (body, cursor) = render_template_with_cursor(t, ctx);
    let prefix_len = head.len() + 1;
    (format!("{head}\n{body}"), cursor.map(|at| at + prefix_len))
}

/// Quote a scalar only when YAML would otherwise read it as something else.
///
/// Quoting everything would be safe and would also make every note Garrulus
/// creates look unlike every note the user writes by hand.
fn yaml_scalar(value: &str) -> String {
    let needs_quotes = value.is_empty()
        || value.starts_with(['&', '*', '!', '%', '@', '`', '>', '|', '{', '[', '#', '-', '?'])
        || value.contains(": ")
        || value.ends_with(':')
        || value.trim() != value;
    if needs_quotes {
        format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
    } else {
        value.to_string()
    }
}

/// A title reduced to lowercase ASCII words joined by single hyphens.
///
/// Accents fold to their base letter rather than being dropped, so *Gravità* is
/// `gravita` and not `gravit`: an Italian vault would otherwise produce filenames
/// with holes in them.
pub fn slugify(source: &str) -> String {
    let mut out = String::with_capacity(source.len());
    let mut pending_separator = false;
    for c in source.chars() {
        let folded = fold(c);
        if folded.is_empty() {
            // Anything that is not a letter or a digit is a word boundary. The
            // separator is only emitted once a real character follows, which
            // collapses runs and trims both ends in one pass.
            pending_separator = !out.is_empty();
            continue;
        }
        if pending_separator {
            out.push('-');
            pending_separator = false;
        }
        out.push_str(&folded);
    }
    out
}

/// One character to its slug form: the empty string for a separator, a lowercase
/// ASCII letter for an accented one, itself for anything already plain.
fn fold(c: char) -> String {
    if c.is_ascii_alphanumeric() {
        return c.to_ascii_lowercase().to_string();
    }
    let base = match c {
        'à' | 'á' | 'â' | 'ã' | 'ä' | 'å' | 'À' | 'Á' | 'Â' | 'Ã' | 'Ä' | 'Å' => "a",
        'æ' | 'Æ' => "ae",
        'ç' | 'Ç' => "c",
        'è' | 'é' | 'ê' | 'ë' | 'È' | 'É' | 'Ê' | 'Ë' => "e",
        'ì' | 'í' | 'î' | 'ï' | 'Ì' | 'Í' | 'Î' | 'Ï' => "i",
        'ñ' | 'Ñ' => "n",
        'ò' | 'ó' | 'ô' | 'õ' | 'ö' | 'ø' | 'Ò' | 'Ó' | 'Ô' | 'Õ' | 'Ö' | 'Ø' => "o",
        'ù' | 'ú' | 'û' | 'ü' | 'Ù' | 'Ú' | 'Û' | 'Ü' => "u",
        'ý' | 'ÿ' | 'Ý' => "y",
        'ß' => "ss",
        // Anything outside Latin-1 that is still a letter or a digit — Greek,
        // Cyrillic, CJK — is kept as itself: a vault written in it would
        // otherwise produce slugs that are entirely hyphens.
        other if other.is_alphanumeric() => return other.to_lowercase().to_string(),
        _ => "",
    };
    base.to_string()
}

/// A Unix timestamp to `(yyyy-MM-dd, HH:mm)`, UTC.
///
/// Pure integer arithmetic (the civil-from-days algorithm) rather than a date
/// crate: the whole need is two formatted strings, and the workspace does not
/// otherwise pay for a calendar. The caller that needs local time has to supply
/// the offset — see the module note.
pub fn civil_from_unix(unix_seconds: i64) -> (String, String) {
    let days = unix_seconds.div_euclid(86_400);
    let seconds_of_day = unix_seconds.rem_euclid(86_400);

    // Shift the epoch to 0000-03-01 so leap days land at the end of the cycle.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let day_of_era = z.rem_euclid(146_097);
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let shifted_month = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * shifted_month + 2) / 5 + 1;
    let month = if shifted_month < 10 { shifted_month + 3 } else { shifted_month - 9 };
    let year = if month <= 2 { year + 1 } else { year };

    let hour = seconds_of_day / 3_600;
    let minute = (seconds_of_day % 3_600) / 60;
    (format!("{year:04}-{month:02}-{day:02}"), format!("{hour:02}:{minute:02}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::note_type::{FieldKind, FieldSpec, NoteType};

    fn ctx() -> TemplateCtx {
        TemplateCtx::new("Crash all'avvio con profilo vuoto", "2026-07-31", "14:22")
    }

    fn a_type(template: &str) -> NoteType {
        NoteType { template: template.to_string(), ..NoteType::new("bug", "Bug") }
    }

    #[test]
    fn the_four_documented_placeholders_expand() {
        let out = expand("{{title}} · {{date}} {{time}} · {{slug}}", &ctx());
        assert_eq!(
            out,
            "Crash all'avvio con profilo vuoto · 2026-07-31 14:22 · crash-all-avvio-con-profilo-vuoto"
        );
    }

    #[test]
    fn an_unknown_placeholder_survives_as_literal_text() {
        let out = expand("{{title}} {{nonexistent}} {{ date }} {{unterminated", &ctx());
        assert_eq!(
            out,
            "Crash all'avvio con profilo vuoto {{nonexistent}} 2026-07-31 {{unterminated"
        );
    }

    #[test]
    fn extra_entries_are_placeholders_too() {
        let context = ctx().with("app", "corvus").with("device", "casa");
        assert_eq!(expand("{{app}}@{{device}}", &context), "corvus@casa");
    }

    #[test]
    fn the_cursor_marker_leaves_the_body_and_comes_back_as_an_offset() {
        let (body, cursor) = render_template_with_cursor(&a_type("## Passi\n1. {{cursor}}\n"), &ctx());
        assert_eq!(body, "## Passi\n1. \n");
        assert_eq!(cursor, Some("## Passi\n1. ".len()));
        assert_eq!(render_template(&a_type("## Passi\n1. {{cursor}}\n"), &ctx()), body);
    }

    #[test]
    fn a_body_without_a_cursor_reports_none() {
        let (body, cursor) = render_template_with_cursor(&a_type("## Atteso\n"), &ctx());
        assert_eq!(body, "## Atteso\n");
        assert_eq!(cursor, None);
    }

    #[test]
    fn frontmatter_carries_the_type_the_title_and_the_defaults() {
        let mut note_type = a_type("body");
        note_type.fields = vec![
            FieldSpec {
                key: "severity".into(),
                label: "Gravità".into(),
                kind: FieldKind::Enum,
                values: vec!["major".into(), "minor".into()],
                default: Some("major".into()),
                required: false,
                board: false,
            },
            FieldSpec {
                key: "version".into(),
                label: "Versione".into(),
                kind: FieldKind::Text,
                values: Vec::new(),
                default: None,
                required: false,
                board: false,
            },
        ];
        let out = render_frontmatter(&note_type, &ctx());
        assert_eq!(
            out,
            "---\ntype: bug\ntitle: Crash all'avvio con profilo vuoto\nseverity: major\n---\n"
        );
    }

    #[test]
    fn the_cursor_offset_survives_the_frontmatter_prefix() {
        let note_type = a_type("1. {{cursor}}\n");
        let (text, cursor) = render_note(&note_type, &ctx());
        let at = cursor.expect("the template declares a caret");
        assert_eq!(&text[at..], "\n");
        assert!(text.starts_with("---\ntype: bug\n"));
    }

    #[test]
    fn a_slug_folds_accents_instead_of_dropping_them() {
        assert_eq!(slugify("Gravità è già enorme"), "gravita-e-gia-enorme");
        assert_eq!(slugify("Crash all'avvio — profilo vuoto!"), "crash-all-avvio-profilo-vuoto");
        assert_eq!(slugify("  ...  "), "");
        assert_eq!(slugify("Über/Straße"), "uber-strasse");
        assert_eq!(slugify("ADR 12: Model D"), "adr-12-model-d");
    }

    #[test]
    fn a_timestamp_becomes_the_conventional_two_strings() {
        // 2026-07-31T14:22:00Z
        assert_eq!(civil_from_unix(1_785_507_720), ("2026-07-31".into(), "14:22".into()));
        assert_eq!(civil_from_unix(0), ("1970-01-01".into(), "00:00".into()));
        // A leap day, the case the shifted-epoch arithmetic exists for.
        assert_eq!(civil_from_unix(1_709_164_800), ("2024-02-29".into(), "00:00".into()));
    }
}
