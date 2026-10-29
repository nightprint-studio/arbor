//! Validation of the identifiers the wizard accepts.
//!
//! Every one of these ends up in a path, a manifest or an argv, so each is a strict allow-list of
//! characters rather than a deny-list of the dangerous ones: a name that passes cannot contain a
//! separator, a space, a quote or a shell metacharacter, which is what lets the runners pass it
//! on without ever quoting.

use super::model::NameRule;

const RUST_KEYWORDS: &[&str] = &[
    "as", "break", "const", "continue", "crate", "else", "enum", "extern", "false", "fn", "for", "if",
    "impl", "in", "let", "loop", "match", "mod", "move", "mut", "pub", "ref", "return", "self",
    "Self", "static", "struct", "super", "trait", "true", "type", "unsafe", "use", "where", "while",
    "async", "await", "dyn", "abstract", "become", "box", "do", "final", "macro", "override",
    "priv", "typeof", "unsized", "virtual", "yield", "try",
];

/// Names Cargo itself refuses (they collide with the sysroot crates or the test harness).
const CARGO_RESERVED: &[&str] = &["test", "core", "std", "alloc", "proc_macro", "proc-macro", "build"];

const JAVA_KEYWORDS: &[&str] = &[
    "abstract", "assert", "boolean", "break", "byte", "case", "catch", "char", "class", "const",
    "continue", "default", "do", "double", "else", "enum", "extends", "final", "finally", "float",
    "for", "goto", "if", "implements", "import", "instanceof", "int", "interface", "long", "native",
    "new", "package", "private", "protected", "public", "return", "short", "static", "strictfp",
    "super", "switch", "synchronized", "this", "throw", "throws", "transient", "try", "void",
    "volatile", "while", "true", "false", "null", "_",
];

/// Device names Windows reserves in every directory, with or without an extension.
const WINDOWS_RESERVED: &[&str] = &[
    "con", "prn", "aux", "nul", "com1", "com2", "com3", "com4", "com5", "com6", "com7", "com8",
    "com9", "lpt1", "lpt2", "lpt3", "lpt4", "lpt5", "lpt6", "lpt7", "lpt8", "lpt9",
];

fn is_windows_reserved(s: &str) -> bool {
    let stem = s.split('.').next().unwrap_or(s).to_ascii_lowercase();
    WINDOWS_RESERVED.contains(&stem.as_str())
}

/// Validate `name` under `rule`. `Err` carries a sentence for the field.
pub fn validate_name(rule: NameRule, name: &str) -> Result<(), String> {
    match rule {
        NameRule::Crate => crate_name(name),
        NameRule::Artifact => artifact_id(name),
        NameRule::Npm => npm_name(name),
    }
}

/// A folder name that is safe on every platform Arbor runs on.
pub fn folder_name(s: &str) -> Result<(), String> {
    if s.is_empty() {
        return Err("Enter a name".into());
    }
    if s.len() > 100 {
        return Err("Too long (100 characters at most)".into());
    }
    if s == "." || s == ".." {
        return Err("Not a valid folder name".into());
    }
    if let Some(c) = s.chars().find(|c| c.is_control() || "<>:\"/\\|?*".contains(*c)) {
        return Err(format!("“{}” can't be part of a folder name", c.escape_debug()));
    }
    if s.ends_with('.') || s.ends_with(' ') || s.starts_with(' ') {
        return Err("Can't start with a space or end with a space or dot".into());
    }
    if is_windows_reserved(s) {
        return Err(format!("“{s}” is reserved by Windows"));
    }
    Ok(())
}

/// A Cargo package name: starts with a letter or `_`, then letters, digits, `-` and `_`.
pub fn crate_name(s: &str) -> Result<(), String> {
    folder_name(s)?;
    let mut chars = s.chars();
    let first = chars.next().unwrap_or(' ');
    if !(first.is_ascii_alphabetic() || first == '_') {
        return Err("Must start with a letter".into());
    }
    if let Some(c) = s.chars().find(|c| !(c.is_ascii_alphanumeric() || *c == '-' || *c == '_')) {
        return Err(format!("“{c}” isn't allowed — use letters, digits, - and _"));
    }
    if s.len() > 64 {
        return Err("Crate names are at most 64 characters".into());
    }
    if RUST_KEYWORDS.contains(&s) {
        return Err(format!("“{s}” is a Rust keyword"));
    }
    if CARGO_RESERVED.contains(&s) {
        return Err(format!("“{s}” is reserved by Cargo"));
    }
    Ok(())
}

/// The identifier a crate name becomes in code: `-` is `_`.
pub fn crate_ident(name: &str) -> String {
    name.replace('-', "_")
}

/// A Maven/Gradle artifact id.
pub fn artifact_id(s: &str) -> Result<(), String> {
    folder_name(s)?;
    let first = s.chars().next().unwrap_or(' ');
    if !first.is_ascii_alphanumeric() {
        return Err("Must start with a letter or digit".into());
    }
    match s.chars().find(|c| !(c.is_ascii_alphanumeric() || ".-_".contains(*c))) {
        Some(c) => Err(format!("“{c}” isn't allowed — use letters, digits, . - and _")),
        None => Ok(()),
    }
}

