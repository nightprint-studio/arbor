//! The bundle **as the buffer has it** — what the editor's i18n panel is looking at.
//!
//! ## Why this is not read from the catalogue
//!
//! [`LabelCatalog`] is built when the project is scanned, so every span in it describes the file as
//! it was on disk at that moment. That is right for the questions it answers ("which languages
//! declare this", "where is it read") and wrong for every question this module answers, because the
//! value in front of you is being **typed**: the key may not exist in the catalogue yet, the value's
//! offsets move on every keystroke, and half a construct mid-edit is the normal state rather than an
//! error. A panel driven off the indexed copy would preview the sentence you had a second ago and
//! write its edits at offsets the buffer has moved — the two failures are indistinguishable from a
//! panel that is simply broken.
//!
//! So the *text* comes from the buffer and the *project* comes from the catalogue: which styles
//! exist, which glossary keys exist, what the other languages say. Neither half can answer alone.
//!
//! ## What lives here
//!
//! - [`bundle_of`] — is this path a translation file at all, and of what.
//! - [`live_value_at`] — the translation under the caret, with the offsets to write back into.
//! - [`markup_spans`] — every construct in the buffer, to colour.
//! - [`studio_view`] — the whole panel's data in one answer.

use serde::Serialize;

use bennu_toml::prelude::{Entry, Manifest};

use crate::catalog::{
    entry_key, is_quoted, place_of, unquote, FileKind, GlossaryDecl, LabelCatalog, StyleDecl,
};
use crate::markup::{self, MarkupProblem, Name, Segment, SegmentKind};

/// How many other-language rows the panel is given.
///
/// Generous because this list is also the language **picker**: a project with forty locales is legal,
/// and a picker that silently stops at twenty-four is a picker that cannot reach half of them. The
/// count of missing languages stays exact regardless — that comes from the catalogue.
const MAX_SIBLINGS: usize = 64;

// ── where a file sits ─────────────────────────────────────────────────────────

/// A translation file: which `i18n/` tree it belongs to, which language, which category.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Bundle {
    /// The `i18n` directory, forward-slashed, no trailing slash.
    pub root: String,
    /// The directory name — `it`, `en`.
    pub lang: String,
    /// The file name without `.toml`, which is the label's category.
    pub category: String,
}

/// `Some` only for a **category** file, `i18n/<lang>/<category>.toml`.
///
/// `languages.toml`, `styles.toml` and `glossary.toml` are not translations: they declare what a
/// translation may name. Nothing deeper than one level is either — the engine does not load it.
pub fn bundle_of(path: &str) -> Option<Bundle> {
    let place = place_of(path)?;
    match place.kind {
        FileKind::Category { lang, category } => Some(Bundle { root: place.root, lang, category }),
        _ => None,
    }
}

// ── the buffer ────────────────────────────────────────────────────────────────

/// One translation as the buffer currently has it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LiveValue {
    /// The key path within the category: `items.new_game`.
    pub key: String,
    /// The markup, unquoted and with TOML escapes resolved.
    pub raw: String,
    /// Byte offset of `raw`'s first character in the file.
    ///
    /// `None` for a **basic** string carrying a backslash escape, whose content is shorter than its
    /// source — an offset into it drifts after the escape. Everything positional degrades on that:
    /// the toolbar cannot write into it and the panel says why, rather than writing to the wrong
    /// byte. A literal string (single quotes) always has one, which is the second reason the markup
    /// wants one.
    pub content_start: Option<usize>,
    /// The value including its quotes.
    pub value_start: usize,
    pub value_end: usize,
    pub line: u32,
}

/// Every translation the buffer declares, in source order.
pub fn live_values(text: &str) -> Vec<LiveValue> {
    let m = Manifest::parse(text);
    m.entries.iter().filter_map(live_of).collect()
}

/// The translation whose key or value contains `at`, when there is one.
///
/// Spans the whole assignment, so a caret anywhere on the line finds it — including on the key,
/// which is where it is after typing one.
pub fn live_value_at(text: &str, at: usize) -> Option<LiveValue> {
    let m = Manifest::parse(text);
    live_of(m.entry_at(at)?)
}

