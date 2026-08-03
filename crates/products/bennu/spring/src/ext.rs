//! `SpringExtension` — the [`FrameworkExtension`] implementation.
//!
//! Owns the model, decides which files are worth parsing, and routes each editor question
//! to the Java or the XML side.
//!
//! ## Which files get parsed
//!
//! Not all of them. A legacy tree has north of a thousand Java sources and almost none of
//! them mention Spring; parsing every one on every index pass would make this extension
//! the most expensive thing in the backend for no gain. The selection is three rounds:
//!
//! 1. **Spring-relevant** files — a cheap `contains` test for `@Service`, `@Value`,
//!    `springframework` and friends ([`looks_spring_relevant`]).
//! 2. **Classes named by XML** — `<bean class="com.acme.Foo"/>` needs `Foo`'s writable
//!    properties, and `Foo` may be a plain POJO with no annotation in it. Its file is
//!    found by simple name.
//! 3. **One round of supertypes** — a class selected above may extend a base class that
//!    was not, and an unresolved supertype turns off the `<property name=>` check for the
//!    whole hierarchy. One extra round recovers the common `Foo extends AbstractFoo`
//!    case; deeper chains stay unresolved, which is the safe direction (the check goes
//!    quiet rather than guessing).

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};

use bennu_ext::prelude::{
    ExtEntry, ExtGutterMark, ExtHighlight, ExtHover, ExtStat, ExtTarget, FileCtx,
    FrameworkExtension, ProjectScan, ScannedFile,
};
use bennu_proto::prelude::{CapabilitySet, CompletionItem, Diagnostic};

use crate::beans::JavaUnit;
use crate::model::{simple_name, strip_generics, SpringModel};
use crate::props::{parse_property_file, PropertySources};
use crate::scan::{looks_spring_relevant, scan_java};
use crate::xml::parse_bean_xml;
use crate::{java_intel, props_intel, xml_intel};

/// The Spring framework extension.
pub struct SpringExtension {
    model: RwLock<Arc<SpringModel>>,
    ready: AtomicBool,
    /// The property file the user pinned as the one to resolve against. Kept outside the
    /// model so setting it does not require a reindex — it is a display choice, not new
    /// information about the project.
    active_property_file: RwLock<Option<String>>,
}

impl Default for SpringExtension {
    fn default() -> Self {
        Self::new()
    }
}

impl SpringExtension {
    pub fn new() -> Self {
        Self {
            model: RwLock::new(Arc::new(SpringModel::default())),
            ready: AtomicBool::new(false),
            active_property_file: RwLock::new(None),
        }
    }

    /// The current model. Cheap (`Arc` clone) and lock-free for the caller, so a query
    /// never holds the lock while it works.
    pub fn model(&self) -> Arc<SpringModel> {
        self.model.read().map(|m| Arc::clone(&m)).unwrap_or_default()
    }

    /// Pin the property file that answers first (see [`PropertySources`]). Rebuilds only
    /// the property view of the model, in place — no rescan.
    pub fn set_active_property_file(&self, path: Option<String>) {
        if let Ok(mut slot) = self.active_property_file.write() {
            *slot = path.filter(|p| !p.is_empty());
        }
        let current = self.model();
        let files = current.props.files().to_vec();
        let active = self.active_property_file.read().ok().and_then(|p| p.clone());
        // Only the property view changes; everything else is carried over as it is. A
        // stale pin (a file that has since disappeared) is ignored by `with_active`
        // rather than clearing the user's choice.
        let next = SpringModel {
            beans: current.beans.clone(),
            endpoints: current.endpoints.clone(),
            injections: current.injections.clone(),
            props: PropertySources::new(files).with_active(active.as_deref()),
            xml_files: current.xml_files.clone(),
            types: current.types.clone(),
            simple_names: current.simple_names.clone(),
            config_bindings: current.config_bindings.clone(),
            property_usages: current.property_usages.clone(),
            metadata: current.metadata.clone(),
        };
        if let Ok(mut slot) = self.model.write() {
            *slot = Arc::new(next);
        }
    }

