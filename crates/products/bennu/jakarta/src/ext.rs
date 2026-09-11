//! The extension — what a host registers.
//!
//! Its model is one thing: [`Discovery`], the bundles Bean Validation will actually read. Every
//! answer here is that plus the file in front of you, and the reason the model is worth building at
//! all is that the question it answers cannot be answered from the file: *is this key one the
//! validator can find?* — which depends on a bundle base name that may be written in a `@Bean` on
//! the other side of the project.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};

use bennu_complete::prelude::{Proposal, Proposals};
use bennu_ext::prelude::{
    ExtEntry, ExtHover, ExtStat, ExtTarget, FileCtx, FrameworkExtension, ProjectScan,
};
use bennu_i18n::prelude::Bundle;
use bennu_proto::prelude::{CapabilitySet, CompletionItem, Diagnostic};

use crate::bundles::{discover, Discovery, Origin};
use crate::constraints::{constraint, is_provider_key, verdict, Verdict, CONSTRAINTS};
use crate::refs::{self, ConstraintUse};

/// How many keys a completion popup offers. Past this it is not a list anybody reads.
const MAX_COMPLETIONS: usize = 200;

/// The diagnostic codes this extension raises. Namespaced by the extension id, like every other.
pub const CODE_UNKNOWN_KEY: &str = "jakarta.validation.unknown-key";
pub const CODE_WRONG_TYPE: &str = "jakarta.validation.wrong-type";
pub const CODE_POINTLESS: &str = "jakarta.validation.no-effect";
pub const CODE_MISSING_BUNDLE: &str = "jakarta.validation.missing-bundle";

#[derive(Default)]
pub struct ValidationExtension {
    found: RwLock<Arc<Discovery>>,
    /// Every constraint in the project, for the catalogue. Rebuilt with the model.
    catalogue: RwLock<Arc<Vec<ExtEntry>>>,
    /// Whether a scan has run. Kept apart from "the discovery has anything in it": a project with
    /// no validation bundles is *ready and empty*, and reporting it as never-ready leaves a panel
    /// waiting forever.
    scanned: AtomicBool,
}

impl ValidationExtension {
    pub fn new() -> Self {
        Self::default()
    }

    fn discovery(&self) -> Option<Arc<Discovery>> {
        let found = self.found.read().ok()?;
        Some(Arc::clone(&found))
    }

    /// The bundle base names constraint messages are redirected to: every validation bundle except the
    /// one the specification names, which a provider reads without being told.
    ///
    /// For a host that interpolates messages itself — the DTO Lab's JVM — and so has to be told what
    /// the project's own `@Bean` told the validator.
    pub fn redirected_bundles(&self) -> Vec<String> {
        self.discovery()
            .map(|found| {
                found
                    .bundles
                    .iter()
                    .map(|b| b.base.clone())
                    .filter(|base| base.as_str() != crate::bundles::SPEC_BASE)
                    .collect()
            })
            .unwrap_or_default()
    }

    /// The constraints in a buffer, parsed. `None` when the file is not Java or does not parse —
    /// which is an ordinary state for a buffer being typed in, not an error.
    fn uses(&self, ctx: &FileCtx<'_>) -> Option<Vec<ConstraintUse>> {
        if ctx.extension() != "java" {
            return None;
        }
        // Cheap reject before the parse: a file with no `@` is most of a legacy project.
        if !ctx.source.contains('@') {
            return None;
        }
        let tree = bennu_java::prelude::parse_java(ctx.source)?;
        Some(refs::constraints_in(tree.root_node(), ctx.source))
    }
}