/// An unscoped npm package name (lowercase; the folder takes it too).
pub fn npm_name(s: &str) -> Result<(), String> {
    folder_name(s)?;
    if s.len() > 214 {
        return Err("npm names are at most 214 characters".into());
    }
    let first = s.chars().next().unwrap_or(' ');
    if !(first.is_ascii_lowercase() || first.is_ascii_digit()) {
        return Err("Must start with a lowercase letter or digit".into());
    }
    match s.chars().find(|c| !(c.is_ascii_lowercase() || c.is_ascii_digit() || ".-_".contains(*c))) {
        Some(c) => Err(format!("“{c}” isn't allowed — lowercase letters, digits, . - and _")),
        None => Ok(()),
    }
}

/// A dotted Java package (also what a Maven group id should look like).
pub fn java_package(s: &str) -> Result<(), String> {
    if s.is_empty() {
        return Err("Enter a package, like com.example".into());
    }
    for seg in s.split('.') {
        let mut chars = seg.chars();
        let ok_first = chars.next().is_some_and(|c| c.is_ascii_alphabetic() || c == '_');
        if !ok_first || !seg.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
            return Err(format!("“{seg}” isn't a valid package segment"));
        }
        if JAVA_KEYWORDS.contains(&seg) {
            return Err(format!("“{seg}” is a Java keyword"));
        }
    }
    Ok(())
}

/// A Maven group id: dotted, letters/digits/`_`/`-` per segment.
pub fn group_id(s: &str) -> Result<(), String> {
    if s.is_empty() {
        return Err("Enter a group, like com.example".into());
    }
    for seg in s.split('.') {
        if seg.is_empty() || !seg.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-') {
            return Err(format!("“{seg}” isn't a valid group segment"));
        }
    }
    Ok(())
}

/// A version string for a manifest.
pub fn version(s: &str) -> Result<(), String> {
    let first = s.chars().next().unwrap_or(' ');
    if !first.is_ascii_alphanumeric() || !s.chars().all(|c| c.is_ascii_alphanumeric() || ".-_+".contains(c)) {
        return Err("Letters, digits and . - _ + only".into());
    }
    Ok(())
}

/// The package a project name suggests: `my-app` → `myapp`.
pub fn package_segment(artifact: &str) -> String {
    let mut out: String =
        artifact.chars().filter(|c| c.is_ascii_alphanumeric()).collect::<String>().to_ascii_lowercase();
    if out.chars().next().map_or(true, |c| c.is_ascii_digit()) {
        out.insert(0, 'a');
    }
    if JAVA_KEYWORDS.contains(&out.as_str()) {
        out.push('_');
    }
    out
}

/// A Java class name derived from an artifact id: `my-cool_app` → `MyCoolApp`.
pub fn class_name(artifact: &str) -> String {
    let mut out = String::new();
    let mut up = true;
    for c in artifact.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(if up { c.to_ascii_uppercase() } else { c });
            up = false;
        } else {
            up = true;
        }
    }
    if out.chars().next().map_or(true, |c| c.is_ascii_digit()) {
        out.insert(0, 'A');
    }
    out
}

/// A human title from a project name: `my-cool_app` → `My Cool App`.
pub fn title_case(name: &str) -> String {
    name.split(['-', '_', '.'])
        .filter(|w| !w.is_empty())
        .map(|w| {
            let mut c = w.chars();
            c.next().map(|f| f.to_ascii_uppercase().to_string() + c.as_str()).unwrap_or_default()
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crate_names() {
        for ok in ["my-app", "my_app", "app2", "_x"] {
            assert!(crate_name(ok).is_ok(), "{ok}");
        }
        for bad in ["", "2app", "my app", "my.app", "fn", "test", "con", "a/b", "..", "-x", "x*"] {
            assert!(crate_name(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn folder_names_reject_separators_and_windows_traps() {
        for bad in ["a/b", "a\\b", "x:", "nul", "NUL.txt", "dot.", " lead", "q?"] {
            assert!(folder_name(bad).is_err(), "{bad:?}");
        }
        assert!(folder_name("my project").is_ok());
        assert!(folder_name(&"x".repeat(101)).is_err());
    }

    #[test]
    fn artifact_and_npm_names() {
        assert!(artifact_id("demo-app.core").is_ok());
        assert!(artifact_id("-demo").is_err());
        assert!(artifact_id("de mo").is_err());
        assert!(npm_name("my-app").is_ok());
        assert!(npm_name("MyApp").is_err());
        assert!(npm_name("@scope/x").is_err());
    }

    #[test]
    fn packages_and_groups() {
        assert!(java_package("com.example.demo").is_ok());
        assert!(java_package("com.1x").is_err());
        assert!(java_package("com.class").is_err());
        assert!(java_package("").is_err());
        assert!(group_id("org.acme-corp").is_ok());
        assert!(group_id("org..x").is_err());
    }

    #[test]
    fn derived_identifiers() {
        assert_eq!(crate_ident("my-app"), "my_app");
        assert_eq!(package_segment("My-App_2"), "myapp2");
        assert_eq!(package_segment("2fast"), "a2fast");
        assert_eq!(package_segment("class"), "class_");
        assert_eq!(class_name("my-cool_app"), "MyCoolApp");
        assert_eq!(class_name("9lives"), "A9lives");
        assert_eq!(title_case("my-cool_app"), "My Cool App");
    }

    #[test]
    fn versions() {
        assert!(version("1.0.0-SNAPSHOT").is_ok());
        assert!(version("").is_err());
        assert!(version("1 0").is_err());
    }
}
