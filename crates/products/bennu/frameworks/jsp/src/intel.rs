//! The editor's answers on a JSP, given the tag libraries it declares.
//!
//! Completion, checks, hover and go-to, all from the same two inputs: the page's
//! `<%@ taglib %>` directives and the TLDs they resolve to. The tags in a legacy page
//! are 80% of what is written in it and until now the editor knew nothing about any of
//! them — no list of what exists, no word about what an attribute means, and no way to
//! reach the file that would have said.
//!
//! **The tag scan is XML's.** A taglib tag is an XML tag, and `bennu-xml` already owns
//! a tolerant scanner over a buffer being typed plus the rule that says which of the
//! four caret positions you are in. A JSP's own constructs are invisible to it — a
//! `<%` is not a tag name, so scriptlets and directives simply are not tags — which is
//! exactly the behaviour wanted here.
//!
//! ## Nothing is reported without the library that would know
//!
//! Every check is gated the same way the schema checks next door are, and for the same
//! reason — a false report costs more than a missed one:
//!
//! - no TLDs resolved at all (a project whose dependencies have not been resolved yet)
//!   → silence, not a page full of warnings;
//! - a prefix the page does not declare → **never reported**, because a legacy page
//!   inherits its prefixes from an included fragment more often than not, and the
//!   include is invisible from here;
//! - a tag declaring `<dynamic-attributes>`, or a tag *file* (whose attributes live in
//!   the `.tag` and not in the TLD) → its attributes are unknown, not empty.

use std::sync::Arc;

use bennu_complete::prelude::{matches_ignore_case, Proposal, Proposals, DEFAULT_CAP};
use bennu_ext::prelude::{ExtHover, ExtTarget};
use bennu_proto::prelude::{CompletionItem, Diagnostic};
use bennu_xml::prelude::{classify, scan, Attr, Caret, Tag, TagKind};

use crate::catalog::TaglibCatalog;
use crate::directives::{taglib_directives, TaglibDirective};
use crate::tld::{AttrDecl, TagDecl, Taglib};

/// A prefix the page binds, and the library it resolved to (if any).
struct Bound<'a> {
    prefix: &'a str,
    lib: Option<&'a Arc<Taglib>>,
    directive: &'a TaglibDirective,
}

/// Resolve every directive in the page once — the input all four answers share.
fn bindings<'a>(cat: &'a TaglibCatalog, directives: &'a [TaglibDirective]) -> Vec<Bound<'a>> {
    directives
        .iter()
        .map(|d| Bound { prefix: &d.prefix, lib: cat.resolve(&d.uri), directive: d })
        .collect()
}

/// The part of a tag the caret is on — the only two a tag library has anything to say about.
enum InTag<'a> {
    /// The tag's own name (`s:iterator`).
    Name,
    /// An attribute's **name** (`value=`), never its value.
    Attr(&'a Attr),
}

/// Where in a tag the caret is, or `None` for anywhere else inside it.
///
/// Load-bearing, and the regression that proved it: a tag's span runs from its `<` to its `>`,
/// so "the caret is inside this tag" is true of every attribute VALUE too — of the OGNL in
/// `test="%{…}"`, of the path in `<s:include value="…">`, of the action in `<s:form action="…">`.
/// Answering those with the tag's TLD declaration meant this extension, which runs early in the
/// go-to chain, swallowed every other resolver's gesture on a JSP. A tag library knows what a tag
/// and an attribute ARE; it knows nothing about what is written inside one.
fn in_tag<'a>(tag: &'a Tag, offset: usize) -> Option<InTag<'a>> {
    if offset >= tag.name_start && offset <= tag.name_end {
        return Some(InTag::Name);
    }
    tag.attrs
        .iter()
        .find(|a| offset >= a.name_start && offset <= a.name_end)
        .map(InTag::Attr)
}

/// The library bound to the prefix of `name` (`s:iterator` → the struts library), with the
/// tag's local name.
fn split<'a, 'b>(bound: &'b [Bound<'a>], name: &'b str) -> Option<(&'b Bound<'a>, &'b str)> {
    let (prefix, local) = name.split_once(':')?;
    let b = bound.iter().find(|b| b.prefix == prefix)?;
    Some((b, local))
}

// ── Completion ──────────────────────────────────────────────────────────────