fn live_of(entry: &Entry) -> Option<LiveValue> {
    let (raw, content_off) = unquote(&entry.value);
    // Only a string is a translation. A number or a bool is legal in a bundle — the engine
    // stringifies it — but it is not markup, and previewing one would put a panel around a value
    // there is nothing to say about.
    if content_off.is_none() && !is_quoted(&entry.value) {
        return None;
    }
    Some(LiveValue {
        key: entry_key(entry),
        raw,
        content_start: content_off.map(|off| entry.value_start + off),
        value_start: entry.value_start,
        value_end: entry.value_end,
        line: entry.line,
    })
}

// ── colouring ─────────────────────────────────────────────────────────────────

/// One span of markup to colour, as a byte offset into the **file**.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MarkupSpan {
    pub start: usize,
    pub end: usize,
    /// Namespaced by extension id, like every other contributed highlight kind.
    pub kind: &'static str,
}

/// The whole construct, so nesting is visible as nesting. Drawn as a background rather than a
/// colour, which is why it can sit under the name spans below without fighting them.
const SPAN_STYLE: &str = "fulcrum.i18n.span.style";
const SPAN_GLOSSARY: &str = "fulcrum.i18n.span.glossary";
const SPAN_CONTROL: &str = "fulcrum.i18n.span.control";
const PLACEHOLDER: &str = "fulcrum.i18n.placeholder";
const STYLE: &str = "fulcrum.i18n.style";
/// A style the stylesheet does not declare. The diagnostic says so too; this is what makes it
/// visible while typing, before the next scan.
const STYLE_UNKNOWN: &str = "fulcrum.i18n.style.unknown";
const GLOSSARY: &str = "fulcrum.i18n.glossary";
const GLOSSARY_UNKNOWN: &str = "fulcrum.i18n.glossary.unknown";
const CONTROL: &str = "fulcrum.i18n.control";
const NAMESPACE: &str = "fulcrum.i18n.namespace";

/// Every construct in every translation of the buffer, ready to colour.
///
/// The catalogue is consulted for one thing only — whether a name exists — and only when the
/// project has the file that would declare it. A project with no `styles.toml` has not written a
/// wrong style name, it has not written the file.
pub fn markup_spans(cat: &LabelCatalog, text: &str) -> Vec<MarkupSpan> {
    let mut out = Vec::new();
    for value in live_values(text) {
        // Without a trustworthy content offset there is no span in the file to colour. The value
        // still gets its diagnostics, on the whole value, from `ext`.
        let Some(base) = value.content_start else { continue };
        let parsed = markup::parse_markup(&value.raw);
        push_spans(cat, &parsed.segments, base, &mut out);
    }
    out
}

fn push_spans(cat: &LabelCatalog, segments: &[Segment], base: usize, out: &mut Vec<MarkupSpan>) {
    // Outer spans are pushed before the names inside them, so a renderer that paints in order
    // draws the background first and the name on top of it.
    for s in segments {
        let whole = |kind| MarkupSpan { start: base + s.start, end: base + s.end, kind };
        let named = |n: &Name, kind| MarkupSpan { start: base + n.start, end: base + n.end, kind };
        match &s.kind {
            SegmentKind::Text { .. } => {}
            SegmentKind::Placeholder { .. } => out.push(whole(PLACEHOLDER)),
            SegmentKind::Style { namespace, styles, content } => {
                out.push(whole(SPAN_STYLE));
                if let Some(ns) = namespace {
                    out.push(named(ns, NAMESPACE));
                }
                for name in styles {
                    let known = !cat.has_stylesheet() || cat.has_style(&name.text);
                    out.push(named(name, if known { STYLE } else { STYLE_UNKNOWN }));
                }
                push_spans(cat, content, base, out);
            }
            SegmentKind::Glossary { namespace, key, content } => {
                out.push(whole(SPAN_GLOSSARY));
                if let Some(ns) = namespace {
                    out.push(named(ns, NAMESPACE));
                }
                let known = !cat.has_glossary_file() || cat.has_glossary_key(&key.text);
                out.push(named(key, if known { GLOSSARY } else { GLOSSARY_UNKNOWN }));
                push_spans(cat, content, base, out);
            }
            SegmentKind::Control { name, content, .. } => {
                out.push(whole(SPAN_CONTROL));
                out.push(named(name, CONTROL));
                push_spans(cat, content, base, out);
            }
        }
    }
}

