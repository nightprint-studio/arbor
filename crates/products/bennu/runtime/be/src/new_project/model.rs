//! The wire types of the wizard, and the [`Plan`] every generator reduces to.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

// ── what the form is made of ─────────────────────────────────────────────────────────────

/// One choice of a select or multi-select.
#[derive(Serialize, Clone, Debug, Default, PartialEq)]
pub struct Opt {
    pub value: String,
    pub label: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// A heading to file the choice under (Spring's dependency groups).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub group: Option<String>,
}

impl Opt {
    pub fn new(value: &str, label: &str) -> Self {
        Self { value: value.into(), label: label.into(), ..Self::default() }
    }
    pub fn described(value: &str, label: &str, description: &str) -> Self {
        Self { description: Some(description.into()), ..Self::new(value, label) }
    }
}

#[derive(Serialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FieldKind {
    Text,
    Select,
    Toggle,
    /// A searchable multi-select with chips — dependencies, extensions.
    Multi,
}

/// A field shows only while another field has the given value (`"true"`/`"false"` for toggles).
#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct ShowWhen {
    pub key: String,
    pub value: String,
}

/// One input of a generator's form. The frontend renders these generically, so a generator is
/// added in Rust alone.
#[derive(Serialize, Clone, Debug)]
pub struct Field {
    pub key: String,
    pub label: String,
    pub kind: FieldKind,
    pub default: Value,
    pub options: Vec<Opt>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hint: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub placeholder: Option<String>,
    /// Kept behind a "More options" fold.
    pub advanced: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub show_when: Option<ShowWhen>,
}

impl Field {
    fn base(key: &str, label: &str, kind: FieldKind, default: Value) -> Self {
        Self {
            key: key.into(),
            label: label.into(),
            kind,
            default,
            options: Vec::new(),
            hint: None,
            placeholder: None,
            advanced: false,
            show_when: None,
        }
    }
    pub fn text(key: &str, label: &str, default: &str) -> Self {
        Self::base(key, label, FieldKind::Text, Value::String(default.into()))
    }
    pub fn select(key: &str, label: &str, default: &str, options: Vec<Opt>) -> Self {
        Self { options, ..Self::base(key, label, FieldKind::Select, Value::String(default.into())) }
    }
    pub fn toggle(key: &str, label: &str, default: bool) -> Self {
        Self::base(key, label, FieldKind::Toggle, Value::Bool(default))
    }
    pub fn multi(key: &str, label: &str, options: Vec<Opt>) -> Self {
        Self { options, ..Self::base(key, label, FieldKind::Multi, Value::Array(Vec::new())) }
    }
    pub fn hint(mut self, hint: &str) -> Self {
        self.hint = Some(hint.into());
        self
    }
    pub fn placeholder(mut self, p: &str) -> Self {
        self.placeholder = Some(p.into());
        self
    }
    pub fn advanced(mut self) -> Self {
        self.advanced = true;
        self
    }
    pub fn when(mut self, key: &str, value: &str) -> Self {
        self.show_when = Some(ShowWhen { key: key.into(), value: value.into() });
        self
    }
}

/// The values of a form, with every field's default underneath what the user set.
pub struct Opts(Map<String, Value>);

impl Opts {
    /// `given` over the defaults of `fields`. A key no field declares is dropped — generators
    /// only ever see options they asked for, and a value of the wrong type falls back.
    pub fn resolve(fields: &[Field], given: &Map<String, Value>) -> Self {
        let mut out = Map::new();
        for f in fields {
            let v = given.get(&f.key).filter(|v| type_matches(f.kind, v)).unwrap_or(&f.default);
            out.insert(f.key.clone(), v.clone());
        }
        Self(out)
    }
    pub fn str(&self, key: &str) -> &str {
        self.0.get(key).and_then(Value::as_str).unwrap_or("").trim()
    }
    pub fn flag(&self, key: &str) -> bool {
        self.0.get(key).and_then(Value::as_bool).unwrap_or(false)
    }
    pub fn list(&self, key: &str) -> Vec<String> {
        self.0
            .get(key)
            .and_then(Value::as_array)
            .map(|a| a.iter().filter_map(Value::as_str).map(str::to_string).collect())
            .unwrap_or_default()
    }
}

