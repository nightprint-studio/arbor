//! Editor answers for a **Java** buffer: highlighting, diagnostics, go-to, hover,
//! completion and gutter marks.
//!
//! Everything here works off a **fresh parse of the buffer** for positions, and off the
//! project model for answers. That split is the point: the buffer is unsaved text that
//! changed on the last keystroke, so spans must come from it; the model is
//! project-shaped and knows things one file cannot (which beans exist, what
//! `app.timeout` resolves to). Using the model for spans would navigate to where a symbol
//! used to be, and re-deriving the project per keystroke would cost the whole scan.
//!
//! Deriving the buffer's own beans / injections / endpoints reuses [`crate::beans`] and
//! [`crate::endpoints`] on a one-file slice, so an editor mark and a panel row can never
//! disagree about what counts.

use bennu_proto::prelude::{CompletionItem, Diagnostic};
use bennu_spel::prelude::{self as spel, Placeholder};

use bennu_ext::prelude::{ExtGutterMark, ExtHighlight, ExtHover, ExtTarget};

use crate::beans::JavaUnit;
use crate::model::{line_at, path_variables, simple_name, SpringModel};
use crate::scan::{scan_java, AnnFacts, JavaFacts};

/// Annotations whose string arguments are mapping paths — their `{var}` segments are
/// template variables and get their own colour.
const PATH_ANNOTATIONS: &[&str] = &[
    "RequestMapping",
    "GetMapping",
    "PostMapping",
    "PutMapping",
    "DeleteMapping",
    "PatchMapping",
];

/// Annotations whose string arguments resolve a property placeholder against the
/// configuration — the only ones an unresolved-key warning is allowed to look at.
const PROPERTY_ANNOTATIONS: &[&str] = &["Value", "Scheduled", "ConditionalOnProperty"];

/// How many class names one completion inside an annotation string offers. A popup is scanned,
/// not read, and a classpath can match thousands.
const CLASS_NAME_LIMIT: usize = 120;

/// Annotations whose string argument names a bean.
const BEAN_NAME_ANNOTATIONS: &[&str] = &["Qualifier", "DependsOn", "Resource"];

/// Parse the buffer into a unit the derivation functions accept.
fn unit(path: &str, source: &str) -> Option<JavaUnit> {
    Some(JavaUnit { facts: scan_java(path, source)?, text: source.to_string() })
}

/// Every annotation written anywhere in the file, flattened.
fn all_annotations(facts: &JavaFacts) -> Vec<&AnnFacts> {
    let mut out = Vec::new();
    for t in &facts.types {
        out.extend(t.annotations.iter());
        for f in &t.fields {
            out.extend(f.annotations.iter());
        }
        for m in &t.methods {
            out.extend(m.annotations.iter());
            for p in &m.params {
                out.extend(p.annotations.iter());
            }
        }
    }
    out
}

// ── Highlighting ─────────────────────────────────────────────────────────────

/// Spans to colour inside annotation strings: property placeholders, SpEL, and the
/// `{var}` segments of a mapping path.
///
/// Only *inside annotations* — a `${…}` in an ordinary Java string is a log template, an
/// SQL fragment, a JS snippet, anything at all, and colouring it as Spring syntax would
/// be a claim about the code that isn't true.
pub fn highlights(path: &str, source: &str) -> Vec<ExtHighlight> {
    let Some(u) = unit(path, source) else { return Vec::new() };
    let mut out = Vec::new();
    for ann in all_annotations(&u.facts) {
        // The `{var}` colour is specific to a request mapping, so it goes through `known`.
        // The placeholder / SpEL colouring above it deliberately does NOT: colouring is the
        // cheapest possible claim, `${…}` inside an annotation is a placeholder by
        // overwhelming convention, and Spring reads them from far more annotations than any
        // catalogue here will ever list. What is gated is everything that *resolves* or
        // *reports* — see `diagnostics` and `caret_at`.
        let is_path = crate::known::is_any(ann, &u.facts, PATH_ANNOTATIONS).is_some();
        // A `@ConditionalOnProperty` names its key OUTRIGHT — no `${…}` around it — so the
        // placeholder pass below sees an ordinary string and colours nothing. But it is the same
        // thing a `@Value("${…}")` key is: it resolves against the configuration, it has a hover
        // and a go-to, and the colour is the only thing on screen that says so. Undistinguished, it
        // read as a string literal, which is precisely the claim that is false about it.
        //
        // The same kind, therefore the same colour. Two ways of writing one concept must not look
        // like two concepts.
        if crate::known::is(ann, &u.facts, "ConditionalOnProperty") {
            for s in &ann.strings {
                // `name`, `value` and a bare positional are the key; `prefix` is the front of it.
                // `havingValue` is what the key is COMPARED TO — a value, not a key, and colouring
                // it would point a go-to at a property that does not exist.
                let is_key =
                    matches!(s.element.as_str(), "name" | "value" | "prefix") || s.element.is_empty();
                if is_key && s.end > s.start {
                    out.push(ExtHighlight {
                        start: s.start,
                        end: s.end,
                        kind: "spring.placeholder.key".to_string(),
                    });
                }
            }
        }
        // A bean written as a plain string — `@Qualifier("fast")`, `@DependsOn("audit")`,
        // `@Resource(name = "ds")`. Exactly the set `caret_at` already answers a go-to and a hover
        // for, which is the rule this colouring follows and not a second opinion: what is coloured
        // is what can be followed. Same kind as a SpEL `@beanName`, because it is the same thing
        // said another way.
        if crate::known::is_any(ann, &u.facts, BEAN_NAME_ANNOTATIONS).is_some() {
            for s in &ann.strings {
                if s.end > s.start && !s.value.trim().is_empty() && s.value.trim() == s.value {
                    out.push(ExtHighlight {
                        start: s.start,
                        end: s.end,
                        kind: "spring.spel.bean".to_string(),
                    });
                }
            }
        }
        // The prefix of a `@ConfigurationProperties` — the same colour a property key gets,
        // because that is what it is: the front of every key the annotated class binds.
        if crate::known::is(ann, &u.facts, "ConfigurationProperties") {
            for s in &ann.strings {
                if s.end > s.start && !s.value.trim().is_empty() {
                    out.push(ExtHighlight {
                        start: s.start,
                        end: s.end,
                        kind: "spring.placeholder.key".to_string(),
                    });
                }
            }
        }
        // A type named as text — see `class_ref`.
        for r in crate::class_ref::refs_of(ann, &u.facts) {
            out.push(ExtHighlight {
                start: r.start,
                end: r.end,
                kind: "spring.class-name".to_string(),
            });
        }
        for s in &ann.strings {
            crate::highlight::expression_highlights(&s.value, s.start, &mut out);
            if is_path {
                crate::highlight::path_var_highlights(&s.value, s.start, &mut out);
            }
        }
    }
    out
}

// ── Diagnostics ──────────────────────────────────────────────────────────────

/// Problems in a Java buffer's Spring syntax.
///
/// Two kinds, both deliberately narrow:
///
/// - **Syntax** — an unclosed `${` / `#{`, an unbalanced bracket. Facts about the text.
/// - **Unresolved property** — a `${key}` in a `@Value` that no property file declares.
///   Guarded three ways, because the honest answer is often "it comes from the
///   environment": the project must have property files at all; the placeholder must have
///   no default; and **another key in the same namespace must exist**. That last guard is
///   what separates a typo (`app.timout` where `app.*` keys exist — flagged) from a value
///   supplied at launch (`${SERVER_PORT}`, or `${server.port}` in a project that declares
///   no `server.*` at all — silent).
pub fn diagnostics(model: &SpringModel, path: &str, source: &str) -> Vec<Diagnostic> {
    let Some(u) = unit(path, source) else { return Vec::new() };
    let mut out = Vec::new();
    for ann in all_annotations(&u.facts) {
        // Verified: a Lombok `@Value` class and a Spring `@Value` field share a simple name
        // and mean unrelated things, and reporting an "unresolved property" against the
        // wrong one is exactly the kind of confident mistake this check must not make.
        let checks_properties =
            crate::known::is_any(ann, &u.facts, PROPERTY_ANNOTATIONS).is_some();
        for s in &ann.strings {
            for issue in spel::placeholder_issues(&s.value) {
                out.push(diag(
                    &issue.message,
                    "warning",
                    "spring-placeholder-syntax",
                    s.start + issue.start,
                    s.start + issue.end,
                ));
            }
            for issue in spel::spel_issues(&s.value) {
                out.push(diag(
                    &issue.message,
                    "warning",
                    "spring-spel-syntax",
                    s.start + issue.start,
                    s.start + issue.end,
                ));
            }
            if checks_properties {
                out.extend(unresolved_properties(model, &s.value, s.start));
            }
        }
    }
    out
}