    /// The pinned property file, if any.
    pub fn active_property_file(&self) -> Option<String> {
        self.active_property_file.read().ok().and_then(|p| p.clone())
    }
}

impl FrameworkExtension for SpringExtension {
    fn id(&self) -> &'static str {
        "spring"
    }

    fn display_name(&self) -> &'static str {
        "Spring"
    }

    fn applies(&self, caps: &CapabilitySet) -> bool {
        caps.spring_xml_di || caps.spring_annotation_di || caps.spring_data_repo
    }

    fn reindex(&self, scan: &ProjectScan<'_>) {
        // XML first: it decides which extra Java files are worth parsing.
        let xml_files: Vec<_> = scan
            .xml
            .iter()
            .filter_map(|f| parse_bean_xml(&f.path.to_string_lossy(), &f.text))
            .collect();

        let units = select_and_scan(scan.java, &xml_files);

        let mut beans = crate::beans::annotation_beans(&units);
        beans.extend(crate::beans::xml_beans(&xml_files));
        let types = crate::beans::type_index(&units);

        let mut simple_names: std::collections::BTreeMap<String, Vec<String>> = Default::default();
        for fqcn in types.keys() {
            simple_names.entry(simple_name(fqcn).to_string()).or_default().push(fqcn.clone());
        }

        let props = PropertySources::new(
            scan.resources
                .iter()
                .filter_map(|f| parse_property_file(&f.path.to_string_lossy(), &f.text))
                .collect(),
        )
        .with_active(self.active_property_file().as_deref());

        let config_bindings = crate::config_props::bindings(&units);
        let property_usages =
            crate::usages::property_usages(&units, &xml_files, &config_bindings);
        let model = SpringModel {
            metadata: build_metadata(scan.descriptors),
            endpoints: crate::endpoints::endpoints(&units),
            injections: crate::beans::injection_points(&units),
            config_bindings,
            property_usages,
            beans,
            props,
            xml_files,
            types,
            simple_names,
        };
        if let Ok(mut slot) = self.model.write() {
            *slot = Arc::new(model);
        }
        self.ready.store(true, Ordering::Release);
    }

    fn is_ready(&self) -> bool {
        self.ready.load(Ordering::Acquire)
    }

    fn diagnostics(&self, ctx: &FileCtx<'_>) -> Vec<Diagnostic> {
        let model = self.model();
        match ctx.extension().as_str() {
            "java" => java_intel::diagnostics(&model, &ctx.path_str(), ctx.source),
            "xml" if xml_intel::is_bean_xml(ctx.source) => {
                xml_intel::diagnostics(&model, &ctx.path_str(), ctx.source)
            }
            _ => Vec::new(),
        }
    }

    fn highlights(&self, ctx: &FileCtx<'_>) -> Vec<ExtHighlight> {
        let path = ctx.path_str();
        match ctx.extension().as_str() {
            "java" => java_intel::highlights(&path, ctx.source),
            "xml" if xml_intel::is_bean_xml(ctx.source) => {
                xml_intel::highlights(&path, ctx.source)
            }
            // `${…}` in a yaml value is the same expression as `${…}` in a `@Value`, and
            // reading it as prose is how a typo in one survives. Same pass, same colours.
            _ if props_intel::is_property_source(&path) => {
                props_intel::highlights(&path, ctx.source)
            }
            _ => Vec::new(),
        }
    }

    fn completions(&self, ctx: &FileCtx<'_>, offset: usize) -> Vec<CompletionItem> {
        let model = self.model();
        let path = ctx.path_str();
        match ctx.extension().as_str() {
            "java" => java_intel::completions(&model, &path, ctx.source, offset),
            "xml" if xml_intel::is_bean_xml(ctx.source) => {
                xml_intel::completions(&model, ctx.source, offset)
            }
            _ if props_intel::is_property_source(&path) => {
                props_intel::completions(&model, &path, ctx.source, offset)
            }
            _ => Vec::new(),
        }
    }

    fn hover(&self, ctx: &FileCtx<'_>, offset: usize) -> Option<ExtHover> {
        let model = self.model();
        let path = ctx.path_str();
        match ctx.extension().as_str() {
            "java" => java_intel::hover(&model, &path, ctx.source, offset),
            "xml" if xml_intel::is_bean_xml(ctx.source) => {
                xml_intel::hover(&model, ctx.source, offset)
            }
            _ if props_intel::is_property_source(&path) => {
                props_intel::hover(&model, &path, ctx.source, offset)
            }
            _ => None,
        }
    }

    /// Ghost text, and only in a property file: a documented default for a key left empty, or
    /// the one continuation a prefix can have. Java and XML have nothing certain to add here.
    fn inline_hint(&self, ctx: &FileCtx<'_>, offset: usize) -> Option<String> {
        let path = ctx.path_str();
        if !props_intel::is_property_source(&path) {
            return None;
        }
        props_intel::inline_hint(&self.model(), &path, ctx.source, offset)
    }

    fn navigate(&self, ctx: &FileCtx<'_>, offset: usize) -> Vec<ExtTarget> {
        let model = self.model();
        let path = ctx.path_str();
        match ctx.extension().as_str() {
            "java" => java_intel::navigate(&model, &path, ctx.source, offset),
            "xml" if xml_intel::is_bean_xml(ctx.source) => {
                xml_intel::navigate(&model, ctx.source, offset)
            }
            _ if props_intel::is_property_source(&path) => {
                props_intel::navigate(&model, &path, ctx.source, offset)
            }
            _ => Vec::new(),
        }
    }

    fn gutter(&self, ctx: &FileCtx<'_>) -> Vec<ExtGutterMark> {
        let model = self.model();
        let path = ctx.path_str();
        match ctx.extension().as_str() {
            "java" => java_intel::gutter(&model, &path, ctx.source),
            "xml" if xml_intel::is_bean_xml(ctx.source) => {
                xml_intel::gutter(&model, &path, ctx.source)
            }
            // An `application*.yml` gets a mark per key something reads, the count as its
            // glyph — so a legacy config file shows at a glance which lines still matter.
            _ if props_intel::is_property_source(&path) => {
                props_intel::gutter(&model, &path, ctx.source)
            }
            _ => Vec::new(),
        }
    }

    fn catalog(&self, kind: &str) -> Vec<ExtEntry> {
        let m = self.model();
        match kind {
            "beans" => m
                .beans
                .iter()
                .map(|b| ExtEntry {
                    id: b.name.clone(),
                    primary: b.name.clone(),
                    secondary: b.fqcn.clone(),
                    kind: b.stereotype.clone(),
                    file: Some(b.file.clone()),
                    offset: Some(b.offset),
                    line: Some(b.line),
                    tags: bean_tags(b),
                    children: Vec::new(),
                })
                .collect(),
            "endpoints" => m
                .endpoints
                .iter()
                .map(|e| {
                    let mut tags = vec![simple_name(&strip_generics(&e.return_type)).to_string()];
                    if !e.produces.is_empty() {
                        tags.push(e.produces.clone());
                    }
                    ExtEntry {
                        id: e.label(),
                        primary: e.path.clone(),
                        secondary: format!("{}#{}", simple_name(&e.class_fqcn), e.handler),
                        kind: if e.methods.is_empty() {
                            "ANY".into()
                        } else {
                            e.methods.join("|")
                        },
                        file: Some(e.file.clone()),
                        offset: Some(e.offset),
                        line: Some(e.line),
                        tags,
                        // One child per parameter, so the panel can expand a route into what
                        // it actually takes without a second request or a second panel.
                        children: e
                            .params
                            .iter()
                            .map(|p| ExtEntry {
                                id: p.name.clone(),
                                primary: p.effective_name().to_string(),
                                secondary: p.type_text.clone(),
                                kind: p.binding.clone(),
                                tags: if p.required {
                                    Vec::new()
                                } else {
                                    vec!["optional".to_string()]
                                },
                                ..ExtEntry::default()
                            })
                            .collect(),
                    }
                })
                .collect(),
            // Every `@ConfigurationProperties`-bound field, keyed by the path it binds — the
            // list you want beside a yaml when you are trying to remember what is bindable.
            "bindings" => m
                .config_bindings
                .iter()
                .map(|b| ExtEntry {
                    id: b.path.clone(),
                    primary: b.path.clone(),
                    secondary: format!("{}.{}", simple_name(&b.owner_fqcn), b.field),
                    kind: simple_name(&strip_generics(&b.type_text)).to_string(),
                    file: None,
                    offset: None,
                    line: None,
                    tags: match m.props.lookup(&b.path) {
                        Some((_, e)) => vec![e.value.clone()],
                        None => Vec::new(),
                    },
                    children: Vec::new(),
                })
                .collect(),
            // The vocabulary itself: every property Spring and the project's libraries accept,
            // whether or not this project sets it. The reference you would otherwise have open
            // in a browser tab, version-matched to the jars actually on the classpath.
            "documented" => m
                .metadata
                .all()
                .map(|p| ExtEntry {
                    id: p.name.clone(),
                    primary: p.name.clone(),
                    secondary: p.description.clone(),
                    kind: simple_name(&strip_generics(&p.type_text)).to_string(),
                    file: None,
                    offset: None,
                    line: None,
                    tags: {
                        let mut tags = Vec::new();
                        if p.deprecation.is_some() {
                            tags.push("deprecated".to_string());
                        }
                        if !p.default_value.is_empty() {
                            tags.push(format!("= {}", p.default_value));
                        }
                        // Whether this project actually sets it — the reason to browse the
                        // list beside a config file rather than in a browser.
                        if m.props.lookup(&p.name).is_some() {
                            tags.push("set".to_string());
                        }
                        tags
                    },
                    children: Vec::new(),
                })
                .collect(),
            "properties" => m
                .props
                .files()
                .iter()
                .flat_map(|f| {
                    f.entries.iter().map(move |e| ExtEntry {
                        id: e.key.clone(),
                        primary: e.key.clone(),
                        secondary: e.value.clone(),
                        kind: f.name.clone(),
                        file: Some(f.path.clone()),
                        offset: Some(e.key_start),
                        line: Some(e.line),
                        tags: if f.profile.is_empty() {
                            Vec::new()
                        } else {
                            vec![f.profile.clone()]
                        },
                        children: Vec::new(),
                    })
                })
                .collect(),
            "property-files" => m
                .props
                .files()
                .iter()
                .map(|f| ExtEntry {
                    id: f.path.clone(),
                    primary: f.name.clone(),
                    secondary: f.path.clone(),
                    kind: if f.profile.is_empty() { "base".into() } else { f.profile.clone() },
                    file: Some(f.path.clone()),
                    offset: Some(0),
                    line: Some(1),
                    tags: (m.props.active_path() == Some(f.path.as_str()))
                        .then(|| vec!["active".to_string()])
                        .unwrap_or_default(),
                    children: Vec::new(),
                })
                .collect(),
            "injections" => m
                .injections
                .iter()
                .map(|i| ExtEntry {
                    id: format!("{}.{}", i.owner_fqcn, i.member),
                    primary: format!("{}.{}", simple_name(&i.owner_fqcn), i.member),
                    secondary: i.type_text.clone(),
                    kind: i.kind.as_str().to_string(),
                    file: Some(i.file.clone()),
                    offset: Some(i.offset),
                    line: Some(i.line),
                    tags: if i.qualifier.is_empty() {
                        Vec::new()
                    } else {
                        vec![format!("@Qualifier({})", i.qualifier)]
                    },
                    children: Vec::new(),
                })
                .collect(),
            _ => Vec::new(),
        }
    }

    fn stats(&self) -> Vec<ExtStat> {
        let m = self.model();
        vec![
            ExtStat { label: "Beans".into(), value: m.beans.len(), catalog: Some("beans".into()) },
            ExtStat {
                label: "Endpoints".into(),
                value: m.endpoints.len(),
                catalog: Some("endpoints".into()),
            },
            ExtStat {
                label: "Injection points".into(),
                value: m.injections.len(),
                catalog: Some("injections".into()),
            },
            ExtStat {
                label: "Properties".into(),
                value: m.props.entry_count(),
                catalog: Some("properties".into()),
            },
            ExtStat {
                label: "Bound properties".into(),
                value: m.config_bindings.len(),
                catalog: Some("bindings".into()),
            },
            ExtStat {
                label: "Documented keys".into(),
                value: m.metadata.len(),
                catalog: Some("documented".into()),
            },
        ]
    }
}

