//! The label catalogue — what the project's `i18n/` trees declare.
//!
//! ## The layout, which is a convention rather than a manifest
//!
//! ```text
//! <root>/i18n/
//!   languages.toml     the declared languages; the first enabled one is the default
//!   styles.toml        one table per style — the names `$red.bold{…}` may use
//!   glossary.toml      one table per glossary entry — the keys `@potion{…}` may use
//!   it/
//!     menu.toml        the file name IS the category
//!     tree.toml
//!   en/
//!     …
//! ```
//!
//! A label is `category:dotted.key` — the category from the file name, the key from the nested
//! tables. `menu:items.new_game` is `new_game` under `[items]` in `it/menu.toml` and in
//! `en/menu.toml`, and those are two **declarations of one label**.
//!
//! ## Several roots, one catalogue
//!
//! A project can have more than one `i18n/` tree — the base game's and each mod's — and the engine
//! merges them with later roots winning. So this is one catalogue over all of them, remembering
//! which file each declaration came from. That is what makes "who owes this translation" answerable
//! at all: the question is per **language**, and a language may be declared in one root and
//! translated in another.
//!
//! ## Why the spans matter more than the values
//!
//! Reading the strings is the easy half. What an editor needs is *where*: the byte range of a
//! value, so going to a label's declaration lands on the text and not on the top of the file, and
//! so a missing translation can be reported on the line that should have held it. Hence
//! [`bennu_toml`] rather than a value tree — see that crate's header.
//!
//! One honest limitation, recorded on the data: a **basic** TOML string containing a backslash
//! escape has content whose offsets no longer line up with the file's, because `\n` is two bytes
//! on disk and one in the value. Those declarations carry no content offset
//! ([`Declaration::content_start`] is `None`) and anything positional about them is reported on the
//! whole value instead of inside it. A literal string (single quotes) — which is what the markup
//! wants anyway, since `\$` is not a valid TOML escape — always has one.

use std::collections::{BTreeMap, BTreeSet};

use bennu_toml::prelude::{Entry, Manifest, ROOT_TABLE};
use serde::Serialize;

/// A language the project declares.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Language {
    pub code: String,
    pub name: String,
    pub native_name: String,
    pub enabled: bool,
    /// The `languages.toml` that declared it.
    pub file: String,
}

/// One declaration of a label, in one language.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Declaration {
    /// Language code, from the directory name.
    pub lang: String,
    /// Absolute path, forward-slashed.
    pub file: String,
    /// The value as the markup was written, unquoted and with TOML escapes resolved.
    pub value: String,
    /// Byte offset of the value in the file, quotes included.
    pub start: usize,
    pub end: usize,
    /// Byte offset of the string's **content**, when it lines up with the file — see the module
    /// doc. `None` for a basic string carrying escapes.
    pub content_start: Option<usize>,
    pub line: u32,
}

/// A style `styles.toml` declares, and the fields the preview renders it with.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct StyleDecl {
    pub name: String,
    /// `light` · `normal` · `medium` · `bold` · `black`.
    pub weight: Option<String>,
    /// Point size, as written.
    pub size: Option<String>,
    /// `none` · `underline` · `line_through`.
    pub decoration: Option<String>,
    /// Whatever the colour was written as — a hex string, a named colour, a table.
    pub color: Option<String>,
    pub file: String,
    pub start: usize,
    pub line: u32,
}

/// A glossary entry `glossary.toml` declares.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct GlossaryDecl {
    pub key: String,
    pub name: String,
    pub description: String,
    /// The style it renders with; the engine defaults it to `glossary-item`.
    pub style: String,
    pub file: String,
    pub start: usize,
    pub line: u32,
}

