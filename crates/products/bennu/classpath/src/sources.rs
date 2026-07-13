//! Java **source** containers — the real `.java` text for a library/JDK type, for the
//! "go to source" view.
//!
//! Distinct from [`crate::source::ClassSource`], which yields *bytecode* members (from which a
//! signatures-only stub is rendered): this yields the **actual source** — with method bodies, local
//! variables, lambdas, anonymous classes — so go-to-into-a-JDK-class shows the real code, not a stub.
//!
//! Two containers, one shape (a ZIP of `.java`):
//!
//! - the JDK's `src.zip` — `lib/src.zip` (JDK 9+, entries `<module>/java/lang/String.java`) or
//!   `src.zip` at the JDK root (JDK 8, flat entries `java/lang/String.java`). Present when a full
//!   JDK (not a bare JRE) is installed; [`crate::jdk::resolve_jdk_sources`] locates it.
//! - a Maven dependency's `-sources.jar` (flat entries) — layered in the same way as the bytecode
//!   dep jars (a follow-up; the type is ready for it).
//!
//! `None` from a lookup means "no source here" — the caller falls back to the signatures-only
//! decompiled stub.

use std::collections::HashMap;
use std::fs::File;
use std::io::Read;
use std::path::Path;
use std::sync::{Mutex, OnceLock};

/// A ZIP of `.java` sources — the JDK's `src.zip` or a dependency's `-sources.jar`. Yields the
/// **real source text** for a binary class name.
///
/// `Send + Sync` (the archive is behind a [`Mutex`], not a `RefCell` like
/// [`crate::source::JarSource`]) so it can live on the shared, `Arc`-held code-intel provider
/// without a wrapping lock at the call site.
pub struct JavaSourceZip {
    archive: Mutex<zip::ZipArchive<File>>,
    /// Lazily-built map from a **module-stripped** relative path (`java/util/Optional.java`) to the
    /// real entry name (flat `java/util/Optional.java` or module-prefixed
    /// `java.base/java/util/Optional.java`). Built once from the central directory on the first
    /// lookup so `source_text` is an O(1) map hit instead of a full-archive linear scan — the scan
    /// spiked CPU (~one core) on every go-to into a dependency type whose source isn't in this ZIP
    /// (a guaranteed miss that used to walk every entry), and re-ran on each Ctrl+B (no miss cache).
    index: OnceLock<HashMap<String, String>>,
}

impl JavaSourceZip {
    /// Open a source ZIP at `path` (`<jdk>/lib/src.zip`, `<jdk>/src.zip`, or a `-sources.jar`).
    /// `Err` on a bad/absent archive.
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self, String> {
        let file = File::open(path.as_ref())
            .map_err(|e| format!("open source zip {}: {e}", path.as_ref().display()))?;
        let archive = zip::ZipArchive::new(file)
            .map_err(|e| format!("read source zip {}: {e}", path.as_ref().display()))?;
        Ok(Self { archive: Mutex::new(archive), index: OnceLock::new() })
    }

    /// The `.java` text for the binary class name `binary_name` (`java/util/Optional`), or `None`
    /// when absent.
    ///
    /// An **inner class** (`java/util/Map$Entry`) maps to its OUTER compilation unit
    /// (`java/util/Map.java`) — inner types live in the enclosing file. Resolution is a single O(1)
    /// hit against a map built once from the archive's central directory (see [`Self::index`]),
    /// which uniformly covers the flat layout (JDK 8 `src.zip` / a `-sources.jar`) and the
    /// module-prefixed layout (JDK 9+, any module — not just a probed subset). A binary not in this
    /// ZIP misses instantly (the caller falls back to the decompiled stub). Non-UTF-8 bytes are
    /// decoded lossily (JDK sources are UTF-8; this only guards a stray dependency source).
    pub fn source_text(&self, binary_name: &str) -> Option<String> {
        let outer = binary_name.split('$').next().unwrap_or(binary_name);
        let rel = format!("{outer}.java");
        let entry = self.index().get(&rel)?.clone();
        let mut ar = self.archive.lock().ok()?;
        read_zip_entry(&mut ar, &entry)
    }

    /// The module-stripped-relative-path → real-entry-name map, built once (lock-free after the
    /// first call) from the central directory via `file_names()` — which reads only the in-memory
    /// directory, never a per-entry local header, so building it is cheap and, crucially, happens
    /// **once** rather than a full-archive scan on every lookup.
    fn index(&self) -> &HashMap<String, String> {
        self.index.get_or_init(|| {
            let ar = self.archive.lock().unwrap_or_else(|p| p.into_inner());
            let mut map = HashMap::new();
            for name in ar.file_names() {
                if !name.ends_with(".java") {
                    continue;
                }
                // A flat entry (`java/util/Map.java`) keys as-is; a module-prefixed one
                // (`java.base/java/util/Map.java`) keys by its path after the module segment, so a
                // caller's binary-derived `rel` resolves regardless of layout. First writer wins —
                // if both shapes exist for the same class, either serves identical source.
                map.entry(strip_module_prefix(name).to_string())
                    .or_insert_with(|| name.to_string());
            }
            map
        })
    }
}