/// Fold every `spring-configuration-metadata.json` the host found into one index.
///
/// The **curated table goes in last**, and that ordering is the whole policy: `absorb` keeps
/// the first description of a key, so a real jar always beats the stand-in, and the stand-in
/// only fills the gaps left when few jars (or none) were resolved. A project with a complete
/// classpath therefore never sees the hand-written table at all.
fn build_metadata(descriptors: &[ScannedFile]) -> crate::metadata::MetadataIndex {
    let mut idx = crate::metadata::MetadataIndex::default();
    for f in descriptors {
        let path = f.path.to_string_lossy().replace('\\', "/");
        if !crate::metadata::is_metadata_path(&path) {
            continue;
        }
        // The jar name is what a hover should say ("described by spring-boot-actuator"), not
        // the whole `…!/META-INF/…` identity.
        let origin = path
            .split_once("!/")
            .map(|(jar, _)| jar.rsplit('/').next().unwrap_or(jar))
            .unwrap_or("this project")
            .to_string();
        idx.absorb(&f.text, &origin);
    }
    idx.merge_defaults(crate::metadata::builtin_index());
    idx
}

fn bean_tags(b: &crate::model::BeanDef) -> Vec<String> {
    let mut tags = Vec::new();
    // Conditions first: whether a bean exists at all outranks how it is scoped, and a list that
    // says "these exist" while half of them are behind a flag describes a context nobody builds.
    for c in &b.conditions {
        tags.push(format!("if {}", c.summary));
    }
    if b.primary {
        tags.push("primary".to_string());
    }
    if !b.scope.is_empty() {
        tags.push(b.scope.clone());
    }
    if !b.profile.is_empty() {
        tags.push(format!("profile:{}", b.profile));
    }
    if b.lazy {
        tags.push("lazy".to_string());
    }
    if b.is_abstract {
        tags.push("abstract".to_string());
    }
    tags
}

