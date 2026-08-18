//! The extension — what a host registers, and the only file that knows the seam exists.
//!
//! Registered as **`fulcrum.i18n`**: a namespaced id, so its catalog kinds and diagnostic codes
//! namespace themselves and a sibling fulcrum subsystem (assets, content) is a new crate plus one
//! registration line rather than an edit here.
//!
//! ## The model is two halves that answer different questions
//!
//! - The [`LabelCatalog`](crate::catalog::LabelCatalog) — what exists, in which languages, with
//!   which styles and glossary entries available to it.
//! - A **usage index** — where each label is read. This is why the extension takes a project scan
//!   at all: "which labels does nothing use" and "where is this one used" cannot be answered from
//!   the file in front of you, and in a fulcrum project most of the answer is in the `.ron` content
//!   rather than in the code.
//!
//! ## Find-usages without a new verb
//!
//! The seam has no `usages` verb, and does not need one: a catalog row carries **children**, and
//! each child carries a file and an offset. So a label's row expands into one child per language
//! (what it says, where it is declared) followed by one child per reading (which file, which line) —
//! and clicking any of them navigates. "Find usages", "find unused" and "which languages are
//! missing" are then the same panel with a filter, which is also how they are actually used: you
//! look at the list, not at one label at a time.
//!
//! ## What is a diagnostic and what is only a tag
//!
//! A **label nothing declares** is a diagnostic: it is a bug that no compiler and no test can see,
//! and the engine renders the label itself on screen when it cannot resolve one — so it survives QA
//! until somebody notices `tree:nodes.drill.name` written on a hexagon.
//!
//! A label **missing in some language** is a *tag* on its catalog row rather than a diagnostic —
//! except when the missing one is the **default** language, which is a real defect because that is
//! the language every other lookup falls back to. Reporting a mid-translation project's every label
//! as a warning would bury the first kind under the second.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};

use bennu_ext::prelude::{
    ExtEntry, ExtHighlight, ExtHover, ExtStat, ExtTarget, FileCtx, FrameworkExtension, ProjectScan,
};
use bennu_proto::prelude::{CapabilitySet, CompletionItem, Diagnostic};

use crate::catalog::LabelCatalog;
use crate::markup::{self, SegmentKind};
use crate::refs;
use crate::studio::{self, StudioView};

/// How many labels a completion popup offers. Past this nobody reads the list, and a project can
/// declare thousands.
const MAX_COMPLETIONS: usize = 200;

/// How many usage rows one label's children carry. A label read from four hundred content files is
/// real, and four hundred rows crossing the IPC seam for one row of the panel is not worth it —
/// the count on the row still says four hundred.
const MAX_USES_SHOWN: usize = 50;

/// One reading of a label.
#[derive(Debug, Clone)]
struct Use {
    file: String,
    offset: usize,
    line: u32,
}

/// The extension.
#[derive(Default)]
pub struct FulcrumI18nExtension {
    catalog: RwLock<Arc<LabelCatalog>>,
    /// label → every place the project reads it.
    uses: RwLock<Arc<HashMap<String, Vec<Use>>>>,
    /// Whether a scan has run. Kept apart from "the catalogue has anything in it": a project with
    /// no `i18n/` tree is *ready and empty*, and reporting it as never-ready leaves the overview
    /// waiting forever.
    scanned: AtomicBool,
}

impl FulcrumI18nExtension {
    pub fn new() -> Self {
        Self::default()
    }

    /// The catalogue, when there is one worth answering from.
    fn resolved(&self) -> Option<Arc<LabelCatalog>> {
        let cat = self.catalog.read().ok()?;
        (!cat.is_empty()).then(|| Arc::clone(&cat))
    }

    fn uses_of(&self, label: &str) -> Vec<Use> {
        self.uses.read().ok().and_then(|u| u.get(label).cloned()).unwrap_or_default()
    }

    /// What the editor's i18n panel draws for the caret at `at`.
    ///
    /// Not a `FrameworkExtension` verb and should not become one: the trait's positional methods all
    /// answer in the seam's *shared* vocabulary (a target, a hover card, a completion), and this
    /// answers in fulcrum's — a parsed markup tree, a stylesheet, the same label in four other
    /// languages. A trait method only one extension could ever implement is the wrong
    /// generalisation of exactly one case, which is the rule the JSP tag-library model already
    /// follows for the same reason.
    ///
    /// Answers on a project with **no bundles at all**, unlike everything above: the panel's job on
    /// a brand-new `i18n/it/menu.toml` is to help write the first line, and refusing until a scan
    /// has seen the file is refusing exactly when it is most useful. The catalogue is then simply
    /// empty, which costs the sibling list and the unknown-name check and nothing else.
    pub fn studio(&self, path: &str, source: &str, at: usize) -> Option<StudioView> {
        let cat = self.catalog.read().ok()?;
        studio::studio_view(&cat, path, source, at)
    }
}

