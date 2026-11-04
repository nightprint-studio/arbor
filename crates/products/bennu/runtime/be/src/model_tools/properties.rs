//! `bennu_config_properties` — configuration keys, their value, and who reads them.

use std::collections::BTreeMap;

use bennu_spring::prelude::{simple_name, SpringModel};
use serde::Serialize;

use super::has;

/// Readers listed per key; a key read from fifty places is rare, and the first few say enough.
const MAX_READERS: usize = 10;

#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct ReaderRow {
    /// `@ConfigurationProperties` | `@Value` | `@ConditionalOnProperty` | `<bean>`…
    pub via: String,
    /// What reads it: `OrderProperties.timeout`.
    pub reader: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct PropertyRow {
    pub key: String,
    /// The value in the active property file; absent when no file sets the key.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
    /// The Java type the code expects, when a single one is known.
    #[serde(rename = "type", skip_serializing_if = "String::is_empty")]
    pub type_name: String,
    pub read_by: Vec<ReaderRow>,
    /// How many readers there are in all (`read_by` is capped).
    pub readers_total: usize,
}

#[derive(Debug, Default)]
pub struct PropertyFilter {
    pub key: Option<String>,
    pub set_only: bool,
}

#[derive(Default)]
struct Draft {
    type_name: String,
    readers: Vec<ReaderRow>,
}

pub fn rows(model: &SpringModel, filter: &PropertyFilter) -> Vec<PropertyRow> {
    let mut drafts: BTreeMap<String, Draft> = BTreeMap::new();
    for b in &model.config_bindings {
        let d = drafts.entry(b.path.clone()).or_default();
        d.type_name = b.type_text.clone();
        d.readers.push(ReaderRow {
            via: "@ConfigurationProperties".into(),
            reader: format!("{}.{}", simple_name(&b.owner_fqcn), b.field),
            file: Some(b.file.clone()).filter(|f| !f.is_empty()),
        });
    }
    for u in &model.property_usages {
        let d = drafts.entry(u.key.clone()).or_default();
        if d.type_name.is_empty() {
            d.type_name = u.type_text.clone();
        }
        d.readers.push(ReaderRow { via: u.kind.clone(), reader: u.label.clone(), file: Some(u.file.clone()).filter(|f| !f.is_empty()) });
    }
    // A key only a yaml sets and no code reads is dead weight worth seeing, but it is the project's
    // files' business (`bennu_catalog` "properties"), not this tool's: it answers who READS a key.
    drafts
        .into_iter()
        .filter(|(key, _)| has(key, &filter.key))
        .map(|(key, d)| {
            let value = model.props.lookup(&key).map(|(_, e)| e.value.clone());
            let readers_total = d.readers.len();
            PropertyRow { key, value, type_name: d.type_name, read_by: d.readers.into_iter().take(MAX_READERS).collect(), readers_total }
        })
        .filter(|r| !filter.set_only || r.value.is_some())
        .collect()
}

pub fn collect(root: &str, filter: &PropertyFilter) -> Vec<PropertyRow> {
    crate::frameworks::spring_model_of(root).map(|m| rows(&m, filter)).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use bennu_spring::prelude::{ConfigBinding, PropertyUsage};

    fn model() -> SpringModel {
        let mut m = SpringModel::default();
        m.config_bindings = vec![ConfigBinding {
            owner_fqcn: "com.acme.HttpProps".into(),
            field: "timeout".into(),
            path: "app.http.timeout".into(),
            type_text: "Duration".into(),
            root_prefix: "app.http".into(),
            file: "/p/HttpProps.java".into(),
            offset: 0,
        }];
        m.property_usages = vec![
            PropertyUsage { key: "app.http.timeout".into(), file: "/p/Client.java".into(), offset: 0, kind: "@Value".into(), label: "Client.timeout".into(), type_text: "int".into() },
            PropertyUsage { key: "app.feature.on".into(), file: "/p/F.java".into(), offset: 0, kind: "@ConditionalOnProperty".into(), label: "F".into(), type_text: String::new() },
        ];
        m
    }

    #[test]
    fn a_key_collects_every_reader_and_keeps_the_binding_type() {
        let rows = rows(&model(), &PropertyFilter::default());
        let timeout = rows.iter().find(|r| r.key == "app.http.timeout").unwrap();
        assert_eq!(timeout.type_name, "Duration", "the binding's type wins over a later @Value's");
        assert_eq!(timeout.readers_total, 2);
        assert_eq!(timeout.read_by[0].reader, "HttpProps.timeout");
        assert!(timeout.value.is_none(), "no property file sets it");
    }

    #[test]
    fn the_key_filter_narrows_and_set_only_drops_unset_keys() {
        let m = model();
        assert_eq!(rows(&m, &PropertyFilter { key: Some("feature".into()), set_only: false }).len(), 1);
        assert!(rows(&m, &PropertyFilter { key: None, set_only: true }).is_empty());
    }
}