/// Pick the Java files worth parsing and parse them. See the module docs for the rounds.
fn select_and_scan(java: &[ScannedFile], xml: &[crate::xml::XmlBeanFile]) -> Vec<JavaUnit> {
    // Simple names of every class an XML names — those files must be parsed even when
    // they carry no annotation at all.
    let mut wanted: Vec<String> = xml
        .iter()
        .flat_map(|f| f.beans.iter())
        .filter(|b| !b.class.is_empty())
        .map(|b| simple_name(&b.class).to_string())
        .collect();
    wanted.sort_unstable();
    wanted.dedup();

    let mut units = scan_files(java, |f| {
        looks_spring_relevant(&f.text) || wanted.iter().any(|w| has_stem(f, w))
    });

    // One round of supertypes, so `Foo extends AbstractFoo` keeps a complete property set.
    let known: Vec<String> =
        units.iter().flat_map(|u| u.facts.types.iter().map(|t| t.name.clone())).collect();
    let missing: Vec<String> = units
        .iter()
        .flat_map(|u| u.facts.types.iter())
        .filter(|t| !t.extends.is_empty())
        .map(|t| simple_name(&crate::model::strip_generics(&t.extends)).to_string())
        .filter(|s| !known.contains(s))
        .collect();
    if !missing.is_empty() {
        let already: Vec<String> = units.iter().map(|u| u.facts.file.clone()).collect();
        units.extend(scan_files(java, |f| {
            !already.contains(&f.path.to_string_lossy().replace('\\', "/"))
                && missing.iter().any(|m| has_stem(f, m))
        }));
    }
    units
}