/// Tag names, attribute names, and the `uri` of a taglib directive.
pub fn completions(cat: &TaglibCatalog, source: &str, offset: usize) -> Vec<CompletionItem> {
    if cat.is_empty() {
        return Vec::new();
    }
    let directives = taglib_directives(source);
    let bound = bindings(cat, &directives);
    let mut out = Proposals::new(DEFAULT_CAP);

    // A `uri="…"` being typed in a directive. Handled before the tag scan because a
    // directive is not a tag and the scanner does not see it at all.
    if let Some(d) = directives.iter().find(|d| within(d.uri_span, offset)) {
        let typed = &source[d.uri_span.0..offset.min(d.uri_span.1)];
        for lib in cat.all() {
            for name in uri_forms(lib) {
                if matches_ignore_case(typed, &name) {
                    let detail = short_source(&lib.source);
                    if !out.offer(Proposal::new(name, "taglib").detail(detail)) && out.is_full() {
                        return out.into_items();
                    }
                }
            }
        }
        return out.into_items();
    }

    let doc = scan(source);
    let Some(caret) = classify(&doc, source, offset) else { return Vec::new() };
    match &caret {
        // Closing a tag has one right answer and the editor already types it.
        Caret::ElementName { closing: true, .. } => Vec::new(),
        Caret::ElementName { prefix, .. } => {
            // `s:it` narrows to one library; a bare `<` offers every declared one, capped —
            // which is what makes the list useful the moment a prefix is typed and merely long
            // before that.
            let (only, typed) = match prefix.split_once(':') {
                Some((p, local)) => (Some(p), local),
                None => (None, prefix.as_str()),
            };
            for b in bound.iter().filter(|b| only.is_none_or(|p| b.prefix == p)) {
                let Some(lib) = b.lib else { continue };
                for tag in &lib.tags {
                    if !matches_ignore_case(typed, &tag.name) {
                        continue;
                    }
                    // The label carries the prefix whether or not one was typed: the token the
                    // editor replaces is the whole name (`:` is a name character in a tag), so
                    // inserting the bare local name would eat the `s:` in front of it.
                    let label = format!("{}:{}", b.prefix, tag.name);
                    let p = Proposal::new(label, "tag").detail(tag_detail(tag));
                    if !out.offer(p) && out.is_full() {
                        return out.into_items();
                    }
                }
            }
            out.into_items()
        }
        Caret::AttrName { element, prefix, .. } => {
            let Some((b, local)) = split(&bound, element) else { return Vec::new() };
            let Some(tag) = b.lib.and_then(|l| l.tag(local)) else { return Vec::new() };
            // Already written ones are gone: an attribute may appear once.
            let written: Vec<&str> =
                doc.tag_at(offset).map(|t| t.attrs.iter().map(|a| a.name.as_str()).collect()).unwrap_or_default();
            for attr in &tag.attrs {
                if written.contains(&attr.name.as_str()) || !matches_ignore_case(prefix, &attr.name) {
                    continue;
                }
                let p = Proposal::new(attr.name.clone(), "attribute").detail(attr_detail(attr));
                if !out.offer(p) && out.is_full() {
                    break;
                }
            }
            out.into_items()
        }
        Caret::AttrValue { .. } | Caret::Content { .. } => Vec::new(),
    }
}

/// How a page may name this library: its declared URI, and — for the older ones, which
/// declare none — the file it lives in, which is what the page writes instead.
fn uri_forms(lib: &Taglib) -> Vec<String> {
    if !lib.uri.is_empty() {
        return vec![lib.uri.clone()];
    }
    match lib.source.replace('\\', "/").rsplit_once('/') {
        Some((_, file)) => vec![file.to_string()],
        None => Vec::new(),
    }
}

fn tag_detail(tag: &TagDecl) -> String {
    let required = tag.attrs.iter().filter(|a| a.required).count();
    match required {
        0 => tag.body_content.clone(),
        1 => "1 required attribute".to_string(),
        n => format!("{n} required attributes"),
    }
}

fn attr_detail(attr: &AttrDecl) -> String {
    let ty = simple_type(&attr.ty);
    match (attr.required, ty.is_empty()) {
        (true, true) => "required".to_string(),
        (true, false) => format!("{ty} · required"),
        (false, true) => String::new(),
        (false, false) => ty,
    }
}

/// `java.lang.String` → `String`. The package is noise in a one-line detail.
fn simple_type(ty: &str) -> String {
    ty.rsplit_once('.').map_or_else(|| ty.to_string(), |(_, s)| s.to_string())
}

/// The file name plus the jar or folder above it — enough to tell two `core.tld`s apart.
fn short_source(source: &str) -> String {
    let norm = source.replace('\\', "/");
    let mut parts = norm.rsplit('/');
    let file = parts.next().unwrap_or_default();
    match parts.next() {
        Some(dir) => format!("{dir}/{file}"),
        None => file.to_string(),
    }
}

