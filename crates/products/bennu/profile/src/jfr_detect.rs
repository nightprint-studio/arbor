//! Can this JDK record with JDK Flight Recorder, and how do we read the file afterwards?
//!
//! Two pure halves so the matrix is unit-tested with fixtures: [`JdkFacts`] (what was observed:
//! the `release` file, the binaries present) and [`classify`] (the decision). The only I/O is
//! [`probe`], which reads `release` and looks for files — it spawns nothing, except the single
//! `java -version` it falls back to when a JDK has no `release` file at all.
//!
//! ## The matrix
//!
//! | JDK                                   | Recording | Reading the file                         |
//! |---------------------------------------|-----------|------------------------------------------|
//! | OpenJ9 / IBM Semeru / IBM J9          | no        | —                                        |
//! | < 8                                   | no        | —                                        |
//! | 8 before 8u262 (Oracle: commercial)   | no        | —                                        |
//! | 8u262+                                | yes       | helper compiled with `javac` (no `jfr`)  |
//! | 9, 10                                 | no        | — (JFR open-sourced in 11)               |
//! | 11                                    | yes       | `jfr` of another JDK 12+, else helper run as `java Helper.java` |
//! | 12+                                   | yes       | the JDK's own `jfr`                      |
//! | any jlink image without `jdk.jfr`     | no        | —                                        |

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// What was observed about a JDK installation.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct JdkFacts {
    pub major: u32,
    /// The `_NNN` update of a `1.8.0_NNN` version.
    pub update: Option<u32>,
    pub vendor: String,
    pub is_openj9: bool,
    /// Module list when the image declares one (a jlink image always does).
    pub modules: Option<Vec<String>>,
    pub has_jfr_cli: bool,
    pub has_javac: bool,
    pub has_jcmd: bool,
}

/// How the recorded file is turned into events.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ParseHow {
    /// `<jdk>/bin/jfr print --json …`
    JfrCli { jfr: PathBuf },
    /// `java ArborJfrDump.java <file>` (source-file mode, JDK 11+).
    HelperSource { java: PathBuf },
    /// `javac` the helper once, then `java -cp <dir> ArborJfrDump <file>` (JDK 8).
    HelperCompiled { java: PathBuf, javac: PathBuf },
}

/// The verdict, with text meant to be shown to the user as-is.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JfrAvailability {
    pub available: bool,
    pub how_to_parse: Option<ParseHow>,
    /// One line: what is the situation.
    pub reason: String,
    /// What the user can do about it; `None` when nothing is wrong.
    pub remedy: Option<String>,
    /// `Major.update` as detected, for the message.
    pub jdk: String,
}

/// Parse `JAVA_VERSION` text: `1.8.0_262` → (8, Some(262)), `11.0.2` → (11, None), `17` → (17, None),
/// `21-ea` → (21, None).
pub fn parse_java_version(v: &str) -> Option<(u32, Option<u32>)> {
    let v = v.trim().trim_matches('"');
    if let Some(rest) = v.strip_prefix("1.") {
        let major: u32 = rest.split(|c: char| !c.is_ascii_digit()).next()?.parse().ok()?;
        let update = v.split('_').nth(1).and_then(|u| {
            u.split(|c: char| !c.is_ascii_digit()).next().and_then(|d| d.parse().ok())
        });
        return Some((major, update));
    }
    let major: u32 = v.split(|c: char| !c.is_ascii_digit()).next()?.parse().ok()?;
    // 8u262-style (`8.0.262`) as some vendors write it.
    let update = if major == 8 {
        v.split(|c: char| !c.is_ascii_digit()).filter(|s| !s.is_empty()).nth(2).and_then(|u| u.parse().ok())
    } else {
        None
    };
    Some((major, update))
}

/// Fields of a JDK `release` file.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct ReleaseInfo {
    pub java_version: Option<String>,
    pub implementor: String,
    pub jvm_variant: String,
    pub modules: Option<Vec<String>>,
}

pub fn parse_release(text: &str) -> ReleaseInfo {
    let mut r = ReleaseInfo::default();
    for line in text.lines() {
        let Some((k, v)) = line.split_once('=') else { continue };
        let v = v.trim().trim_matches('"').to_string();
        match k.trim() {
            "JAVA_VERSION" => r.java_version = Some(v),
            "IMPLEMENTOR" => r.implementor = v,
            "JVM_VARIANT" => r.jvm_variant = v,
            "MODULES" => r.modules = Some(v.split_whitespace().map(str::to_string).collect()),
            _ => {}
        }
    }
    r
}