/// Strip a JDK 9+ module segment from a `src.zip` entry name, yielding the layout-independent
/// relative path (`java.base/java/util/Map.java` → `java/util/Map.java`). A module segment is the
/// first path component and always contains a `.` (`java.base`, `jdk.compiler`); a package
/// component never does (`java`, `com`, `org`) — so a flat entry (`com/acme/Foo.java`) is returned
/// unchanged. A worst-case misjudgement only yields a lookup miss (→ stub fallback), never a scan.
fn strip_module_prefix(name: &str) -> &str {
    match name.split_once('/') {
        Some((head, rest)) if head.contains('.') => rest,
        _ => name,
    }
}

/// Read one `.java` entry `name` from `ar` as text, or `None` when it's absent. Bytes are decoded
/// UTF-8-lossily so a stray non-UTF-8 dependency source is shown (mojibake) rather than dropped.
fn read_zip_entry(ar: &mut zip::ZipArchive<File>, name: &str) -> Option<String> {
    // Bind the entry to a local so the borrowed `ZipFile` is dropped before `ar` is used again
    // (zip 2.x ties the entry's lifetime to the archive borrow — same care as `JarSource`).
    let mut bytes = Vec::new();
    match ar.by_name(name) {
        Ok(mut zf) => zf.read_to_end(&mut bytes).ok()?,
        Err(_) => return None,
    };
    Some(String::from_utf8_lossy(&bytes).into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::path::PathBuf;

    /// Build a `.java` source zip in a temp file and open it as a [`JavaSourceZip`].
    fn source_zip(entries: &[(&str, &str)]) -> (JavaSourceZip, PathBuf) {
        // A per-call unique-ish name without `rand`: the test module path + the entry count + the
        // first entry name. Good enough to avoid collisions between the two tests here.
        let stem = entries.first().map(|(n, _)| n.replace(['/', '.'], "_")).unwrap_or_default();
        let path = std::env::temp_dir().join(format!("bennu_src_{}_{}.zip", entries.len(), stem));
        let file = File::create(&path).expect("create temp zip");
        let mut zw = zip::ZipWriter::new(file);
        let opts = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored);
        for (name, body) in entries {
            zw.start_file(*name, opts).expect("start entry");
            zw.write_all(body.as_bytes()).expect("write entry");
        }
        zw.finish().expect("finish zip");
        (JavaSourceZip::open(&path).expect("open zip"), path)
    }

    #[test]
    fn resolves_flat_module_prefixed_and_inner_classes() {
        let (zip, path) = source_zip(&[
            // Module-prefixed (JDK 9+ src.zip shape).
            ("java.base/java/util/Optional.java", "package java.util; class Optional { /*body*/ }"),
            // Flat (JDK 8 src.zip / a -sources.jar shape).
            ("com/acme/Foo.java", "package com.acme; class Foo {}"),
        ]);

        // Module-prefixed lookup by binary name (no module known to the caller).
        let opt = zip.source_text("java/util/Optional").expect("Optional resolves");
        assert!(opt.contains("class Optional"), "real body served: {opt}");

        // Flat lookup.
        assert!(zip.source_text("com/acme/Foo").is_some(), "flat entry resolves");

        // An INNER class maps to its outer compilation unit's file.
        let inner = zip.source_text("java/util/Optional$1").expect("inner maps to outer");
        assert!(inner.contains("class Optional"), "inner class served from the outer file");

        // A class not present → None (caller falls back to the stub).
        assert!(zip.source_text("java/util/Absent").is_none());

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn resolves_class_in_any_module_without_scanning() {
        // A module NOT in the old probe list (`jdk.compiler`) — used to require the full-archive
        // scan; now the central-directory index resolves it uniformly.
        let (zip, path) = source_zip(&[(
            "jdk.compiler/com/sun/tools/javac/Main.java",
            "package com.sun.tools.javac; class Main {}",
        )]);
        assert!(zip.source_text("com/sun/tools/javac/Main").is_some(), "unprobed module resolves");
        assert!(zip.source_text("com/sun/tools/javac/Absent").is_none());
        let _ = std::fs::remove_file(path);
    }
}
