//! Breakpoints in **library** source — a class read out of a `-sources.jar` or the JDK's `src.zip`.
//!
//! A project breakpoint is found through the class index: its file declares types this project
//! owns, and those are what the VM is asked about. A library view declares nothing the index
//! knows, which is why a click in one used to install nothing at all. What it does have is a
//! **name**: the view is the source of exactly one top-level class, and that class is the whole of
//! what the VM needs — the file it is cached in is incidental.
//!
//! So a library breakpoint is identified by that top-level class (carried on the breakpoint as
//! `class`, and recovered from the view's cache path when an older entry lacks it). Everything the
//! file declares — named inner classes, anonymous ones — has a binary name starting `Outer$`, which
//! is the only thing the class-prepare pattern and the nested-type walk need to know.
//!
//! Pure on purpose: every function here is a statement about names and paths, tested without a VM.

use std::path::{Path, PathBuf};

use bennu_proto::prelude::Breakpoint;

/// Where library source views are cached — the directory the editor opens them from.
pub(crate) fn library_views_root() -> PathBuf {
    arbor_core::prelude::bennu_data_dir().join("decompiled")
}

/// `org/springframework/web/client/RestClient.java` → `org.springframework.web.client.RestClient`.
///
/// A source entry as a sources jar (or the view cache, which mirrors its layout) spells it. `None`
/// for anything that is not the compilation unit of a class: not `.java`, an empty segment, or a
/// segment no Java name can have — which is what keeps `package-info.java`, `module-info.java` and
/// `META-INF/…` out.
pub(crate) fn class_of_source_entry(entry: &str) -> Option<String> {
    let slashed = entry.replace('\\', "/");
    let stem = slashed.trim_start_matches('/').strip_suffix(".java")?;
    let segments: Vec<&str> = stem.split('/').collect();
    if segments.iter().any(|s| !is_java_name(s)) {
        return None;
    }
    Some(segments.join("."))
}

/// Whether `segment` can be one part of a qualified Java name.
fn is_java_name(segment: &str) -> bool {
    let mut chars = segment.chars();
    matches!(chars.next(), Some(c) if c.is_alphabetic() || c == '_' || c == '$')
        && chars.all(|c| c.is_alphanumeric() || c == '_' || c == '$')
}

/// The top-level class a library view at `file` is the source of, when `file` is one — a path
/// under `views_root`. `None` for every other file, which is how a project file is told apart.
///
/// Compared case-insensitively and across both slash spellings: the editor hands over a
/// forward-slashed path, the data dir is native, and on Windows the two routinely differ in case.
pub(crate) fn library_class_of_view(file: &str, views_root: &Path) -> Option<String> {
    let file = file.replace('\\', "/");
    let root = views_root.to_string_lossy().replace('\\', "/");
    let root = root.trim_end_matches('/');
    if root.is_empty() || file.len() <= root.len() + 1 || !file.is_char_boundary(root.len()) {
        return None;
    }
    let (head, rest) = file.split_at(root.len());
    if !head.eq_ignore_ascii_case(root) || !rest.starts_with('/') {
        return None;
    }
    class_of_source_entry(rest)
}

/// The top-level class a breakpoint is a **library** breakpoint on, or `None` for a project one.
///
/// The recorded class wins — it is the identity, and it survives the view cache being cleared.
/// The path is the fallback for an entry written before the class was recorded.
pub(crate) fn library_class_of(at: &Breakpoint, views_root: &Path) -> Option<String> {
    let recorded = at.class.trim();
    if !recorded.is_empty() {
        return Some(arbor_logscan::prelude::outer_class(recorded).to_string());
    }
    library_class_of_view(&at.file, views_root)
}

/// The class-prepare patterns that cover everything a top-level class's file declares: the class
/// itself, and `Outer$*` — every nested and anonymous class, at any depth, loaded whenever.
///
/// The second is not redundant. A breakpoint inside an anonymous class body belongs to `Outer$1`,
/// and a Spring class's builder to `Outer$DefaultBuilder`; neither is loaded with its outer type.
pub(crate) fn class_prepare_patterns(top_level: &str) -> [String; 2] {
    [top_level.to_string(), format!("{top_level}$*")]
}

/// Whether the loaded class `loaded` is declared in `top_level`'s compilation unit — the class
/// itself or one nested in it. `RestClientBuilder` is NOT part of `RestClient`, which a plain
/// prefix test would get wrong.
pub(crate) fn declared_in(loaded: &str, top_level: &str) -> bool {
    loaded == top_level
        || loaded.strip_prefix(top_level).is_some_and(|rest| rest.starts_with('$'))
}

#[cfg(test)]
mod tests {
    use super::*;
    use bennu_proto::prelude::DebugConfig;

    #[test]
    fn a_source_entry_names_its_class() {
        assert_eq!(
            class_of_source_entry("org/springframework/web/client/RestClient.java").as_deref(),
            Some("org.springframework.web.client.RestClient")
        );
        assert_eq!(class_of_source_entry("/java/util/Map.java").as_deref(), Some("java.util.Map"));
        assert_eq!(class_of_source_entry("com\\acme\\Order.java").as_deref(), Some("com.acme.Order"));
        // The default package is a package.
        assert_eq!(class_of_source_entry("Main.java").as_deref(), Some("Main"));
    }