// ── the panel ─────────────────────────────────────────────────────────────────

/// Another language's side of the same label — **whether or not it has one**.
///
/// The rows that declare nothing are the ones the language picker most needs: "translate this into
/// German" is the reason you open the picker, and a list of only the languages already done cannot
/// express it. So a non-declaring row carries the file it *would* be written in, derived from the
/// tree's own layout, and the panel opens that file rather than refusing to go anywhere.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Sibling {
    pub lang: String,
    /// The language's own name for itself, when `languages.toml` gave one. Empty otherwise.
    pub name: String,
    /// Whether this language declares the label at all. When `false`, everything below except
    /// `file` is empty and `offset` is 0.
    pub declares: bool,
    /// Whether the language is switched on. A disabled one is shown and marked: it is declared, so
    /// it is a place a translation can go, but nothing is owed to it.
    pub enabled: bool,
    /// The markup as written.
    pub value: String,
    /// What the sentence says, constructs flattened away.
    pub text: String,
    /// The placeholders it uses — what makes "this language forgot `{amount}`" visible.
    pub params: Vec<String>,
    /// Where it is declared — or, when it is not, where it would be.
    pub file: String,
    pub offset: usize,
    pub line: u32,
}

/// Everything the editor's i18n panel draws, for one caret.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StudioView {
    /// `menu:items.new_game`.
    pub label: String,
    pub lang: String,
    pub category: String,
    /// The `i18n/` directory this file belongs to — what the language picker builds a path from.
    pub root: String,
    /// The markup as the buffer has it.
    pub raw: String,
    /// See [`LiveValue::content_start`] — `None` means the toolbar goes read-only.
    pub content_start: Option<usize>,
    pub value_start: usize,
    pub value_end: usize,
    pub line: u32,
    pub segments: Vec<Segment>,
    pub problems: Vec<MarkupProblem>,
    /// Placeholder names, in order, deduplicated.
    pub params: Vec<String>,
    /// Whether the catalogue has this label at all. `false` on a key just typed — which is not a
    /// problem, and is why the panel shows the preview either way.
    pub known: bool,
    pub siblings: Vec<Sibling>,
    /// Enabled languages that do not declare it.
    pub missing: Vec<String>,
    /// The whole stylesheet: the picker's list, and the preview's rendering.
    pub styles: Vec<StyleDecl>,
    pub glossary: Vec<GlossaryDecl>,
    /// The project's own control vocabulary — see
    /// [`LabelCatalog::controls`](crate::catalog::LabelCatalog::controls).
    pub controls: Vec<String>,
    pub has_stylesheet: bool,
    pub has_glossary: bool,
}