impl FrameworkExtension for FulcrumI18nExtension {
    fn id(&self) -> &'static str {
        "fulcrum.i18n"
    }

    fn display_name(&self) -> &'static str {
        "fulcrum i18n"
    }

    fn applies(&self, caps: &CapabilitySet) -> bool {
        caps.fulcrum_i18n
    }

    fn reindex(&self, scan: &ProjectScan<'_>) {
        // The bundles are `.toml` under an `i18n/` directory; the catalogue ignores everything
        // else, so the whole resource bucket can be handed over unfiltered.
        let bundles: Vec<(String, String)> = scan
            .resources
            .iter()
            .map(|f| (f.path.to_string_lossy().replace('\\', "/"), f.text.clone()))
            .collect();
        let built = Arc::new(LabelCatalog::build(&bundles));

        // Every place the project reads a label. `.ron` first because that is where most of them
        // are — a content file declares a label where an older design inlined a string.
        let mut uses: HashMap<String, Vec<Use>> = HashMap::new();
        for f in scan.ron.iter().chain(scan.rust.iter()) {
            let path = f.path.to_string_lossy().replace('\\', "/");
            for r in refs::labels_in(&path, &f.text) {
                uses.entry(r.label).or_default().push(Use {
                    file: path.clone(),
                    offset: r.start,
                    line: r.line,
                });
            }
        }

        if let Ok(mut slot) = self.catalog.write() {
            *slot = built;
        }
        if let Ok(mut slot) = self.uses.write() {
            *slot = Arc::new(uses);
        }
        self.scanned.store(true, Ordering::Relaxed);
    }

    fn is_ready(&self) -> bool {
        self.scanned.load(Ordering::Relaxed)
    }

    /// Framework problems in a file: a label nothing declares, and a bundle's own markup.
    ///
    /// ## The bundle half reads the BUFFER, not the catalogue
    ///
    /// It used to walk the catalogue's declarations for this file, and that was wrong in a way only
    /// visible while typing: the catalogue was built when the project was scanned, so both the text
    /// it checked and the offsets it reported were the file as it was on disk. Typing `$bold{`
    /// produced no warning until the next scan, fixing one left the squiggle where it was, and after
    /// a few edits the offsets pointed at the wrong characters entirely.
    ///
    /// Reading the buffer fixes all three, and it is also what keeps the editor and the i18n panel
    /// telling the same story — the panel has always read the buffer, and two views of one value
    /// disagreeing is worse than either being late. It is cheaper as well: the old loop scanned every
    /// label in the project to find the handful in this file.
    ///
    /// The catalogue is still consulted for what only it knows — whether a style, a glossary key or a
    /// language exists — and each of those checks gates itself on the project having the file that
    /// would declare it.
    fn diagnostics(&self, ctx: &FileCtx<'_>) -> Vec<Diagnostic> {
        let path = ctx.path_str();
        let mut out = Vec::new();

        // ── a reading of a label nothing declares ──────────────────────────────
        //
        // The one check that genuinely needs a built catalogue: "nothing declares this" cannot be
        // answered before anything has been read, and answering it anyway would flag every label in
        // the project on startup.
        if let Some(cat) = self.resolved() {
            for r in refs::labels_in(&path, ctx.source) {
                if !cat.knows(&r.label) {
                    out.push(Diagnostic {
                        message: format!("no i18n bundle declares `{}`", r.label),
                        severity: "warning".to_string(),
                        code: "fulcrum.i18n.unknown-label".to_string(),
                        start: r.start,
                        end: r.end,
                    });
                }
            }
        }

        // ── a bundle's own markup ──────────────────────────────────────────────
        let Some(bundle) = studio::bundle_of(&path) else { return out };
        // Read directly rather than through `resolved()`: an unclosed brace is wrong whether or not
        // the project has been scanned, and a brand-new bundle is exactly where the first one is
        // typed. An empty catalogue simply answers "no stylesheet, no glossary" below.
        let Ok(cat) = self.catalog.read() else { return out };

        for value in studio::live_values(ctx.source) {
            let parsed = markup::parse_markup(&value.raw);
            // Offsets are relative to the value's content, which a basic string carrying escapes
            // does not have — those fall back to the whole value, which is still the right line.
            let span = |s: usize, e: usize| match value.content_start {
                Some(off) => (off + s, off + e),
                None => (value.value_start, value.value_end),
            };

            for p in &parsed.problems {
                let (start, end) = span(p.start, p.end);
                out.push(Diagnostic {
                    message: p.message.clone(),
                    severity: "warning".to_string(),
                    code: "fulcrum.i18n.markup".to_string(),
                    start,
                    end,
                });
            }

            // A style the stylesheet does not declare renders as the default and silently loses the
            // emphasis it was written for. Only checked when there IS a stylesheet: a project without
            // one has not written a wrong name, it has not written the file.
            if cat.has_stylesheet() {
                for name in markup::style_refs(&parsed.segments) {
                    if cat.has_style(&name.text) {
                        continue;
                    }
                    let (start, end) = span(name.start, name.end);
                    out.push(Diagnostic {
                        message: format!("no style named `{}` in styles.toml", name.text),
                        severity: "warning".to_string(),
                        code: "fulcrum.i18n.unknown-style".to_string(),
                        start,
                        end,
                    });
                }
            }
            if cat.has_glossary_file() {
                for name in markup::glossary_refs(&parsed.segments) {
                    if cat.has_glossary_key(&name.text) {
                        continue;
                    }
                    let (start, end) = span(name.start, name.end);
                    out.push(Diagnostic {
                        message: format!("no glossary entry named `{}`", name.text),
                        severity: "warning".to_string(),
                        code: "fulcrum.i18n.unknown-glossary".to_string(),
                        start,
                        end,
                    });
                }
            }

            // Missing in the DEFAULT language is a defect rather than a tag: that is the language
            // every other lookup falls back to, so the fallback itself fails. Reported on the whole
            // value, because it is the declaration that is missing rather than anything inside this
            // one.
            if let Some(default) = cat.default_language() {
                let label = format!("{}:{}", bundle.category, value.key);
                if bundle.lang != default && cat.declaration_in(&label, default).is_none() {
                    out.push(Diagnostic {
                        message: format!(
                            "`{label}` is not declared in `{default}`, the fallback language"
                        ),
                        severity: "warning".to_string(),
                        code: "fulcrum.i18n.no-fallback".to_string(),
                        start: value.value_start,
                        end: value.value_end,
                    });
                }
            }
        }
        out
    }

    /// The markup inside a bundle's string values.
    ///
    /// TOML has one opinion about `'Dealt $red.bold{{amount}} damage'`: it is a string. Which is
    /// exactly why a mistake in it is invisible — the file looks syntactically perfect. These spans
    /// colour the parts that are structure rather than prose, so a `$` that never opened a brace and
    /// a placeholder that is really two words read differently from the sentence around them.
    ///
    /// Read from the **buffer**, not from the catalogue: see [`crate::studio`].
    fn highlights(&self, ctx: &FileCtx<'_>) -> Vec<ExtHighlight> {
        let path = ctx.path_str();
        if studio::bundle_of(&path).is_none() {
            return Vec::new();
        }
        // Unlike the checks above this runs with an empty catalogue too — the colouring of a
        // construct does not depend on the project, only the "no such name" variant of it does, and
        // that gates itself on the file that would declare the name existing.
        let Ok(cat) = self.catalog.read() else { return Vec::new() };
        studio::markup_spans(&cat, ctx.source)
            .into_iter()
            .map(|s| ExtHighlight { start: s.start, end: s.end, kind: s.kind.to_string() })
            .collect()
    }

    /// Every file that declares the label at the caret, one target per language.
    fn navigate(&self, ctx: &FileCtx<'_>, offset: usize) -> Vec<ExtTarget> {
        let Some(cat) = self.resolved() else { return Vec::new() };
        let path = ctx.path_str();
        let Some(r) = refs::label_at(&path, ctx.source, offset) else { return Vec::new() };
        cat.declarations(&r.label)
            .iter()
            .map(|d| ExtTarget {
                file: d.file.clone(),
                offset: d.content_start.unwrap_or(d.start),
                label: format!("{} — {}", d.lang, file_name(&d.file)),
                detail: markup::flatten(&markup::parse_markup(&d.value).segments),
            })
            .collect()
    }

    /// What the label says, in every language it says it in — the question you open the bundle to
    /// answer, asked without leaving the file.
    fn hover(&self, ctx: &FileCtx<'_>, offset: usize) -> Option<ExtHover> {
        let cat = self.resolved()?;
        let path = ctx.path_str();
        let r = refs::label_at(&path, ctx.source, offset)?;
        let decls = cat.declarations(&r.label);
        if decls.is_empty() {
            return Some(ExtHover {
                title: r.label,
                signature: "not declared by any i18n bundle".to_string(),
                doc: String::new(),
            });
        }
        let flat = |raw: &str| markup::flatten(&markup::parse_markup(raw).segments);
        let signature = flat(&decls[0].value);
        let mut doc: Vec<String> =
            decls.iter().map(|d| format!("{} — {}", d.lang, flat(&d.value))).collect();
        let owed = cat.untranslated(&r.label);
        if !owed.is_empty() {
            doc.push(format!("untranslated in {}", owed.join(", ")));
        }
        // The placeholders it expects: a caller that forgets one renders `{amount}` on screen.
        let params = markup::placeholders(&markup::parse_markup(&decls[0].value).segments);
        if !params.is_empty() {
            doc.push(format!("parameters: {}", params.join(", ")));
        }
        Some(ExtHover { title: r.label, signature, doc: doc.join("\n") })
    }

    /// The labels that continue what is being typed inside a string.
    fn completions(&self, ctx: &FileCtx<'_>, offset: usize) -> Vec<CompletionItem> {
        let Some(cat) = self.resolved() else { return Vec::new() };
        let path = ctx.path_str();
        let Some(prefix) = refs::label_prefix_at(&path, ctx.source, offset) else {
            return Vec::new();
        };
        cat.labels()
            .filter(|l| l.starts_with(&prefix))
            .take(MAX_COMPLETIONS)
            .map(|l| CompletionItem {
                label: l.to_string(),
                kind: "text".to_string(),
                detail: cat
                    .declarations(l)
                    .first()
                    .map(|d| markup::flatten(&markup::parse_markup(&d.value).segments)),
                auto_import: None,
                ..Default::default()
            })
            .collect()
    }

    /// `labels` — one row per label, expanding into its translations and then its readings.
    fn catalog(&self, kind: &str) -> Vec<ExtEntry> {
        let Some(cat) = (kind == "labels").then(|| self.resolved()).flatten() else {
            return Vec::new();
        };
        let flat = |raw: &str| markup::flatten(&markup::parse_markup(raw).segments);
        cat.labels()
            .map(|label| {
                let decls = cat.declarations(label);
                let first = decls.first();
                let uses = self.uses_of(label);
                let mut tags = Vec::new();
                match uses.len() {
                    0 => tags.push("unused".to_string()),
                    1 => tags.push("1 use".to_string()),
                    n => tags.push(format!("{n} uses")),
                }
                let owed = cat.untranslated(label);
                if !owed.is_empty() {
                    tags.push(format!("missing {}", owed.join(", ")));
                }

                // One child per language, then one per reading — see the module doc for why
                // find-usages lives here rather than behind a verb of its own.
                let mut children: Vec<ExtEntry> = decls
                    .iter()
                    .map(|d| ExtEntry {
                        id: format!("{label}@{}", d.file),
                        primary: d.lang.clone(),
                        secondary: flat(&d.value),
                        kind: "locale".to_string(),
                        file: Some(d.file.clone()),
                        offset: Some(d.content_start.unwrap_or(d.start)),
                        line: Some(d.line),
                        ..ExtEntry::default()
                    })
                    .collect();
                children.extend(uses.iter().take(MAX_USES_SHOWN).map(|u| ExtEntry {
                    id: format!("{label}#{}:{}", u.file, u.offset),
                    primary: file_name(&u.file).to_string(),
                    secondary: format!("line {}", u.line),
                    kind: "use".to_string(),
                    file: Some(u.file.clone()),
                    offset: Some(u.offset),
                    line: Some(u.line),
                    ..ExtEntry::default()
                }));

                ExtEntry {
                    id: label.to_string(),
                    primary: label.to_string(),
                    secondary: first.map(|d| flat(&d.value)).unwrap_or_default(),
                    // The category, which is the grouping the panel offers.
                    kind: label.split_once(':').map(|(c, _)| c.to_string()).unwrap_or_default(),
                    file: first.map(|d| d.file.clone()),
                    offset: first.map(|d| d.content_start.unwrap_or(d.start)),
                    line: first.map(|d| d.line),
                    tags,
                    children,
                }
            })
            .collect()
    }

    fn stats(&self) -> Vec<ExtStat> {
        let Some(cat) = self.resolved() else {
            return vec![ExtStat {
                label: "Labels".to_string(),
                value: 0,
                catalog: Some("labels".to_string()),
            }];
        };
        vec![ExtStat {
            label: "Labels".to_string(),
            value: cat.label_count(),
            catalog: Some("labels".to_string()),
        }]
    }
}

