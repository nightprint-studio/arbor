//! A Spring configuration file read as a tree — mappings, lists and scalars — for a class to be written
//! from it.
//!
//! Not the reader the Spring model uses, on purpose. That one flattens every file to dotted keys and
//! skips lists whole, which is exactly right for completing and resolving a key and exactly wrong here:
//! a `List<Server>` is written from the shape of the list's elements, and a flat key has none.
//!
//! The files are read by [`crate::config_yaml`] and [`crate::config_props`]; which keys the caret or a
//! selection names, and the tree narrowed to them, is [`crate::config_keys`].

use serde::Serialize;

use crate::config_props::read_properties;
use crate::config_yaml::read_yaml;

/// A value in a configuration tree.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum ConfigValue {
    /// `quoted` — written in quotes, which makes it text whatever it looks like.
    Scalar { text: String, quoted: bool },
    List(Vec<ConfigValue>),
    /// Keys in the order they are first written.
    Map(Vec<(String, ConfigValue)>),
}

impl ConfigValue {
    pub(crate) fn empty() -> Self {
        ConfigValue::Scalar { text: String::new(), quoted: false }
    }

    pub(crate) fn scalar(text: &str) -> Self {
        let text = text.trim();
        for quote in ['"', '\''] {
            if text.len() >= 2 && text.starts_with(quote) && text.ends_with(quote) {
                return ConfigValue::Scalar { text: text[1..text.len() - 1].to_string(), quoted: true };
            }
        }
        match text {
            "~" | "null" | "Null" | "NULL" => ConfigValue::empty(),
            _ => ConfigValue::Scalar { text: text.to_string(), quoted: false },
        }
    }

    /// The value under a dotted `prefix`, keys compared the way Spring binds them — ignoring case and
    /// everything that is not a letter or a digit, so `mail-server` is `mailServer`.
    pub fn at(&self, prefix: &str) -> Option<&ConfigValue> {
        let mut here = self;
        for segment in prefix.split('.').filter(|s| !s.is_empty()) {
            let ConfigValue::Map(entries) = here else { return None };
            here = &entries.iter().find(|(key, _)| canonical(key) == canonical(segment))?.1;
        }
        Some(here)
    }

    /// Lay `other` over this: mappings merge key by key, lists are joined (a list's elements are read
    /// for their shape, and every element says something), and a scalar already here stays unless it
    /// is empty.
    pub fn merge(&mut self, other: ConfigValue) {
        match (self, other) {
            (ConfigValue::Map(mine), ConfigValue::Map(theirs)) => {
                for (key, value) in theirs {
                    match mine.iter_mut().find(|(k, _)| canonical(k) == canonical(&key)) {
                        Some((_, existing)) => existing.merge(value),
                        None => mine.push((key, value)),
                    }
                }
            }
            (ConfigValue::List(mine), ConfigValue::List(theirs)) => mine.extend(theirs),
            (slot @ ConfigValue::Scalar { .. }, other) if slot.is_blank() => *slot = other,
            _ => {}
        }
    }

    fn is_blank(&self) -> bool {
        matches!(self, ConfigValue::Scalar { text, .. } if text.is_empty())
    }
}

/// A key as Spring compares it.
pub fn canonical(key: &str) -> String {
    key.chars().filter(|c| c.is_ascii_alphanumeric()).map(|c| c.to_ascii_lowercase()).collect()
}

/// Read a configuration file by its name: `.properties`, else YAML.
pub fn read_config(file_name: &str, text: &str) -> ConfigValue {
    match file_name.to_ascii_lowercase().ends_with(".properties") {
        true => read_properties(text),
        false => read_yaml(text),
    }
}

#[cfg(test)]
impl ConfigValue {
    /// A scalar's text, for a test to compare — anything else is a failure worth its message.
    pub(crate) fn text(&self) -> &str {
        match self {
            ConfigValue::Scalar { text, .. } => text,
            other => panic!("not a scalar: {other:?}"),
        }
    }
}
