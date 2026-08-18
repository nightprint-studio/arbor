//! The message-bundle extension — what a host registers.
//!
//! Its model is two things: the [`BundleCatalog`] (what exists) and a usage count per key (where
//! it is read). The second is why the extension takes a project scan at all — "which keys does
//! nothing use" cannot be answered from the file in front of you, and in a legacy app most of
//! the answer is in the pages.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};

use bennu_ext::prelude::{
    ExtEntry, ExtHover, ExtStat, ExtTarget, FileCtx, FrameworkExtension, ProjectScan,
};
use bennu_proto::prelude::{CapabilitySet, CompletionItem, Diagnostic};

use crate::catalog::BundleCatalog;
use crate::refs;

/// How many keys a completion popup offers. Past this the list is not something anyone reads,
/// and a legacy bundle can hold thousands.
const MAX_COMPLETIONS: usize = 200;

/// The extension.
#[derive(Default)]
pub struct MessagesExtension {
    catalog: RwLock<Arc<BundleCatalog>>,
    /// key → how many times the project reads it. Absent means never.
    usage: RwLock<Arc<HashMap<String, usize>>>,
    /// Whether a scan has run. Kept apart from "the catalog has anything in it": a project with
    /// no bundles is *ready and empty*, and reporting it as never-ready leaves the overview
    /// waiting forever.
    scanned: AtomicBool,
}

impl MessagesExtension {
    pub fn new() -> Self {
        Self::default()
    }

    /// The catalog, when there is one worth answering from.
    pub fn resolved(&self) -> Option<Arc<BundleCatalog>> {
        let cat = self.catalog.read().ok()?;
        (!cat.is_empty()).then(|| Arc::clone(&cat))
    }

    fn usage_of(&self, key: &str) -> usize {
        self.usage.read().ok().and_then(|u| u.get(key).copied()).unwrap_or(0)
    }
}