/// Everything the project's `i18n/` trees declare.
#[derive(Debug, Clone, Default)]
pub struct LabelCatalog {
    /// The `i18n` directories found, forward-slashed.
    roots: Vec<String>,
    languages: Vec<Language>,
    /// label → its declarations, one per language that has it. Ordered so the catalogue reads
    /// the same on every open.
    labels: BTreeMap<String, Vec<Declaration>>,
    styles: BTreeMap<String, StyleDecl>,
    glossary: BTreeMap<String, GlossaryDecl>,
    /// Control names the project's own translations use, most-used first — see [`Self::controls`].
    controls: Vec<String>,
}

impl LabelCatalog {
    /// Build from every `.toml` under an `i18n/` directory. Files elsewhere are ignored, so the
    /// caller may hand over the whole project's TOML without filtering.
    pub fn build(files: &[(String, String)]) -> LabelCatalog {
        let mut cat = LabelCatalog::default();
        let mut roots = BTreeSet::new();
        // Languages first: a category file under `xx/` is only a translation if `xx` is declared,
        // and without that rule a stray directory becomes a phantom language.
        for (path, text) in files {
            let Some(place) = place_of(path) else { continue };
            roots.insert(place.root.clone());
            if matches!(place.kind, FileKind::Languages) {
                cat.read_languages(path, text);
            }
        }
        cat.roots = roots.into_iter().collect();
        for (path, text) in files {
            let Some(place) = place_of(path) else { continue };
            match place.kind {
                FileKind::Languages => {}
                FileKind::Styles => cat.read_styles(path, text),
                FileKind::Glossary => cat.read_glossary(path, text),
                FileKind::Category { lang, category } => {
                    // A directory nobody declared is not a language. Reporting its keys as
                    // translations would invent a language the engine will never load.
                    if cat.languages.iter().any(|l| l.code == lang) {
                        cat.read_category(path, text, &lang, &category);
                    }
                }
            }
        }
        cat.controls = harvest_controls(&cat.labels);
        cat
    }

    pub fn is_empty(&self) -> bool {
        self.labels.is_empty()
    }

    pub fn roots(&self) -> &[String] {
        &self.roots
    }

    pub fn languages(&self) -> &[Language] {
        &self.languages
    }

    /// The language a lookup falls back to: the first enabled one, else the first declared.
    ///
    /// Never hard-coded to `en` — a project translated only into Italian has no `en` at all, and
    /// assuming one makes every fallback miss.
    pub fn default_language(&self) -> Option<&str> {
        self.languages
            .iter()
            .find(|l| l.enabled)
            .or_else(|| self.languages.first())
            .map(|l| l.code.as_str())
    }

    pub fn label_count(&self) -> usize {
        self.labels.len()
    }

    pub fn labels(&self) -> impl Iterator<Item = &str> {
        self.labels.keys().map(String::as_str)
    }

    /// Whether any language declares `label`.
    pub fn knows(&self, label: &str) -> bool {
        self.labels.contains_key(label)
    }

    pub fn declarations(&self, label: &str) -> &[Declaration] {
        self.labels.get(label).map(Vec::as_slice).unwrap_or_default()
    }

    /// The declaration in `lang`, when there is one.
    pub fn declaration_in(&self, label: &str, lang: &str) -> Option<&Declaration> {
        self.declarations(label).iter().find(|d| d.lang == lang)
    }

    /// The **enabled** languages that do not declare `label`.
    ///
    /// Enabled only, because a language that is declared but switched off is not owed anything —
    /// reporting it would make every label in a project with a work-in-progress locale look
    /// incomplete.
    pub fn untranslated(&self, label: &str) -> Vec<&str> {
        let have: BTreeSet<&str> =
            self.declarations(label).iter().map(|d| d.lang.as_str()).collect();
        self.languages
            .iter()
            .filter(|l| l.enabled && !have.contains(l.code.as_str()))
            .map(|l| l.code.as_str())
            .collect()
    }

    /// Every category a label may name — the file names, deduplicated. What completion offers
    /// before the `:` has been typed.
    pub fn categories(&self) -> Vec<&str> {
        let mut out: Vec<&str> =
            self.labels.keys().filter_map(|k| k.split_once(':').map(|(c, _)| c)).collect();
        out.dedup();
        out
    }

