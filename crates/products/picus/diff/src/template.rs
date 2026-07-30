//! Saved comparison configurations.
//!
//! A useful diff configuration is not something anybody writes twice. The
//! exclusions that make a report readable in one shop — the migration table, the
//! audit columns, the three lookup tables that are the only contents worth
//! comparing — are the product of an afternoon of tuning, and they are the same
//! every time that comparison is run. So a [`DiffConfig`] is nameable, storable
//! and pickable, and the run says which template it came from.
//!
//! This module holds no I/O. Where the templates live (a profile file, a project
//! file, both) is the caller's decision; here they are a value that serialises.

use serde::{Deserialize, Serialize};

use crate::config::{
    ColumnFilter, ContentCheck, CountCheck, DiffConfig, IndexCheck, NameFilter, SequenceCheck,
};

/// A configuration with a name and a reason to exist.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiffTemplate {
    /// Stable across renames — a run records the id it used.
    pub id: String,
    pub name: String,
    /// What this template is for, in the words that would let somebody else pick
    /// it. Empty is allowed and always a missed opportunity.
    #[serde(default)]
    pub description: String,
    /// Shipped with the product. Editable — a copy is made on edit by the caller
    /// — but never deleted, so the list cannot be emptied into uselessness.
    #[serde(default)]
    pub builtin: bool,
    pub config: DiffConfig,
}

impl DiffTemplate {
    pub fn new(id: impl Into<String>, name: impl Into<String>, config: DiffConfig) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            description: String::new(),
            builtin: false,
            config,
        }
    }

    pub fn describe(mut self, description: impl Into<String>) -> Self {
        self.description = description.into();
        self
    }

    fn shipped(
        id: &str,
        name: &str,
        description: &str,
        config: DiffConfig,
    ) -> Self {
        Self {
            id: id.to_string(),
            name: name.to_string(),
            description: description.to_string(),
            builtin: true,
            config,
        }
    }

    /// Structure only — the run you start with, and the only one that is cheap on
    /// a large database.
    pub fn structure() -> Self {
        Self::shipped(
            "structure",
            "Structure",
            "Relations, columns, indexes, constraints, triggers and sequences. Reads catalogue metadata only, so it is safe to run against anything.",
            DiffConfig::default(),
        )
    }

    /// Structure plus how much data there is, without reading any of it.
    pub fn structure_and_counts() -> Self {
        let config = DiffConfig {
            counts: CountCheck { enabled: true, ..CountCheck::default() },
            ..DiffConfig::default()
        };
        Self::shipped(
            "structure-and-counts",
            "Structure and counts",
            "Everything the structure template compares, plus a row count per table. A count is one statement per table and is the cheapest way to see that a restore came up short.",
            config,
        )
    }

    /// Reference data: the small tables whose *contents* are part of the schema
    /// in every way that matters.
    pub fn reference_data() -> Self {
        let config = DiffConfig {
            counts: CountCheck { enabled: true, ..CountCheck::default() },
            contents: ContentCheck { enabled: true, ..ContentCheck::default() },
            ..DiffConfig::default()
        };
        Self::shipped(
            "reference-data",
            "Reference data",
            "Structure, counts and the contents of the tables listed in the template. Add one entry per lookup table with the columns that identify a row; contents are read, so nothing is compared until a table is named.",
            config,
        )
    }

    /// Two copies of one database, where the noise is the point of the filter.
    pub fn environments() -> Self {
        let config = DiffConfig {
            columns: ColumnFilter {
                ignore_patterns: vec!["created_*".into(), "updated_*".into(), "*_by".into()],
                ignore_defaults: true,
                ignore_position: true,
            },
            indexes: IndexCheck {
                filter: NameFilter::exclude(["*_tmp", "*_temp_*"]),
                ..IndexCheck::default()
            },
            sequences: SequenceCheck {
                // Two live environments are never on the same number; only a gap
                // large enough to mean "restored without its sequences" matters.
                warning_threshold: 10_000,
                ..SequenceCheck::default()
            },
            ..DiffConfig::default()
        };
        Self::shipped(
            "environments",
            "Two environments",
            "Structure between two installations of the same application: audit columns, defaults and sequence drift are expected and filtered out, so what is left is a real divergence.",
            config,
        )
    }
}

/// The templates a user has, builtin ones included.
/// Serialised as a bare list — the wrapper is here for the methods, not for a
/// level of nesting the frontend would have to unwrap.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct DiffTemplates {
    pub templates: Vec<DiffTemplate>,
}

impl Default for DiffTemplates {
    fn default() -> Self {
        Self::builtin()
    }
}

impl DiffTemplates {
    /// The four the product ships with.
    pub fn builtin() -> Self {
        Self {
            templates: vec![
                DiffTemplate::structure(),
                DiffTemplate::structure_and_counts(),
                DiffTemplate::reference_data(),
                DiffTemplate::environments(),
            ],
        }
    }

    pub fn get(&self, id: &str) -> Option<&DiffTemplate> {
        self.templates.iter().find(|t| t.id == id)
    }

    /// Add or replace **in place**: a template that is edited keeps its position
    /// in the list, because the list is a menu somebody has learned the shape of.
    pub fn upsert(&mut self, template: DiffTemplate) {
        match self.templates.iter().position(|t| t.id == template.id) {
            Some(at) => self.templates[at] = template,
            None => self.templates.push(template),
        }
    }

    /// Remove a user template. Refuses on a builtin one — and says so by
    /// returning `false` rather than silently doing nothing.
    pub fn remove(&mut self, id: &str) -> bool {
        match self.templates.iter().position(|t| t.id == id && !t.builtin) {
            Some(at) => {
                self.templates.remove(at);
                true
            }
            None => false,
        }
    }

    /// The configuration to run with, falling back to the defaults when the id is
    /// unknown — a template deleted since a run was scheduled must not stop the
    /// run.
    pub fn config_for(&self, id: &str) -> DiffConfig {
        self.get(id).map(|t| t.config.clone()).unwrap_or_default()
    }
}