impl FrameworkExtension for MessagesExtension {
    fn id(&self) -> &'static str {
        "messages"
    }

    fn display_name(&self) -> &'static str {
        "Message bundles"
    }

    /// A project that renders text to somebody. Not every project with a `.properties` file: a
    /// library's `logging.properties` is not a bundle, and offering a Messages panel on a build
    /// module is noise. Under-offer rather than over-offer — the documented rule.
    fn applies(&self, caps: &CapabilitySet) -> bool {
        caps.jsp_views
            || caps.struts_xml_config
            || caps.entando_japs
            || caps.spring_xml_di
            || caps.spring_annotation_di
    }

    fn reindex(&self, scan: &ProjectScan<'_>) {
        let files: Vec<(String, String)> = scan
            .resources
            .iter()
            .map(|f| (f.path.to_string_lossy().replace('\\', "/"), f.text.clone()))
            .collect();
        let built = Arc::new(BundleCatalog::build(&files));

        // Every place the project reads a key. Pages first because that is where most of them
        // are; the XML is the validators, the Java is the actions.
        let mut usage: HashMap<String, usize> = HashMap::new();
        for f in scan.pages.iter().chain(scan.xml.iter()).chain(scan.java.iter()) {
            let path = f.path.to_string_lossy().replace('\\', "/");
            for r in refs::keys_in(&path, &f.text) {
                *usage.entry(r.key).or_insert(0) += 1;
            }
        }

        if let Ok(mut slot) = self.catalog.write() {
            *slot = built;
        }
        if let Ok(mut slot) = self.usage.write() {
            *slot = Arc::new(usage);
        }
        self.scanned.store(true, Ordering::Relaxed);
    }

    fn is_ready(&self) -> bool {
        self.scanned.load(Ordering::Relaxed)
    }

    /// A key nothing declares.
    ///
    /// The check that pays for the whole crate: a mistyped key is invisible to the compiler, to
    /// every test, and — because Struts renders the key itself when it cannot resolve one — often
    /// invisible in QA too, right up until somebody notices a screen with `note.login.intro`
    /// written on it.
    ///
    /// Silent when the project has no bundles at all: that means we failed to find them, not that
    /// every key on the page is wrong.
    fn diagnostics(&self, ctx: &FileCtx<'_>) -> Vec<Diagnostic> {
        let Some(cat) = self.resolved() else { return Vec::new() };
        let path = ctx.path_str();
        refs::keys_in(&path, ctx.source)
            .into_iter()
            .filter(|r| !cat.knows(&r.key))
            .map(|r| Diagnostic {
                message: format!("no bundle declares `{}`", r.key),
                severity: "warning".to_string(),
                code: "messages.unknown-key".to_string(),
                start: r.start,
                end: r.end,
            })
            .collect()
    }

    /// Every file that declares this key, one target per locale.
    fn navigate(&self, ctx: &FileCtx<'_>, offset: usize) -> Vec<ExtTarget> {
        let Some(cat) = self.resolved() else { return Vec::new() };
        let path = ctx.path_str();
        let Some(r) = refs::key_at(&path, ctx.source, offset) else { return Vec::new() };
        cat.declarations(&r.key)
            .into_iter()
            .map(|d| ExtTarget {
                file: d.bundle.path.clone(),
                offset: d.entry.start,
                label: d.bundle.file_name().to_string(),
                detail: d.entry.value.clone(),
            })
            .collect()
    }

    /// What the key actually says, in every language it says it in — the question you open the
    /// bundle to answer, asked without leaving the page.
    fn hover(&self, ctx: &FileCtx<'_>, offset: usize) -> Option<ExtHover> {
        let cat = self.resolved()?;
        let path = ctx.path_str();
        let r = refs::key_at(&path, ctx.source, offset)?;
        let decls = cat.declarations(&r.key);
        if decls.is_empty() {
            return Some(ExtHover {
                title: r.key.clone(),
                signature: "not declared by any bundle".to_string(),
                doc: String::new(),
            });
        }
        let signature = decls[0].entry.value.clone();
        let mut doc: Vec<String> = decls
            .iter()
            .map(|d| format!("{} — {}", d.bundle.locale_label(), d.entry.value))
            .collect();
        let owed = cat.untranslated(&r.key);
        if !owed.is_empty() {
            doc.push(format!("untranslated in {}", owed.join(", ")));
        }
        Some(ExtHover { title: r.key, signature, doc: doc.join("\n") })
    }

    /// The keys that continue what is being typed inside a key attribute.
    fn completions(&self, ctx: &FileCtx<'_>, offset: usize) -> Vec<CompletionItem> {
        let Some(cat) = self.resolved() else { return Vec::new() };
        let path = ctx.path_str();
        let Some(prefix) = refs::key_prefix_at(&path, ctx.source, offset) else {
            return Vec::new();
        };
        cat.keys()
            .into_iter()
            .filter(|k| k.starts_with(&prefix))
            .take(MAX_COMPLETIONS)
            .map(|k| CompletionItem {
                label: k.to_string(),
                kind: "text".to_string(),
                detail: cat.declarations(k).first().map(|d| d.entry.value.clone()),
                auto_import: None,
                ..Default::default()
            })
            .collect()
    }

    /// `keys` — one row per key, expanding into its translations.
    fn catalog(&self, kind: &str) -> Vec<ExtEntry> {
        let Some(cat) = (kind == "keys").then(|| self.resolved()).flatten() else {
            return Vec::new();
        };
        cat.keys()
            .into_iter()
            .map(|key| {
                let decls = cat.declarations(key);
                let first = decls.first();
                let uses = self.usage_of(key);
                let mut tags = Vec::new();
                match uses {
                    0 => tags.push("unused".to_string()),
                    1 => tags.push("1 use".to_string()),
                    n => tags.push(format!("{n} uses")),
                }
                let owed = cat.untranslated(key);
                if !owed.is_empty() {
                    tags.push(format!("missing {}", owed.join(", ")));
                }
                ExtEntry {
                    id: key.to_string(),
                    primary: key.to_string(),
                    secondary: first.map(|d| d.entry.value.clone()).unwrap_or_default(),
                    kind: first.map(|d| d.bundle.base.clone()).unwrap_or_default(),
                    file: first.map(|d| d.bundle.path.clone()),
                    offset: first.map(|d| d.entry.start),
                    line: first.map(|d| d.entry.line),
                    tags,
                    children: decls
                        .iter()
                        .map(|d| ExtEntry {
                            id: format!("{key}@{}", d.bundle.path),
                            primary: d.bundle.locale_label().to_string(),
                            secondary: d.entry.value.clone(),
                            kind: "locale".to_string(),
                            file: Some(d.bundle.path.clone()),
                            offset: Some(d.entry.start),
                            line: Some(d.entry.line),
                            ..ExtEntry::default()
                        })
                        .collect(),
                }
            })
            .collect()
    }

    fn stats(&self) -> Vec<ExtStat> {
        let count = self.resolved().map_or(0, |c| c.key_count());
        vec![ExtStat {
            label: "Message keys".to_string(),
            value: count,
            catalog: Some("keys".to_string()),
        }]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bennu_ext::prelude::ScannedFile;
    use std::path::{Path, PathBuf};

    fn file(path: &str, text: &str) -> ScannedFile {
        ScannedFile { path: PathBuf::from(path), text: text.to_string() }
    }

    fn ext() -> MessagesExtension {
        let ext = MessagesExtension::new();
        let resources = [
            file("/p/messages.properties", "login.title=Sign in\nnever.used=x\nonly.en=y\n"),
            file("/p/messages_it.properties", "login.title=Accedi\nnever.used=x\n"),
        ];
        let pages = [file("/p/login.jsp", "<s:text name=\"login.title\"/><fmt:message key=\"gone.missing\"/>")];
        ext.reindex(&ProjectScan {
            resources: &resources,
            pages: &pages,
            ..ProjectScan::empty(Path::new("/p"))
        });
        ext
    }

    fn ctx<'a>(path: &'a Path, source: &'a str) -> FileCtx<'a> {
        FileCtx { path, source }
    }

    #[test]
    fn a_key_no_bundle_declares_is_reported_where_it_is_written() {
        let src = "<s:text name=\"login.title\"/><fmt:message key=\"gone.missing\"/>";
        let p = PathBuf::from("/p/login.jsp");
        let d = ext().diagnostics(&ctx(&p, src));
        assert_eq!(d.len(), 1, "only the key nothing declares");
        assert_eq!(&src[d[0].start..d[0].end], "gone.missing");
        assert_eq!(d[0].code, "messages.unknown-key");
    }

    #[test]
    fn go_to_offers_one_target_per_translation() {
        let src = "<s:text name=\"login.title\"/>";
        let p = PathBuf::from("/p/login.jsp");
        let t = ext().navigate(&ctx(&p, src), src.find("login.title").unwrap() + 2);
        assert_eq!(t.len(), 2);
        assert_eq!(t[0].detail, "Sign in");
        assert_eq!(t[1].label, "messages_it.properties");
    }

    #[test]
    fn hover_says_what_the_key_reads_in_every_language() {
        let src = "<s:text name=\"login.title\"/>";
        let p = PathBuf::from("/p/login.jsp");
        let h = ext().hover(&ctx(&p, src), src.find("login.title").unwrap() + 2).unwrap();
        assert_eq!(h.title, "login.title");
        assert_eq!(h.signature, "Sign in");
        assert!(h.doc.contains("it — Accedi"));
    }

    #[test]
    fn hover_on_an_untranslated_key_says_who_owes_it() {
        let src = "<fmt:message key=\"only.en\"/>";
        let p = PathBuf::from("/p/x.jsp");
        let h = ext().hover(&ctx(&p, src), src.find("only.en").unwrap() + 2).unwrap();
        assert!(h.doc.contains("untranslated in it"), "doc was `{}`", h.doc);
    }

    #[test]
    fn the_catalog_counts_uses_and_names_the_translations() {
        let rows = ext().catalog("keys");
        let used = rows.iter().find(|r| r.id == "login.title").unwrap();
        assert_eq!(used.secondary, "Sign in");
        assert_eq!(used.tags, ["1 use"]);
        assert_eq!(used.children.len(), 2, "one row per locale");

        let dead = rows.iter().find(|r| r.id == "never.used").unwrap();
        assert_eq!(dead.tags, ["unused"]);

        let partial = rows.iter().find(|r| r.id == "only.en").unwrap();
        assert_eq!(partial.tags, ["unused", "missing it"]);
    }

    #[test]
    fn completion_offers_the_keys_that_continue_the_prefix() {
        let src = "<fmt:message key=\"lo\"/>";
        let p = PathBuf::from("/p/x.jsp");
        let items = ext().completions(&ctx(&p, src), src.find("lo").unwrap() + 2);
        assert_eq!(items.iter().map(|i| i.label.as_str()).collect::<Vec<_>>(), ["login.title"]);
        assert_eq!(items[0].detail.as_deref(), Some("Sign in"));
    }

    #[test]
    fn a_project_with_no_bundles_says_nothing_at_all() {
        let ext = MessagesExtension::new();
        ext.reindex(&ProjectScan::empty(Path::new("/p")));
        let src = "<fmt:message key=\"anything\"/>";
        let p = PathBuf::from("/p/x.jsp");
        assert!(ext.is_ready(), "an empty project is ready, not pending");
        assert!(ext.diagnostics(&ctx(&p, src)).is_empty(), "no bundles means no verdict");
        assert!(ext.catalog("keys").is_empty());
    }
}