/// The `${key}` placeholders in one string that no property file declares.
fn unresolved_properties(model: &SpringModel, text: &str, base: usize) -> Vec<Diagnostic> {
    if model.props.is_empty() {
        return Vec::new(); // nothing to check against
    }
    spel::placeholders(text)
        .into_iter()
        .filter(|p| p.default.is_none() && p.is_resolvable_key())
        .filter(|p| !model.props.declares(&p.key))
        .filter(|p| namespace_is_configured(model, &p.key))
        .map(|p| {
            diag(
                &format!("`{}` is not declared in any application property file", p.key),
                "warning",
                "spring-unresolved-property",
                base + p.key_start,
                base + p.key_end,
            )
        })
        .collect()
}

/// Whether the key's first segment is a namespace the configuration actually uses.
///
/// The guard that keeps this check honest: a project that declares `app.name` and
/// `app.timeout` clearly configures `app.*` here, so `app.timout` is a typo worth
/// flagging. A key whose whole namespace is absent is far more likely to come from the
/// environment, a launch argument, or a starter's defaults — and the editor is not the
/// authority on any of those.
fn namespace_is_configured(model: &SpringModel, key: &str) -> bool {
    let Some((head, _)) = key.split_once('.') else { return false };
    let prefix = format!("{head}.");
    model.props.keys().iter().any(|k| k.starts_with(&prefix))
}

fn diag(message: &str, severity: &str, code: &str, start: usize, end: usize) -> Diagnostic {
    Diagnostic {
        message: message.to_string(),
        severity: severity.to_string(),
        code: code.to_string(),
        start,
        end,
    }
}

// ── What is under the caret ──────────────────────────────────────────────────

/// The thing a Spring-aware caret can be on. Resolved once and shared by go-to and hover,
/// so the two can never disagree about what you are pointing at.
enum Caret {
    /// A `${key}` inside an annotation string.
    Property(Placeholder),
    /// A bean name — a `@Qualifier("…")` value or a SpEL `@bean` reference.
    BeanName(String),
    /// A configuration key written plainly rather than as a placeholder — the shape
    /// `@ConditionalOnProperty(name = "app.feature.enabled")` uses, where the key decides
    /// whether the bean exists at all.
    PropertyKey(String),
    /// An injection point's declared name, with its type and qualifier.
    Injection { type_text: String, qualifier: String, member: String },
    /// A field bound by `@ConfigurationProperties` — the caret is on its name, and the
    /// interesting thing about it is the key it binds, which appears nowhere in the source.
    ConfigProperty { field: String, type_text: String, paths: Vec<String> },
    /// A type named as a string — `@ConditionalOnClass(name = "…")`. See [`crate::class_ref`].
    ClassName(crate::class_ref::ClassRef),
    /// The `prefix` of a `@ConfigurationProperties` — not a key but the front of a family of
    /// them, which is why it needs its own answer: `props.lookup` finds nothing for it, because
    /// in a yaml a prefix is a block and only its leaves are entries.
    PropertyPrefix(String),
}

fn caret_at_with_model(model: &SpringModel, u: &JavaUnit, offset: usize) -> Option<Caret> {
    // A configuration-properties field first: the caret is on a plain field name, which every
    // other branch here would pass over, and the key it binds is the one thing about it you
    // cannot read off the screen.
    for t in &u.facts.types {
        for f in &t.fields {
            if offset < f.name_offset || offset > f.name_offset + f.name.len() {
                continue;
            }
            let paths: Vec<String> = model
                .config_bindings_for(&t.fqcn, &f.name)
                .into_iter()
                .map(|b| b.path.clone())
                .collect();
            if !paths.is_empty() {
                return Some(Caret::ConfigProperty {
                    field: f.name.clone(),
                    type_text: f.type_text.clone(),
                    paths,
                });
            }
        }
    }
    caret_at(u, offset)
}

fn caret_at(u: &JavaUnit, offset: usize) -> Option<Caret> {
    let anns = all_annotations(&u.facts);
    // A type named as text, first: its string would otherwise fall through to the bean-name branch
    // below on any annotation that shares an element name, and a class is not a bean.
    if let Some(r) = crate::class_ref::class_ref_at(&u.facts, &anns, offset) {
        return Some(Caret::ClassName(r));
    }
    for ann in all_annotations(&u.facts) {
        // A conditional names its key outright — no `${…}` to find, so the placeholder path
        // below would walk straight past the one string that matters here.
        if crate::known::is(ann, &u.facts, "ConditionalOnProperty") {
            let inside = ann
                .strings
                .iter()
                .any(|s| offset >= s.start && offset <= s.end && s.element != "havingValue");
            if inside {
                let key = crate::beans::conditional_property_key(ann);
                if !key.is_empty() {
                    return Some(Caret::PropertyKey(key));
                }
            }
        }
        // The prefix of a `@ConfigurationProperties`. It names configuration as directly as a
        // `@Value` key does — it is the front of every key the class binds — and it was the one
        // configuration string in Java with no colour, no hover and nowhere to go.
        if crate::known::is(ann, &u.facts, "ConfigurationProperties") {
            for s in &ann.strings {
                if offset >= s.start && offset <= s.end && !s.value.trim().is_empty() {
                    return Some(Caret::PropertyPrefix(crate::usages::canonical_key(s.value.trim())));
                }
            }
        }
        let names_bean = crate::known::is_any(ann, &u.facts, BEAN_NAME_ANNOTATIONS).is_some();
        for s in &ann.strings {
            if offset < s.start || offset > s.end {
                continue;
            }
            let rel = offset - s.start;
            // A placeholder wins over the enclosing string: `@Value("${a}")` with the
            // caret in `a` is about the property, not about the annotation.
            if let Some(p) = spel::placeholder_at(&s.value, rel) {
                if p.is_resolvable_key() {
                    return Some(Caret::Property(p));
                }
            }
            if let Some(r) = spel::bean_ref_at(&s.value, rel) {
                return Some(Caret::BeanName(r.name));
            }
            if names_bean && !s.value.is_empty() {
                return Some(Caret::BeanName(s.value.clone()));
            }
        }
    }
    // An injection point's own name (the field / parameter you would click).
    let points = crate::beans::injection_points(std::slice::from_ref(u));
    let hit = points
        .into_iter()
        .find(|p| offset >= p.offset && offset <= p.offset + p.member.len())?;
    Some(Caret::Injection {
        type_text: hit.type_text,
        qualifier: hit.qualifier,
        member: hit.member,
    })
}

