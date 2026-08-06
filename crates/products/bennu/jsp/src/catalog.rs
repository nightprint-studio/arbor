//! Which tag library a `uri="…"` means, and where its TLD is.
//!
//! The question looks like a lookup and is not, because a legacy page declares its
//! libraries in three different registers and all three are in use in the same file:
//!
//! ```jsp
//! <%@ taglib prefix="s"  uri="/struts-tags" %>                        <!-- the TLD's own <uri> -->
//! <%@ taglib prefix="wp" uri="aps-core.tld" %>                        <!-- a file, by name -->
//! <%@ taglib prefix="c"  uri="http://java.sun.com/jsp/jstl/core" %>   <!-- a URI, from a jar -->
//! ```
//!
//! So resolution is a short ladder, most-specific first: the URI a TLD claims for
//! itself, then a `web.xml` mapping (which exists precisely to override the first),
//! then the path — because `uri="aps-core.tld"` and `uri="/WEB-INF/tld/aps-core.tld"`
//! are the *same declaration written two ways*, and a container resolves both.
//!
//! Nothing here reads the filesystem. TLDs arrive as text, from the project or from a
//! jar entry, exactly like the schemas next door: a resolver that opens archives cannot
//! run where this one has to.

use std::collections::HashMap;
use std::sync::Arc;

use crate::tld::{parse_tld, Taglib};

/// Every tag library this project can resolve, indexed the three ways a page names one.
#[derive(Debug, Default)]
pub struct TaglibCatalog {
    /// By the `<uri>` the TLD declares for itself, and by any `web.xml` alias.
    by_uri: HashMap<String, Arc<Taglib>>,
    /// By forward-slashed source path, for the by-file-name rule.
    by_path: Vec<(String, Arc<Taglib>)>,
    /// Every library, once, in the order read — what the "which taglibs can I use" panel
    /// and the `uri=` completion list.
    all: Vec<Arc<Taglib>>,
    /// TLDs that were found and could **not** be read.
    ///
    /// Kept because the alternative is the worst answer available: a file that failed to parse
    /// is indistinguishable from a file that does not exist, and the check would then tell the
    /// user that a library sitting in their own `WEB-INF` does not exist. A `uri` pointing at
    /// one of these resolves to nothing *and* is never reported.
    unreadable: Vec<String>,
}

impl TaglibCatalog {
    /// Build from the TLDs the host found: `(source path, text)` pairs, project files and
    /// jar-extracted ones alike, plus the `web.xml` `<taglib-uri>` → `<taglib-location>`
    /// aliases (see [`web_xml_aliases`]).
    pub fn build(files: &[(String, String)], aliases: &[(String, String)]) -> Self {
        let mut cat = TaglibCatalog::default();
        for (path, text) in files {
            let Some(lib) = parse_tld(text, path) else {
                cat.unreadable.push(norm(path));
                continue;
            };
            let lib = Arc::new(lib);
            if !lib.uri.is_empty() {
                cat.by_uri.entry(lib.uri.clone()).or_insert_with(|| Arc::clone(&lib));
            }
            cat.by_path.push((norm(path), Arc::clone(&lib)));
            cat.all.push(lib);
        }
        // A `web.xml` alias is the deployment's own answer and outranks the TLD's claim —
        // overriding one is the reason the element exists.
        for (uri, location) in aliases {
            if let Some(lib) = cat.by_location(location) {
                cat.by_uri.insert(uri.clone(), lib);
            }
        }
        cat
    }

    /// The library a `uri="…"` names, if this project has it.
    pub fn resolve(&self, uri: &str) -> Option<&Arc<Taglib>> {
        if let Some(lib) = self.by_uri.get(uri) {
            return Some(lib);
        }
        // Not a declared URI — try it as a location. Only when it *looks* like one, so a
        // genuine URI that nothing ships (`http://acme/tags`) reports as unresolved rather
        // than matching some unrelated file whose name happens to end the same way.
        if uri.ends_with(".tld") {
            return self.by_path.iter().find(|(p, _)| path_matches(p, uri)).map(|(_, l)| l);
        }
        None
    }

    /// Every library, for the `uri=` completion and the catalog panel.
    pub fn all(&self) -> &[Arc<Taglib>] {
        &self.all
    }

    /// Is there a TLD **file** at this location that simply could not be read?
    ///
    /// The one question that separates "you named something that does not exist" from "I could
    /// not read what you named", and only the first of those is worth telling anyone about.
    pub fn is_unreadable(&self, location: &str) -> bool {
        self.unreadable.iter().any(|p| path_matches(p, location))
    }

    pub fn is_empty(&self) -> bool {
        self.all.is_empty()
    }

    fn by_location(&self, location: &str) -> Option<Arc<Taglib>> {
        self.by_path.iter().find(|(p, _)| path_matches(p, location)).map(|(_, l)| Arc::clone(l))
    }
}