/// Observe a JDK home. `java -version` is consulted only when there is no usable `release` file.
pub fn probe(java_home: &Path) -> Option<JdkFacts> {
    let exe = |n: &str| {
        let p = java_home.join("bin").join(if cfg!(windows) { format!("{n}.exe") } else { n.to_string() });
        p.is_file()
    };
    if !exe("java") {
        return None;
    }
    let release = std::fs::read_to_string(java_home.join("release")).ok().map(|t| parse_release(&t));
    let mut facts = JdkFacts {
        has_jfr_cli: exe("jfr"),
        has_javac: exe("javac"),
        has_jcmd: exe("jcmd"),
        ..JdkFacts::default()
    };
    let mut from_release = false;
    if let Some(r) = release {
        if let Some((major, update)) = r.java_version.as_deref().and_then(parse_java_version) {
            facts.major = major;
            facts.update = update;
            from_release = true;
        }
        facts.vendor = r.implementor.clone();
        facts.is_openj9 = r.jvm_variant.to_ascii_lowercase().contains("j9")
            || r.implementor.to_ascii_lowercase().contains("semeru");
        facts.modules = r.modules;
    }
    if !from_release || facts.vendor.is_empty() {
        if let Some(text) = java_version_text(java_home) {
            let lower = text.to_ascii_lowercase();
            if lower.contains("openj9") || lower.contains("j9 vm") || lower.contains("ibm j9") {
                facts.is_openj9 = true;
            }
            if !from_release {
                if let Some((major, update)) = version_from_banner(&text) {
                    facts.major = major;
                    facts.update = update;
                }
            }
            if facts.vendor.is_empty() {
                facts.vendor = if lower.contains("java(tm)") { "Oracle".into() } else { String::new() };
            }
        }
    }
    (facts.major != 0).then_some(facts)
}

/// `java version "1.8.0_202"` / `openjdk version "17.0.2" …` → version.
pub fn version_from_banner(text: &str) -> Option<(u32, Option<u32>)> {
    let start = text.find('"')? + 1;
    let end = start + text[start..].find('"')?;
    parse_java_version(&text[start..end])
}