/// Go-to targets for the caret. Empty when it is on nothing Spring knows about — which is
/// most of a Java file, and is why this is cheap to call on every Ctrl+B.
pub fn navigate(model: &SpringModel, path: &str, source: &str, offset: usize) -> Vec<ExtTarget> {
    let Some(u) = unit(path, source) else { return Vec::new() };
    match caret_at_with_model(model, &u, offset) {
        // A bound field navigates to wherever its key is actually declared — the whole point
        // of knowing the path. Several roots reach the same field, so several keys may exist.
        Some(Caret::ConfigProperty { paths, .. }) => paths
            .iter()
            .filter_map(|p| model.props.lookup(p))
            .map(|(f, e)| ExtTarget {
                file: f.path.clone(),
                offset: e.key_start,
                label: e.key.clone(),
                detail: format!("{} · {}", f.name, e.value),
            })
            .collect(),
        Some(Caret::Property(p)) => model
            .props
            .lookup(&p.key)
            .map(|(f, e)| {
                vec![ExtTarget {
                    file: f.path.clone(),
                    offset: e.key_start,
                    label: e.key.clone(),
                    detail: format!("{} · {}", f.name, e.value),
                }]
            })
            .unwrap_or_default(),
        Some(Caret::BeanName(name)) => {
            model.bean(&name).map(|b| vec![bean_target(b)]).unwrap_or_default()
        }
        Some(Caret::PropertyKey(key)) => model
            .props
            .lookup(&key)
            .map(|(f, e)| {
                vec![ExtTarget {
                    file: f.path.clone(),
                    offset: e.key_start,
                    label: e.key.clone(),
                    detail: format!("{} · {}", f.name, e.value),
                }]
            })
            .unwrap_or_default(),
        Some(Caret::Injection { type_text, qualifier, .. }) => {
            model.candidates(&type_text, &qualifier).into_iter().map(bean_target).collect()
        }
        // A prefix names a BLOCK, not a line — so the target is the first key declared under it
        // in each property file, which puts the caret inside the block rather than at a leaf
        // nobody asked about. One row per file, because "which application.yml" is exactly the
        // question a project with profiles makes you ask.
        Some(Caret::PropertyPrefix(prefix)) => model
            .prefix_sites(&prefix)
            .into_iter()
            .map(|(f, e)| ExtTarget {
                file: f.path.clone(),
                offset: e.key_start,
                label: e.key.clone(),
                detail: f.name.clone(),
            })
            .collect(),
        // A class named as text. Only the PROJECT's own types can be answered from here — an
        // `ExtTarget` is a file and an offset, and a class inside a jar has neither. The editor
        // takes the library case from the same coloured span, through the decompiled view every
        // other library go-to already uses.
        Some(Caret::ClassName(r)) => model
            .type_of(&r.fqcn)
            .map(|t| {
                vec![ExtTarget {
                    file: t.file.clone(),
                    offset: t.offset,
                    label: simple_name(&t.fqcn).to_string(),
                    detail: t.fqcn.clone(),
                }]
            })
            .unwrap_or_default(),
        None => Vec::new(),
    }
}

fn bean_target(b: &crate::model::BeanDef) -> ExtTarget {
    ExtTarget {
        file: b.file.clone(),
        offset: b.offset,
        label: if b.fqcn.is_empty() { b.name.clone() } else { simple_name(&b.fqcn).to_string() },
        detail: format!("{} · {}", b.name, b.stereotype),
    }
}

/// The hover card for the caret.
pub fn hover(model: &SpringModel, path: &str, source: &str, offset: usize) -> Option<ExtHover> {
    let u = unit(path, source)?;
    match caret_at_with_model(model, &u, offset)? {
        // A prefix. The useful answer is whether anything actually declares it and where — a
        // prefix that matches nothing is a class binding defaults for ever, and it looks exactly
        // like one that works.
        Caret::PropertyPrefix(prefix) => {
            let sites = model.prefix_sites(&prefix);
            let doc = match sites.len() {
                0 => "No property file declares anything under this prefix.".to_string(),
                1 => format!("Declared in {}", sites[0].0.name),
                n => format!(
                    "Declared in {n}: {}",
                    sites.iter().map(|(f, _)| f.name.as_str()).collect::<Vec<_>>().join(", ")
                ),
            };
            Some(ExtHover { title: prefix.clone(), signature: "prefix".to_string(), doc })
        }
        // A type named as text. What a reader wants to know is whether the name is *right* — a
        // typo here does not fail to compile, it silently turns the condition off for ever — so
        // the card answers that and nothing else. It never says "not found": this crate cannot
        // see the classpath, and a missing class is the ordinary, intended state of a
        // `@ConditionalOnClass` anyway.
        Caret::ClassName(r) => Some(match model.type_of(&r.fqcn) {
            Some(t) => ExtHover {
                title: simple_name(&t.fqcn).to_string(),
                signature: t.fqcn.clone(),
                doc: "Declared in this project — the condition tests for it at runtime."
                    .to_string(),
            },
            None => ExtHover {
                title: simple_name(&r.fqcn).to_string(),
                signature: r.fqcn.clone(),
                doc: "Read as a class name at runtime. Written as a string because the condition \
                      exists for the case where it is absent."
                    .to_string(),
            },
        }),
        Caret::ConfigProperty { field, type_text, paths } => {
            // The key comes first because it is the answer to the question you hovered with:
            // "what do I write in the yaml for this field".
            let value = paths.iter().find_map(|p| model.props.lookup(p));
            let doc = match (&value, paths.len()) {
                (Some((f, e)), _) => format!("Set to `{}` in {}", e.value, f.name),
                (None, 1) => "Not set in any property file.".to_string(),
                (None, n) => format!("Bound from {n} configuration roots; not set in any file."),
            };
            Some(ExtHover {
                title: paths.join("  ·  "),
                signature: format!("{type_text} {field}"),
                doc,
            })
        }
        Caret::Property(p) => Some(match model.props.lookup(&p.key) {
            Some((f, e)) => ExtHover {
                title: p.key.clone(),
                signature: if e.value.is_empty() { "(empty)".to_string() } else { e.value.clone() },
                doc: format!("Declared in {}", f.name),
            },
            None => ExtHover {
                title: p.key.clone(),
                signature: p.default.clone().unwrap_or_else(|| "(unresolved)".to_string()),
                doc: match &p.default {
                    Some(_) => "Not declared in any property file — the default applies.".to_string(),
                    None => "Not declared in any property file.".to_string(),
                },
            },
        }),
        Caret::PropertyKey(key) => {
            // The useful answer is not "here is a key" but "does this bean exist right now".
            let (signature, doc) = match model.props.lookup(&key) {
                Some((f, e)) => (
                    e.value.clone(),
                    format!("Declared in {} — the condition reads this.", f.name),
                ),
                None => (
                    "(not set)".to_string(),
                    "Not declared in any property file — whether the condition holds depends on the environment.".to_string(),
                ),
            };
            Some(ExtHover { title: key, signature, doc })
        }
        Caret::BeanName(name) => model.bean(&name).map(|b| ExtHover {
            title: b.name.clone(),
            signature: if b.fqcn.is_empty() { b.stereotype.clone() } else { b.fqcn.clone() },
            doc: describe_bean(b),
        }),
        Caret::Injection { type_text, qualifier, member } => {
            let candidates = model.candidates(&type_text, &qualifier);
            Some(ExtHover {
                title: member,
                signature: type_text,
                doc: match candidates.len() {
                    0 => "No bean of this type was found in the project.".to_string(),
                    1 => format!("Injected with `{}` ({}).", candidates[0].name, candidates[0].stereotype),
                    n => format!(
                        "{n} candidate beans: {}",
                        candidates.iter().map(|b| b.name.as_str()).collect::<Vec<_>>().join(", ")
                    ),
                },
            })
        }
    }
}

fn describe_bean(b: &crate::model::BeanDef) -> String {
    let mut parts = vec![format!("Declared by {}", b.stereotype)];
    if !b.scope.is_empty() {
        parts.push(format!("scope {}", b.scope));
    }
    if b.primary {
        parts.push("@Primary".to_string());
    }
    if !b.profile.is_empty() {
        parts.push(format!("profile {}", b.profile));
    }
    // Whether it exists at all comes last but reads loudest — a bean behind a flag is a
    // different thing from a bean.
    for c in &b.conditions {
        parts.push(format!("only if {}", c.summary));
    }
    parts.join(" · ")
}

// ── Completion ───────────────────────────────────────────────────────────────