    #[test]
    fn what_is_not_a_class_has_no_name() {
        assert_eq!(class_of_source_entry("org/acme/package-info.java"), None);
        assert_eq!(class_of_source_entry("module-info.java"), None);
        assert_eq!(class_of_source_entry("META-INF/MANIFEST.MF"), None);
        assert_eq!(class_of_source_entry("org/acme/Order.class"), None);
        assert_eq!(class_of_source_entry("org//Order.java"), None);
        assert_eq!(class_of_source_entry(".java"), None);
    }

    /// The editor's spelling of a view (forward slashes, whatever case) against the native data dir.
    #[test]
    fn a_library_view_is_recognised_under_the_cache_and_nowhere_else() {
        let root = Path::new("C:\\Users\\me\\AppData\\bennu\\decompiled");
        assert_eq!(
            library_class_of_view(
                "c:/users/me/appdata/bennu/decompiled/org/springframework/web/client/RestClient.java",
                root,
            )
            .as_deref(),
            Some("org.springframework.web.client.RestClient")
        );
        assert_eq!(library_class_of_view("C:/p/src/main/java/com/acme/Order.java", root), None);
        // A sibling whose name merely starts the same is not under it.
        assert_eq!(
            library_class_of_view("C:/Users/me/AppData/bennu/decompiled-old/a/B.java", root),
            None
        );
        assert_eq!(library_class_of_view("C:/Users/me/AppData/bennu/decompiled", root), None);
    }

    #[test]
    fn the_recorded_class_is_the_identity_and_the_path_only_a_fallback() {
        let root = Path::new("/data/decompiled");
        let recorded = Breakpoint {
            file: "/somewhere/else/Whatever.java".to_string(),
            line: 12,
            class: "org.acme.Client$Builder".to_string(),
            ..Breakpoint::default()
        };
        assert_eq!(library_class_of(&recorded, root).as_deref(), Some("org.acme.Client"));

        let legacy = Breakpoint {
            file: "/data/decompiled/org/acme/Client.java".to_string(),
            line: 12,
            ..Breakpoint::default()
        };
        assert_eq!(library_class_of(&legacy, root).as_deref(), Some("org.acme.Client"));

        let project = Breakpoint {
            file: "/p/src/main/java/org/acme/Client.java".to_string(),
            line: 12,
            ..Breakpoint::default()
        };
        assert_eq!(library_class_of(&project, root), None);
    }

    #[test]
    fn class_prepare_covers_the_class_and_everything_nested_in_it() {
        let [own, nested] = class_prepare_patterns("org.springframework.web.client.RestClient");
        assert_eq!(own, "org.springframework.web.client.RestClient");
        assert_eq!(nested, "org.springframework.web.client.RestClient$*");
        // JDWP accepts a star at one end only; both patterns must be ones the VM takes.
        for pattern in [&own, &nested] {
            assert!(!pattern.trim_end_matches('*').contains('*'));
        }
    }

    #[test]
    fn a_nested_class_belongs_to_its_outer_file_and_a_longer_name_does_not() {
        assert!(declared_in("org.acme.RestClient", "org.acme.RestClient"));
        assert!(declared_in("org.acme.RestClient$Builder", "org.acme.RestClient"));
        assert!(declared_in("org.acme.RestClient$1", "org.acme.RestClient"));
        assert!(declared_in("org.acme.RestClient$Builder$2", "org.acme.RestClient"));
        assert!(!declared_in("org.acme.RestClientBuilder", "org.acme.RestClient"));
        assert!(!declared_in("org.acme.Rest", "org.acme.RestClient"));
    }

    /// The persisted shape: a library breakpoint keeps its class across a save and a load, and a
    /// project breakpoint is written exactly as it was before the field existed.
    #[test]
    fn a_library_breakpoint_survives_the_config_round_trip() {
        let config = DebugConfig {
            breakpoints: vec![
                Breakpoint {
                    file: "C:/data/decompiled/org/acme/Client.java".to_string(),
                    line: 118,
                    condition: "retries > 2".to_string(),
                    class: "org.acme.Client".to_string(),
                    ..Breakpoint::default()
                },
                Breakpoint {
                    file: "C:/p/src/main/java/com/acme/Order.java".to_string(),
                    line: 7,
                    ..Breakpoint::default()
                },
            ],
            exceptions: Vec::new(),
            watches: Vec::new(),
        };

        let text = toml::to_string(&config).expect("serialises");
        let back: DebugConfig = toml::from_str(&text).expect("deserialises");
        assert_eq!(back, config);
        assert_eq!(text.matches("class =").count(), 1, "only the library one carries a class");

        let json = serde_json::to_value(&config).expect("json");
        let back: DebugConfig = serde_json::from_value(json).expect("json back");
        assert_eq!(back, config);

        // An entry from before the field existed still reads — as a project breakpoint.
        let old: Breakpoint =
            toml::from_str("file = \"C:/p/A.java\"\nline = 3\nenabled = true\n").expect("old");
        assert!(old.class.is_empty());
    }
}
