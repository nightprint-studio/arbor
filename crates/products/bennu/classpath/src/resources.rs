//! Reading **non-class** entries out of dependency jars.
//!
//! The rest of this crate opens jars to decode bytecode. This module opens them for the other
//! thing they carry: the descriptor files a library ships to describe *itself*.
//!
//! The motivating case is Spring Boot's `META-INF/spring-configuration-metadata.json` — every
//! starter packages a machine-readable list of the properties it accepts, with types, defaults
//! and prose. Reading it out of the resolved jars gives a consumer a vocabulary that is exact
//! for the versions this project actually depends on, and it works offline. Nothing about the
//! mechanism is Spring-specific, though, which is why the entry names are a parameter: a jar
//! entry is a jar entry, and this crate is where opening one belongs.
//!
//! ## Cost, and why it is acceptable
//!
//! Opening a ZIP reads its central directory, not its contents — so the per-jar cost is one
//! seek plus a small read even for a large jar, and a jar that carries none of the wanted
//! entries costs nothing further. Over a few hundred dependencies that is tens of
//! milliseconds. Still: this is I/O over the whole classpath, so callers run it off the
//! request path and cache the result against the jar list.
//!
//! ## Failures are skips
//!
//! A jar that cannot be opened, a truncated archive — each costs exactly its own entry. One
//! unreadable dependency out of three hundred must not cost the vocabulary carried by the rest,
//! so nothing here returns an error.
//!
//! ## Bytes, not text
//!
//! An entry comes back as **bytes**, and deciding what they say is the caller's. A jar is full
//! of text that is not UTF-8: a `.properties` is ISO-8859-1 by the `Properties.load` spec, and a
//! descriptor written on a Windows box in 2009 is Windows-1252 whatever its XML prolog claims.
//! Guessing here would mean either a lossy decode — an accent replaced by `U+FFFD`, information
//! destroyed at the lowest layer where it can never be recovered — or a second, divergent copy of
//! the encoding policy that this project already has one of. Neither belongs in the crate whose
//! job is opening zips.

use std::io::Read;
use std::path::{Path, PathBuf};

/// One entry read out of one jar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JarResource {
    /// The jar it came from.
    pub jar: PathBuf,
    /// The entry name inside the jar, as requested.
    pub entry: String,
    /// The entry's raw bytes. Undecoded on purpose — see the module doc.
    pub bytes: Vec<u8>,
    /// `<jar file name>!/<entry>` — the conventional way to name a jar entry, and a stable
    /// display identity for a consumer that never sees the filesystem.
    pub id: String,
}

/// Read `entries` from every jar in `jars`, skipping whatever cannot be read.
///
/// The result is ordered by jar, then by the order the entries were asked for — so a caller
/// that folds these into an index in sequence gets a deterministic outcome, which matters when
/// the fold is "first description of a key wins".
pub fn read_jar_entries(jars: &[PathBuf], entries: &[&str]) -> Vec<JarResource> {
    let mut out = Vec::new();
    for jar in jars {
        read_from_jar(jar, entries, &mut out);
    }
    out
}

/// Read every entry whose name `wanted` accepts, from every jar in `jars`.
///
/// The other half of [`read_jar_entries`], for the case where the caller knows the *shape* of the
/// name rather than the name itself: a framework ships its schema as `struts-2.5.dtd` or
/// `spring-beans-5.3.xsd`, and which versions are on this classpath is exactly what the caller is
/// trying to find out.
///
/// Costs more than the by-name form — the whole central directory is walked rather than one
/// lookup per name — so it is bounded: `limit` caps the entries taken from any single jar, which
/// keeps a pathological artifact (a jar of ten thousand generated schemas) from turning one
/// project scan into a stall. Same skip-on-failure rule as everything else here.
pub fn read_jar_entries_matching(
    jars: &[PathBuf],
    wanted: impl Fn(&str) -> bool,
    limit: usize,
) -> Vec<JarResource> {
    let mut out = Vec::new();
    for jar in jars {
        let Ok(file) = std::fs::File::open(jar) else { continue };
        let Ok(mut archive) = zip::ZipArchive::new(std::io::BufReader::new(file)) else { continue };
        let name = jar.file_name().and_then(|n| n.to_str()).unwrap_or_default().to_string();
        let mut taken = 0usize;
        // Names first, then reads: `by_index` borrows the archive mutably, so collecting the
        // matches up front is what lets the loop below open them one at a time.
        let matches: Vec<(usize, String)> = (0..archive.len())
            .filter_map(|i| archive.name_for_index(i).map(|n| (i, n.to_string())))
            .filter(|(_, n)| wanted(n))
            .take(limit)
            .collect();
        for (index, entry) in matches {
            if taken >= limit {
                break;
            }
            let Ok(mut zipped) = archive.by_index(index) else { continue };
            let mut bytes = Vec::new();
            if zipped.read_to_end(&mut bytes).is_err() || bytes.is_empty() {
                continue;
            }
            out.push(JarResource {
                jar: jar.to_path_buf(),
                id: format!("{name}!/{entry}"),
                bytes,
                entry,
            });
            taken += 1;
        }
    }
    out
}