fn scan_files(java: &[ScannedFile], keep: impl Fn(&ScannedFile) -> bool) -> Vec<JavaUnit> {
    java.iter()
        .filter(|f| keep(f))
        .filter_map(|f| {
            scan_java(&f.path.to_string_lossy(), &f.text)
                .map(|facts| JavaUnit { facts, text: f.text.clone() })
        })
        .collect()
}

/// Whether a file is `<name>.java`.
fn has_stem(f: &ScannedFile, name: &str) -> bool {
    f.path.file_stem().and_then(|s| s.to_str()).is_some_and(|s| s == name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    /// The Spring imports on one line. `known` resolves every annotation through them, so a
    /// fixture without them is declaring its own — which is exactly what that check rejects.
    const IMPORTS: &str = "import org.springframework.stereotype.*; import org.springframework.web.bind.annotation.*; import org.springframework.beans.factory.annotation.*;";

    /// A scanned Java file, with the imports spliced onto its `package` line.
    fn java_file(path: &str, text: &str) -> ScannedFile {
        let text = match text.find('\n') {
            Some(nl) if text.trim_start().starts_with("package") => {
                format!("{}{IMPORTS}{}", &text[..nl], &text[nl..])
            }
            _ => format!("{IMPORTS}\n{text}"),
        };
        ScannedFile { path: PathBuf::from(path), text }
    }

    fn file(path: &str, text: &str) -> ScannedFile {
        ScannedFile { path: PathBuf::from(path), text: text.to_string() }
    }

    fn spring_caps() -> CapabilitySet {
        CapabilitySet { spring_annotation_di: true, ..CapabilitySet::default() }
    }

    fn indexed(java: Vec<ScannedFile>, xml: Vec<ScannedFile>, res: Vec<ScannedFile>) -> SpringExtension {
        indexed_with(java, xml, res, vec![])
    }

    fn indexed_with(
        java: Vec<ScannedFile>,
        xml: Vec<ScannedFile>,
        res: Vec<ScannedFile>,
        descriptors: Vec<ScannedFile>,
    ) -> SpringExtension {
        let ext = SpringExtension::new();
        ext.reindex(&ProjectScan {
            root: std::path::Path::new("/p"),
            java: &java,
            xml: &xml,
            resources: &res,
            descriptors: &descriptors,
        });
        ext
    }

    #[test]
    fn it_applies_only_to_a_spring_project() {
        let ext = SpringExtension::new();
        assert!(ext.applies(&spring_caps()));
        assert!(ext.applies(&CapabilitySet { spring_xml_di: true, ..CapabilitySet::default() }));
        assert!(!ext.applies(&CapabilitySet { struts_xml_config: true, ..CapabilitySet::default() }));
    }

    #[test]
    fn an_unindexed_extension_answers_nothing_rather_than_panicking() {
        let ext = SpringExtension::new();
        let ctx = FileCtx { path: std::path::Path::new("/p/A.java"), source: "class A {}" };
        assert!(!ext.is_ready());
        assert!(ext.diagnostics(&ctx).is_empty());
        assert!(ext.catalog("beans").is_empty());
        assert!(ext.navigate(&ctx, 0).is_empty());
    }

    #[test]
    fn a_full_index_populates_every_catalog() {
        let ext = indexed(
            vec![java_file(
                "/p/src/main/java/com/acme/OrderController.java",
                "package com.acme;\n@RestController @RequestMapping(\"/orders\")\nclass OrderController {\n  @Autowired private OrderService svc;\n  @GetMapping(\"/{id}\") String get(String id) { return null; }\n}\n",
            ),
            java_file(
                "/p/src/main/java/com/acme/OrderService.java",
                "package com.acme;\n@Service class OrderService {}\n",
            )],
            vec![],
            vec![file("/p/src/main/resources/application.yml", "app:\n  timeout: 30\n")],
        );
        assert!(ext.is_ready());
        let beans: Vec<_> = ext.catalog("beans").into_iter().map(|e| e.primary).collect();
        assert!(beans.contains(&"orderService".to_string()));
        assert!(beans.contains(&"orderController".to_string()));
        assert_eq!(ext.catalog("endpoints")[0].id, "GET /orders/{id}");
        assert_eq!(ext.catalog("injections")[0].primary, "OrderController.svc");
        assert_eq!(ext.catalog("properties")[0].primary, "app.timeout");
        assert_eq!(ext.catalog("property-files")[0].primary, "application.yml");
        assert!(ext.catalog("nope").is_empty());

        let stats = ext.stats();
        assert_eq!(stats.iter().find(|s| s.label == "Beans").unwrap().value, 2);
        assert_eq!(stats.iter().find(|s| s.label == "Endpoints").unwrap().value, 1);
    }

    #[test]
    fn a_pojo_named_by_an_xml_bean_is_parsed_even_with_no_annotation() {
        // The selection rule that matters: `LegacyDao` mentions Spring nowhere, but the
        // XML names it, and its setters are what the `<property>` check needs.
        let ext = indexed(
            vec![file(
                "/p/src/main/java/com/acme/LegacyDao.java",
                "package com.acme;\npublic class LegacyDao { public void setUrl(String u) {} }\n",
            )],
            vec![file(
                "/p/src/main/resources/beans.xml",
                "<beans><bean id=\"dao\" class=\"com.acme.LegacyDao\"><property name=\"url\" value=\"x\"/></bean></beans>",
            )],
            vec![],
        );
        let m = ext.model();
        assert!(m.types.contains_key("com.acme.LegacyDao"), "selected by the XML reference");
        assert!(m.types["com.acme.LegacyDao"].properties_complete);
        assert_eq!(m.bean("dao").unwrap().fqcn, "com.acme.LegacyDao");
    }

    #[test]
    fn a_supertype_is_pulled_in_for_one_round() {
        let ext = indexed(
            vec![
                java_file(
                    "/p/src/main/java/com/acme/Child.java",
                    "package com.acme;\n@Service public class Child extends AbstractBase { public void setOwn(String s) {} }\n",
                ),
                file(
                    "/p/src/main/java/com/acme/AbstractBase.java",
                    "package com.acme;\npublic class AbstractBase { public void setShared(String s) {} }\n",
                ),
            ],
            vec![],
            vec![],
        );
        let m = ext.model();
        let child = &m.types["com.acme.Child"];
        assert!(child.properties.contains(&"shared".to_string()), "supertype folded in");
        assert!(child.properties_complete);
    }

    #[test]
    fn pinning_a_property_file_changes_what_resolves_without_a_rescan() {
        let ext = indexed(
            vec![],
            vec![],
            vec![
                file("/p/application.yml", "app:\n  mode: base\n"),
                file("/p/application-dev.yml", "app:\n  mode: dev\n"),
            ],
        );
        assert_eq!(ext.model().props.lookup("app.mode").unwrap().1.value, "base");
        ext.set_active_property_file(Some("/p/application-dev.yml".to_string()));
        assert_eq!(ext.model().props.lookup("app.mode").unwrap().1.value, "dev");
        assert_eq!(ext.active_property_file().as_deref(), Some("/p/application-dev.yml"));
        // The pin survives a reindex.
        ext.reindex(&ProjectScan {
            root: std::path::Path::new("/p"),
            java: &[],
            xml: &[],
            resources: &[
                file("/p/application.yml", "app:\n  mode: base\n"),
                file("/p/application-dev.yml", "app:\n  mode: dev\n"),
            ],
            descriptors: &[],
        });
        assert_eq!(ext.model().props.lookup("app.mode").unwrap().1.value, "dev");
    }

    #[test]
    fn queries_route_by_file_kind() {
        let ext = indexed(
            vec![java_file(
                "/p/src/main/java/com/acme/S.java",
                "package com.acme;\n@Service class S {}\n",
            )],
            vec![],
            vec![],
        );
        let java = FileCtx {
            path: std::path::Path::new("/p/X.java"),
            source: "class X { @Value(\"${a.b}\") int v; }",
        };
        assert!(!ext.highlights(&java).is_empty());

        let xml = FileCtx {
            path: std::path::Path::new("/p/beans.xml"),
            source: "<beans><bean id=\"a\" class=\"C\"><property name=\"p\" value=\"${a.b}\"/></bean></beans>",
        };
        assert!(!ext.highlights(&xml).is_empty());

        // A property file gets the same expression colouring — that is the point of routing
        // it here rather than leaving `${…}` to read as prose in a yaml.
        let yaml = FileCtx {
            path: std::path::Path::new("/p/application.yml"),
            source: "app:\n  size: ${MAX:200MB}\n",
        };
        assert!(ext.highlights(&yaml).iter().any(|h| h.kind == "spring.placeholder.key"));

        // A file kind the extension has nothing to do with.
        let other = FileCtx { path: std::path::Path::new("/p/notes.md"), source: "${a.b}" };
        assert!(ext.highlights(&other).is_empty());
        assert!(ext.gutter(&other).is_empty());
        assert!(ext.inline_hint(&other, 0).is_none());
    }

    /// The descriptors a host reads out of the dependency jars become the completion
    /// vocabulary — and they outrank the curated stand-in for the keys they cover.
    #[test]
    fn a_jar_descriptor_supplies_the_vocabulary_and_beats_the_builtin_table() {
        let ext = indexed_with(
            vec![],
            vec![],
            vec![],
            vec![file(
                "/m2/acme-starter-1.0.jar!/META-INF/spring-configuration-metadata.json",
                r#"{"properties":[
                     {"name":"acme.retries","type":"java.lang.Integer","defaultValue":3,
                      "description":"How many times a call is retried."},
                     {"name":"server.port","type":"java.lang.Integer","description":"Overridden."}
                   ]}"#,
            )],
        );
        let m = ext.model();
        let acme = m.metadata.lookup("acme.retries").expect("the starter's own key");
        assert_eq!(acme.default_value, "3");
        assert_eq!(acme.origin, "acme-starter-1.0.jar", "the hover names the jar, not the entry");
        assert_eq!(
            m.metadata.lookup("server.port").unwrap().description,
            "Overridden.",
            "a real descriptor wins over the curated table",
        );
        // …and the table still covers what no descriptor mentioned.
        assert!(m.metadata.lookup("spring.datasource.url").is_some());

        let ctx = FileCtx {
            path: std::path::Path::new("/p/application.properties"),
            source: "acme.ret",
        };
        assert_eq!(ext.inline_hint(&ctx, 8).as_deref(), Some("ries"));
        assert!(ext.completions(&ctx, 8).iter().any(|c| c.label == "acme.retries"));
    }

    /// Even with no jars read at all the feature is useful — the point of the curated table.
    #[test]
    fn a_project_with_no_descriptors_still_completes_the_common_keys() {
        let ext = indexed(vec![], vec![], vec![]);
        let ctx =
            FileCtx { path: std::path::Path::new("/p/application.properties"), source: "server.po" };
        assert!(ext.completions(&ctx, 9).iter().any(|c| c.label == "server.port"));
        assert!(!ext.catalog("documented").is_empty());
    }
}
