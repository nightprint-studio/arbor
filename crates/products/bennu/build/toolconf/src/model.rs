//! What a documented configuration key is, and the rule that decides whether *this* project's
//! version of the tool understands it.
//!
//! Deliberately `&'static str` throughout: every table in this crate is a literal compiled into the
//! binary, and a key that is data rather than allocation is a key that costs nothing to carry.

use std::cmp::Ordering;

/// One key a tool documents.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ConfigKey {
    /// The key as it is written in the file.
    pub key: &'static str,
    /// The type as the tool's own documentation writes it (`boolean`, `list`, `identifier`).
    pub type_text: &'static str,
    /// What the tool does when the key is absent. Empty when the documentation gives no default —
    /// which is not the same as "none", and is why this is not an `Option` with an invented value.
    pub default_value: &'static str,
    /// One or two sentences: what the key changes. Read out in the hover, and the reason a config
    /// file becomes navigable rather than something to keep a browser tab open beside.
    pub doc: &'static str,
    /// The **closed** set of legal values, empty when the value is free-form.
    ///
    /// Only when it is genuinely closed. Offering a made-up shortlist for a free-form value dresses
    /// a guess up as a choice, which is worse than offering nothing.
    pub values: &'static [&'static str],
    /// The first release of the tool that understands the key. Empty means *as far back as the
    /// file format itself goes* — see [`Catalogue::min_version`].
    ///
    /// Empty is also what an **unverified** version means, and that is deliberate: a key with no
    /// `since` is always offered, so the cost of not knowing is a key too many rather than a key
    /// missing from a project that has it. Under-report the *gate*, never the vocabulary.
    pub since: &'static str,
    /// The release that deprecated the key, empty while it is current. A deprecated key is still
    /// offered — it is in the file being read, and hiding it would make the hover unavailable
    /// exactly where the explanation is needed — but it says so.
    pub deprecated_since: &'static str,
    /// What to use instead, for a deprecated key. Empty otherwise.
    pub replacement: &'static str,
}

impl ConfigKey {
    /// A literal with only the parts most keys need. The rest are filled in field by field.
    pub const fn new(key: &'static str, type_text: &'static str, doc: &'static str) -> Self {
        Self {
            key,
            type_text,
            default_value: "",
            doc,
            values: &[],
            since: "",
            deprecated_since: "",
            replacement: "",
        }
    }

    pub const fn default_value(mut self, v: &'static str) -> Self {
        self.default_value = v;
        self
    }

    pub const fn values(mut self, v: &'static [&'static str]) -> Self {
        self.values = v;
        self
    }

    pub const fn since(mut self, v: &'static str) -> Self {
        self.since = v;
        self
    }

    pub const fn deprecated(mut self, since: &'static str, replacement: &'static str) -> Self {
        self.deprecated_since = since;
        self.replacement = replacement;
        self
    }

    /// Whether a project running `version` of the tool understands this key.
    ///
    /// `None` — the version could not be resolved — answers **yes**, for the reason spelled out on
    /// [`Self::since`]: a project whose Lombok arrives transitively through a starter should get the
    /// whole vocabulary rather than none of it.
    pub fn known_in(&self, version: Option<&str>) -> bool {
        let (Some(version), false) = (version, self.since.is_empty()) else { return true };
        !matches!(bennu_maven::prelude::compare_versions(version, self.since), Ordering::Less)
    }
}

/// One tool's whole vocabulary, and how to find out which version of it the project is on.
#[derive(Debug, Clone, Copy)]
pub struct Catalogue {
    /// Stable slug, used in diagnostic codes (`toolconf.lombok.unknown-key`).
    pub tool: &'static str,
    /// What the tool is called in prose.
    pub display_name: &'static str,
    /// The file this catalogue describes, by name.
    pub file_name: &'static str,
    /// The coordinates whose resolved version dates the vocabulary, most authoritative first: a
    /// project may declare `junit-jupiter`, or only `junit-jupiter-api`, or neither and let a BOM
    /// manage it.
    pub artifacts: &'static [(&'static str, &'static str)],
    /// The oldest release in which the file means anything at all. Reported in the hover of a key
    /// with no `since` of its own, so "which version do I need" always has an answer.
    pub min_version: &'static str,
    /// The keys, in the order they should be offered — which is the order they are written here.
    pub keys: &'static [ConfigKey],
}

impl Catalogue {
    /// The keys this project's version understands, in table order.
    pub fn known(&self, version: Option<&str>) -> impl Iterator<Item = &'static ConfigKey> + '_ {
        let version = version.map(str::to_string);
        self.keys.iter().filter(move |k| k.known_in(version.as_deref()))
    }

    /// The documented key with exactly this name, whether or not this project's version has it.
    ///
    /// Version-blind on purpose: this backs the hover, and the one moment a reader most needs to be
    /// told that a key exists but arrived in 1.18.22 is when it is sitting in front of them doing
    /// nothing.
    pub fn lookup(&self, key: &str) -> Option<&'static ConfigKey> {
        self.keys.iter().find(|k| k.key == key)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const KEY: ConfigKey = ConfigKey::new("a.b", "boolean", "doc").since("1.16.14");

    #[test]
    fn a_version_gate_admits_the_release_that_introduced_the_key() {
        assert!(KEY.known_in(Some("1.16.14")));
        assert!(KEY.known_in(Some("1.18.30")));
        assert!(!KEY.known_in(Some("1.16.12")));
    }

    #[test]
    fn an_unknown_version_admits_everything() {
        assert!(KEY.known_in(None));
    }

    #[test]
    fn a_key_with_no_since_is_admitted_by_any_version() {
        assert!(ConfigKey::new("x", "boolean", "").known_in(Some("0.1")));
    }

    #[test]
    fn numeric_segments_compare_as_numbers_not_as_text() {
        // The reason this crate borrows Maven's comparator rather than comparing strings:
        // `1.18.4` reads as older than `1.18.30` only if `4` and `30` are numbers.
        let k = ConfigKey::new("x", "boolean", "").since("1.18.30");
        assert!(!k.known_in(Some("1.18.4")));
        assert!(k.known_in(Some("1.18.30")));
    }
}