    pub fn has_style(&self, name: &str) -> bool {
        self.styles.contains_key(name)
    }

    pub fn styles(&self) -> impl Iterator<Item = &StyleDecl> {
        self.styles.values()
    }

    pub fn style(&self, name: &str) -> Option<&StyleDecl> {
        self.styles.get(name)
    }

    /// Whether the stylesheet declares anything at all.
    ///
    /// The gate on the unknown-style check: a project with no `styles.toml` has not written a
    /// wrong style name, it has not written the file — and flagging every style in it would be a
    /// wall of noise on a project that renders correctly.
    pub fn has_stylesheet(&self) -> bool {
        !self.styles.is_empty()
    }

    pub fn has_glossary_key(&self, key: &str) -> bool {
        self.glossary.contains_key(key)
    }

    pub fn glossary(&self) -> impl Iterator<Item = &GlossaryDecl> {
        self.glossary.values()
    }

    pub fn has_glossary_file(&self) -> bool {
        !self.glossary.is_empty()
    }

    /// The control names the project's translations actually use, most-used first.
    ///
    /// There is no catalogue of controls to check against — what `~slow` or `~sleep(0.8)` mean is
    /// the consumer's, and i18n knows only the *form*. So the honest answer to "which controls may
    /// I write" is the project's own vocabulary, which is also the useful one: it converges on the
    /// handful a project has settled on instead of offering an invented list the engine may not
    /// implement. Ordered by use so the common ones come first, then by name so it is stable.
    pub fn controls(&self) -> &[String] {
        &self.controls
    }

    // ── readers ───────────────────────────────────────────────────────────────

    fn read_languages(&mut self, path: &str, text: &str) {
        let m = Manifest::parse(text);
        // `[[languages]]` is an array of tables, so every element shares the path `languages` and
        // its entries arrive in source order — which is what "the first enabled one" depends on.
        let mut current: Option<Language> = None;
        for e in m.entries.iter().filter(|e| e.table == "languages") {
            let value = unquote(&e.value).0;
            if e.key == "code" {
                if let Some(done) = current.take() {
                    self.push_language(done);
                }
                current = Some(Language {
                    code: value,
                    name: String::new(),
                    native_name: String::new(),
                    enabled: false,
                    file: path.to_string(),
                });
                continue;
            }
            let Some(lang) = current.as_mut() else { continue };
            match e.key.as_str() {
                "name" => lang.name = value,
                "native_name" => lang.native_name = value,
                "enabled" => lang.enabled = e.value.trim() == "true",
                _ => {}
            }
        }
        if let Some(done) = current.take() {
            self.push_language(done);
        }
    }

    /// A language already declared by another root is not declared twice — the engine's `merge`
    /// appends only the ones it has not seen.
    fn push_language(&mut self, lang: Language) {
        if lang.code.is_empty() || self.languages.iter().any(|l| l.code == lang.code) {
            return;
        }
        self.languages.push(lang);
    }

    fn read_styles(&mut self, path: &str, text: &str) {
        let m = Manifest::parse(text);
        for t in &m.tables {
            // A style is a top-level table; `[red.hover]` would be a field group of `red`, not a
            // style of its own.
            if t.path.contains('.') {
                continue;
            }
            let mut decl = StyleDecl {
                name: t.path.clone(),
                file: path.to_string(),
                start: t.start,
                line: t.line,
                ..StyleDecl::default()
            };
            for e in m.entries_in(&t.path) {
                let v = unquote(&e.value).0;
                match e.key.as_str() {
                    "weight" => decl.weight = Some(v),
                    "size" => decl.size = Some(v),
                    "decoration" => decl.decoration = Some(v),
                    "color" => decl.color = Some(v),
                    _ => {}
                }
            }
            // Later roots win, matching the engine's `Stylesheet::merge`.
            self.styles.insert(decl.name.clone(), decl);
        }
    }

