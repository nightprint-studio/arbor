//! New-file templates for Rust.
//!
//! The Java side has its own ([`bennu_java`]'s scaffolder) and the two do not share a
//! module for a reason that shows up in the very first field: a Java file's name is
//! dictated by the type inside it, while a Rust file's name **is the module** and the
//! types inside it are free. So the two ask the user for different things, and a shared
//! "new file" abstraction would have to be told which one it was every time.
//!
//! What the user types is the **module name**, in the language's own casing
//! (`atlas_player`). A type template derives its type name from it (`AtlasPlayer`),
//! because a file that declares one obvious thing is named after that thing.

/// What to scaffold.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RustFileKind {
    /// An empty `.rs`.
    File,
    Struct,
    Enum,
    Trait,
    /// A directory module: `name/mod.rs`. The one kind that creates a directory, which
    /// is why it is a kind and not a checkbox — `foo.rs` and `foo/mod.rs` are two
    /// different decisions about how the module will grow.
    Module,
    /// A file whose whole point is the `#[cfg(test)] mod tests` in it.
    Tests,
}

/// The scaffolded file: its path **relative to the chosen directory** (so a module kind
/// can nest), and its initial content.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RustScaffold {
    pub file_name: String,
    pub content: String,
}

/// `atlas_player` / `AtlasPlayer` / `atlas-player.rs` → `atlas_player`.
pub fn module_name(input: &str) -> String {
    let stem = input.trim().trim_end_matches(".rs");
    let mut out = String::new();
    let mut prev_lower = false;
    for ch in stem.chars() {
        if ch == '-' || ch == ' ' || ch == '.' {
            out.push('_');
            prev_lower = false;
        } else if ch.is_ascii_uppercase() {
            if prev_lower {
                out.push('_');
            }
            out.push(ch.to_ascii_lowercase());
            prev_lower = false;
        } else {
            out.push(ch);
            prev_lower = ch.is_ascii_lowercase() || ch.is_ascii_digit();
        }
    }
    // Collapse the runs a mixed input produces (`Atlas__Player`), and never start or end
    // on a separator — `mod _foo;` is legal and `mod foo_;` is nobody's intent.
    let mut collapsed = String::with_capacity(out.len());
    for ch in out.chars() {
        if ch == '_' && collapsed.ends_with('_') {
            continue;
        }
        collapsed.push(ch);
    }
    collapsed.trim_matches('_').to_string()
}

/// `atlas_player` → `AtlasPlayer`.
pub fn type_name(module: &str) -> String {
    module
        .split('_')
        .filter(|s| !s.is_empty())
        .map(|part| {
            let mut c = part.chars();
            match c.next() {
                Some(first) => first.to_ascii_uppercase().to_string() + c.as_str(),
                None => String::new(),
            }
        })
        .collect()
}

/// Build the file for `kind` named `input`. An empty name yields an empty file name,
/// which the caller treats as "nothing to create".
pub fn scaffold(kind: RustFileKind, input: &str) -> RustScaffold {
    let module = module_name(input);
    if module.is_empty() {
        return RustScaffold { file_name: String::new(), content: String::new() };
    }
    let ty = type_name(&module);
    let (file_name, content) = match kind {
        RustFileKind::File => (format!("{module}.rs"), String::new()),
        RustFileKind::Module => (format!("{module}/mod.rs"), format!("//! {module}\n")),
        RustFileKind::Struct => (
            format!("{module}.rs"),
            format!("pub struct {ty} {{}}\n\nimpl {ty} {{\n    pub fn new() -> Self {{\n        Self {{}}\n    }}\n}}\n"),
        ),
        RustFileKind::Enum => (format!("{module}.rs"), format!("pub enum {ty} {{}}\n")),
        RustFileKind::Trait => (format!("{module}.rs"), format!("pub trait {ty} {{}}\n")),
        RustFileKind::Tests => (
            format!("{module}.rs"),
            "#[cfg(test)]\nmod tests {\n    use super::*;\n\n    #[test]\n    fn it_works() {\n        todo!()\n    }\n}\n"
                .to_string(),
        ),
    };
    RustScaffold { file_name, content }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_name_becomes_a_module_whatever_it_was_typed_as() {
        assert_eq!(module_name("AtlasPlayer"), "atlas_player");
        assert_eq!(module_name("atlas-player.rs"), "atlas_player");
        assert_eq!(module_name("  atlas player "), "atlas_player");
        assert_eq!(module_name("Atlas__Player"), "atlas_player");
        assert_eq!(module_name("_"), "");
    }

    #[test]
    fn the_type_is_named_after_the_module() {
        assert_eq!(type_name("atlas_player"), "AtlasPlayer");
        assert_eq!(type_name("frame"), "Frame");
    }

    #[test]
    fn a_module_lands_in_its_own_directory() {
        let s = scaffold(RustFileKind::Module, "AtlasPlayer");
        assert_eq!(s.file_name, "atlas_player/mod.rs");
    }

    #[test]
    fn a_struct_is_named_after_the_file() {
        let s = scaffold(RustFileKind::Struct, "atlas_player");
        assert_eq!(s.file_name, "atlas_player.rs");
        assert!(s.content.contains("pub struct AtlasPlayer"));
    }

    #[test]
    fn an_empty_name_scaffolds_nothing() {
        assert!(scaffold(RustFileKind::Struct, "   ").file_name.is_empty());
    }
}