/// The panel's data for the caret at `at` in `path`, or `None` when that caret is not on a
/// translation.
///
/// `None` covers three different situations on purpose — not a bundle file, not on a value, on a
/// value that is not a string — because the panel does the same thing in all three: it says nothing
/// is selected. Distinguishing them would be three empty states for one gesture.
pub fn studio_view(cat: &LabelCatalog, path: &str, source: &str, at: usize) -> Option<StudioView> {
    let bundle = bundle_of(path)?;
    let live = live_value_at(source, at)?;
    let label = format!("{}:{}", bundle.category, live.key);
    let parsed = markup::parse_markup(&live.raw);
    let params = markup::placeholders(&parsed.segments);

    let decls = cat.declarations(&label);
    let missing: Vec<String> = cat.untranslated(&label).iter().map(|l| l.to_string()).collect();

    // Driven by the DECLARED LANGUAGES rather than by the declarations, which is what puts the
    // untranslated ones in the list — see [`Sibling`]. The language being edited is skipped: it is
    // the buffer, and showing the indexed copy of it beside the live one is two versions of the same
    // sentence with the stale one labelled as authoritative.
    let siblings: Vec<Sibling> = cat
        .languages()
        .iter()
        .filter(|l| l.code != bundle.lang)
        .take(MAX_SIBLINGS)
        .map(|l| match cat.declaration_in(&label, &l.code) {
            Some(d) => {
                let segs = markup::parse_markup(&d.value).segments;
                Sibling {
                    lang: l.code.clone(),
                    name: l.name.clone(),
                    declares: true,
                    enabled: l.enabled,
                    value: d.value.clone(),
                    text: markup::flatten(&segs),
                    params: markup::placeholders(&segs),
                    file: d.file.clone(),
                    offset: d.content_start.unwrap_or(d.start),
                    line: d.line,
                }
            }
            None => Sibling {
                lang: l.code.clone(),
                name: l.name.clone(),
                declares: false,
                enabled: l.enabled,
                value: String::new(),
                text: String::new(),
                params: Vec::new(),
                // Where it would go. The layout is the engine's own, so this is a fact about the
                // tree rather than a guess — and the file may not exist yet, which is the point.
                file: format!("{}/{}/{}.toml", bundle.root, l.code, bundle.category),
                offset: 0,
                line: 0,
            },
        })
        .collect();

    Some(StudioView {
        label,
        lang: bundle.lang,
        category: bundle.category,
        root: bundle.root,
        raw: live.raw,
        content_start: live.content_start,
        value_start: live.value_start,
        value_end: live.value_end,
        line: live.line,
        segments: parsed.segments,
        problems: parsed.problems,
        params,
        known: !decls.is_empty(),
        siblings,
        missing,
        styles: cat.styles().cloned().collect(),
        glossary: cat.glossary().cloned().collect(),
        controls: cat.controls().to_vec(),
        has_stylesheet: cat.has_stylesheet(),
        has_glossary: cat.has_glossary_file(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const LANGUAGES: &str = r#"
[[languages]]
code = "it"
name = "Italian"
enabled = true

[[languages]]
code = "en"
name = "English"
enabled = true
"#;

    const STYLES: &str = "[red]\ncolor = \"#ff0000\"\n\n[bold]\nweight = \"bold\"\n";
    const GLOSSARY: &str = "[potion]\nname = \"Potion\"\ndescription = \"heals\"\n";

    fn catalog(it: &str, en: &str) -> LabelCatalog {
        LabelCatalog::build(&[
            ("/p/i18n/languages.toml".to_string(), LANGUAGES.to_string()),
            ("/p/i18n/styles.toml".to_string(), STYLES.to_string()),
            ("/p/i18n/glossary.toml".to_string(), GLOSSARY.to_string()),
            ("/p/i18n/it/menu.toml".to_string(), it.to_string()),
            ("/p/i18n/en/menu.toml".to_string(), en.to_string()),
        ])
    }

    #[test]
    fn a_category_file_is_a_bundle_and_the_declaring_ones_are_not() {
        let b = bundle_of("/p/i18n/it/menu.toml").expect("a category file");
        assert_eq!(b.lang, "it");
        assert_eq!(b.category, "menu");
        assert_eq!(b.root, "/p/i18n");
        for other in ["/p/i18n/languages.toml", "/p/i18n/styles.toml", "/p/i18n/glossary.toml"] {
            assert!(bundle_of(other).is_none(), "{other} declares, it does not translate");
        }
        assert!(bundle_of("/p/src/main.rs").is_none());
    }

    #[test]
    fn the_value_under_the_caret_is_found_from_the_key_as_well_as_from_the_text() {
        let text = "[items]\nnew_game = 'Nuova partita'\n";
        let on_key = live_value_at(text, text.find("new_game").unwrap() + 2).expect("on the key");
        let on_text = live_value_at(text, text.find("Nuova").unwrap()).expect("in the value");
        assert_eq!(on_key, on_text);
        assert_eq!(on_key.key, "items.new_game");
        assert_eq!(on_key.raw, "Nuova partita");
        // The offset points at the text, not at the quote.
        assert_eq!(on_key.content_start, Some(text.find("Nuova").unwrap()));
    }

    #[test]
    fn a_value_that_is_not_a_string_is_not_a_translation() {
        let text = "count = 3\nflag = true\n";
        assert!(live_value_at(text, 2).is_none());
        assert!(live_value_at(text, text.find("flag").unwrap()).is_none());
        assert!(live_values(text).is_empty());
    }

    #[test]
    fn a_basic_string_with_an_escape_has_no_trustworthy_offset() {
        let text = "a = \"line\\nbreak\"\n";
        let live = live_value_at(text, 1).expect("still a translation");
        assert_eq!(live.raw, "line\nbreak");
        // Which is what makes the toolbar read-only rather than writing to the wrong byte.
        assert_eq!(live.content_start, None);
    }

    #[test]
    fn every_construct_gets_a_span_and_the_names_sit_inside_them() {
        let cat = catalog("greet = '$red.bold{Ciao} {name}'\n", "");
        let text = "greet = '$red.bold{Ciao} {name}'\n";
        let spans = markup_spans(&cat, text);

        let kinds: Vec<&str> = spans.iter().map(|s| s.kind).collect();
        assert!(kinds.contains(&SPAN_STYLE));
        assert!(kinds.contains(&PLACEHOLDER));
        assert_eq!(kinds.iter().filter(|k| **k == STYLE).count(), 2, "red and bold");

        // The placeholder span covers the braces too, so it reads as one token.
        let ph = spans.iter().find(|s| s.kind == PLACEHOLDER).unwrap();
        assert_eq!(&text[ph.start..ph.end], "{name}");
        // A style name span is the name alone — which is what makes "no such style" underline the
        // one wrong word instead of the whole span.
        let red = spans.iter().find(|s| s.kind == STYLE).unwrap();
        assert_eq!(&text[red.start..red.end], "red");
    }

    #[test]
    fn a_style_the_stylesheet_lacks_is_coloured_as_unknown() {
        let cat = catalog("a = '$nope{x}'\n", "");
        let spans = markup_spans(&cat, "a = '$nope{x}'\n");
        assert!(spans.iter().any(|s| s.kind == STYLE_UNKNOWN));
        assert!(!spans.iter().any(|s| s.kind == STYLE));
    }

    #[test]
    fn without_a_stylesheet_nothing_is_unknown() {
        let cat = LabelCatalog::build(&[
            ("/p/i18n/languages.toml".to_string(), LANGUAGES.to_string()),
            ("/p/i18n/it/menu.toml".to_string(), "a = '$nope{x}'\n".to_string()),
        ]);
        let spans = markup_spans(&cat, "a = '$nope{x}'\n");
        assert!(spans.iter().any(|s| s.kind == STYLE));
        assert!(!spans.iter().any(|s| s.kind == STYLE_UNKNOWN));
    }

    #[test]
    fn a_glossary_key_the_project_lacks_is_coloured_as_unknown() {
        let cat = catalog("a = '@potion{una pozione} @elixir{x}'\n", "");
        let spans = markup_spans(&cat, "a = '@potion{una pozione} @elixir{x}'\n");
        assert_eq!(spans.iter().filter(|s| s.kind == GLOSSARY).count(), 1);
        assert_eq!(spans.iter().filter(|s| s.kind == GLOSSARY_UNKNOWN).count(), 1);
    }

    #[test]
    fn the_view_carries_the_other_languages_and_not_the_one_being_edited() {
        let cat = catalog("hello = 'Ciao {name}'\n", "hello = 'Hello {name}'\n");
        let source = "hello = 'Ciao {name}'\n";
        let view = studio_view(&cat, "/p/i18n/it/menu.toml", source, 2).expect("a translation");

        assert_eq!(view.label, "menu:hello");
        assert_eq!(view.lang, "it");
        assert!(view.known);
        assert_eq!(view.params, vec!["name"]);
        assert_eq!(view.siblings.len(), 1, "only `en`");
        assert_eq!(view.siblings[0].lang, "en");
        assert!(view.siblings[0].declares);
        assert_eq!(view.siblings[0].text, "Hello name");
        assert_eq!(view.siblings[0].params, vec!["name"]);
    }

    /// The row a language picker exists for: a declared language with no translation yet, carrying
    /// the file the translation would go in — which is the whole gesture "now do the English".
    #[test]
    fn a_language_that_owes_a_translation_is_a_row_with_somewhere_to_go() {
        let cat = catalog("only_it = 'Solo italiano'\n", "");
        let view =
            studio_view(&cat, "/p/i18n/it/menu.toml", "only_it = 'Solo italiano'\n", 2).unwrap();
        assert_eq!(view.missing, vec!["en"]);
        assert_eq!(view.root, "/p/i18n");

        assert_eq!(view.siblings.len(), 1);
        let en = &view.siblings[0];
        assert_eq!(en.lang, "en");
        assert!(!en.declares, "it has no translation");
        assert!(en.enabled);
        assert_eq!(en.name, "English", "the picker shows the name, not only the code");
        assert_eq!(en.file, "/p/i18n/en/menu.toml", "where it would be written");
        assert!(en.text.is_empty());
    }

    #[test]
    fn a_key_that_is_not_indexed_yet_still_previews() {
        // What every newly typed line looks like: the buffer has it, the catalogue does not.
        let cat = catalog("hello = 'Ciao'\n", "hello = 'Hello'\n");
        let source = "hello = 'Ciao'\njust_typed = 'Nuovo $bold{testo}'\n";
        let view = studio_view(&cat, "/p/i18n/it/menu.toml", source, source.find("Nuovo").unwrap())
            .expect("previewed anyway");
        assert_eq!(view.label, "menu:just_typed");
        assert!(!view.known, "the catalogue has never seen it");
        // `en` is still a row — it is a declared language, and it does not have this key either.
        assert_eq!(view.siblings.len(), 1);
        assert!(!view.siblings[0].declares);
        assert_eq!(view.segments.len(), 2, "the text, then the style span");
    }

    #[test]
    fn the_view_carries_the_projects_own_control_vocabulary() {
        let cat = catalog("a = '~slow{x} ~sleep(0.8) ~slow{y}'\n", "b = '~shake{z}'\n");
        let view = studio_view(&cat, "/p/i18n/it/menu.toml", "a = '~slow{x}'\n", 1).unwrap();
        // Most-used first, and drawn from both languages: it is the project's vocabulary, not the
        // file's.
        assert_eq!(view.controls, vec!["slow", "shake", "sleep"]);
    }

    #[test]
    fn a_caret_that_is_not_on_a_translation_answers_nothing() {
        let cat = catalog("a = 'x'\n", "");
        let source = "a = 'x'\n\n\n";
        assert!(studio_view(&cat, "/p/i18n/it/menu.toml", source, source.len() - 1).is_none());
        // …and neither does a file that is not a bundle, whatever is in it.
        assert!(studio_view(&cat, "/p/other.toml", "a = 'x'\n", 1).is_none());
    }

    #[test]
    fn the_problems_of_a_half_typed_construct_reach_the_panel() {
        let cat = catalog("a = 'x'\n", "");
        let view = studio_view(&cat, "/p/i18n/it/menu.toml", "a = '$bold{oops'\n", 1).unwrap();
        assert!(!view.problems.is_empty(), "an unclosed brace is a problem, not a refusal");
        assert!(!view.segments.is_empty(), "and what did parse is still there");
    }
}