    fn read_glossary(&mut self, path: &str, text: &str) {
        let m = Manifest::parse(text);
        for t in &m.tables {
            if t.path.contains('.') {
                continue;
            }
            let mut decl = GlossaryDecl {
                key: t.path.clone(),
                style: "glossary-item".to_string(),
                file: path.to_string(),
                start: t.start,
                line: t.line,
                ..GlossaryDecl::default()
            };
            for e in m.entries_in(&t.path) {
                let v = unquote(&e.value).0;
                match e.key.as_str() {
                    "name" => decl.name = v,
                    "description" => decl.description = v,
                    "style" => decl.style = v,
                    _ => {}
                }
            }
            self.glossary.insert(decl.key.clone(), decl);
        }
    }

    fn read_category(&mut self, path: &str, text: &str, lang: &str, category: &str) {
        let m = Manifest::parse(text);
        for e in &m.entries {
            // Only strings are translations. A number or a bool is legal in the engine (it
            // stringifies them) but it is not markup, and treating one as a label would put a
            // row in the catalogue nothing can be said about.
            let (value, content_start) = unquote(&e.value);
            if content_start.is_none() && !is_quoted(&e.value) {
                continue;
            }
            let label = format!("{category}:{}", entry_key(e));
            let decl = Declaration {
                lang: lang.to_string(),
                file: path.to_string(),
                value,
                start: e.value_start,
                end: e.value_end,
                content_start: content_start.map(|off| e.value_start + off),
                line: e.line,
            };
            let list = self.labels.entry(label).or_default();
            // Later roots override the same language's earlier declaration, as the engine's
            // `Translations::merge` does.
            match list.iter().position(|d| d.lang == decl.lang) {
                Some(i) => list[i] = decl,
                None => list.push(decl),
            }
        }
    }
}

// ── the layout ────────────────────────────────────────────────────────────────

/// What an `i18n/` file is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum FileKind {
    Languages,
    Styles,
    Glossary,
    Category { lang: String, category: String },
}

pub(crate) struct Place {
    /// The `i18n` directory, forward-slashed and without a trailing slash.
    pub(crate) root: String,
    pub(crate) kind: FileKind,
}

/// Where a path sits in an `i18n/` tree, or `None` when it is not in one.
pub(crate) fn place_of(path: &str) -> Option<Place> {
    let path = path.replace('\\', "/");
    if !path.ends_with(".toml") {
        return None;
    }
    // The LAST `i18n/` segment, so a project that happens to live under a directory called `i18n`
    // does not make its whole tree one.
    let at = path.rfind("/i18n/")?;
    let root = path[..at + 5].to_string();
    let rest = &path[at + 6..];
    let name = rest.rsplit('/').next().unwrap_or(rest);
    match (rest.contains('/'), name) {
        (false, "languages.toml") => Some(Place { root, kind: FileKind::Languages }),
        (false, "styles.toml") => Some(Place { root, kind: FileKind::Styles }),
        (false, "glossary.toml") => Some(Place { root, kind: FileKind::Glossary }),
        // A loose `.toml` at the root of `i18n/` is not a category — the category directory IS the
        // language, and a file with no language cannot be a translation of anything.
        (false, _) => None,
        (true, _) => {
            let (lang, tail) = rest.split_once('/')?;
            // One level only: `it/menu.toml`. A deeper file is not something the engine loads.
            if tail.contains('/') {
                return None;
            }
            Some(Place {
                root,
                kind: FileKind::Category {
                    lang: lang.to_string(),
                    category: tail.trim_end_matches(".toml").to_string(),
                },
            })
        }
    }
}

