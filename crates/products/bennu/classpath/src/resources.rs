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
//! A jar that cannot be opened, an entry that is not valid UTF-8, a truncated archive — each
//! costs exactly its own entry. One unreadable dependency out of three hundred must not cost
//! the vocabulary carried by the rest, so nothing here returns an error.

use std::io::Read;
use std::path::{Path, PathBuf};

/// One entry read out of one jar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JarResource {
    /// The jar it came from.
    pub jar: PathBuf,
    /// The entry name inside the jar, as requested.
    pub entry: String,
    /// The decoded text (UTF-8, lossily).
    pub text: String,
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

fn read_from_jar(jar: &Path, entries: &[&str], out: &mut Vec<JarResource>) {
    let Ok(file) = std::fs::File::open(jar) else { return };
    let Ok(mut archive) = zip::ZipArchive::new(std::io::BufReader::new(file)) else { return };
    let name = jar.file_name().and_then(|n| n.to_str()).unwrap_or_default().to_string();
    for entry in entries {
        let Ok(mut zipped) = archive.by_name(entry) else { continue };
        let mut text = String::new();
        // `read_to_string` fails on non-UTF-8; these descriptors are JSON and therefore UTF-8
        // by spec, so a failure means a corrupt entry and skipping is the right answer.
        if zipped.read_to_string(&mut text).is_err() || text.is_empty() {
            continue;
        }
        out.push(JarResource {
            jar: jar.to_path_buf(),
            entry: (*entry).to_string(),
            id: format!("{name}!/{entry}"),
            text,
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
        let mut zip = zip::ZipWriter::new(std::fs::File::create(path).unwrap());
        for (name, body) in files {
            zip.start_file(*name, SimpleFileOptions::default()).unwrap();
            zip.write_all(body.as_bytes()).unwrap();
        }
        zip.finish().unwrap();
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
        assert_eq!(found[0].text, r#"{"properties":[]}"#);
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
        assert_eq!(found[0].text, "{}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn several_entries_can_be_asked_for_at_once_and_keep_their_order() {
        let dir = temp_dir("multi");
        let jar = dir.join("both.jar");
        make_jar(&jar, &[("b.json", "second"), ("a.json", "first")]);
        let found = read_jar_entries(&[jar], &["a.json", "b.json"]);
        let texts: Vec<&str> = found.iter().map(|r| r.text.as_str()).collect();
        // The order asked for, not the order the archive happens to store them in — a caller
        // that folds "first one wins" depends on this being deterministic.
        assert_eq!(texts, ["first", "second"]);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