/// Everything `jar` holds, by name: `(binary class names, other entry names)`.
///
/// Class names are slash-form with the `.class` gone (`org/springframework/stereotype/Service`),
/// exactly as [`ClassSource::class_names`](crate::source::ClassSource::class_names) yields them.
/// The second list is every other entry that is not a directory — the XML, DTDs, XSDs, TLDs and
/// property files a library ships, which on a legacy classpath are as worth navigating to as the
/// bytecode is.
///
/// One pass over the central directory for both, because the caller who wants one usually wants
/// the other and opening the archive twice doubles the only cost there is. Reading names does not
/// decompress anything: this is a seek and a small read even on a large jar.
///
/// Same skip-on-failure rule as the rest of this module — an unreadable jar contributes nothing
/// and costs nobody else.
pub fn jar_entry_names(jar: &Path) -> (Vec<String>, Vec<String>) {
    let mut classes = Vec::new();
    let mut resources = Vec::new();
    let Ok(file) = std::fs::File::open(jar) else { return (classes, resources) };
    let Ok(archive) = zip::ZipArchive::new(std::io::BufReader::new(file)) else {
        return (classes, resources);
    };
    for i in 0..archive.len() {
        let Some(name) = archive.name_for_index(i) else { continue };
        if let Some(binary) = name.strip_suffix(".class") {
            classes.push(binary.to_string());
        } else if !name.ends_with('/') {
            resources.push(name.to_string());
        }
    }
    (classes, resources)
}

/// The raw bytes of one entry of one jar, or `None` when the jar or the entry is absent.
///
/// Undecoded, like everything else here — what those bytes say is the caller's question (see the
/// module doc).
pub fn read_jar_entry_bytes(jar: &Path, entry: &str) -> Option<Vec<u8>> {
    let file = std::fs::File::open(jar).ok()?;
    let mut archive = zip::ZipArchive::new(std::io::BufReader::new(file)).ok()?;
    let mut zipped = archive.by_name(entry).ok()?;
    let mut bytes = Vec::new();
    zipped.read_to_end(&mut bytes).ok()?;
    Some(bytes)
}