/// Every control name the declarations use, most-used first.
///
/// Costs one markup parse per declaration, paid once when the catalogue is built rather than on the
/// keystroke that opens a picker — which is the only reason it is a stored field and not a method.
fn harvest_controls(labels: &BTreeMap<String, Vec<Declaration>>) -> Vec<String> {
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    for decls in labels.values() {
        for d in decls {
            for name in crate::markup::control_refs(&crate::markup::parse_markup(&d.value).segments)
            {
                *counts.entry(name.text).or_default() += 1;
            }
        }
    }
    let mut out: Vec<(String, usize)> = counts.into_iter().collect();
    // Most-used first; `BTreeMap` already ordered the names, and a stable sort keeps that as the
    // tie-break, so two controls used once each stay alphabetical.
    out.sort_by(|a, b| b.1.cmp(&a.1));
    out.into_iter().map(|(name, _)| name).collect()
}

/// The label key an entry declares: `new_game` under `[items]` is `items.new_game`.
///
/// Shared with the live-buffer reader in [`crate::studio`] deliberately — a key composed one way
/// while indexing and another way while typing means the panel is looking at a label the catalogue
/// does not have, and the symptom is an empty panel on a line that is plainly a translation.
pub(crate) fn entry_key(entry: &Entry) -> String {
    match entry.table.as_str() {
        ROOT_TABLE => entry.key.clone(),
        table => format!("{table}.{}", entry.key),
    }
}

/// Whether a raw TOML value is a quoted string.
pub(crate) fn is_quoted(raw: &str) -> bool {
    let v = raw.trim();
    (v.starts_with('"') && v.len() >= 2) || (v.starts_with('\'') && v.len() >= 2)
}

/// The content of a TOML string value, and the byte offset of that content within `raw`.
///
/// The offset is `None` when it cannot be trusted: a **basic** string with a backslash escape has
/// content shorter than its source, so an offset into the value would drift after the escape. A
/// literal string (single quotes) never escapes anything, which is also why the markup wants one —
/// `"\$"` is not a valid TOML escape, and `'\$'` is exactly the two characters.
pub(crate) fn unquote(raw: &str) -> (String, Option<usize>) {
    let trimmed = raw.trim_start();
    let lead = raw.len() - trimmed.len();
    let v = trimmed.trim_end();

    for (open, literal) in [("'''", true), ("\"\"\"", false), ("'", true), ("\"", false)] {
        if v.len() >= open.len() * 2 && v.starts_with(open) && v.ends_with(open) {
            let inner = &v[open.len()..v.len() - open.len()];
            if literal {
                return (inner.to_string(), Some(lead + open.len()));
            }
            let (text, clean) = unescape_basic(inner);
            return (text, clean.then_some(lead + open.len()));
        }
    }
    // Not a string: a bool, a number, an inline table. Handed back verbatim so the language
    // reader can test `enabled` against it.
    (v.to_string(), None)
}

/// TOML basic-string escapes. The `bool` is whether the text came through unchanged — which is what
/// decides whether an offset into it still means anything.
fn unescape_basic(s: &str) -> (String, bool) {
    if !s.contains('\\') {
        return (s.to_string(), true);
    }
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some('r') => out.push('\r'),
            Some('"') => out.push('"'),
            Some('\\') => out.push('\\'),
            Some('u') => {
                let hex: String = chars.by_ref().take(4).collect();
                match u32::from_str_radix(&hex, 16).ok().and_then(char::from_u32) {
                    Some(ch) => out.push(ch),
                    // An invalid escape is kept as written: the file is wrong, and inventing a
                    // character would hide that.
                    None => {
                        out.push_str("\\u");
                        out.push_str(&hex);
                    }
                }
            }
            Some(other) => {
                out.push('\\');
                out.push(other);
            }
            None => out.push('\\'),
        }
    }
    (out, false)
}

#[cfg(test)]
mod tests {
    use super::*;

    const LANGUAGES: &str = r#"
[[languages]]
code = "it"
name = "Italian"
native_name = "Italiano"
enabled = true

[[languages]]
code = "en"
name = "English"
native_name = "English"
enabled = true

[[languages]]
code = "jp"
name = "Japanese"
native_name = "日本語"
enabled = false
"#;

