//! What a generator produces: new files, members for the class at the caret, snippets for somewhere
//! else (a property file, a build file), and notes. Nothing here touches the disk.

use serde::Serialize;
use serde_json::Value;

/// Which source tree a generated file belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum SourceRoot {
    /// `src/main/java`
    Main,
    /// `src/test/java`
    Test,
    /// Next to the file at the caret (non-Java output).
    Beside,
}

/// A file the generator creates.
#[derive(Debug, Clone, Serialize)]
pub struct GenFile {
    pub root: SourceRoot,
    /// The Java package; its directories are the file's folder. Empty for `Beside`.
    pub package: String,
    /// File name with extension.
    pub name: String,
    pub text: String,
    /// Language id for highlighting: `java`, `rust`, `typescript`, `yaml`.
    pub language: &'static str,
}

impl GenFile {
    pub fn java(root: SourceRoot, package: &str, class: &str, text: String) -> Self {
        GenFile { root, package: package.to_string(), name: format!("{class}.java"), text, language: "java" }
    }
}

/// Where inserted members go.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum MemberTarget {
    /// Before the closing brace of the class at the caret.
    Class,
    /// At the caret, as written (Rust and TypeScript files).
    Caret,
}

/// Members written into the file under the caret.
#[derive(Debug, Clone, Serialize)]
pub struct Members {
    pub target: MemberTarget,
    pub text: String,
    /// Fully-qualified imports the members need (Java only).
    pub imports: Vec<String>,
}

/// Text meant for a place the generator does not write to: an `application.yml` key, a `pom.xml` dependency.
#[derive(Debug, Clone, Serialize)]
pub struct Extra {
    pub label: String,
    pub language: &'static str,
    pub text: String,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct Output {
    pub files: Vec<GenFile>,
    pub members: Option<Members>,
    pub extras: Vec<Extra>,
    pub notes: Vec<String>,
    /// Annotations added to the class at the caret (or its fields).
    pub wiring: Vec<Wiring>,
    /// `group:artifact` the project lacks and the output needs.
    pub dependencies: Vec<String>,
}

impl Output {
    pub fn file(file: GenFile) -> Self {
        Output { files: vec![file], ..Default::default() }
    }

    pub fn members(text: String, imports: Vec<String>) -> Self {
        Output { members: Some(Members { target: MemberTarget::Class, text, imports }), ..Default::default() }
    }

    pub fn note(mut self, text: impl Into<String>) -> Self {
        self.notes.push(text.into());
        self
    }

    pub fn extra(mut self, label: impl Into<String>, language: &'static str, text: String) -> Self {
        self.extras.push(Extra { label: label.into(), language, text });
        self
    }

    pub fn needs(mut self, dependencies: Vec<String>) -> Self {
        for d in dependencies {
            if !self.dependencies.contains(&d) {
                self.dependencies.push(d);
            }
        }
        self
    }
}

/// Options for a generator, read from JSON with every missing field defaulted.
pub fn parse_options<T: serde::de::DeserializeOwned + Default>(options: &Value) -> Result<T, String> {
    if options.is_null() {
        return Ok(T::default());
    }
    serde_json::from_value(options.clone()).map_err(|e| format!("Invalid options: {e}"))
}

/// A Java class name typed by the user.
pub fn require_class_name(name: &str, what: &str) -> Result<(), String> {
    if crate::names::is_identifier(name) {
        Ok(())
    } else {
        Err(format!("{what} `{name}` is not a valid Java name"))
    }
}

/// A dotted package typed by the user; empty is the default package.
pub fn require_package(package: &str) -> Result<(), String> {
    if package.is_empty() || package.split('.').all(crate::names::is_identifier) {
        Ok(())
    } else {
        Err(format!("`{package}` is not a valid package name"))
    }
}

/// A fragment that is written into code as-is (an access rule, a header value): one line, no statement separators.
pub fn require_inline(text: &str, what: &str) -> Result<(), String> {
    if text.contains(['\n', '\r', ';']) {
        Err(format!("{what} must be a single expression"))
    } else {
        Ok(())
    }
}

/// What an annotation added to existing code is attached to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum WireTarget {
    /// A field of the class at the caret, by name.
    Field(String),
    /// The class at the caret.
    Class,
}

/// An annotation a generator adds to existing code so the new class is used (`@Convert`, `@JsonSerialize`).
#[derive(Debug, Clone, Serialize)]
pub struct Wiring {
    pub target: WireTarget,
    pub annotation: String,
    /// Imports the annotation needs, fully qualified.
    pub imports: Vec<String>,
}