// ── Checks ──────────────────────────────────────────────────────────────────

/// What the page says that its own libraries do not have.
pub fn diagnostics(cat: &TaglibCatalog, source: &str) -> Vec<Diagnostic> {
    if cat.is_empty() {
        return Vec::new();
    }
    let directives = taglib_directives(source);
    let bound = bindings(cat, &directives);
    let mut out = Vec::new();

    for b in &bound {
        // A library whose descriptor is right there and merely unreadable is not a library the
        // page got wrong, and saying so would send someone looking for a file they can see.
        if b.lib.is_none() && !b.directive.uri.is_empty() && !cat.is_unreadable(&b.directive.uri) {
            out.push(warn(
                format!(
                    "No tag library on this project's classpath declares “{}”.",
                    b.directive.uri
                ),
                "unknown-taglib-uri",
                b.directive.uri_span,
            ));
        }
    }

    let doc = scan(source);
    for tag in doc.tags.iter().filter(|t| t.kind != TagKind::Close) {
        let Some((b, local)) = split(&bound, &tag.name) else { continue };
        let Some(lib) = b.lib else { continue };
        let Some(decl) = lib.tag(local) else {
            out.push(warn(
                format!("The “{}” library declares no tag “{local}”.", b.prefix),
                "unknown-tag",
                (tag.name_start, tag.name_end),
            ));
            continue;
        };
        if !decl.attrs_are_closed() {
            continue;
        }
        for attr in &tag.attrs {
            if decl.attr(&attr.name).is_none() {
                out.push(warn(
                    format!("<{}> has no attribute “{}”.", tag.name, attr.name),
                    "unknown-tag-attribute",
                    (attr.name_start, attr.name_end),
                ));
            }
        }
        // A tag still being typed has not had the chance to carry its required attributes yet.
        if tag.closed {
            let missing: Vec<&str> = decl
                .attrs
                .iter()
                .filter(|a| a.required && !tag.attrs.iter().any(|w| w.name == a.name))
                .map(|a| a.name.as_str())
                .collect();
            if !missing.is_empty() {
                out.push(warn(
                    format!("<{}> requires {}.", tag.name, join(&missing)),
                    "missing-required-attribute",
                    (tag.name_start, tag.name_end),
                ));
            }
        }
    }
    out
}

fn join(names: &[&str]) -> String {
    match names {
        [one] => format!("“{one}”"),
        [head @ .., last] => {
            let head = head.iter().map(|n| format!("“{n}”")).collect::<Vec<_>>().join(", ");
            format!("{head} and “{last}”")
        }
        [] => String::new(),
    }
}

fn warn(message: String, code: &str, span: (usize, usize)) -> Diagnostic {
    Diagnostic {
        message,
        severity: "warning".to_string(),
        code: code.to_string(),
        start: span.0,
        end: span.1,
    }
}

// ── Go to declaration ───────────────────────────────────────────────────────

/// The TLD behind what the caret is on: a directive's `uri`, a tag name, an attribute name.
///
/// This is the answer to the gesture that had none — Ctrl+click on `uri="/struts-tags"`
/// used to resolve only when the URI happened to also be a path that existed, which is
/// true of the project's own TLDs and of none of the ones inside jars.
pub fn navigate(cat: &TaglibCatalog, source: &str, offset: usize) -> Vec<ExtTarget> {
    let directives = taglib_directives(source);
    let bound = bindings(cat, &directives);

    if let Some(b) = bound.iter().find(|b| within(b.directive.uri_span, offset)) {
        let Some(lib) = b.lib else { return Vec::new() };
        return vec![ExtTarget {
            file: lib.source.clone(),
            offset: 0,
            label: short_source(&lib.source),
            detail: tag_count(lib),
        }];
    }

    let doc = scan(source);
    let Some(tag) = doc.tag_at(offset) else { return Vec::new() };
    let Some(part) = in_tag(tag, offset) else { return Vec::new() };
    let Some((b, local)) = split(&bound, &tag.name) else { return Vec::new() };
    let Some(lib) = b.lib else { return Vec::new() };
    let Some(decl) = lib.tag(local) else { return Vec::new() };

    // On an attribute name, land on the attribute's own declaration rather than the tag's.
    let (target_offset, label) = match part {
        InTag::Attr(a) => match decl.attr(&a.name) {
            Some(attr) => (attr.offset, format!("{}:{} / {}", b.prefix, local, attr.name)),
            // An attribute the library does not declare has no declaration to open, and the
            // tag's would be a different answer to a different question.
            None => return Vec::new(),
        },
        InTag::Name => (decl.offset, format!("{}:{}", b.prefix, local)),
    };
    vec![ExtTarget {
        file: lib.source.clone(),
        offset: target_offset,
        label,
        detail: short_source(&lib.source),
    }]
}