    fn f(path: &str, text: &str) -> (String, String) {
        (path.to_string(), text.to_string())
    }

    fn catalog() -> LabelCatalog {
        LabelCatalog::build(&[
            f("/p/content/core/i18n/languages.toml", LANGUAGES),
            f(
                "/p/content/core/i18n/styles.toml",
                "[red]\ncolor = \"#ff0000\"\n\n[bold]\nweight = \"bold\"\n",
            ),
            f(
                "/p/content/core/i18n/glossary.toml",
                "[potion]\nname = \"Pozione\"\ndescription = \"Cura\"\n",
            ),
            f(
                "/p/content/core/i18n/it/menu.toml",
                "[items]\nnew_game = \"Nuova partita\"\nquit = \"Esci\"\n",
            ),
            f("/p/content/core/i18n/en/menu.toml", "[items]\nnew_game = \"New game\"\n"),
            f("/p/content/core/i18n/it/tree.toml", "[nodes.drill]\nname = 'Trapano $bold{+}'\n"),
        ])
    }

    #[test]
    fn a_label_is_the_file_name_and_the_nested_key() {
        let c = catalog();
        assert!(c.knows("menu:items.new_game"));
        assert!(c.knows("tree:nodes.drill.name"));
        assert!(!c.knows("menu:items.nope"));
    }

    #[test]
    fn one_label_has_one_declaration_per_language() {
        let c = catalog();
        let d = c.declarations("menu:items.new_game");
        assert_eq!(d.len(), 2);
        assert_eq!(d.iter().map(|x| x.lang.as_str()).collect::<Vec<_>>(), ["it", "en"]);
        assert_eq!(d[0].value, "Nuova partita");
    }

    /// The check the whole crate is for: a label translated in one language and not the other.
    #[test]
    fn an_untranslated_label_names_who_owes_it() {
        let c = catalog();
        assert_eq!(c.untranslated("menu:items.quit"), ["en"]);
        assert!(c.untranslated("menu:items.new_game").is_empty());
    }

    /// A declared-but-disabled language is owed nothing — otherwise every label in a project with
    /// a work-in-progress locale reads as incomplete.
    #[test]
    fn a_disabled_language_is_not_owed_a_translation() {
        let c = catalog();
        assert!(!c.untranslated("menu:items.quit").contains(&"jp"));
        assert_eq!(c.languages().len(), 3, "it is still declared, though");
    }

    #[test]
    fn the_default_language_is_the_first_enabled_one() {
        let c = catalog();
        assert_eq!(c.default_language(), Some("it"));
    }

    /// Never `en`: a project translated only into Italian has no `en`, and assuming one makes
    /// every fallback miss.
    #[test]
    fn the_default_language_is_never_assumed() {
        let c = LabelCatalog::build(&[f(
            "/p/i18n/languages.toml",
            "[[languages]]\ncode = \"it\"\nenabled = true\n",
        )]);
        assert_eq!(c.default_language(), Some("it"));
    }

    #[test]
    fn a_declaration_points_at_the_value_in_the_file() {
        let text = "[items]\nnew_game = \"Nuova partita\"\n";
        let c = LabelCatalog::build(&[
            f("/p/i18n/languages.toml", LANGUAGES),
            f("/p/i18n/it/menu.toml", text),
        ]);
        let d = &c.declarations("menu:items.new_game")[0];
        assert_eq!(&text[d.start..d.end], "\"Nuova partita\"");
        let content = d.content_start.expect("a clean string has an offset");
        assert_eq!(&text[content..content + d.value.len()], "Nuova partita");
    }

    /// A literal string keeps its content offset, which is what makes a markup problem reportable
    /// inside the value — and literal strings are what the markup needs anyway, since `\$` is not
    /// a valid TOML escape.
    #[test]
    fn a_literal_string_keeps_its_offset() {
        let text = "tooltip = 'costa 5\\$'\n";
        let c = LabelCatalog::build(&[
            f("/p/i18n/languages.toml", LANGUAGES),
            f("/p/i18n/it/tree.toml", text),
        ]);
        let d = &c.declarations("tree:tooltip")[0];
        assert_eq!(d.value, "costa 5\\$");
        assert!(d.content_start.is_some());
    }