impl FrameworkExtension for ValidationExtension {
    fn id(&self) -> &'static str {
        "jakarta.validation"
    }

    fn display_name(&self) -> &'static str {
        "Bean Validation"
    }

    fn applies(&self, caps: &CapabilitySet) -> bool {
        caps.bean_validation
    }

    fn reindex(&self, scan: &ProjectScan<'_>) {
        let resources: Vec<(String, String)> =
            scan.resources.iter().map(|f| (slash(&f.path), f.text.clone())).collect();
        let java: Vec<(String, String)> =
            scan.java.iter().map(|f| (slash(&f.path), f.text.clone())).collect();
        let found = Arc::new(discover(&resources, &java));

        // The catalogue is built here rather than on demand: it is a walk of every Java file, and
        // a panel that costs a full parse of the project to open is a panel nobody opens twice.
        let mut rows: Vec<ExtEntry> = Vec::new();
        for (path, text) in &java {
            let Some(tree) = bennu_java::prelude::parse_java(text) else { continue };
            for used in refs::constraints_in(tree.root_node(), text) {
                rows.push(entry_for(&used, path, text, &found));
            }
        }
        rows.sort_by(|a, b| a.primary.cmp(&b.primary).then(a.secondary.cmp(&b.secondary)));

        if let Ok(mut slot) = self.found.write() {
            *slot = found;
        }
        if let Ok(mut slot) = self.catalogue.write() {
            *slot = Arc::new(rows);
        }
        self.scanned.store(true, Ordering::Relaxed);
    }

    fn is_ready(&self) -> bool {
        self.scanned.load(Ordering::Relaxed)
    }

    /// Three things, and each one is invisible until somebody sees it in production.
    ///
    /// 1. **A message key no validation bundle declares.** Bean Validation renders the key itself,
    ///    braces included, into the form the user is looking at.
    /// 2. **A constraint the engine has no validator for** — `@NotBlank` on an `int`. Hibernate
    ///    Validator refuses this at *startup*, so it is not a wrong message, it is an application
    ///    that does not come up.
    /// 3. **A bundle named by the code that nothing implements**, which turns every custom message
    ///    in the application into (1) at once.
    ///
    /// Silent about (1) when the project has no validation bundle at all: that means the messages
    /// are literals or defaults, not that every key is wrong.
    fn diagnostics(&self, ctx: &FileCtx<'_>) -> Vec<Diagnostic> {
        let Some(found) = self.discovery() else { return Vec::new() };
        let Some(uses) = self.uses(ctx) else { return Vec::new() };
        let mut out = Vec::new();

        for used in &uses {
            let Some(c) = constraint(&used.name) else { continue };

            if !found.is_empty() {
                if let Some(message) = &used.message {
                    for key in refs::keys_in_message(message, c) {
                        if found.declares(&key.key) || is_provider_key(&key.key) {
                            continue;
                        }
                        out.push(Diagnostic {
                            message: format!(
                                "no validation bundle declares `{}` — the message will render as \
                                 `{{{}}}`",
                                key.key, key.key
                            ),
                            severity: "warning".to_string(),
                            code: CODE_UNKNOWN_KEY.to_string(),
                            start: key.start,
                            end: key.end,
                        });
                    }
                }
            }

            let Some(written) = used.target_type.as_deref() else { continue };
            match verdict(c, written) {
                Verdict::Cannot(why) => out.push(Diagnostic {
                    message: format!(
                        "@{} cannot be applied to `{written}` — {why}. Hibernate Validator \
                         refuses this at startup",
                        used.name
                    ),
                    severity: "error".to_string(),
                    code: CODE_WRONG_TYPE.to_string(),
                    start: used.start,
                    end: used.end,
                }),
                Verdict::Pointless(why) => out.push(Diagnostic {
                    message: format!("@{} has no effect here — {why}", used.name),
                    severity: "warning".to_string(),
                    code: CODE_POINTLESS.to_string(),
                    start: used.start,
                    end: used.end,
                }),
                Verdict::Fits | Verdict::Unknown => {}
            }
        }

        // The bundle the code names, reported **on the line that names it** rather than on the
        // hundred messages it breaks.
        let path = ctx.path_str();
        for bundle in found.unimplemented() {
            let Some(site) = &bundle.declared_in else { continue };
            if site.file != path {
                continue;
            }
            out.push(Diagnostic {
                message: format!(
                    "no `{}.properties` in this project — every message resolved through this \
                     bundle will render as its own key",
                    bundle.base
                ),
                severity: "warning".to_string(),
                code: CODE_MISSING_BUNDLE.to_string(),
                start: site.offset,
                end: site.offset + bundle.base.len(),
            });
        }
        out
    }

    /// The keys the validator can actually resolve, offered inside a constraint message.
    ///
    /// Deliberately not every key in the project: offering one from `messages.properties` here
    /// would be offering a key that resolves to nothing at runtime, which is worse than offering
    /// none — the popup would be teaching the mistake.
    fn completions(&self, ctx: &FileCtx<'_>, offset: usize) -> Vec<CompletionItem> {
        let Some(found) = self.discovery() else { return Vec::new() };
        let Some(uses) = self.uses(ctx) else { return Vec::new() };
        let Some((typed, start, end)) = refs::key_prefix_at(&uses, offset) else {
            return Vec::new();
        };

        let mut out = Proposals::new(MAX_COMPLETIONS);
        for key in &found.keys {
            if !key.starts_with(&typed) {
                continue;
            }
            out.offer(Proposal::new(key.clone(), "message-key").detail(text_of(&found, key)));
        }
        out.into_items()
            .into_iter()
            .map(|item| CompletionItem {
                replace_start: Some(start),
                replace_end: Some(end),
                ..item
            })
            .collect()
    }

    /// What the message will actually say, and — for a constraint rather than a key — what it
    /// checks and what it can be put on.
    fn hover(&self, ctx: &FileCtx<'_>, offset: usize) -> Option<ExtHover> {
        let found = self.discovery()?;
        let uses = self.uses(ctx)?;

        // A key first: it is the narrower target, and the caret is inside the annotation either way.
        if let Some((used, c)) = refs::at_offset(&uses, offset) {
            if let Some(message) = &used.message {
                if let Some(key) =
                    refs::keys_in_message(message, c).into_iter().find(|k| offset >= k.start && offset <= k.end)
                {
                    let (signature, doc) = if is_provider_key(&key.key) {
                        (String::new(), "The provider's own default message.".to_string())
                    } else {
                        match declaration(&found, &key.key) {
                            Some((path, value)) => (
                                value,
                                format!("Declared in {}", path.rsplit('/').next().unwrap_or(&path)),
                            ),
                            None => (
                                String::new(),
                                format!(
                                    "No validation bundle declares this — the message will render \
                                     as `{{{}}}`.",
                                    key.key
                                ),
                            ),
                        }
                    };
                    return Some(ExtHover { title: key.key.clone(), signature, doc });
                }
            }
        }

        // Otherwise the constraint itself.
        let used = uses.iter().find(|u| offset >= u.start && offset <= u.end)?;
        let c = constraint(&used.name)?;
        let mut doc = c.doc.to_string();
        if !c.attributes.is_empty() {
            doc.push_str(&format!(
                " Attributes: {}. A message may interpolate any of them as `{{name}}` without \
                 naming a bundle key.",
                c.attributes.join(", ")
            ));
        }
        Some(ExtHover {
            title: format!("@{}", c.name),
            signature: format!("{}.{}", c.package, c.name),
            doc,
        })
    }

    /// The bundle files declaring the key under the caret — one target per locale.
    fn navigate(&self, ctx: &FileCtx<'_>, offset: usize) -> Vec<ExtTarget> {
        let Some(found) = self.discovery() else { return Vec::new() };
        let Some(uses) = self.uses(ctx) else { return Vec::new() };
        let Some((used, c)) = refs::at_offset(&uses, offset) else { return Vec::new() };
        let Some(message) = &used.message else { return Vec::new() };
        let Some(key) = refs::keys_in_message(message, c)
            .into_iter()
            .find(|k| offset >= k.start && offset <= k.end)
        else {
            return Vec::new();
        };
        declarations(&found, &key.key)
    }

    fn catalog(&self, kind: &str) -> Vec<ExtEntry> {
        if kind != "constraints" {
            return Vec::new();
        }
        self.catalogue.read().map(|c| (**c).clone()).unwrap_or_default()
    }

    fn stats(&self) -> Vec<ExtStat> {
        let constraints = self.catalogue.read().map(|c| c.len()).unwrap_or(0);
        let found = self.discovery();
        let bundles = found.as_ref().map(|f| f.bundles.len()).unwrap_or(0);
        let keys = found.as_ref().map(|f| f.keys.len()).unwrap_or(0);
        vec![
            ExtStat {
                label: "Constraints".to_string(),
                value: constraints,
                catalog: Some("constraints".to_string()),
            },
            // No catalog to drill into: the bundles are two or three rows and their whole story is
            // told by the number plus the diagnostic on the line that names one.
            ExtStat { label: "Validation bundles".to_string(), value: bundles, catalog: None },
            ExtStat { label: "Validation messages".to_string(), value: keys, catalog: None },
        ]
    }
}

