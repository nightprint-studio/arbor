//! What a convention applies *to* — the unit the user configures.
//!
//! Targets are named for the role, not for the grammar: every language has something that plays
//! the part of "the constant" even when one spells it `static final` and another `const`. That is
//! what lets one settings screen configure every pack, and what lets a project say "constants are
//! `UPPER_SNAKE_CASE`" once instead of once per language.
//!
//! A pack maps its own node kinds — or its language server's symbol kinds — onto these (see
//! [`crate::pack::DeclSource`]); a target no pack produces simply never appears.

use serde::{Deserialize, Serialize};

/// The kind of declaration a convention applies to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Target {
    /// A class, interface, enum, record, annotation, struct, trait — anything that declares a type.
    #[serde(rename = "type")]
    Type,
    /// A method or free function.
    #[serde(rename = "method")]
    Method,
    /// An instance / mutable member.
    #[serde(rename = "field")]
    Field,
    /// A member that cannot change: Java `static final`, Rust `const`/`static`.
    #[serde(rename = "constant")]
    Constant,
    /// A formal parameter (including a `catch` parameter).
    #[serde(rename = "parameter")]
    Parameter,
    /// A local variable.
    #[serde(rename = "local")]
    Local,
    /// A generic type parameter (`<T>`).
    #[serde(rename = "type-parameter")]
    TypeParameter,
    /// An enum constant / variant.
    #[serde(rename = "enum-constant")]
    EnumConstant,
    /// One segment of a package / module path.
    #[serde(rename = "package")]
    Package,
}

impl Target {
    /// Every target, in the order a settings screen should list them: the declarations a reader
    /// meets from the outside in — the type first, its members next, then what lives inside a body.
    pub const ALL: [Target; 9] = [
        Target::Type,
        Target::Method,
        Target::Field,
        Target::Constant,
        Target::Parameter,
        Target::Local,
        Target::TypeParameter,
        Target::EnumConstant,
        Target::Package,
    ];

    /// The stable slug — the TOML key, and the tail of the diagnostic `code` (`naming-method`).
    pub const fn as_str(self) -> &'static str {
        match self {
            Target::Type => "type",
            Target::Method => "method",
            Target::Field => "field",
            Target::Constant => "constant",
            Target::Parameter => "parameter",
            Target::Local => "local",
            Target::TypeParameter => "type-parameter",
            Target::EnumConstant => "enum-constant",
            Target::Package => "package",
        }
    }

    /// The diagnostic `code` for a violation of this target — `naming-method`, `naming-local`, …
    ///
    /// Per-target rather than one `naming` code for the whole pack, because suppression is the
    /// thing that makes this usable on a legacy tree: a project may well want its methods held to
    /// the convention and its inherited field names left alone.
    pub const fn code(self) -> &'static str {
        match self {
            Target::Type => "naming-type",
            Target::Method => "naming-method",
            Target::Field => "naming-field",
            Target::Constant => "naming-constant",
            Target::Parameter => "naming-parameter",
            Target::Local => "naming-local",
            Target::TypeParameter => "naming-type-parameter",
            Target::EnumConstant => "naming-enum-constant",
            Target::Package => "naming-package",
        }
    }

    /// How the diagnostic names it in a sentence ("Method `get_user` …").
    pub const fn label(self) -> &'static str {
        match self {
            Target::Type => "Type",
            Target::Method => "Method",
            Target::Field => "Field",
            Target::Constant => "Constant",
            Target::Parameter => "Parameter",
            Target::Local => "Local variable",
            Target::TypeParameter => "Type parameter",
            Target::EnumConstant => "Enum constant",
            Target::Package => "Package",
        }
    }

    /// Whether renaming this can only ever touch the file it is declared in.
    ///
    /// This is the line between a fix that is safe to apply unseen and one that is not. A local or
    /// a parameter is scope-exact: no index, no reflection, no JSP or OGNL string can be referring
    /// to it. Everything else is reachable from outside the file — by a caller, by a framework
    /// binding a name out of a config file, by a mapper — so its fix is offered one at a time and
    /// never applied without a preview.
    ///
    /// **Necessary, not sufficient.** It is a fact about the *kind* of declaration, and holds only
    /// when a grammar found it: a language server's outline reports a module binding as a
    /// `variable`, and that is precisely what another file imports. Callers must ask
    /// [`crate::pack::Pack::fix_is_file_local`], which combines this with where the declaration
    /// came from.
    pub const fn is_file_local(self) -> bool {
        matches!(self, Target::Local | Target::Parameter)
    }
}

impl std::fmt::Display for Target {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codes_are_unique_and_prefixed() {
        let mut seen = std::collections::BTreeSet::new();
        for target in Target::ALL {
            assert!(target.code().starts_with("naming-"), "{target} has an unprefixed code");
            assert!(seen.insert(target.code()), "{target} duplicates a code");
        }
    }

    #[test]
    fn only_locals_and_parameters_are_file_local() {
        let file_local: Vec<Target> =
            Target::ALL.into_iter().filter(|t| t.is_file_local()).collect();
        assert_eq!(file_local, [Target::Parameter, Target::Local]);
    }
}