    /// An escaped basic string cannot carry one, and says so rather than carrying a wrong one.
    #[test]
    fn an_escaped_basic_string_declines_an_offset() {
        let c = LabelCatalog::build(&[
            f("/p/i18n/languages.toml", LANGUAGES),
            f("/p/i18n/it/menu.toml", "a = \"one\\ntwo\"\n"),
        ]);
        let d = &c.declarations("menu:a")[0];
        assert_eq!(d.value, "one\ntwo");
        assert_eq!(d.content_start, None);
    }

    #[test]
    fn styles_and_glossary_entries_are_their_table_names() {
        let c = catalog();
        assert!(c.has_style("red"));
        assert!(c.has_style("bold"));
        assert!(!c.has_style("italic"));
        assert_eq!(c.style("red").and_then(|s| s.color.clone()), Some("#ff0000".to_string()));
        assert!(c.has_glossary_key("potion"));
        // The engine defaults it, so the catalogue does too.
        assert_eq!(c.glossary().next().map(|g| g.style.clone()), Some("glossary-item".to_string()));
    }

    /// A directory nobody declared is not a language — reporting its keys would invent one the
    /// engine will never load.
    #[test]
    fn an_undeclared_directory_is_not_a_language() {
        let c = LabelCatalog::build(&[
            f("/p/i18n/languages.toml", "[[languages]]\ncode = \"it\"\nenabled = true\n"),
            f("/p/i18n/it/menu.toml", "a = \"x\"\n"),
            f("/p/i18n/de/menu.toml", "a = \"y\"\n"),
        ]);
        assert_eq!(c.declarations("menu:a").len(), 1);
        assert_eq!(c.declarations("menu:a")[0].lang, "it");
    }

    /// Two roots merge, and a language declared in one is translated in the other.
    #[test]
    fn a_second_root_merges_and_overrides() {
        let c = LabelCatalog::build(&[
            f("/p/content/core/i18n/languages.toml", LANGUAGES),
            f("/p/content/core/i18n/it/menu.toml", "a = \"base\"\nb = \"solo base\"\n"),
            f("/p/mods/x/i18n/it/menu.toml", "a = \"mod\"\n"),
        ]);
        assert_eq!(c.roots().len(), 2);
        // Later wins for the same language, as the engine's merge does.
        assert_eq!(c.declaration_in("menu:a", "it").map(|d| d.value.clone()), Some("mod".to_string()));
        assert!(c.knows("menu:b"), "what only the base declares survives");
    }

    #[test]
    fn a_file_outside_an_i18n_tree_is_ignored() {
        let c = LabelCatalog::build(&[
            f("/p/i18n/languages.toml", LANGUAGES),
            f("/p/Cargo.toml", "[package]\nname = \"x\"\n"),
            f("/p/content/audio.toml", "volume = \"1\"\n"),
        ]);
        assert!(c.is_empty());
    }

    #[test]
    fn a_non_string_value_is_not_a_label() {
        let c = LabelCatalog::build(&[
            f("/p/i18n/languages.toml", LANGUAGES),
            f("/p/i18n/it/menu.toml", "count = 3\nflag = true\ntext = \"x\"\n"),
        ]);
        assert!(c.knows("menu:text"));
        assert!(!c.knows("menu:count"));
        assert!(!c.knows("menu:flag"));
    }

    #[test]
    fn an_empty_project_is_empty_and_harmless() {
        let c = LabelCatalog::build(&[]);
        assert!(c.is_empty());
        assert!(!c.has_stylesheet());
        assert_eq!(c.default_language(), None);
        assert!(c.untranslated("anything").is_empty());
    }
}