/// One catalogue row: the constrained member, what constrains it, and whether its message resolves.
fn entry_for(used: &ConstraintUse, path: &str, text: &str, found: &Discovery) -> ExtEntry {
    let owner = used.owner.clone().unwrap_or_default();
    let member = used.target_name.clone().unwrap_or_default();
    let mut tags = Vec::new();
    if let Some(t) = &used.target_type {
        tags.push(t.clone());
    }

    let detail = match (&used.message, constraint(&used.name)) {
        (Some(message), Some(c)) => {
            let keys = refs::keys_in_message(message, c);
            // The tag that makes the panel worth opening: which messages will render as their own
            // key when somebody actually trips the constraint.
            for key in &keys {
                if !found.declares(&key.key) && !is_provider_key(&key.key) {
                    tags.push(format!("missing: {}", key.key));
                }
            }
            message.text.clone()
        }
        // No message written: the provider's default, which always resolves.
        _ => "default message".to_string(),
    };

    ExtEntry {
        id: format!("{owner}#{member}@{}", used.name),
        primary: if member.is_empty() { owner.clone() } else { format!("{owner}.{member}") },
        secondary: detail,
        kind: format!("@{}", used.name),
        file: Some(path.to_string()),
        offset: Some(used.start),
        line: Some(text[..used.start.min(text.len())].bytes().filter(|&b| b == b'\n').count() as u32 + 1),
        tags,
        children: Vec::new(),
    }
}