/// Candidates inside an annotation string: property keys after an open `${`, bean names
/// inside a `@Qualifier`.
pub fn completions(
    model: &SpringModel,
    class_names: Option<&dyn crate::ext::ClassNameSource>,
    path: &str,
    source: &str,
    offset: usize,
) -> Vec<CompletionItem> {
    let Some(u) = unit(path, source) else { return Vec::new() };
    // A type written as a string. Completed from the CLASSPATH and not from the project's own
    // types: `@ConditionalOnClass(name = "…")` names somebody else's class almost by definition —
    // it is written as a string precisely because the class may not be there to import.
    {
        let anns = all_annotations(&u.facts);
        if let Some(r) = crate::class_ref::class_ref_at(&u.facts, &anns, offset) {
            let Some(src) = class_names else { return Vec::new() };
            // What has been typed is the text BEFORE the caret, not the whole string: completing
            // `com.acme.Ord|erService` on the whole value would match nothing.
            let typed = &r.fqcn[..(offset - r.start).min(r.fqcn.len())];
            return src
                .matching(typed, CLASS_NAME_LIMIT)
                .into_iter()
                .map(|fqcn| CompletionItem {
                    detail: Some(fqcn.rsplit_once('.').map(|(pkg, _)| pkg.to_string()).unwrap_or_default()),
                    label: fqcn,
                    kind: "class".to_string(),
                    auto_import: None,
                    ..Default::default()
                })
                .collect();
        }
    }
    for ann in all_annotations(&u.facts) {
        for s in &ann.strings {
            if offset < s.start || offset > s.end {
                continue;
            }
            let rel = offset - s.start;
            if in_open_placeholder(&s.value, rel) {
                return model
                    .props
                    .keys()
                    .into_iter()
                    .map(|k| CompletionItem {
                        detail: model.props.lookup(&k).map(|(f, e)| format!("{} · {}", e.value, f.name)),
                        label: k,
                        kind: "property".to_string(),
                        auto_import: None,
                        ..Default::default()
                    })
                    .collect();
            }
            if crate::known::is_any(ann, &u.facts, BEAN_NAME_ANNOTATIONS).is_some() {
                return bean_completions(model);
            }
        }
    }
    Vec::new()
}

/// Bean-name candidates, sorted and deduplicated.
fn bean_completions(model: &SpringModel) -> Vec<CompletionItem> {
    let mut seen: Vec<&str> = Vec::new();
    let mut out = Vec::new();
    for b in &model.beans {
        if seen.contains(&b.name.as_str()) {
            continue;
        }
        seen.push(&b.name);
        out.push(CompletionItem {
            label: b.name.clone(),
            kind: "bean".to_string(),
            detail: Some(if b.fqcn.is_empty() {
                b.stereotype.clone()
            } else {
                format!("{} · {}", b.fqcn, b.stereotype)
            }),
            auto_import: None,
            ..Default::default()
        });
    }
    out
}

/// Whether `offset` sits inside a `${` that hasn't been closed before it — i.e. the user
/// is typing a key right now, which is exactly when the key list is useful and the only
/// time it isn't noise.
fn in_open_placeholder(text: &str, offset: usize) -> bool {
    let before = &text[..offset.min(text.len())];
    match (before.rfind("${"), before.rfind('}')) {
        (Some(open), Some(close)) => open > close,
        (Some(_), None) => true,
        _ => false,
    }
}

// ── Gutter ───────────────────────────────────────────────────────────────────

/// What satisfies an injection point, said in full.
///
/// A bean out of the project is a fact and needs no qualification. One out of a **dependency** is
/// two more things the reader has no other way to learn: **whose** it is, and **on what terms** —
/// a library bean is what Spring *may* register, and its `@ConditionalOn…` is what decides. Saying
/// only "Injected with `schemaAuthorizationManager`" about one of those states a certainty nobody
/// has.
fn injected_with(bean: &crate::model::BeanDef) -> String {
    let mut out = format!("Injected with `{}`", bean.name);
    if !bean.artifact.is_empty() {
        out.push_str(&format!(" — from {}", bean.artifact));
    }
    match bean.conditions.len() {
        0 => {}
        // One condition reads as a sentence; several would make the tooltip a list, and the count
        // is the honest summary — the panel is where they are enumerated.
        1 => out.push_str(&format!(", if {}", bean.conditions[0].summary)),
        n => out.push_str(&format!(", under {n} conditions")),
    }
    out
}

/// Gutter marks for a Java buffer: beans, injection points and endpoints.
///
/// Positions come from the buffer (so a mark follows an edit), targets from the model
/// (so it points at the whole project). The buffer's own beans / injections / endpoints
/// are derived with the same functions the project scan uses.
pub fn gutter(model: &SpringModel, path: &str, source: &str) -> Vec<ExtGutterMark> {
    let Some(u) = unit(path, source) else { return Vec::new() };
    let units = std::slice::from_ref(&u);
    let mut out = Vec::new();

    // Bean declarations → where this bean is injected, and where XML names its class.
    for b in crate::beans::annotation_beans(units) {
        let targets = usage_targets(model, &b);
        out.push(ExtGutterMark {
            line: line_at(source, b.offset),
            kind: "bean".to_string(),
            tooltip: match targets.len() {
                0 => format!("Spring bean `{}` ({})", b.name, b.stereotype),
                1 => format!("Spring bean `{}` — 1 injection point", b.name),
                n => format!("Spring bean `{}` — {n} injection points", b.name),
            },
            targets,
        });
    }

    // Injection points → the beans that could satisfy them.
    for p in crate::beans::injection_points(units) {
        let candidates = model.candidates(&p.type_text, &p.qualifier);
        // Nothing found, and the type is not the project's: the beans that would satisfy it live
        // in a jar, which this model does not read — a bean inside one is what Spring *may*
        // register, and `@ConditionalOn…` decides the rest.
        //
        // So there is no mark. Not a mark that says "no matching bean found", which asserts what
        // cannot be known; and not one that says so honestly either, because a gutter mark whose
        // whole content is *I cannot tell you* costs a slot in the gutter and a hover to learn
        // nothing. An `@Autowired` field of a framework type is the commonest shape there is, and
        // one of these beside every single one of them is the mark you learn to stop reading.
        //
        // The same rule the usage counts follow: when the engine cannot see, it says nothing —
        // and it keeps saying "no matching bean" where that IS a finding, on a project type.
        if candidates.is_empty() && !model.declares_type(&p.type_text) {
            continue;
        }
        out.push(ExtGutterMark {
            line: line_at(source, p.offset),
            kind: "inject".to_string(),
            tooltip: match candidates.len() {
                // Nothing found, and there are two very different reasons for that. When the
                // project declares the type, the model has looked everywhere a bean of it could
                // be declared and "none" is a finding. When it does NOT — the type comes from a
                // jar — the model has not looked there at all and never will: it holds the
                // project's own beans by design, because a bean inside a jar is what Spring *may*
                // register and `@ConditionalOn…` decides the rest.
                //
                // Saying "no matching bean found" in the second case asserts something this engine
                // cannot know, on the commonest shape there is — an `@Autowired` field of a
                // framework type. Allowlisting the artifact under Settings → Beans does not change
                // it either, because those beans are listed and navigable and take no part in
                // autowiring. So it says what is actually true instead.
                // Only ever reached for a type the project declares — see the `continue` above.
                0 => format!("`{}` — no matching bean found", p.type_text),
                1 => injected_with(candidates[0]),
                n => format!("{n} candidate beans for `{}`", p.type_text),
            },
            targets: candidates.into_iter().map(bean_target).collect(),
        });
    }

    // Endpoints → decorative, but the route is the tooltip, which is the whole point.
    for e in crate::endpoints::endpoints(units) {
        out.push(ExtGutterMark {
            line: line_at(source, e.offset),
            kind: "endpoint".to_string(),
            tooltip: e.label(),
            targets: Vec::new(),
        });
    }
    out
}

/// Where a bean is used: every injection point it could satisfy.
fn usage_targets(model: &SpringModel, bean: &crate::model::BeanDef) -> Vec<ExtTarget> {
    model
        .injections
        .iter()
        .filter(|i| model.candidates(&i.type_text, &i.qualifier).iter().any(|c| c.name == bean.name))
        .map(|i| ExtTarget {
            file: i.file.clone(),
            offset: i.offset,
            label: format!("{}.{}", simple_name(&i.owner_fqcn), i.member),
            detail: format!("{} injection", i.kind.as_str()),
        })
        .collect()
}

/// The `{var}` names of every mapping path in a buffer — used by the endpoint tooling and
/// re-exported for tests of the highlight pass.
pub fn mapping_path_variables(path: &str, source: &str) -> Vec<String> {
    let Some(u) = unit(path, source) else { return Vec::new() };
    all_annotations(&u.facts)
        .into_iter()
        .filter(|a| crate::known::is_any(a, &u.facts, PATH_ANNOTATIONS).is_some())
        .flat_map(|a| a.strings.iter())
        .flat_map(|s| path_variables(&s.value))
        .collect()
}