fn file_name(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

/// Whether a segment tree contains anything but plain text — the cheap test for "is this markup or
/// is it a sentence", used by the frontend to decide whether a preview is worth showing.
pub fn is_rich(raw: &str) -> bool {
    markup::parse_markup(raw)
        .segments
        .iter()
        .any(|s| !matches!(s.kind, SegmentKind::Text { .. }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use bennu_ext::prelude::ScannedFile;
    use std::path::{Path, PathBuf};

    fn file(path: &str, text: &str) -> ScannedFile {
        ScannedFile { path: PathBuf::from(path), text: text.to_string() }
    }

    const LANGUAGES: &str = "\
[[languages]]
code = \"it\"
enabled = true

[[languages]]
code = \"en\"
enabled = true
";

    fn ext() -> FulcrumI18nExtension {
        let ext = FulcrumI18nExtension::new();
        let resources = [
            file("/p/content/core/i18n/languages.toml", LANGUAGES),
            file("/p/content/core/i18n/styles.toml", "[bold]\nweight = \"bold\"\n"),
            file(
                "/p/content/core/i18n/it/tree.toml",
                "[nodes.drill]\nname = 'Trapano'\n[nodes.gone]\nname = 'Orfano'\n",
            ),
            file("/p/content/core/i18n/en/tree.toml", "[nodes.drill]\nname = 'Drill'\n"),
        ];
        let ron = [file(
            "/p/content/core/tree.ron",
            "(id: \"drill\", name: \"tree:nodes.drill.name\", desc: \"tree:nodes.drill.desc\")",
        )];
        ext.reindex(&ProjectScan {
            root: Path::new("/p"),
            java: &[],
            xml: &[],
            resources: &resources,
            pages: &[],
            schemas: &[],
            descriptors: &[],
            taglibs: &[],
            rust: &[],
            ron: &ron,
        });
        ext
    }

    fn ctx<'a>(path: &'a Path, source: &'a str) -> FileCtx<'a> {
        FileCtx { path, source }
    }

    /// The check that pays for the crate: a label the content reads and no bundle declares. No
    /// compiler and no test sees it, and the engine renders the label itself on screen.
    #[test]
    fn a_label_no_bundle_declares_is_reported_where_it_is_read() {
        let src = "(name: \"tree:nodes.drill.name\", desc: \"tree:nodes.drill.desc\")";
        let p = PathBuf::from("/p/content/core/tree.ron");
        let d = ext().diagnostics(&ctx(&p, src));
        let unknown: Vec<&Diagnostic> =
            d.iter().filter(|x| x.code == "fulcrum.i18n.unknown-label").collect();
        assert_eq!(unknown.len(), 1, "only the one nothing declares");
        assert_eq!(&src[unknown[0].start..unknown[0].end], "tree:nodes.drill.desc");
    }

    #[test]
    fn go_to_offers_one_target_per_language() {
        let src = "(name: \"tree:nodes.drill.name\")";
        let p = PathBuf::from("/p/content/core/tree.ron");
        let t = ext().navigate(&ctx(&p, src), src.find("drill").unwrap());
        assert_eq!(t.len(), 2);
        assert_eq!(t[0].detail, "Trapano");
        assert!(t[1].label.starts_with("en —"));
    }

    #[test]
    fn hover_says_what_it_reads_in_every_language() {
        let src = "(name: \"tree:nodes.drill.name\")";
        let p = PathBuf::from("/p/content/core/tree.ron");
        let h = ext().hover(&ctx(&p, src), src.find("drill").unwrap()).expect("a hover");
        assert_eq!(h.signature, "Trapano");
        assert!(h.doc.contains("en — Drill"));
    }

    #[test]
    fn hover_on_a_partly_translated_label_says_who_owes_it() {
        let src = "(name: \"tree:nodes.gone.name\")";
        let p = PathBuf::from("/p/content/core/tree.ron");
        let h = ext().hover(&ctx(&p, src), src.find("gone").unwrap()).expect("a hover");
        assert!(h.doc.contains("untranslated in en"), "doc was `{}`", h.doc);
    }

    #[test]
    fn the_catalogue_counts_uses_and_carries_both_kinds_of_child() {
        let rows = ext().catalog("labels");
        let used = rows.iter().find(|r| r.id == "tree:nodes.drill.name").expect("the row");
        assert_eq!(used.secondary, "Trapano");
        assert_eq!(used.tags, ["1 use"]);
        // Two locales and one reading — which is what makes this row a find-usages result too.
        let kinds: Vec<&str> = used.children.iter().map(|c| c.kind.as_str()).collect();
        assert_eq!(kinds, ["locale", "locale", "use"]);
        assert_eq!(used.children[2].secondary, "line 1");

        let dead = rows.iter().find(|r| r.id == "tree:nodes.gone.name").expect("the row");
        assert_eq!(dead.tags, ["unused", "missing en"]);
    }

    #[test]
    fn completion_offers_the_labels_that_continue_the_prefix() {
        let src = "(name: \"tree:nodes.d\")";
        let p = PathBuf::from("/p/content/core/tree.ron");
        let items = ext().completions(&ctx(&p, src), src.find("nodes.d").unwrap() + 7);
        assert_eq!(
            items.iter().map(|i| i.label.as_str()).collect::<Vec<_>>(),
            ["tree:nodes.drill.name"],
        );
        assert_eq!(items[0].detail.as_deref(), Some("Trapano"));
    }

    /// An unknown style renders as the default and loses the emphasis it was written for — silently,
    /// which is why it is worth a diagnostic.
    #[test]
    fn an_unknown_style_is_reported_inside_the_value() {
        let ext = FulcrumI18nExtension::new();
        let resources = [
            file("/p/i18n/languages.toml", LANGUAGES),
            file("/p/i18n/styles.toml", "[bold]\nweight = \"bold\"\n"),
            file("/p/i18n/it/menu.toml", "a = 'ciao $bolde{mondo}'\n"),
        ];
        ext.reindex(&ProjectScan {
            root: Path::new("/p"),
            java: &[],
            xml: &[],
            resources: &resources,
            pages: &[],
            schemas: &[],
            descriptors: &[],
            taglibs: &[],
            rust: &[],
            ron: &[],
        });
        let src = "a = 'ciao $bolde{mondo}'\n";
        let p = PathBuf::from("/p/i18n/it/menu.toml");
        let d = ext.diagnostics(&ctx(&p, src));
        let style: Vec<&Diagnostic> =
            d.iter().filter(|x| x.code == "fulcrum.i18n.unknown-style").collect();
        assert_eq!(style.len(), 1);
        assert_eq!(&src[style[0].start..style[0].end], "bolde", "the span is the style name");
    }

    /// A project with no `styles.toml` has not written a wrong style name — it has not written the
    /// file, and flagging every style in it would be a wall of noise.
    #[test]
    fn without_a_stylesheet_no_style_is_wrong() {
        let ext = FulcrumI18nExtension::new();
        let resources = [
            file("/p/i18n/languages.toml", LANGUAGES),
            file("/p/i18n/it/menu.toml", "a = 'ciao $anything{mondo}'\n"),
        ];
        ext.reindex(&ProjectScan {
            root: Path::new("/p"),
            java: &[],
            xml: &[],
            resources: &resources,
            pages: &[],
            schemas: &[],
            descriptors: &[],
            taglibs: &[],
            rust: &[],
            ron: &[],
        });
        let p = PathBuf::from("/p/i18n/it/menu.toml");
        let d = ext.diagnostics(&ctx(&p, "a = 'ciao $anything{mondo}'\n"));
        assert!(d.iter().all(|x| x.code != "fulcrum.i18n.unknown-style"));
    }

    #[test]
    fn a_project_with_no_bundles_says_nothing_at_all() {
        let ext = FulcrumI18nExtension::new();
        ext.reindex(&ProjectScan {
            root: Path::new("/p"),
            java: &[],
            xml: &[],
            resources: &[],
            pages: &[],
            schemas: &[],
            descriptors: &[],
            taglibs: &[],
            rust: &[],
            ron: &[],
        });
        let src = "(name: \"anything:at.all\")";
        let p = PathBuf::from("/p/x.ron");
        assert!(ext.is_ready(), "an empty project is ready, not pending");
        assert!(ext.diagnostics(&ctx(&p, src)).is_empty(), "no bundles means no verdict");
        assert!(ext.catalog("labels").is_empty());
    }

    /// The change that made the editor and the panel agree: a problem typed since the last scan is
    /// reported, and one fixed since the last scan is gone. Both used to be wrong — the check read
    /// the catalogue's copy of the file, which is whatever was on disk when the project was scanned.
    #[test]
    fn markup_problems_are_read_from_the_buffer_not_from_the_index() {
        let ext = ext();
        let path = PathBuf::from("/p/content/core/i18n/it/tree.toml");
        let codes = |src: &str| -> Vec<String> {
            ext.diagnostics(&ctx(&path, src)).into_iter().map(|d| d.code).collect()
        };

        // The indexed copy of this file is clean (see `ext()`), and this buffer is not.
        let typing = "[nodes.drill]\nname = 'Il $bold{Trapano'\n";
        assert!(codes(typing).iter().any(|c| c == "fulcrum.i18n.markup"), "unclosed, right now");

        // A style the stylesheet does not have, typed into a value that did not have one.
        let wrong_style = "[nodes.drill]\nname = 'Il $nope{Trapano}'\n";
        let d = ext.diagnostics(&ctx(&path, wrong_style));
        let style: Vec<&Diagnostic> =
            d.iter().filter(|x| x.code == "fulcrum.i18n.unknown-style").collect();
        assert_eq!(style.len(), 1);
        assert_eq!(&wrong_style[style[0].start..style[0].end], "nope", "on the name alone");

        // And the clean buffer is clean, even though the index still holds whatever it holds.
        let clean = "[nodes.drill]\nname = 'Il Trapano'\n";
        assert!(
            !codes(clean).iter().any(|c| c == "fulcrum.i18n.markup"),
            "a fixed value stops being reported without waiting for a rescan",
        );
    }

    /// The seam's side of the colouring. What each span *is* is pinned in `studio`; what matters
    /// here is that the gate lets a bundle through and keeps everything else out — a `.toml` that is
    /// not a translation getting markup colours would repaint half a Cargo manifest.
    #[test]
    fn a_bundle_gets_markup_spans_and_nothing_else_does() {
        let ext = ext();
        let source = "[nodes.drill]\nname = 'Il $bold{Trapano} di {owner}'\n";
        let path = PathBuf::from("/p/content/core/i18n/it/tree.toml");
        let spans = ext.highlights(&ctx(&path, source));

        let kinds: Vec<&str> = spans.iter().map(|s| s.kind.as_str()).collect();
        assert!(kinds.contains(&"fulcrum.i18n.placeholder"), "{kinds:?}");
        assert!(kinds.contains(&"fulcrum.i18n.style"), "{kinds:?}");

        // Same text, in a file that is not a translation.
        let other = PathBuf::from("/p/Cargo.toml");
        assert!(ext.highlights(&ctx(&other, source)).is_empty());
        // …and the declaring files of the tree itself are not translations either.
        let styles = PathBuf::from("/p/content/core/i18n/styles.toml");
        assert!(ext.highlights(&ctx(&styles, source)).is_empty());
    }

    /// The panel reads the buffer, so a key the index has never seen still answers — which is every
    /// line at the moment it is typed.
    #[test]
    fn the_studio_answers_for_a_key_that_is_not_indexed_yet() {
        let ext = ext();
        let source = "[nodes.drill]\nname = 'Trapano'\nblurb = 'Scava $bold{giu}'\n";
        let at = source.find("Scava").unwrap();
        let view = ext.studio("/p/content/core/i18n/it/tree.toml", source, at).expect("a view");

        assert_eq!(view.label, "tree:nodes.drill.blurb");
        assert!(!view.known);
        assert!(view.has_stylesheet);
        assert_eq!(view.styles.len(), 1, "the stylesheet reaches the picker");
        // A caret on a file that is not a bundle answers nothing at all.
        assert!(ext.studio("/p/Cargo.toml", source, at).is_none());
    }

    #[test]
    fn a_catalog_kind_that_is_not_ours_is_empty() {
        assert!(ext().catalog("keys").is_empty());
    }
}