/// The first declaration of `key`, as `(path, value)`.
fn declaration(found: &Discovery, key: &str) -> Option<(String, String)> {
    declarations_raw(found, key).into_iter().next()
}

/// Every file declaring `key`, as go-to targets.
fn declarations(found: &Discovery, key: &str) -> Vec<ExtTarget> {
    read_bundles(found)
        .into_iter()
        .filter_map(|b| {
            let entry = b.entry(key)?;
            Some(ExtTarget {
                file: b.path.clone(),
                offset: entry.start,
                label: b.file_name().to_string(),
                detail: entry.value.clone(),
            })
        })
        .collect()
}

fn declarations_raw(found: &Discovery, key: &str) -> Vec<(String, String)> {
    read_bundles(found)
        .into_iter()
        .filter_map(|b| b.entry(key).map(|e| (b.path.clone(), e.value.clone())))
        .collect()
}

/// The text `key` resolves to, for a completion row's detail. Empty when it resolves to nothing,
/// which cannot happen for a key that came out of the discovery.
fn text_of(found: &Discovery, key: &str) -> String {
    declaration(found, key).map(|(_, value)| value).unwrap_or_default()
}

/// Re-read the discovered bundle files.
///
/// The discovery keeps paths and keys, not parsed entries: the values are wanted by a hover and a
/// go-to, which happen once each, and holding every translation of every message in memory to save
/// a read of a file the OS has cached is the wrong trade on a legacy tree.
fn read_bundles(found: &Discovery) -> Vec<Bundle> {
    found
        .bundles
        .iter()
        .flat_map(|b| b.files.iter())
        .filter_map(|path| std::fs::read_to_string(path).ok().map(|t| Bundle::parse(path, &t)))
        .collect()
}

fn slash(path: &std::path::Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

/// Every constraint this crate knows, for a caller that wants the vocabulary rather than the
/// project's use of it.
pub fn known_constraints() -> &'static [crate::constraints::Constraint] {
    CONSTRAINTS
}

/// Which origin, in words — used by the overview.
pub fn origin_label(origin: Origin) -> &'static str {
    origin.label()
}