fn read_from_jar(jar: &Path, entries: &[&str], out: &mut Vec<JarResource>) {
    let Ok(file) = std::fs::File::open(jar) else { return };
    let Ok(mut archive) = zip::ZipArchive::new(std::io::BufReader::new(file)) else { return };
    let name = jar.file_name().and_then(|n| n.to_str()).unwrap_or_default().to_string();
    for entry in entries {
        let Ok(mut zipped) = archive.by_name(entry) else { continue };
        let mut bytes = Vec::new();
        if zipped.read_to_end(&mut bytes).is_err() || bytes.is_empty() {
            continue;
        }
        out.push(JarResource {
            jar: jar.to_path_buf(),
            entry: (*entry).to_string(),
            id: format!("{name}!/{entry}"),
            bytes,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use zip::write::SimpleFileOptions;

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir()
            .join(format!("bennu-classpath-res-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn make_jar(path: &Path, files: &[(&str, &str)]) {
        let as_bytes: Vec<(&str, &[u8])> = files.iter().map(|(n, b)| (*n, b.as_bytes())).collect();
        make_jar_bytes(path, &as_bytes);
    }

    /// The same, for a fixture whose point is that it is NOT UTF-8.
    fn make_jar_bytes(path: &Path, files: &[(&str, &[u8])]) {
        let mut zip = zip::ZipWriter::new(std::fs::File::create(path).unwrap());
        for (name, body) in files {
            zip.start_file(*name, SimpleFileOptions::default()).unwrap();
            zip.write_all(body).unwrap();
        }
        zip.finish().unwrap();
    }

    #[test]
    fn listing_a_jar_separates_its_bytecode_from_everything_else() {
        let dir = temp_dir("names");
        let jar = dir.join("struts2-core-2.5.30.jar");
        make_jar(
            &jar,
            &[
                ("org/apache/struts2/ServletActionContext.class", "bytecode"),
                ("org/apache/struts2/dispatcher/Dispatcher$1.class", "bytecode"),
                ("struts-default.xml", "<struts/>"),
                ("META-INF/struts-plugin.xml", "<struts/>"),
            ],
        );
        let (mut classes, mut resources) = jar_entry_names(&jar);
        classes.sort();
        resources.sort();
        assert_eq!(
            classes,
            [
                "org/apache/struts2/ServletActionContext",
                "org/apache/struts2/dispatcher/Dispatcher$1",
            ],
            "the `.class` suffix is gone and the name is left in slash form"
        );
        assert_eq!(resources, ["META-INF/struts-plugin.xml", "struts-default.xml"]);
    }

    #[test]
    fn a_missing_or_unreadable_jar_lists_nothing_rather_than_failing() {
        let dir = temp_dir("absent");
        let (classes, resources) = jar_entry_names(&dir.join("not-here.jar"));
        assert!(classes.is_empty() && resources.is_empty());
        assert!(read_jar_entry_bytes(&dir.join("not-here.jar"), "anything").is_none());
    }

    /// The case a lossy read used to destroy: `0xE0` is `a` with a grave accent in ISO-8859-1
    /// and not valid UTF-8 at all. It has to reach the caller as the byte it is — decoding is a
    /// decision made further up, and it cannot be made at all once the byte is a `U+FFFD`.
    #[test]
    fn an_entry_that_is_not_utf8_reaches_the_caller_intact() {
        let dir = temp_dir("one");
        let jar = dir.join("acme-1.0.jar");
        make_jar_bytes(&jar, &[("config/app.properties", b"city=citt\xe0\n".as_slice())]);
        assert_eq!(
            read_jar_entry_bytes(&jar, "config/app.properties").as_deref(),
            Some(b"city=citt\xe0\n".as_slice()),
        );
        assert!(read_jar_entry_bytes(&jar, "config/absent.properties").is_none());
    }

    /// The schema case: the caller knows the shape of the name, not the name — which versions
    /// of a framework's DTD are on this classpath is exactly what it is trying to find out.
    #[test]
    fn entries_can_be_matched_by_shape_rather_than_by_name() {
        let dir = temp_dir("match");
        let jar = dir.join("struts2-core-2.5.30.jar");
        make_jar(
            &jar,
            &[
                ("struts-2.5.dtd", "<!ELEMENT struts EMPTY>"),
                ("struts-2.3.dtd", "<!ELEMENT struts EMPTY>"),
                ("org/apache/struts2/Thing.class", "not really bytecode"),
            ],
        );
        let found = read_jar_entries_matching(
            std::slice::from_ref(&jar),
            |n| n.ends_with(".dtd"),
            16,
        );
        let mut names: Vec<&str> = found.iter().map(|r| r.entry.as_str()).collect();
        names.sort();
        assert_eq!(names, ["struts-2.3.dtd", "struts-2.5.dtd"]);
        assert!(found[0].id.starts_with("struts2-core-2.5.30.jar!/"));

        // Bounded, so one pathological artifact cannot stall a project scan.
        assert_eq!(read_jar_entries_matching(std::slice::from_ref(&jar), |n| n.ends_with(".dtd"), 1).len(), 1);
    }

    #[test]
    fn a_wanted_entry_is_read_and_identified_by_jar_and_entry() {
        let dir = temp_dir("read");
        let jar = dir.join("acme-starter-1.0.jar");
        make_jar(
            &jar,
            &[
                ("META-INF/spring-configuration-metadata.json", r#"{"properties":[]}"#),
                ("com/acme/Thing.class", "not really bytecode"),
            ],
        );
        let found = read_jar_entries(
            std::slice::from_ref(&jar),
            &["META-INF/spring-configuration-metadata.json"],
        );
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].bytes, br#"{"properties":[]}"#);
        assert_eq!(found[0].id, "acme-starter-1.0.jar!/META-INF/spring-configuration-metadata.json");
        assert_eq!(found[0].jar, jar);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The common case by far: most jars carry none of the wanted entries, and that must be
    /// silent rather than an error to handle.
    #[test]
    fn a_jar_without_the_entry_contributes_nothing() {
        let dir = temp_dir("miss");
        let jar = dir.join("plain.jar");
        make_jar(&jar, &[("com/acme/Thing.class", "x")]);
        assert!(read_jar_entries(&[jar], &["META-INF/wanted.json"]).is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// One broken dependency out of many must cost only itself.
    #[test]
    fn an_unreadable_jar_is_skipped_without_taking_the_others_with_it() {
        let dir = temp_dir("broken");
        let good = dir.join("good.jar");
        make_jar(&good, &[("META-INF/wanted.json", "{}")]);
        let broken = dir.join("broken.jar");
        std::fs::write(&broken, b"this is not a zip archive").unwrap();
        let missing = dir.join("absent.jar");

        let found = read_jar_entries(&[broken, missing, good], &["META-INF/wanted.json"]);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].bytes, b"{}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn several_entries_can_be_asked_for_at_once_and_keep_their_order() {
        let dir = temp_dir("multi");
        let jar = dir.join("both.jar");
        make_jar(&jar, &[("b.json", "second"), ("a.json", "first")]);
        let found = read_jar_entries(&[jar], &["a.json", "b.json"]);
        let texts: Vec<&[u8]> = found.iter().map(|r| r.bytes.as_slice()).collect();
        // The order asked for, not the order the archive happens to store them in — a caller
        // that folds "first one wins" depends on this being deterministic.
        assert_eq!(texts, [b"first".as_slice(), b"second".as_slice()]);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