fn tag_count(lib: &Taglib) -> String {
    match lib.tags.len() {
        1 => "1 tag".to_string(),
        n => format!("{n} tags"),
    }
}

// ── Hover ───────────────────────────────────────────────────────────────────

/// What a tag or attribute is for — the TLD's own prose, which is where the framework's
/// documentation was written down in the first place.
pub fn hover(cat: &TaglibCatalog, source: &str, offset: usize) -> Option<ExtHover> {
    let directives = taglib_directives(source);
    let bound = bindings(cat, &directives);

    if let Some(b) = bound.iter().find(|b| within(b.directive.uri_span, offset)) {
        let lib = b.lib?;
        return Some(ExtHover {
            title: b.directive.uri.clone(),
            signature: format!("{} · {}", short_source(&lib.source), tag_count(lib)),
            doc: lib.description.clone(),
        });
    }

    let doc = scan(source);
    let tag = doc.tag_at(offset)?;
    // Same rule as `navigate`: a tag library speaks about a tag and an attribute, not about
    // what is written inside one. A hover over an attribute VALUE belongs to whoever knows the
    // expression language — here it would shadow the action-property card.
    let part = in_tag(tag, offset)?;
    let (b, local) = split(&bound, &tag.name)?;
    let decl = b.lib?.tag(local)?;
    Some(match part {
        InTag::Attr(a) => {
            let attr = decl.attr(&a.name)?;
            ExtHover {
                title: attr.name.clone(),
                signature: attr_signature(attr),
                doc: attr.description.clone(),
            }
        }
        InTag::Name => ExtHover {
            title: format!("{}:{}", b.prefix, local),
            signature: decl.implementation.clone(),
            doc: decl.description.clone(),
        },
    })
}

fn attr_signature(attr: &AttrDecl) -> String {
    let mut parts = Vec::new();
    if !attr.ty.is_empty() {
        parts.push(attr.ty.clone());
    }
    parts.push(if attr.required { "required" } else { "optional" }.to_string());
    if attr.rtexprvalue {
        parts.push("accepts an expression".to_string());
    }
    parts.join(" · ")
}

/// Is the caret in this span, its far edge included (a caret sitting just after the last
/// character of a value is still in it)?
///
/// `(0, 0)` is [`TaglibDirective`]'s "there is no such attribute" and never a real span — a
/// directive's value cannot start at byte 0, since `<%@ taglib ` comes first. An **empty**
/// value (`uri=""`, the caret mid-typing) is a real span with equal ends, and it is the one
/// position where the completion matters most.
fn within(span: (usize, usize), offset: usize) -> bool {
    span != (0, 0) && offset >= span.0 && offset <= span.1
}

#[cfg(test)]
mod tests {
    use super::*;

    const TLD: &str = r"<taglib>
  <uri>/struts-tags</uri>
  <tag>
    <name>iterator</name>
    <info>Iterate</info>
    <attribute><name>value</name><required>false</required></attribute>
    <attribute><name>var</name><required>true</required></attribute>
  </tag>
</taglib>";

    fn catalog() -> TaglibCatalog {
        TaglibCatalog::build(&[("/p/WEB-INF/struts-tags.tld".into(), TLD.into())], &[])
    }

    const PAGE: &str = concat!(
        "<%@ taglib prefix=\"s\" uri=\"/struts-tags\"%>\n",
        "<s:iterator var=\"x\" value=\"%{list}\"></s:iterator>\n",
    );

    #[test]
    fn a_declared_prefix_completes_its_own_tags() {
        let at = PAGE.find("<s:iterator").expect("tag") + "<s:it".len();
        let items = completions(&catalog(), PAGE, at);
        // With the prefix, because the whole `s:it` is what gets replaced.
        assert!(items.iter().any(|i| i.label == "s:iterator"), "{items:?}");
    }

    #[test]
    fn attributes_come_from_the_tld_and_the_written_ones_are_gone() {
        let at = PAGE.find(" value=\"%{list}\"").expect("attr") + 1;
        let items = completions(&catalog(), PAGE, at);
        // `var` is already written on this tag, so offering it would offer the impossible.
        assert!(!items.iter().any(|i| i.label == "var"));
    }