#[cfg(test)]
mod tests {
    /// A `record` carrying `@ConfigurationProperties` — the ordinary shape in modern Spring Boot.
    #[test]
    fn a_record_prefix_is_seen() {
        let src = java("package p;\n@ConfigurationProperties(prefix = \"app.pa-gateway.cors\")\npublic record CorsProps(java.util.List<String> allowedOrigins) {}\n");
        let hs = highlights(PATH, &src);
        let keys: Vec<&str> = hs.iter().filter(|h| h.kind == "spring.placeholder.key").map(|h| &src[h.start..h.end]).collect();
        assert_eq!(keys, vec!["app.pa-gateway.cors"], "{hs:?}");
    }

    /// The prefix of a `@ConfigurationProperties` is configuration, so it is coloured as
    /// configuration — it was the one such string in Java that read as a plain literal.
    #[test]
    fn a_configuration_properties_prefix_is_coloured_as_a_key() {
        let src = java("package p;\n@ConfigurationProperties(prefix = \"app.http.client\")\nclass C {}\n");
        let hs = highlights(PATH, &src);
        let keys: Vec<&str> = hs
            .iter()
            .filter(|h| h.kind == "spring.placeholder.key")
            .map(|h| &src[h.start..h.end])
            .collect();
        assert_eq!(keys, vec!["app.http.client"], "{hs:?}");
    }

    /// And go-to on it lands inside the block it names — the FIRST key declared under it, because
    /// in a yaml the prefix itself is not an entry and `lookup` finds nothing for it.
    #[test]
    fn go_to_on_a_prefix_lands_on_the_first_key_under_it() {
        let m = model_with("", "app:\n  http:\n    client:\n      read-timeout: 5s\n      connect-timeout: 2s\n");
        let src = java("package p;\n@ConfigurationProperties(prefix = \"app.http.client\")\nclass C {}\n");
        let at = src.find("app.http.client").unwrap() + 4;
        let t = navigate(&m, PATH, &src, at);
        assert_eq!(t.len(), 1, "{t:?}");
        assert_eq!(t[0].label, "app.http.client.read-timeout");
    }

    /// The reported case end to end: a record whose only bound key is a **list**, under a nested
    /// prefix. Every part of it used to fail together and for one reason — the yaml reader threw
    /// the key away with the sequence — so the prefix looked configured nowhere.
    #[test]
    fn go_to_on_a_prefix_finds_a_list_valued_key() {
        let m = model_with(
            "",
            "appaltiecontratti:\n  application:\n    pa-gateway:\n      cors:\n        allowed-origins:\n          - \"http://a\"\n",
        );
        let src = java("package p;\n@ConfigurationProperties(prefix = \"appaltiecontratti.application.pa-gateway.cors\")\npublic record CorsProps(java.util.List<String> allowedOrigins) {}\n");
        let at = src.find("appaltiecontratti.application").unwrap() + 5;
        let t = navigate(&m, PATH, &src, at);
        assert_eq!(t.len(), 1, "{t:?}");
        assert_eq!(t[0].label, "appaltiecontratti.application.pa-gateway.cors.allowed-origins");
    }

    /// Relaxed binding, both ways round: the annotation may say `httpClient` where the yaml says
    /// `http-client`, and they are the same prefix to Spring.
    #[test]
    fn a_prefix_matches_across_relaxed_spellings() {
        let m = model_with("", "app:\n  http-client:\n    read-timeout: 5s\n");
        let src = java("package p;\n@ConfigurationProperties(prefix = \"app.httpClient\")\nclass C {}\n");
        let at = src.find("app.httpClient").unwrap() + 2;
        assert_eq!(navigate(&m, PATH, &src, at).len(), 1);
    }

    /// A prefix nothing declares says so, rather than looking like one that works — a class
    /// binding defaults for ever is invisible otherwise.
    #[test]
    fn a_prefix_nothing_declares_says_so() {
        let m = model_with("", "other:\n  thing: 1\n");
        let src = java("package p;\n@ConfigurationProperties(prefix = \"app.absent\")\nclass C {}\n");
        let at = src.find("app.absent").unwrap() + 2;
        let h = hover(&m, PATH, &src, at).expect("a prefix has a card");
        assert!(h.doc.contains("No property file declares"), "{}", h.doc);
    }

    /// A bean written as a plain string is coloured as a bean — the same colour a SpEL
    /// `@beanName` gets, because it is the same thing said another way.
    #[test]
    fn a_qualifier_name_is_coloured_as_a_bean() {
        let src = java("class C { @Qualifier(\"fast\") Engine e; }");
        let hs = highlights(PATH, &src);
        let beans: Vec<&str> = hs
            .iter()
            .filter(|h| h.kind == "spring.spel.bean")
            .map(|h| &src[h.start..h.end])
            .collect();
        assert_eq!(beans, vec!["fast"], "{hs:?}");
    }

    /// A class named as a string gets its own kind — it is neither a property nor a bean, and
    /// colouring it as either would point its go-to at the wrong index.
    #[test]
    fn a_conditional_on_class_name_is_coloured_as_a_class() {
        let src = java("package p;\n@ConditionalOnClass(name = \"com.zaxxer.hikari.HikariDataSource\")\nclass C {}\n");
        let hs = highlights(PATH, &src);
        let types: Vec<&str> = hs
            .iter()
            .filter(|h| h.kind == "spring.class-name")
            .map(|h| &src[h.start..h.end])
            .collect();
        assert_eq!(types, vec!["com.zaxxer.hikari.HikariDataSource"], "{hs:?}");
    }

    /// Go-to on one lands on the project's own class when it declares it.
    #[test]
    fn go_to_on_a_named_class_reaches_a_project_type() {
        let m = model_with("package com.acme;\n@Service class OrderServiceImpl implements OrderService {}\n", "");
        let src = java("package p;\n@ConditionalOnClass(name = \"com.acme.OrderServiceImpl\")\nclass C {}\n");
        let at = src.find("com.acme.OrderServiceImpl").unwrap() + 12;
        let t = navigate(&m, PATH, &src, at);
        assert_eq!(t.len(), 1, "{t:?}");
        assert_eq!(t[0].label, "OrderServiceImpl");
    }

