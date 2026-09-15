//! Every bundle in the project, indexed by key.
//!
//! The one thing this makes possible that reading the files does not: a key is a fact about the
//! **bundle**, not about a file. `login.title` exists if any translation of `messages` declares
//! it; it is untranslated in Italian if `messages_it` does not. Both questions need the whole set
//! at once, and both are asked constantly by a person editing a page.

use std::collections::HashMap;

use crate::bundle::{Bundle, Entry};

/// Where a key is declared: the bundle file, and the entry inside it.
pub struct Declaration<'a> {
    pub bundle: &'a Bundle,
    pub entry: &'a Entry,
}

/// The project's message bundles.
#[derive(Debug, Default)]
pub struct BundleCatalog {
    bundles: Vec<Bundle>,
    /// key → the bundles declaring it, as indices into `bundles`.
    by_key: HashMap<String, Vec<usize>>,
}

impl BundleCatalog {
    /// Build from `(path, text)` pairs. Non-`.properties` paths are ignored, so a caller can
    /// hand over every resource it has.
    pub fn build(files: &[(String, String)]) -> Self {
        let mut bundles: Vec<Bundle> = files
            .iter()
            .filter(|(p, _)| p.to_ascii_lowercase().ends_with(".properties"))
            .map(|(p, t)| Bundle::parse(p, t))
            .collect();
        // Stable order: by bundle, then default locale first — which is the order a person
        // reads them in, and the order a "first declaration" answer should follow.
        bundles.sort_by(|a, b| {
            a.base.cmp(&b.base).then(a.locale.len().cmp(&b.locale.len())).then(a.path.cmp(&b.path))
        });

        let mut by_key: HashMap<String, Vec<usize>> = HashMap::new();
        for (i, b) in bundles.iter().enumerate() {
            for e in &b.entries {
                let slot = by_key.entry(e.key.clone()).or_default();
                if slot.last() != Some(&i) {
                    slot.push(i);
                }
            }
        }
        BundleCatalog { bundles, by_key }
    }

    pub fn is_empty(&self) -> bool {
        self.by_key.is_empty()
    }

    pub fn bundles(&self) -> &[Bundle] {
        &self.bundles
    }

    /// How many distinct keys the project declares.
    pub fn key_count(&self) -> usize {
        self.by_key.len()
    }

    /// Whether any bundle declares `key`.
    pub fn knows(&self, key: &str) -> bool {
        self.by_key.contains_key(key)
    }

    /// Everywhere `key` is declared, default locale first.
    pub fn declarations(&self, key: &str) -> Vec<Declaration<'_>> {
        let Some(idx) = self.by_key.get(key) else { return Vec::new() };
        idx.iter()
            .filter_map(|i| {
                let bundle = self.bundles.get(*i)?;
                let entry = bundle.entry(key)?;
                Some(Declaration { bundle, entry })
            })
            .collect()
    }

    /// Every key, sorted — the catalog panel's rows.
    pub fn keys(&self) -> Vec<&str> {
        let mut out: Vec<&str> = self.by_key.keys().map(String::as_str).collect();
        out.sort_unstable();
        out
    }

    /// The locales `base` is translated into, in the order the bundles are held (default first).
    pub fn locales_of(&self, base: &str) -> Vec<&str> {
        let mut out: Vec<&str> = Vec::new();
        for b in self.bundles.iter().filter(|b| b.base == base) {
            if !out.contains(&b.locale.as_str()) {
                out.push(&b.locale);
            }
        }
        out
    }

    /// The locales of `key`'s own bundle that do NOT declare it — the translations somebody
    /// still owes.
    ///
    /// Answered per bundle rather than project-wide: two unrelated bundles having different
    /// locale sets is normal, and comparing across them would report a debt nobody has.
    pub fn untranslated(&self, key: &str) -> Vec<&str> {
        let Some(idx) = self.by_key.get(key) else { return Vec::new() };
        let bases: Vec<&str> =
            idx.iter().filter_map(|i| self.bundles.get(*i)).map(|b| b.base.as_str()).collect();
        let mut out: Vec<&str> = Vec::new();
        for b in &self.bundles {
            if !bases.contains(&b.base.as_str()) || b.entry(key).is_some() {
                continue;
            }
            if !out.contains(&b.locale.as_str()) {
                out.push(&b.locale);
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cat() -> BundleCatalog {
        BundleCatalog::build(&[
            ("/p/messages.properties".into(), "login.title=Sign in\nonly.default=x\n".into()),
            ("/p/messages_it.properties".into(), "login.title=Accedi\n".into()),
            ("/p/errors.properties".into(), "boom=Bang\n".into()),
            ("/p/application.yml".into(), "a: 1\n".into()),
        ])
    }

    #[test]
    fn a_key_is_a_fact_about_the_bundle_not_the_file() {
        let c = cat();
        assert!(c.knows("login.title"));
        let d = c.declarations("login.title");
        assert_eq!(d.len(), 2);
        assert_eq!(d[0].bundle.locale, "", "the default file answers first");
        assert_eq!(d[1].entry.value, "Accedi");
    }

    #[test]
    fn a_yaml_is_not_a_bundle() {
        assert_eq!(cat().bundles().len(), 3);
    }

    #[test]
    fn an_untranslated_key_names_the_locales_that_owe_it() {
        let c = cat();
        assert_eq!(c.untranslated("only.default"), ["it"]);
        assert!(c.untranslated("login.title").is_empty(), "translated everywhere it is declared");
        // `errors` has no Italian file at all, so nothing is owed there.
        assert!(c.untranslated("boom").is_empty());
    }

    #[test]
    fn keys_are_the_union_across_bundles_sorted() {
        assert_eq!(cat().keys(), ["boom", "login.title", "only.default"]);
        assert_eq!(cat().key_count(), 3);
    }

    #[test]
    fn locales_are_listed_per_bundle_default_first() {
        assert_eq!(cat().locales_of("messages"), ["", "it"]);
        assert_eq!(cat().locales_of("errors"), [""]);
    }
}
