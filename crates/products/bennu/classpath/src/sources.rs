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

use std::fs::File;
use std::io::Read;
use std::path::Path;
use std::sync::Mutex;

/// The module segments a JDK 9+ `src.zip` prefixes its entries with
/// (`java.base/java/lang/String.java`). Probed in order for a direct lookup before a full scan;
/// mirrors [`crate::jdk`]'s classpath probe list, so the common `java.*` / `javax.*` sources
/// resolve without scanning the whole archive.
const JDK_SOURCE_MODULES: &[&str] = &[
    "java.base",
    "java.sql",
    "java.xml",
    "java.naming",
    "java.desktop",
    "java.logging",
    "java.management",
    "java.net.http",
    "java.rmi",
    "java.compiler",
    "java.instrument",
    "java.scripting",
    "java.security.jgss",
    "java.transaction.xa",
];

/// A ZIP of `.java` sources — the JDK's `src.zip` or a dependency's `-sources.jar`. Yields the
/// **real source text** for a binary class name.
///
/// `Send + Sync` (the archive is behind a [`Mutex`], not a `RefCell` like
/// [`crate::source::JarSource`]) so it can live on the shared, `Arc`-held code-intel provider
/// without a wrapping lock at the call site.
pub struct JavaSourceZip {
    archive: Mutex<zip::ZipArchive<File>>,
}

impl JavaSourceZip {
    /// Open a source ZIP at `path` (`<jdk>/lib/src.zip`, `<jdk>/src.zip`, or a `-sources.jar`).
    /// `Err` on a bad/absent archive.
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self, String> {
        let file = File::open(path.as_ref())
            .map_err(|e| format!("open source zip {}: {e}", path.as_ref().display()))?;
        let archive = zip::ZipArchive::new(file)
            .map_err(|e| format!("read source zip {}: {e}", path.as_ref().display()))?;
        Ok(Self { archive: Mutex::new(archive) })
    }

    /// The `.java` text for the binary class name `binary_name` (`java/util/Optional`), or `None`
    /// when absent.
    ///
    /// An **inner class** (`java/util/Map$Entry`) maps to its OUTER compilation unit
    /// (`java/util/Map.java`) — inner types live in the enclosing file. The lookup tries, in order:
    /// the flat layout (JDK 8 `src.zip` / a `-sources.jar`, `java/util/Map.java`), the
    /// module-prefixed layout (JDK 9+, `<module>/java/util/Map.java`) over the common modules, then a
    /// one-shot scan for a class in a module we didn't probe. Non-UTF-8 bytes are decoded lossily
    /// (JDK sources are UTF-8; this only guards a stray dependency source).
    pub fn source_text(&self, binary_name: &str) -> Option<String> {
        let outer = binary_name.split('$').next().unwrap_or(binary_name);
        let rel = format!("{outer}.java");
        let mut ar = self.archive.lock().ok()?;

        // 1. Flat: `java/util/Optional.java` (JDK 8 src.zip, dependency -sources.jar).
        if let Some(text) = read_zip_entry(&mut ar, &rel) {
            return Some(text);
        }
        // 2. Module-prefixed (JDK 9+): `<module>/java/util/Optional.java` over the common modules.
        for module in JDK_SOURCE_MODULES {
            if let Some(text) = read_zip_entry(&mut ar, &format!("{module}/{rel}")) {
                return Some(text);
            }
        }
        // 3. Fallback: a class in a module not in the probe list — scan for the entry whose path
        //    ends in `/<rel>` (the `/` anchors the boundary so `xOptional.java` can't false-match).
        let suffix = format!("/{rel}");
        let hit = (0..ar.len())
            .filter_map(|i| ar.by_index(i).ok().map(|e| e.name().to_string()))
            .find(|name| name.ends_with(&suffix))?;
        read_zip_entry(&mut ar, &hit)
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
}