fn java_version_text(java_home: &Path) -> Option<String> {
    let java = java_home.join("bin").join(if cfg!(windows) { "java.exe" } else { "java" });
    let mut cmd = std::process::Command::new(java);
    cmd.arg("-version").stdin(std::process::Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    let out = cmd.output().ok()?;
    Some(format!("{}{}", String::from_utf8_lossy(&out.stderr), String::from_utf8_lossy(&out.stdout)))
}

/// Decide. `alt_jfr_cli` is the `jfr` binary of *another* installed JDK 12+, which can read a
/// recording made by a JDK 11 (the file format is forward-readable).
pub fn classify(f: &JdkFacts, java: &Path, javac: Option<&Path>, own_jfr: Option<&Path>, alt_jfr_cli: Option<&Path>) -> JfrAvailability {
    let jdk = match f.update {
        Some(u) => format!("{}u{}", f.major, u),
        None => f.major.to_string(),
    };
    let no = |reason: &str, remedy: &str| JfrAvailability {
        available: false,
        how_to_parse: None,
        reason: reason.to_string(),
        remedy: Some(remedy.to_string()),
        jdk: jdk.clone(),
    };

    if f.is_openj9 {
        return no(
            "This JVM is OpenJ9 (IBM Semeru / IBM J9), which has no JDK Flight Recorder.",
            "Use a HotSpot JDK for profiling (Eclipse Temurin, Oracle JDK, Amazon Corretto, Azul Zulu) \
             and select it as the project's JDK. OpenJ9 offers its own tooling (-Xtrace, Health Center) which Bennu cannot read.",
        );
    }
    if f.major < 8 {
        return no(
            &format!("JDK {} predates Flight Recorder.", f.major),
            "Profile with JDK 8u262 or newer (11 or 17 recommended).",
        );
    }
    if f.major == 9 || f.major == 10 {
        return no(
            &format!("JDK {} has no open-source Flight Recorder (it arrived in JDK 11).", f.major),
            "Select JDK 11 or newer for the profiling run (the application may still target an older language level).",
        );
    }
    if f.major == 8 && f.update.map_or(true, |u| u < 262) {
        let oracle = f.vendor.to_ascii_lowercase().contains("oracle") || f.vendor.is_empty();
        return no(
            &format!("JDK {jdk} predates the Flight Recorder backport (8u262)."),
            &if oracle {
                "Update to JDK 8u262 or newer, or use JDK 11+. Older Oracle JDK 8 builds only record with \
                 -XX:+UnlockCommercialFeatures, which requires a commercial license; Bennu does not enable it."
                    .to_string()
            } else {
                "Update to JDK 8u262 or newer, or use JDK 11+.".to_string()
            },
        );
    }
    if let Some(mods) = &f.modules {
        if !mods.iter().any(|m| m == "jdk.jfr") {
            return no(
                "This runtime image was assembled with jlink and does not contain the module jdk.jfr.",
                "Rebuild the image with --add-modules jdk.jfr, or select a full JDK for the profiling run.",
            );
        }
    }

    let parse = if let Some(jfr) = own_jfr.filter(|_| f.has_jfr_cli) {
        Some(ParseHow::JfrCli { jfr: jfr.to_path_buf() })
    } else if let Some(alt) = alt_jfr_cli {
        Some(ParseHow::JfrCli { jfr: alt.to_path_buf() })
    } else if f.major >= 11 {
        Some(ParseHow::HelperSource { java: java.to_path_buf() })
    } else {
        javac.filter(|_| f.has_javac).map(|jc| ParseHow::HelperCompiled { java: java.to_path_buf(), javac: jc.to_path_buf() })
    };

    match parse {
        Some(how) => {
            let via = match &how {
                ParseHow::JfrCli { .. } => "the jfr tool",
                ParseHow::HelperSource { .. } => "a built-in reader run with `java` (this JDK has no jfr tool)",
                ParseHow::HelperCompiled { .. } => "a built-in reader compiled with javac (JDK 8 has no jfr tool)",
            };
            JfrAvailability {
                available: true,
                how_to_parse: Some(how),
                reason: format!("JDK {jdk}: Flight Recorder available, recordings are read with {via}."),
                remedy: None,
                jdk,
            }
        }
        None => no(
            &format!("JDK {jdk} can record, but nothing is available to read the recording: it is a JRE without javac."),
            "Select a full JDK (it must contain javac), or install any JDK 12+ whose jfr tool can read the file.",
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn facts(major: u32, update: Option<u32>) -> JdkFacts {
        JdkFacts { major, update, vendor: "Eclipse Adoptium".into(), has_javac: true, has_jcmd: true, ..Default::default() }
    }
    fn run(f: &JdkFacts, alt: Option<&Path>) -> JfrAvailability {
        let own = Path::new("/jdk/bin/jfr");
        classify(f, Path::new("/jdk/bin/java"), Some(Path::new("/jdk/bin/javac")), Some(own), alt)
    }

    #[test]
    fn versions_parse() {
        assert_eq!(parse_java_version("1.8.0_262"), Some((8, Some(262))));
        assert_eq!(parse_java_version("\"11.0.2\""), Some((11, None)));
        assert_eq!(parse_java_version("21-ea"), Some((21, None)));
        assert_eq!(parse_java_version("17"), Some((17, None)));
        assert_eq!(version_from_banner("openjdk version \"17.0.2\" 2022-01-18"), Some((17, None)));
    }

    #[test]
    fn release_file_is_read() {
        let r = parse_release("JAVA_VERSION=\"17.0.8\"\nIMPLEMENTOR=\"IBM Corporation\"\nJVM_VARIANT=\"Openj9\"\nMODULES=\"java.base jdk.jfr\"\n");
        assert_eq!(r.java_version.as_deref(), Some("17.0.8"));
        assert_eq!(r.jvm_variant, "Openj9");
        assert_eq!(r.modules.unwrap(), vec!["java.base", "jdk.jfr"]);
    }

    #[test]
    fn modern_jdk_uses_its_own_jfr() {
        let mut f = facts(21, None);
        f.has_jfr_cli = true;
        let a = run(&f, None);
        assert!(a.available);
        assert!(matches!(a.how_to_parse, Some(ParseHow::JfrCli { .. })));
    }

    #[test]
    fn jdk11_without_cli_prefers_another_jdks_jfr_then_the_source_helper() {
        let f = facts(11, None);
        let with_alt = run(&f, Some(Path::new("/other/bin/jfr")));
        assert_eq!(with_alt.how_to_parse, Some(ParseHow::JfrCli { jfr: "/other/bin/jfr".into() }));
        let alone = run(&f, None);
        assert!(matches!(alone.how_to_parse, Some(ParseHow::HelperSource { .. })));
    }

    #[test]
    fn jdk8_backport_compiles_the_helper_and_old_builds_are_refused() {
        let ok = run(&facts(8, Some(332)), None);
        assert!(matches!(ok.how_to_parse, Some(ParseHow::HelperCompiled { .. })));
        let old = run(&facts(8, Some(202)), None);
        assert!(!old.available);
        assert!(old.remedy.unwrap().contains("8u262"));
        let mut oracle = facts(8, Some(202));
        oracle.vendor = "Oracle Corporation".into();
        assert!(run(&oracle, None).remedy.unwrap().contains("commercial"));
    }

    #[test]
    fn openj9_9_10_and_jlink_without_jfr_are_refused_with_a_remedy() {
        let mut j9 = facts(17, None);
        j9.is_openj9 = true;
        let a = run(&j9, None);
        assert!(!a.available && a.reason.contains("OpenJ9"));
        assert!(!run(&facts(9, None), None).available);
        assert!(!run(&facts(10, None), None).available);
        let mut jl = facts(17, None);
        jl.modules = Some(vec!["java.base".into()]);
        let a = run(&jl, None);
        assert!(!a.available && a.remedy.unwrap().contains("--add-modules jdk.jfr"));
        assert!(!run(&facts(7, None), None).available);
    }

    #[test]
    fn jre_without_javac_on_jdk8_cannot_read() {
        let mut f = facts(8, Some(300));
        f.has_javac = false;
        let a = run(&f, None);
        assert!(!a.available);
        assert!(a.reason.contains("without javac"));
    }
}
