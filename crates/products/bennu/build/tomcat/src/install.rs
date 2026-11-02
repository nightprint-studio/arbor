//! What an unpacked Tomcat is.
//!
//! An installation is a directory with `bin/bootstrap.jar` and `lib/catalina.jar`. Its version is
//! read from `RELEASE-NOTES` — the file every distribution ships and whose first heading names the
//! release — and, failing that, from the directory name (`apache-tomcat-9.0.98`), which is how the
//! archive unpacks and how people leave it.

use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::catalog::line_of;

/// A usable Tomcat installation and what follows from its version.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Installation {
    /// `9.0.98`.
    pub version: String,
    /// The directory (`CATALINA_HOME`), with forward slashes.
    pub home: String,
    /// The release line, `9.0`.
    pub line: String,
    /// `"javax"` or `"jakarta"`.
    pub namespace: String,
    /// The oldest Java the line runs on.
    pub min_java: u32,
    /// The Servlet specification it implements.
    pub servlet: String,
}

/// The version named by a `RELEASE-NOTES` file: `Apache Tomcat Version 9.0.98`.
pub fn parse_release_notes(text: &str) -> Option<String> {
    let at = text.find("Apache Tomcat Version ")? + "Apache Tomcat Version ".len();
    let version: String = text[at..].chars().take_while(|c| c.is_ascii_digit() || *c == '.').collect();
    let version = version.trim_end_matches('.').to_string();
    (version.split('.').count() >= 3).then_some(version)
}

fn version_from_dir_name(home: &Path) -> Option<String> {
    let name = home.file_name()?.to_string_lossy().to_string();
    let v = name.strip_prefix("apache-tomcat-")?;
    (!v.is_empty() && v.chars().next()?.is_ascii_digit()).then(|| v.to_string())
}

fn forward(p: &Path) -> String {
    p.to_string_lossy().replace('\\', "/")
}

/// Read the installation at `home`, or say what is missing.
pub fn inspect(home: &Path) -> Result<Installation, String> {
    if !home.is_dir() {
        return Err(format!("{} is not a folder.", home.display()));
    }
    for needed in ["bin/bootstrap.jar", "lib/catalina.jar", "conf/web.xml"] {
        if !home.join(needed).is_file() {
            return Err(format!("{} does not look like a Tomcat installation: {needed} is missing.", home.display()));
        }
    }
    let version = std::fs::read_to_string(home.join("RELEASE-NOTES"))
        .ok()
        .and_then(|t| parse_release_notes(&t))
        .or_else(|| version_from_dir_name(home))
        .ok_or_else(|| format!("Could not tell which version of Tomcat is in {}.", home.display()))?;
    let line = line_of(&version).ok_or_else(|| {
        format!("Tomcat {version} is not a version Bennu can run (supported: 8.5, 9.0, 10.1 and 11.0).")
    })?;
    Ok(Installation {
        version,
        home: forward(home),
        line: line.line.to_string(),
        namespace: line.namespace.to_string(),
        min_java: line.min_java,
        servlet: line.servlet.to_string(),
    })
}

/// The installations one level under `dir` — where Bennu unpacks the ones it downloads. Newest
/// version first; a folder that is not a Tomcat is skipped, not reported.
pub fn list_installs(dir: &Path) -> Vec<Installation> {
    let Ok(entries) = std::fs::read_dir(dir) else { return Vec::new() };
    let mut found: Vec<Installation> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .filter_map(|p| inspect(&p).ok())
        .collect();
    let key = |v: &str| -> Vec<u32> { v.split('.').filter_map(|p| p.parse().ok()).collect() };
    found.sort_by(|a, b| key(&b.version).cmp(&key(&a.version)));
    found
}

/// Where a version unpacks inside `dir`.
pub fn home_in(dir: &Path, version: &str) -> PathBuf {
    dir.join(version)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fake_tomcat(dir: &Path, notes: Option<&str>) {
        for f in ["bin/bootstrap.jar", "lib/catalina.jar", "conf/web.xml"] {
            let p = dir.join(f);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, b"x").unwrap();
        }
        if let Some(n) = notes {
            std::fs::write(dir.join("RELEASE-NOTES"), n).unwrap();
        }
    }

    fn temp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("bennu-tomcat-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn the_release_notes_name_the_version() {
        let t = "                     Apache Tomcat Version 9.0.98\n                  Release Notes\n";
        assert_eq!(parse_release_notes(t).as_deref(), Some("9.0.98"));
        assert_eq!(parse_release_notes("nothing here"), None);
        assert_eq!(parse_release_notes("Apache Tomcat Version 9.0"), None);
    }

    #[test]
    fn an_installation_is_read_with_its_namespace() {
        let d = temp("inspect");
        let home = d.join("anything");
        fake_tomcat(&home, Some("Apache Tomcat Version 10.1.34\n"));
        let i = inspect(&home).unwrap();
        assert_eq!((i.version.as_str(), i.namespace.as_str(), i.min_java), ("10.1.34", "jakarta", 11));
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn the_folder_name_is_the_fallback_and_a_non_tomcat_is_refused() {
        let d = temp("fallback");
        let home = d.join("apache-tomcat-8.5.100");
        fake_tomcat(&home, None);
        assert_eq!(inspect(&home).unwrap().version, "8.5.100");
        let err = inspect(&d.join("missing")).unwrap_err();
        assert!(err.contains("not a folder"), "{err}");
        std::fs::create_dir_all(d.join("empty")).unwrap();
        assert!(inspect(&d.join("empty")).unwrap_err().contains("bin/bootstrap.jar"));
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn installs_are_listed_newest_first() {
        let d = temp("list");
        fake_tomcat(&d.join("a"), Some("Apache Tomcat Version 9.0.9\n"));
        fake_tomcat(&d.join("b"), Some("Apache Tomcat Version 9.0.100\n"));
        std::fs::create_dir_all(d.join("not-tomcat")).unwrap();
        let versions: Vec<String> = list_installs(&d).into_iter().map(|i| i.version).collect();
        assert_eq!(versions, ["9.0.100", "9.0.9"]);
        let _ = std::fs::remove_dir_all(&d);
    }
}
