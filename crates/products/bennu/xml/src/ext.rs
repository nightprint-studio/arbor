//! `XmlExtension` — the [`FrameworkExtension`] implementation.
//!
//! ## It applies everywhere
//!
//! Unlike the other extensions, this one is not gated on a capability, and that is deliberate:
//! there is no "this is an XML project" bit, and there does not need to be. **The grammar is the
//! gate.** A document whose schema nobody could resolve gets nothing at all, and that answer
//! costs one scan and one map lookup — so the extension is free where it has nothing to say, and
//! a capability check would only be a second, less accurate way of asking the same question.
//!
//! ## Grammars are cached by what the document asks for
//!
//! Not by file path. Two hundred `*.hbm.xml` files name the same DTD, and parsing it two hundred
//! times — per keystroke — would be the whole cost of the feature. The key is the document's own
//! *request*: its `DOCTYPE` system id plus its `xsi:schemaLocation`s, plus the directory it sits
//! in (which is what a relative location resolves against).
//!
//! That key also invalidates itself correctly: change the `schemaLocation` and the key changes,
//! so the next answer is computed against the new schema without anything having to notice.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};

use bennu_ext::prelude::{
    ExtEntry, ExtHover, ExtStat, ExtTarget, FileCtx, FrameworkExtension, ProjectScan,
};
use bennu_proto::prelude::{CapabilitySet, CompletionItem, Diagnostic};

use crate::catalog::{Catalog, SchemaFile};
use crate::grammar::Grammar;
use crate::intel;
use crate::scan::{scan, Scan};

/// The XML schema extension.
pub struct XmlExtension {
    catalog: RwLock<Arc<Catalog>>,
    /// Request key → the grammar it resolves to. `None` is cached too: "we already looked and
    /// there is nothing" is the common answer and the one worth not re-deriving.
    grammars: RwLock<HashMap<String, Option<Arc<Grammar>>>>,
    ready: AtomicBool,
}

impl Default for XmlExtension {
    fn default() -> Self {
        Self::new()
    }
}

impl XmlExtension {
    pub fn new() -> Self {
        Self {
            catalog: RwLock::new(Arc::new(Catalog::default())),
            grammars: RwLock::new(HashMap::new()),
            ready: AtomicBool::new(false),
        }
    }

    /// The grammar a buffer is written against, resolved once per distinct request.
    pub fn grammar(&self, path: &str, doc: &Scan) -> Option<Arc<Grammar>> {
        let key = request_key(path, doc);
        if let Ok(cache) = self.grammars.read() {
            if let Some(hit) = cache.get(&key) {
                return hit.clone();
            }
        }
        let catalog = self.catalog.read().ok().map(|c| Arc::clone(&c))?;
        let resolved = catalog.grammar_for(path, doc).map(Arc::new);
        if let Ok(mut cache) = self.grammars.write() {
            cache.insert(key, resolved.clone());
        }
        resolved
    }

    /// Everything an editor answer needs: the scan of this buffer and its grammar. `None` when
    /// the file is not XML, or when nothing describes it — which is the same answer, because in
    /// both cases the honest contribution is nothing.
    fn context(&self, ctx: &FileCtx<'_>) -> Option<(Scan, Arc<Grammar>)> {
        if !is_xml(&ctx.extension()) {
            return None;
        }
        let path = ctx.path_str();
        let doc = scan(ctx.source);
        let grammar = self.grammar(&path, &doc)?;
        (!grammar.is_empty()).then_some((doc, grammar))
    }
}

/// The extensions worth scanning. `.xml` and the handful of Java-ecosystem files that are XML
/// under a different name — a `.tld` or a `.jspx` declares a schema exactly like an `.xml` does,
/// and refusing it on the strength of its extension would be pedantry.
fn is_xml(extension: &str) -> bool {
    matches!(
        extension,
        "xml" | "xsd" | "xsl" | "xslt" | "xhtml" | "jspx" | "tld" | "tagx" | "pom" | "wsdl"
            | "iml" | "fxml"
    )
}