    /// Completion inside one offers CLASSPATH names, filtered by what has been typed before the
    /// caret rather than by the whole string.
    #[test]
    fn completing_a_named_class_offers_classpath_types() {
        struct Fake;
        impl crate::ext::ClassNameSource for Fake {
            fn matching(&self, typed: &str, _limit: usize) -> Vec<String> {
                assert_eq!(typed, "com.zax", "only what precedes the caret is typed");
                vec!["com.zaxxer.hikari.HikariDataSource".to_string()]
            }
        }
        let m = SpringModel::default();
        let src = java("package p;\n@ConditionalOnClass(name = \"com.zaxxer.hikari.HikariDataSource\")\nclass C {}\n");
        let at = src.find("com.zaxxer").unwrap() + "com.zax".len();
        let items = completions(&m, Some(&Fake), PATH, &src, at);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].label, "com.zaxxer.hikari.HikariDataSource");
        assert_eq!(items[0].kind, "class");
    }

    /// And offers nothing at all when no classpath has been handed over — an empty popup, never a
    /// list of the project's own types pretending to be the answer.
    #[test]
    fn completing_a_named_class_without_a_classpath_offers_nothing() {
        let m = SpringModel::default();
        let src = java("package p;\n@ConditionalOnClass(name = \"com.zax\")\nclass C {}\n");
        let at = src.find("com.zax").unwrap() + 3;
        assert!(completions(&m, None, PATH, &src, at).is_empty());
    }

    /// The key of a `@ConditionalOnProperty` is a property key written without `${…}`, and it is
    /// coloured as one — the placeholder scan alone sees a plain string and says nothing, which is
    /// how a name with a go-to behind it ended up looking like a literal.
    #[test]
    fn a_conditional_on_property_key_is_coloured_like_a_placeholder_key() {
        let src = java("package p;\n@ConditionalOnProperty(name = \"app.routing.mode\", havingValue = \"schema-driven\")\nclass C {}\n");
        let hs = highlights(PATH, &src);
        let key_spans: Vec<&str> = hs
            .iter()
            .filter(|h| h.kind == "spring.placeholder.key")
            .map(|h| &src[h.start..h.end])
            .collect();
        assert_eq!(key_spans, vec!["app.routing.mode"], "{hs:?}");
    }

    /// `havingValue` is what the key is compared to. Colouring it would offer a go-to to a
    /// property nobody declared.
    #[test]
    fn the_compared_value_is_not_coloured_as_a_key() {
        let src = java("package p;\n@ConditionalOnProperty(name = \"a.b\", havingValue = \"on\")\nclass C {}\n");
        let hs = highlights(PATH, &src);
        assert!(
            !hs.iter().any(|h| h.kind == "spring.placeholder.key" && &src[h.start..h.end] == "on"),
            "{hs:?}"
        );
    }

    /// A `prefix` is the front of the key, so it is part of it.
    #[test]
    fn a_prefix_is_part_of_the_key() {
        let src = java("package p;\n@ConditionalOnProperty(prefix = \"app.feature\", name = \"enabled\")\nclass C {}\n");
        let hs = highlights(PATH, &src);
        let mut key_spans: Vec<&str> = hs
            .iter()
            .filter(|h| h.kind == "spring.placeholder.key")
            .map(|h| &src[h.start..h.end])
            .collect();
        key_spans.sort_unstable();
        assert_eq!(key_spans, vec!["app.feature", "enabled"], "{hs:?}");
    }
    use super::*;
    use crate::props::{parse_property_file, PropertySources};

    const PATH: &str = "/p/src/main/java/com/acme/S.java";

    /// The Spring imports a real file carries, on one line. `known` resolves every
    /// annotation through them — a fixture without them is declaring its own `@Service`,
    /// which is precisely what that check rejects.
    ///
    /// `context.annotation` is in here because `@Bean` and `@Configuration` live there, and
    /// without it no fixture could express a **factory method** — the commonest shape a Spring
    /// application declares a bean in, and one nothing here was able to test.
    const IMPORTS: &str = "import org.springframework.beans.factory.annotation.*; import org.springframework.web.bind.annotation.*; import org.springframework.stereotype.*; import org.springframework.boot.context.properties.*; import org.springframework.boot.autoconfigure.condition.*; import org.springframework.context.annotation.*;";

    /// A buffer with those imports spliced onto the `package` line (or the front).
    fn java(src: &str) -> String {
        match src.find('\n') {
            Some(nl) if src.trim_start().starts_with("package") => {
                format!("{}{IMPORTS}{}", &src[..nl], &src[nl..])
            }
            _ => format!("{IMPORTS}\n{src}"),
        }
    }

    fn model_with(src: &str, yaml: &str) -> SpringModel {
        let java = java(src);
        let u = JavaUnit {
            facts: scan_java("/p/src/main/java/com/acme/Beans.java", &java).unwrap(),
            text: java.clone(),
        };
        let units = std::slice::from_ref(&u);
        // The types the fixture declares, and their simple names — the way `ext.rs` builds the
        // real model. Left out, `simple_names` was empty in every test while production always
        // fills it, so a fixture answered "this project declares nothing" and any rule resting on
        // that was being tested against a configuration that does not exist.
        let types = crate::beans::type_index(units);
        let mut simple_names: std::collections::BTreeMap<String, Vec<String>> = Default::default();
        for fqcn in types.keys() {
            simple_names.entry(crate::model::simple_name(fqcn).to_string()).or_default().push(fqcn.clone());
        }
        let mut m = SpringModel {
            beans: crate::beans::annotation_beans(units),
            injections: crate::beans::injection_points(units),
            endpoints: crate::endpoints::endpoints(units),
            config_bindings: crate::config_props::bindings(units),
            types,
            simple_names,
            ..SpringModel::default()
        };
        if !yaml.is_empty() {
            m.props = PropertySources::new(vec![
                parse_property_file("/p/application.yml", yaml).unwrap()
            ]);
        }
        m
    }

    fn kinds_at(source: &str, needle: &str) -> Vec<String> {
        let at = source.find(needle).unwrap();
        highlights(PATH, source)
            .into_iter()
            .filter(|h| h.start <= at && at < h.end)
            .map(|h| h.kind)
            .collect()
    }

    #[test]
    fn placeholder_parts_are_coloured_separately() {
        let src = "class C { @Value(\"${app.timeout:30}\") int t; }";
        let hs = highlights(PATH, src);
        let kinds: Vec<_> = hs.iter().map(|h| h.kind.as_str()).collect();
        assert!(kinds.contains(&"spring.placeholder"));
        assert!(kinds.contains(&"spring.placeholder.key"));
        assert!(kinds.contains(&"spring.placeholder.default"));
        let key = hs.iter().find(|h| h.kind == "spring.placeholder.key").unwrap();
        assert_eq!(&src[key.start..key.end], "app.timeout");
    }

    #[test]
    fn spel_bean_references_get_their_own_colour() {
        let src = "class C { @Value(\"#{@cfg.timeout}\") int t; }";
        assert!(kinds_at(src, "@cfg").contains(&"spring.spel.bean".to_string()));
    }

    #[test]
    fn a_placeholder_outside_an_annotation_is_left_alone() {
        // A `${}` in an ordinary string is a log template, an SQL fragment, anything —
        // claiming it is Spring syntax would be a claim about the code that isn't true.
        let src = "class C { String s = \"${not.spring}\"; }";
        assert!(highlights(PATH, src).is_empty());
    }

    #[test]
    fn mapping_path_variables_are_highlighted() {
        let src = java("class C { @GetMapping(\"/orders/{id}\") void m() {} }");
        let hs = highlights(PATH, &src);
        let v = hs.iter().find(|h| h.kind == "spring.path-var").expect("path var");
        assert_eq!(&src[v.start..v.end], "{id}");
        assert_eq!(mapping_path_variables(PATH, &src), ["id"]);
    }

    #[test]
    fn a_projects_own_mapping_annotation_gets_no_path_colour() {
        // Same text, an annotation of the same name from somewhere else: `{id}` here is a
        // string, not a template variable, and claiming otherwise would be a guess.
        let src = "import com.acme.web.GetMapping;\nclass C { @GetMapping(\"/orders/{id}\") void m() {} }";
        assert!(!highlights(PATH, src).iter().any(|h| h.kind == "spring.path-var"));
    }

    #[test]
    fn syntax_problems_are_reported_where_they_are() {
        let src = "class C { @Value(\"${app.name\") String s; }";
        let m = SpringModel::default();
        let d = diagnostics(&m, PATH, src);
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].code, "spring-placeholder-syntax");
        assert_eq!(d[0].severity, "warning");
        assert!(src[d[0].start..d[0].end].starts_with("${"));
    }

    #[test]
    fn an_unknown_key_in_a_configured_namespace_is_flagged() {
        let m = model_with("class B {}", "app:\n  timeout: 30\n  name: x\n");
        let src = java("class C { @Value(\"${app.timout}\") int t; }");
        let d = diagnostics(&m, PATH, &src);
        assert_eq!(d.len(), 1, "a typo in a namespace the project configures");
        assert_eq!(d[0].code, "spring-unresolved-property");
        assert_eq!(&src[d[0].start..d[0].end], "app.timout");
    }

    #[test]
    fn a_lombok_value_is_not_read_as_a_property_injection() {
        // `lombok.Value` and Spring's `@Value` share a simple name and mean unrelated
        // things. Nothing may be reported against the wrong one.
        let m = model_with("class B {}", "app:\n  timeout: 30\n  name: x\n");
        let src = "import lombok.Value;\n@Value class C { String appTimout; }";
        assert!(diagnostics(&m, PATH, src).is_empty());
    }

    #[test]
    fn keys_that_might_come_from_the_environment_stay_silent() {
        let m = model_with("class B {}", "app:\n  timeout: 30\n");
        // No `server.*` is configured here, so this is far more likely a launch argument.
        assert!(diagnostics(&m, PATH, &java("class C { @Value(\"${server.port}\") int p; }")).is_empty());
        // A default means it can never fail to resolve.
        assert!(diagnostics(&m, PATH, &java("class C { @Value(\"${app.nope:5}\") int p; }")).is_empty());
        // Not a dotted key — an env var, not a Spring key.
        assert!(diagnostics(&m, PATH, &java("class C { @Value(\"${HOME}\") String h; }")).is_empty());
        // Outside a property annotation entirely.
        assert!(diagnostics(&m, PATH, &java("class C { @Header(\"${app.nope}\") String h; }")).is_empty());
    }

    #[test]
    fn a_project_with_no_property_files_is_never_flagged() {
        let m = model_with("class B {}", "");
        assert!(diagnostics(&m, PATH, &java("class C { @Value(\"${any.thing}\") int t; }")).is_empty());
    }

    #[test]
    fn go_to_on_a_placeholder_lands_on_the_key_in_the_yaml() {
        let yaml = "app:\n  timeout: 30\n";
        let m = model_with("class B {}", yaml);
        let src = "class C { @Value(\"${app.timeout}\") int t; }";
        let at = src.find("timeout").unwrap();
        let t = navigate(&m, PATH, src, at);
        assert_eq!(t.len(), 1);
        assert_eq!(t[0].file, "/p/application.yml");
        assert_eq!(&yaml[t[0].offset..t[0].offset + 7], "timeout");
    }

    #[test]
    fn hover_on_a_placeholder_shows_the_resolved_value() {
        let m = model_with("class B {}", "app:\n  timeout: 30\n");
        let src = "class C { @Value(\"${app.timeout}\") int t; }";
        let h = hover(&m, PATH, src, src.find("timeout").unwrap()).unwrap();
        assert_eq!(h.title, "app.timeout");
        assert_eq!(h.signature, "30");
        assert!(h.doc.contains("application.yml"));
    }

    #[test]
    fn hover_on_an_unknown_key_says_so_without_inventing_a_value() {
        let m = model_with("class B {}", "app:\n  x: 1\n");
        let src = "class C { @Value(\"${app.mystery}\") int t; }";
        let h = hover(&m, PATH, src, src.find("mystery").unwrap()).unwrap();
        assert_eq!(h.signature, "(unresolved)");
    }

    #[test]
    fn go_to_on_a_qualifier_lands_on_the_bean() {
        let m = model_with("package com.acme;\n@Service(\"fast\") class FastEngine implements Engine {}\n", "");
        let src = java("class C { @Autowired @Qualifier(\"fast\") Engine e; }");
        let t = navigate(&m, PATH, &src, src.find("fast\"").unwrap());
        assert_eq!(t.len(), 1);
        assert_eq!(t[0].label, "FastEngine");
        assert_eq!(t[0].detail, "fast · @Service");
    }

    #[test]
    fn a_qualifier_from_another_package_resolves_nothing() {
        let m = model_with("package com.acme;\n@Service(\"fast\") class FastEngine implements Engine {}\n", "");
        let src = "import com.acme.own.Qualifier;\nclass C { @Qualifier(\"fast\") Engine e; }";
        assert!(navigate(&m, PATH, src, src.find("fast\"").unwrap()).is_empty());
    }

    #[test]
    fn go_to_on_an_injected_field_offers_the_candidates() {
        let m = model_with("package com.acme;\n@Service class OrderServiceImpl implements OrderService {}\n", "");
        let src = java("class C { @Autowired private OrderService svc; }");
        let t = navigate(&m, PATH, &src, src.find("svc").unwrap() + 1);
        assert_eq!(t.len(), 1);
        assert_eq!(t[0].label, "OrderServiceImpl");
    }

    #[test]
    fn hover_on_an_injection_point_counts_the_candidates() {
        let m = model_with(
            "package com.acme;\n@Service class A implements Engine {}\n@Service class B implements Engine {}\n",
            "",
        );
        let src = java("class C { @Autowired private Engine e; }");
        let h = hover(&m, PATH, &src, src.find(" e;").unwrap() + 1).unwrap();
        assert!(h.doc.starts_with("2 candidate beans"), "got {}", h.doc);
    }

    #[test]
    fn hovering_a_bound_field_shows_the_key_it_binds_and_its_value() {
        // The key `app.http.client.read-timeout` appears nowhere in this source — that is the
        // whole reason to show it.
        let project = "package p;\n@ConfigurationProperties(prefix = \"app.http\")\nclass Http { private Client client; }\nclass Client { private int readTimeout; }\n";
        let m = model_with(project, "app:\n  http:\n    client:\n      read-timeout: 5000\n");
        let src = java("package p;\nclass Client { private int readTimeout; }\n");
        let h = hover(&m, PATH, &src, src.find("readTimeout").unwrap() + 2).unwrap();
        assert_eq!(h.title, "app.http.client.read-timeout");
        assert_eq!(h.signature, "int readTimeout");
        assert!(h.doc.contains("5000"), "got {}", h.doc);
    }

    #[test]
    fn a_bound_field_navigates_to_the_key_in_the_yaml() {
        let project = "package p;\n@ConfigurationProperties(prefix = \"app\")\nclass A { private String name; }\n";
        let yaml = "app:\n  name: bennu\n";
        let m = model_with(project, yaml);
        let src = java("package p;\nclass A { private String name; }\n");
        let t = navigate(&m, PATH, &src, src.find("name;").unwrap() + 1);
        assert_eq!(t.len(), 1);
        assert_eq!(&yaml[t[0].offset..t[0].offset + 4], "name");
    }

    #[test]
    fn an_unset_bound_field_still_shows_its_key() {
        let project = "package p;\n@ConfigurationProperties(prefix = \"app\")\nclass A { private String mystery; }\n";
        let m = model_with(project, "app:\n  other: 1\n");
        let src = java("package p;\nclass A { private String mystery; }\n");
        let h = hover(&m, PATH, &src, src.find("mystery").unwrap() + 2).unwrap();
        assert_eq!(h.title, "app.mystery");
        assert!(h.doc.contains("Not set"));
    }

    #[test]
    fn a_field_of_an_unbound_class_hovers_as_nothing() {
        let m = model_with("package p;\nclass Plain { private String name; }\n", "app:\n  name: x\n");
        let src = java("package p;\nclass Plain { private String name; }\n");
        assert!(hover(&m, PATH, &src, src.find("name;").unwrap() + 1).is_none());
    }

    #[test]
    fn a_conditional_property_key_hovers_with_its_current_value() {
        let m = model_with("class B {}", "app:\n  feature:\n    enabled: true\n");
        let src = java(
            "@ConditionalOnProperty(name = \"app.feature.enabled\", havingValue = \"true\")\nclass C {}",
        );
        let at = src.find("app.feature.enabled").unwrap() + 4;
        let h = hover(&m, PATH, &src, at).unwrap();
        assert_eq!(h.title, "app.feature.enabled");
        assert_eq!(h.signature, "true");
        // And it navigates to where it is set.
        assert_eq!(navigate(&m, PATH, &src, at).len(), 1);
    }

    #[test]
    fn a_conditional_bean_says_what_it_depends_on() {
        let m = model_with(
            "package com.acme;\n@Service @ConditionalOnProperty(name = \"app.dev\", havingValue = \"true\") class DevService implements Svc {}\n",
            "",
        );
        let b = m.bean("devService").expect("registered even though it is conditional");
        assert_eq!(b.conditions.len(), 1);
        assert_eq!(b.conditions[0].summary, "app.dev = true");
        // Hovering the injection point that resolves to it mentions the condition.
        let src = java("class C { @Autowired Svc s; }");
        let h = hover(&m, PATH, &src, src.find(" s;").unwrap() + 1).unwrap();
        assert!(h.doc.contains("devService"), "got {}", h.doc);
    }

    #[test]
    fn a_caret_on_ordinary_code_resolves_to_nothing() {
        let m = model_with("class B {}", "app:\n  x: 1\n");
        let src = "class C { int plain = 3; }";
        assert!(navigate(&m, PATH, src, 12).is_empty());
        assert!(hover(&m, PATH, src, 12).is_none());
    }

    #[test]
    fn completion_offers_property_keys_only_inside_an_open_placeholder() {
        let m = model_with("class B {}", "app:\n  timeout: 30\n  name: x\n");
        let src = "class C { @Value(\"${\") int t; }";
        let at = src.find("${").unwrap() + 2;
        let items = completions(&m, None, PATH, src, at);
        assert_eq!(items.iter().map(|i| i.label.as_str()).collect::<Vec<_>>(), ["app.name", "app.timeout"]);
        // Closed placeholder → the caret is in plain text again.
        let closed = "class C { @Value(\"${app.name} \") int t; }";
        let after = closed.find("} ").unwrap() + 2;
        assert!(completions(&m, None, PATH, closed, after).is_empty());
    }

    #[test]
    fn completion_offers_bean_names_inside_a_qualifier() {
        let m = model_with("package com.acme;\n@Service class OrderService {}\n", "");
        let src = java("class C { @Qualifier(\"\") Object o; }");
        let at = src.find("\"\"").unwrap() + 1;
        let items = completions(&m, None, PATH, &src, at);
        assert_eq!(items[0].label, "orderService");
        assert_eq!(items[0].kind, "bean");
    }

    #[test]
    fn gutter_marks_beans_injections_and_endpoints() {
        let m = model_with("package com.acme;\n@Service class OrderServiceImpl implements OrderService {}\n", "");
        // The imports go on the `package` line, so the line numbers asserted below hold.
        let src = java("package com.acme;\n@RestController\nclass C {\n  @Autowired OrderService svc;\n  @GetMapping(\"/x\") void m() {}\n}\n");
        let marks = gutter(&m, PATH, &src);
        let inject = marks.iter().find(|g| g.kind == "inject").expect("injection mark");
        assert_eq!(inject.line, 4);
        assert_eq!(inject.targets.len(), 1);
        assert_eq!(inject.tooltip, "Injected with `orderServiceImpl`");
        let endpoint = marks.iter().find(|g| g.kind == "endpoint").expect("endpoint mark");
        assert_eq!(endpoint.line, 5);
        assert_eq!(endpoint.tooltip, "GET /x");
    }

    #[test]
    fn a_bean_declaration_points_at_where_it_is_injected() {
        // The model must know BOTH sides — the consumer whose field is the target, and
        // the impl whose name the candidate check matches on.
        let project = "package com.acme;\n@Service class C { @Autowired OrderService svc; }\n@Service class OrderServiceImpl implements OrderService {}\n";
        let m = model_with(project, "");
        let src = java("package com.acme;\n@Service class OrderServiceImpl implements OrderService {}\n");
        let marks = gutter(&m, PATH, &src);
        let bean = marks.iter().find(|g| g.kind == "bean").expect("bean mark");
        assert_eq!(bean.line, 2);
        assert_eq!(bean.targets.len(), 1);
        assert_eq!(bean.targets[0].label, "C.svc");
        assert!(bean.tooltip.contains("1 injection point"));
    }
    #[test]
    fn nothing_is_marked_beside_an_injection_of_a_type_the_project_does_not_declare() {
        // `SchemaAuthorizationManager` comes from a Spring jar. The model holds the project's own
        // beans by design, so it can say neither that a bean exists nor that none does — and a
        // gutter mark whose whole content is "I cannot tell you" beside every `@Autowired` field of
        // a framework type is the mark you learn to stop reading.
        let m = model_with("package com.acme;\n@Service class OrderServiceImpl implements OrderService {}\n", "");
        let src = java(
            "package com.acme;\n@Service class C {\n  @Autowired SchemaAuthorizationManager mgr;\n}\n",
        );
        assert!(
            !gutter(&m, PATH, &src).iter().any(|g| g.kind == "inject"),
            "a library type the model cannot see gets no mark at all"
        );
    }

    #[test]
    fn a_project_type_with_no_bean_is_still_reported() {
        // The other half, and the one that must survive: here the model HAS looked everywhere a
        // bean of this type could be declared, so "none" is a finding rather than a shrug.
        let project = "package com.acme;\ninterface OrderService {}\n@Service class C { @Autowired OrderService svc; }\n";
        let m = model_with(project, "");
        let src = java("package com.acme;\n@Service class C {\n  @Autowired OrderService svc;\n}\n");
        let inject = gutter(&m, PATH, &src)
            .into_iter()
            .find(|g| g.kind == "inject")
            .expect("a project type with no bean is worth a mark");
        assert!(inject.tooltip.contains("no matching bean found"), "{}", inject.tooltip);
    }

    #[test]
    fn a_bean_factory_method_satisfies_an_injection_of_its_return_type() {
        // The shape a Spring Boot configuration is written in: the TYPE comes from a jar, the
        // BEAN is declared here by a `@Bean` method. The candidate check has to match on the
        // method's return type, not on whether the project declares the type.
        let project = "package com.acme;\n\
             @AutoConfiguration class SecurityConfigurator {\n\
             \x20 @Bean SchemaAuthorizationManager schema_authorization_manager() { return null; }\n\
             }\n";
        let m = model_with(project, "");
        let src = java(
            "package com.acme;\n@Service class C {\n  @Autowired SchemaAuthorizationManager mgr;\n}\n",
        );
        let inject = gutter(&m, PATH, &src)
            .into_iter()
            .find(|g| g.kind == "inject")
            .expect("the `@Bean` method declares one");
        assert!(
            inject.tooltip.contains("schema_authorization_manager"),
            "expected the factory method's bean, got {}",
            inject.tooltip
        );
    }

    // ── a bean an allowlisted dependency declares ─────────────────────────────────────────────

    fn library_bean(name: &str, fqcn: &str, conditions: &[&str]) -> crate::model::BeanDef {
        let group = crate::library_beans::LibraryBeanGroup {
            group_id: "it.acme".into(),
            artifact_id: "shared-security".into(),
            version: "2.1.0".into(),
            beans: vec![crate::library_beans::LibraryBean {
                name: name.into(),
                fqcn: fqcn.into(),
                stereotype: "@Bean".into(),
                declared_in: "it.acme.SecurityConfigurator".into(),
                conditions: conditions.iter().map(|c| c.to_string()).collect(),
                primary: false,
            }],
        };
        crate::library_beans::bean_defs_of(&group).remove(0)
    }

    #[test]
    fn a_bean_from_an_allowlisted_dependency_satisfies_an_injection() {
        // The shape the whole feature exists for: the type comes from a jar AND so does the
        // `@Bean` method that declares it, because the configuration class lives in a shared
        // starter. Nothing in the project's own sources mentions either.
        let mut m = model_with("package com.acme;\n@Service class C {}\n", "");
        m.library_beans = vec![library_bean(
            "schema_authorization_manager",
            "it.acme.SchemaAuthorizationManager",
            &[],
        )];
        let src = java(
            "package com.acme;\n@Service class C {\n  @Autowired SchemaAuthorizationManager mgr;\n}\n",
        );
        let inject = gutter(&m, PATH, &src)
            .into_iter()
            .find(|g| g.kind == "inject")
            .expect("an allowlisted dependency's bean is a candidate");
        assert!(inject.tooltip.contains("schema_authorization_manager"), "{}", inject.tooltip);
        assert!(
            inject.tooltip.contains("it.acme:shared-security"),
            "it has to say whose bean it is: {}",
            inject.tooltip
        );
    }

    #[test]
    fn a_conditional_library_bean_says_on_what_terms() {
        // A library bean is what Spring **may** register. Reporting it as a flat certainty is the
        // one thing this must not do — `@ConditionalOnMissingBean` is precisely the case where
        // your own bean wins instead.
        let mut m = model_with("package com.acme;\n@Service class C {}\n", "");
        m.library_beans = vec![library_bean(
            "schema_authorization_manager",
            "it.acme.SchemaAuthorizationManager",
            &["@ConditionalOnMissingBean"],
        )];
        let src = java(
            "package com.acme;\n@Service class C {\n  @Autowired SchemaAuthorizationManager mgr;\n}\n",
        );
        let inject = gutter(&m, PATH, &src)
            .into_iter()
            .find(|g| g.kind == "inject")
            .expect("still a candidate");
        assert!(inject.tooltip.contains("ConditionalOnMissingBean"), "{}", inject.tooltip);
    }

    #[test]
    fn the_project_s_own_bean_is_not_labelled_with_an_artifact() {
        // The label is for what came from somewhere else. On your own bean it would be noise on
        // every injection in the codebase.
        let project = "package com.acme;\ninterface OrderService {}\n@Service class OrderServiceImpl implements OrderService {}\n";
        let m = model_with(project, "");
        let src = java("package com.acme;\n@Service class C {\n  @Autowired OrderService svc;\n}\n");
        let inject = gutter(&m, PATH, &src).into_iter().find(|g| g.kind == "inject").unwrap();
        assert_eq!(inject.tooltip, "Injected with `orderServiceImpl`");
    }

}