/// Does this TLD's path answer that location? A location is web-app-relative
/// (`/WEB-INF/tld/aps-core.tld`) or a bare file name, and the path is absolute — so the
/// test is a **segment-aligned suffix**, which is what keeps `core.tld` from matching
/// `aps-core.tld`.
fn path_matches(path: &str, location: &str) -> bool {
    let tail = norm(location);
    let tail = tail.trim_start_matches('/');
    path == tail
        || path.strip_suffix(tail).is_some_and(|head| head.is_empty() || head.ends_with('/'))
}

fn norm(p: &str) -> String {
    p.replace('\\', "/")
}

/// The `<taglib><taglib-uri>…</taglib-uri><taglib-location>…</taglib-location></taglib>`
/// pairs of a `web.xml`, which is how a pre-2.0 deployment bound a URI to a file.
///
/// A tolerant text scan rather than an XML parse: it runs over whatever `web.xml` the
/// project has, including one being edited, and the two elements it wants sit next to each
/// other in a shape that has not changed since 2001.
pub fn web_xml_aliases(source: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut rest = source;
    while let Some(at) = rest.find("<taglib>") {
        rest = &rest[at + "<taglib>".len()..];
        let end = rest.find("</taglib>").unwrap_or(rest.len());
        let block = &rest[..end];
        let uri = element_text(block, "taglib-uri");
        let location = element_text(block, "taglib-location");
        if let (Some(uri), Some(location)) = (uri, location) {
            out.push((uri, location));
        }
        rest = &rest[end..];
    }
    out
}

fn element_text(block: &str, name: &str) -> Option<String> {
    let open = format!("<{name}>");
    let close = format!("</{name}>");
    let start = block.find(&open)? + open.len();
    let end = block[start..].find(&close)? + start;
    Some(block[start..end].trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lib(uri: &str) -> String {
        format!("<taglib><uri>{uri}</uri><tag><name>t</name></tag></taglib>")
    }

    fn catalog() -> TaglibCatalog {
        TaglibCatalog::build(
            &[
                ("/p/lib/struts-tags.tld".into(), lib("/struts-tags")),
                ("/p/src/main/webapp/WEB-INF/tld/aps-core.tld".into(), lib("")),
                ("/p/lib/core.tld".into(), lib("")),
            ],
            &[],
        )
    }

    #[test]
    fn a_declared_uri_resolves_to_the_library_that_claims_it() {
        assert!(catalog().resolve("/struts-tags").is_some());
    }

    #[test]
    fn a_location_resolves_however_the_page_spells_it() {
        let cat = catalog();
        assert!(cat.resolve("aps-core.tld").is_some());
        assert!(cat.resolve("/WEB-INF/tld/aps-core.tld").is_some());
    }

    #[test]
    fn a_suffix_that_is_not_a_whole_segment_is_not_a_match() {
        // `core.tld` must find `/p/lib/core.tld` and never `aps-core.tld`.
        let cat = catalog();
        let found = cat.resolve("core.tld").expect("resolves");
        assert_eq!(found.source, "/p/lib/core.tld");
    }

    #[test]
    fn a_uri_nobody_ships_stays_unresolved_instead_of_matching_something_close() {
        assert!(catalog().resolve("http://acme.example/tags").is_none());
    }

    #[test]
    fn a_tld_that_could_not_be_read_is_remembered_as_unreadable() {
        let cat = TaglibCatalog::build(
            &[("/p/WEB-INF/broken.tld".into(), "<taglib".into())],
            &[],
        );
        assert!(cat.resolve("/WEB-INF/broken.tld").is_none());
        // …but it is not a `uri` the page got wrong: the file is there, and the check keeps quiet.
        assert!(cat.is_unreadable("/WEB-INF/broken.tld"));
        assert!(!cat.is_unreadable("/WEB-INF/other.tld"));
    }

    #[test]
    fn a_web_xml_alias_outranks_the_tld_s_own_claim() {
        let cat = TaglibCatalog::build(
            &[("/p/WEB-INF/aps-core.tld".into(), lib("/original"))],
            &[("/aps-core".to_string(), "/WEB-INF/aps-core.tld".to_string())],
        );
        assert!(cat.resolve("/aps-core").is_some());
        // The original claim keeps working — an alias adds a name, it does not remove one.
        assert!(cat.resolve("/original").is_some());
    }

    #[test]
    fn web_xml_pairs_are_read_out_of_the_descriptor() {
        let xml = r"<web-app>
          <taglib>
            <taglib-uri>/aps-core</taglib-uri>
            <taglib-location>/WEB-INF/aps-core.tld</taglib-location>
          </taglib>
        </web-app>";
        assert_eq!(
            web_xml_aliases(xml),
            vec![("/aps-core".to_string(), "/WEB-INF/aps-core.tld".to_string())]
        );
    }
}