    #[test]
    fn a_uri_being_typed_completes_from_the_catalog() {
        let page = "<%@ taglib prefix=\"s\" uri=\"\"%>";
        let at = page.find("uri=\"").expect("uri") + 5;
        let items = completions(&catalog(), page, at);
        assert!(items.iter().any(|i| i.label == "/struts-tags"));
    }

    #[test]
    fn a_tag_the_library_does_not_declare_is_reported() {
        let page = "<%@ taglib prefix=\"s\" uri=\"/struts-tags\"%>\n<s:iterate var=\"x\"/>";
        let d = diagnostics(&catalog(), page);
        assert!(d.iter().any(|d| d.code == "unknown-tag"), "{d:?}");
    }

    #[test]
    fn a_missing_required_attribute_is_reported_and_a_present_one_is_not() {
        let missing = "<%@ taglib prefix=\"s\" uri=\"/struts-tags\"%>\n<s:iterator value=\"%{l}\"></s:iterator>";
        assert!(diagnostics(&catalog(), missing).iter().any(|d| d.code == "missing-required-attribute"));
        assert!(diagnostics(&catalog(), PAGE).iter().all(|d| d.code != "missing-required-attribute"));
    }

    #[test]
    fn a_prefix_the_page_never_declared_is_never_reported() {
        // It is nearly always declared by an included fragment, which is invisible from here.
        let page = "<wp:info key=\"x\"/>";
        assert!(diagnostics(&catalog(), page).is_empty());
    }

    #[test]
    fn nothing_is_reported_when_no_library_resolved_at_all() {
        let empty = TaglibCatalog::default();
        let page = "<%@ taglib prefix=\"s\" uri=\"/nope\"%>\n<s:whatever/>";
        assert!(diagnostics(&empty, page).is_empty());
    }

    #[test]
    fn a_uri_nothing_declares_is_reported_on_the_uri_itself() {
        let page = "<%@ taglib prefix=\"z\" uri=\"/no-such-tags\"%>";
        let d = diagnostics(&catalog(), page);
        let hit = d.iter().find(|d| d.code == "unknown-taglib-uri").expect("reported");
        assert_eq!(&page[hit.start..hit.end], "/no-such-tags");
    }

    #[test]
    fn ctrl_click_on_a_uri_opens_the_tld_it_resolved_to() {
        let at = PAGE.find("/struts-tags").expect("uri") + 2;
        let targets = navigate(&catalog(), PAGE, at);
        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0].file, "/p/WEB-INF/struts-tags.tld");
    }

    #[test]
    fn ctrl_click_on_a_tag_lands_on_its_declaration_and_on_an_attribute_on_that() {
        let cat = catalog();
        let on_tag = PAGE.find("s:iterator").expect("tag") + 3;
        let t = navigate(&cat, PAGE, on_tag);
        assert!(TLD[t[0].offset..].starts_with("<tag>"), "lands on the <tag>");

        let on_attr = PAGE.find(" var=").expect("attr") + 2;
        let a = navigate(&cat, PAGE, on_attr);
        assert!(TLD[a[0].offset..].starts_with("<attribute>"), "lands on the <attribute>");
    }

    /// The regression this guards: a tag's span covers its attribute values, so "inside the tag"
    /// was true of every OGNL expression, include path and action reference on the page — and
    /// this extension, running early in the go-to chain, answered all of them with the TLD.
    #[test]
    fn nothing_is_answered_from_inside_an_attribute_value() {
        let cat = catalog();
        let in_value = PAGE.find("%{list}").expect("the OGNL value") + 2;
        assert!(navigate(&cat, PAGE, in_value).is_empty(), "the value belongs to another resolver");
        assert!(hover(&cat, PAGE, in_value).is_none());

        // The two positions that ARE this crate's to answer still answer.
        assert!(!navigate(&cat, PAGE, PAGE.find("s:iterator").expect("tag") + 3).is_empty());
        assert!(!navigate(&cat, PAGE, PAGE.find(" var=").expect("attr") + 2).is_empty());
    }

    #[test]
    fn an_attribute_the_library_never_declared_is_left_alone() {
        let page = "<%@ taglib prefix=\"s\" uri=\"/struts-tags\"%>\n<s:iterator id=\"x\" var=\"y\"/>";
        let at = page.find(" id=").expect("attr") + 2;
        assert!(navigate(&catalog(), page, at).is_empty());
    }

    #[test]
    fn hovering_an_attribute_says_what_the_tld_says() {
        let at = PAGE.find(" var=").expect("attr") + 2;
        let h = hover(&catalog(), PAGE, at).expect("hovers");
        assert_eq!(h.title, "var");
        assert!(h.signature.contains("required"));
    }
}