fn type_matches(kind: FieldKind, v: &Value) -> bool {
    match kind {
        FieldKind::Text | FieldKind::Select => v.is_string(),
        FieldKind::Toggle => v.is_boolean(),
        FieldKind::Multi => v.is_array(),
    }
}

// ── generators ───────────────────────────────────────────────────────────────────────────

/// How the Name field is validated — it is a different identifier in each ecosystem.
#[derive(Serialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum NameRule {
    /// A Cargo package name.
    Crate,
    /// A Maven/Gradle artifact id (also the folder).
    Artifact,
    /// An npm package name (also the folder).
    Npm,
}

/// A generator as the catalog lists it.
#[derive(Serialize, Clone, Debug)]
pub struct GeneratorInfo {
    pub id: String,
    pub category: String,
    pub label: String,
    pub description: String,
    /// A name the frontend maps to an icon.
    pub icon: String,
    pub name_rule: NameRule,
    /// Tools that must be installed (`npm`, `npx`, …); empty for locally generated projects.
    pub requires: Vec<String>,
    /// Whether the options come from a remote service (the form loads asynchronously).
    pub remote: bool,
    pub available: bool,
    /// Why it is not available.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

// ── requests ─────────────────────────────────────────────────────────────────────────────

/// What the wizard sends to preview or create.
#[derive(Deserialize, Clone, Debug, Default)]
pub struct CreateArgs {
    pub generator: String,
    pub name: String,
    pub location: String,
    #[serde(default)]
    pub git: bool,
    #[serde(default)]
    pub options: Map<String, Value>,
}

/// A thing wrong with the form, tied to the field it is about (`name`, `location`, or an option
/// key) so the frontend can show it where the user is looking.
#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct Problem {
    pub field: String,
    pub message: String,
}

impl Problem {
    pub fn new(field: &str, message: impl Into<String>) -> Self {
        Self { field: field.into(), message: message.into() }
    }
}

// ── plans ────────────────────────────────────────────────────────────────────────────────

/// One file a generator writes. `path` is relative to the project root and may itself have been
/// templated; the runner re-checks it for traversal regardless.
#[derive(Clone, Debug, PartialEq)]
pub struct GenFile {
    pub path: String,
    pub content: String,
    pub executable: bool,
}

impl GenFile {
    pub fn new(path: impl Into<String>, content: impl Into<String>) -> Self {
        Self { path: path.into(), content: content.into(), executable: false }
    }
}

/// Where a project's content comes from.
#[derive(Clone, Debug, PartialEq)]
pub enum Source {
    /// Written directly.
    Files(Vec<GenFile>),
    /// A zip from one of the allow-listed hosts; its single top folder, if any, is stripped.
    Download { url: String },
    /// An official scaffolding CLI run in a scratch folder; it must create `produces` there.
    Command { argv: Vec<String>, produces: String },
}

/// A command run in the finished project folder. Failure is reported but never undoes the project.
#[derive(Clone, Debug, PartialEq)]
pub struct Step {
    pub label: String,
    pub argv: Vec<String>,
}

/// Everything a run does, decided before anything is touched.
#[derive(Clone, Debug, PartialEq)]
pub struct Plan {
    pub source: Source,
    pub post: Vec<Step>,
    /// Files to open afterwards, relative to the project, first that exists wins.
    pub open: Vec<String>,
    /// Lines for the "what will be created" panel.
    pub summary: Vec<String>,
}

impl Plan {
    pub fn files(files: Vec<GenFile>, open: &[&str], summary: Vec<String>) -> Self {
        Self {
            source: Source::Files(files),
            post: Vec::new(),
            open: open.iter().map(|s| s.to_string()).collect(),
            summary,
        }
    }
}

/// The inputs a generator's `plan` function sees.
pub struct PlanCx<'a> {
    pub name: &'a str,
    pub opts: Opts,
    /// Whether a Git repository will be created (generators then add a `.gitignore`).
    pub git: bool,
}

/// Wire form of the preview.
#[derive(Serialize, Debug)]
pub struct Preview {
    pub problems: Vec<Problem>,
    pub target: super::target::TargetCheck,
    /// Project-relative paths for a locally generated project, capped.
    pub files: Vec<String>,
    pub summary: Vec<String>,
    /// The command a CLI generator will run, for display only.
    pub command: Option<String>,
    pub ok: bool,
}