/// What this document asked for, as a cache key.
fn request_key(path: &str, doc: &Scan) -> String {
    let dir = path.replace('\\', "/").rsplit_once('/').map(|(d, _)| d.to_string()).unwrap_or_default();
    let doctype = doc.doctype.as_ref().map(|d| d.system_id.as_str()).unwrap_or_default();
    let locations: Vec<&str> = doc
        .tags
        .iter()
        .find(|t| t.kind != crate::scan::TagKind::Close)
        .map(|root| {
            root.attrs
                .iter()
                .filter(|a| a.local().ends_with("schemaLocation"))
                .map(|a| a.value.as_str())
                .collect()
        })
        .unwrap_or_default();
    // The root element is part of the key because the built-in fallback keys off it: a POM and a
    // Struts config that both name no schema are not the same request.
    format!("{dir}\u{1}{doctype}\u{1}{}\u{1}{}", locations.join(" "), doc.root().unwrap_or_default())
}

impl FrameworkExtension for XmlExtension {
    fn id(&self) -> &'static str {
        "xml"
    }

    fn display_name(&self) -> &'static str {
        "XML Schema"
    }

    /// Always. See the module docs: the grammar is the gate, and it is a better one than any
    /// capability bit could be.
    fn applies(&self, _caps: &CapabilitySet) -> bool {
        true
    }

    fn reindex(&self, project: &ProjectScan<'_>) {
        let files = project
            .schemas
            .iter()
            .map(|f| SchemaFile { path: f.path.to_string_lossy().replace('\\', "/"), text: f.text.clone() })
            .collect();
        if let Ok(mut slot) = self.catalog.write() {
            *slot = Arc::new(Catalog::new(files));
        }
        // The schemas may have changed under them, so every cached answer is suspect.
        if let Ok(mut cache) = self.grammars.write() {
            cache.clear();
        }
        self.ready.store(true, Ordering::Release);
    }

    fn is_ready(&self) -> bool {
        self.ready.load(Ordering::Acquire)
    }

    fn diagnostics(&self, ctx: &FileCtx<'_>) -> Vec<Diagnostic> {
        match self.context(ctx) {
            Some((doc, grammar)) => intel::diagnostics(&grammar, &doc),
            None => Vec::new(),
        }
    }

    fn completions(&self, ctx: &FileCtx<'_>, offset: usize) -> Vec<CompletionItem> {
        match self.context(ctx) {
            Some((doc, grammar)) => intel::completions(&grammar, &doc, ctx.source, offset),
            None => Vec::new(),
        }
    }

    fn inline_hint(&self, ctx: &FileCtx<'_>, offset: usize) -> Option<String> {
        let (doc, grammar) = self.context(ctx)?;
        intel::inline_hint(&grammar, &doc, ctx.source, offset)
    }

    fn hover(&self, ctx: &FileCtx<'_>, offset: usize) -> Option<ExtHover> {
        let (doc, grammar) = self.context(ctx)?;
        intel::hover(&grammar, &doc, ctx.source, offset)
    }

    fn navigate(&self, ctx: &FileCtx<'_>, offset: usize) -> Vec<ExtTarget> {
        match self.context(ctx) {
            Some((doc, grammar)) => intel::navigate(&grammar, &doc, ctx.source, offset),
            None => Vec::new(),
        }
    }

    /// The schemas this project can resolve against — the answer to "why is my file not being
    /// checked", which is otherwise invisible.
    fn catalog(&self, kind: &str) -> Vec<ExtEntry> {
        if kind != "schemas" {
            return Vec::new();
        }
        let Ok(cache) = self.grammars.read() else { return Vec::new() };
        let mut out: Vec<ExtEntry> = Vec::new();
        for grammar in cache.values().flatten() {
            if out.iter().any(|e| e.id == grammar.source) {
                continue;
            }
            out.push(ExtEntry {
                id: grammar.source.clone(),
                primary: grammar.source.rsplit('/').next().unwrap_or(&grammar.source).to_string(),
                secondary: grammar.source.clone(),
                kind: grammar.kind.map(|k| k.label().to_string()).unwrap_or_default(),
                tags: vec![format!("{} elements", grammar.elements.len())],
                ..ExtEntry::default()
            });
        }
        out.sort_by(|a, b| a.primary.cmp(&b.primary));
        out
    }

    fn stats(&self) -> Vec<ExtStat> {
        let available = self.catalog.read().map(|c| c.len()).unwrap_or(0);
        vec![ExtStat {
            label: "Schemas".into(),
            value: available,
            catalog: Some("schemas".into()),
        }]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bennu_ext::prelude::ScannedFile;
    use std::path::{Path, PathBuf};

    const DTD: &str = "<!ELEMENT struts (package*)>\n<!ELEMENT package EMPTY>\n\
                       <!ATTLIST package name CDATA #REQUIRED>";

    fn indexed(schemas: &[(&str, &str)]) -> XmlExtension {
        let files: Vec<ScannedFile> = schemas
            .iter()
            .map(|(p, t)| ScannedFile { path: PathBuf::from(p), text: t.to_string() })
            .collect();
        let ext = XmlExtension::new();
        ext.reindex(&ProjectScan {
            root: Path::new("/p"),
            java: &[],
            xml: &[],
            resources: &[],
            pages: &[],
            schemas: &files,
            descriptors: &[],
            taglibs: &[],
        });
        ext
    }

    const DOC: &str = "<!DOCTYPE struts SYSTEM \"http://struts.apache.org/dtds/struts-2.5.dtd\">\n\
                       <struts><package/></struts>";

    #[test]
    fn a_document_whose_schema_is_in_a_jar_is_answered_for() {
        let ext = indexed(&[("struts2-core.jar!/struts-2.5.dtd", DTD)]);
        let ctx = FileCtx { path: Path::new("/p/struts.xml"), source: DOC };
        assert_eq!(
            ext.diagnostics(&ctx).into_iter().map(|d| d.message).collect::<Vec<_>>(),
            ["`package` requires the attribute `name`"],
        );
        let at = DOC.find("<package").unwrap() + 2;
        assert_eq!(ext.hover(&ctx, at).unwrap().title, "package");
        assert_eq!(ext.navigate(&ctx, at).len(), 1);
    }

    /// The gate that replaces a capability check: no schema, no answers, no cost.
    #[test]
    fn a_document_with_no_resolvable_schema_is_left_entirely_alone() {
        let ext = indexed(&[]);
        let ctx = FileCtx { path: Path::new("/p/struts.xml"), source: DOC };
        assert!(ext.diagnostics(&ctx).is_empty());
        assert!(ext.completions(&ctx, 80).is_empty());
        assert!(ext.hover(&ctx, 80).is_none());
        assert!(ext.inline_hint(&ctx, 80).is_none());
    }

    #[test]
    fn a_non_xml_file_is_never_answered_for() {
        let ext = indexed(&[("struts2-core.jar!/struts-2.5.dtd", DTD)]);
        let ctx = FileCtx { path: Path::new("/p/Notes.java"), source: DOC };
        assert!(ext.diagnostics(&ctx).is_empty());
        assert!(ext.navigate(&ctx, 80).is_empty());
        // But the XML-under-another-name files are.
        let ctx = FileCtx { path: Path::new("/p/tags.tld"), source: DOC };
        assert!(!ext.diagnostics(&ctx).is_empty());
    }

    /// Two hundred `*.hbm.xml` naming the same DTD must parse it once, not two hundred times per
    /// keystroke.
    #[test]
    fn documents_asking_for_the_same_schema_share_one_grammar() {
        let ext = indexed(&[("struts2-core.jar!/struts-2.5.dtd", DTD)]);
        let a = ext.grammar("/p/a.xml", &scan(DOC)).unwrap();
        let b = ext.grammar("/p/b.xml", &scan(DOC)).unwrap();
        assert!(Arc::ptr_eq(&a, &b), "same request, same grammar");
        assert_eq!(ext.catalog("schemas").len(), 1);
    }

    #[test]
    fn changing_what_a_document_asks_for_changes_the_answer() {
        let ext = indexed(&[
            ("struts2-core.jar!/struts-2.5.dtd", DTD),
            ("other.jar!/other.dtd", "<!ELEMENT other EMPTY>"),
        ]);
        let first = ext.grammar("/p/a.xml", &scan(DOC)).unwrap();
        let switched = DOC.replace("struts-2.5.dtd", "other.dtd");
        let second = ext.grammar("/p/a.xml", &scan(&switched)).unwrap();
        assert!(!Arc::ptr_eq(&first, &second));
        assert!(second.element("other").is_some());
    }

    #[test]
    fn the_builtin_pom_answers_where_nothing_ships_a_schema() {
        let ext = indexed(&[]);
        // The caret immediately after a `<` typed inside `<project>`.
        let pom = "<project><modelVersion>4.0.0</modelVersion><";
        let ctx = FileCtx { path: Path::new("/p/pom.xml"), source: pom };
        let labels: Vec<String> =
            ext.completions(&ctx, pom.len()).into_iter().map(|c| c.label).collect();
        assert!(labels.contains(&"dependencies".to_string()), "got: {labels:?}");
    }
}
