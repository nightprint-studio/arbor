//! The kinds of template — what each is for, and the templates Bennu ships for one.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TemplateKind {
    /// A new file, from the name typed and the folder it goes in.
    NewFile,
    /// Code generated from the class at the caret.
    Class,
    /// The configuration a `@ConfigurationProperties` class binds.
    ConfigProperties,
    /// A `@ConfigurationProperties` class, from the keys of a configuration file.
    ConfigClass,
    /// The DTO Lab's validation tests.
    ValidationTests,
    /// An abbreviation that expands where it is typed.
    Live,
    /// A postfix template: typed after a value's dot, it rewrites the value — `orders.logv`.
    Postfix,
}

impl TemplateKind {
    pub const ALL: [TemplateKind; 7] = [
        Self::NewFile,
        Self::Class,
        Self::ConfigProperties,
        Self::ConfigClass,
        Self::ValidationTests,
        Self::Live,
        Self::Postfix,
    ];

    /// The stable id — the directory name and the wire name.
    pub fn id(self) -> &'static str {
        match self {
            Self::NewFile => "new-file",
            Self::Class => "class",
            Self::ConfigProperties => "config-properties",
            Self::ConfigClass => "config-class",
            Self::ValidationTests => "validation-tests",
            Self::Live => "live",
            Self::Postfix => "postfix",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|kind| kind.id() == id)
    }

    pub fn title(self) -> &'static str {
        match self {
            Self::NewFile => "New file",
            Self::Class => "From a class",
            Self::ConfigProperties => "Configuration properties",
            Self::ConfigClass => "Configuration class",
            Self::ValidationTests => "Validation tests",
            Self::Live => "Abbreviations",
            Self::Postfix => "Postfix",
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            Self::NewFile => "Offered in the New file dialog, beside the built-in kinds: the file is named after what you type, and its content is the template.",
            Self::Class => "Generated from the class at the caret — members added to it, a new file beside it (a repository for an entity, a mapper), or text to copy.",
            Self::ConfigProperties => "The keys a @ConfigurationProperties class binds, written as a property file you can copy or append.",
            Self::ConfigClass => "A @ConfigurationProperties class written from the keys under a prefix of application.yml or .properties — a record or a class, with a nested type for each group of keys.",
            Self::ValidationTests => "The DTO Lab's tests: one case per way each constraint can fail, with what the project's validator reported for it.",
            Self::Live => "Abbreviations: the name is what you type, the template is the snippet it expands to. The language in the file name says where it is offered — logd.java.jinja in Java, dbg.rs.jinja in Rust, and one with no language in every file.",
            Self::Postfix => "Postfix templates: typed after a value's dot, they rewrite the value — orders.logv. The name is what you type; bennu.applies says which values it is offered on (iterable, optional, a class name…), and the template reads the expression, its type and a name for it.",
        }
    }

    /// The extension a new template of this kind gets when it is not copied from another.
    pub fn default_extension(self) -> &'static str {
        match self {
            Self::ConfigProperties => "yml",
            _ => "java",
        }
    }

    /// Whether the kind can only run on a Java project.
    ///
    /// Five of the seven read a Java class, the Spring model, Bean Validation or a Java value's type,
    /// so on a Cargo project they have nothing to run on and are not offered. The other two are about
    /// the file rather than the language: a **New file** template writes whatever its name says it
    /// writes, and an **abbreviation** is offered in the language it was written for.
    pub fn java_only(self) -> bool {
        !matches!(self, Self::NewFile | Self::Live)
    }

    /// Whether a new template of this kind may be written for any language, so the dialog asks which.
    /// The rest write Java, a property file or a test — what they generate is the kind's own business.
    pub fn picks_language(self) -> bool {
        !self.java_only()
    }
}

/// A template compiled into Bennu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Builtin {
    pub name: &'static str,
    /// The extension of what it generates (`java`, `yml`).
    pub extension: &'static str,
    pub text: &'static str,
    /// Only a starting point to copy: not offered where templates of its kind are chosen, because
    /// what it does Bennu already does without a template.
    pub starter: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The two halves of the catalogue: what needs a Java project, and what is about the file.
    #[test]
    fn only_the_kinds_that_read_java_are_java_only() {
        assert!(TemplateKind::Class.java_only());
        assert!(TemplateKind::ConfigClass.java_only());
        assert!(TemplateKind::ConfigProperties.java_only());
        assert!(TemplateKind::ValidationTests.java_only());
        // A postfix template is chosen by the Java type of the value it is typed after.
        assert!(TemplateKind::Postfix.java_only());
        assert!(!TemplateKind::NewFile.java_only());
        assert!(!TemplateKind::Live.java_only());
        // A kind that is not Java's is one whose language the author chooses.
        for kind in TemplateKind::ALL {
            assert_eq!(kind.picks_language(), !kind.java_only());
        }
    }

    #[test]
    fn a_kind_is_found_by_the_id_it_serialises_as() {
        for kind in TemplateKind::ALL {
            assert_eq!(TemplateKind::from_id(kind.id()), Some(kind));
            assert_eq!(serde_json::to_value(kind).unwrap(), kind.id());
        }
        assert_eq!(TemplateKind::from_id("postfix"), Some(TemplateKind::Postfix));
        assert_eq!(TemplateKind::from_id("nope"), None);
    }
}
