//! The web application's `/WEB-INF/lib`, as a real folder.
//!
//! A live deployment serves `src/main/webapp` and adds the project's dependency jars to it. Mounting each
//! jar on its own (`FileResourceSet` at `/WEB-INF/lib/<name>`) loads them fine, but Tomcat cannot **list**
//! a folder that exists only as single-file mounts: `ServletContext.getResourcePaths("/WEB-INF/lib/")`
//! answers `null`. Legacy applications enumerate that folder at start-up (to discover plugin jars) and
//! dereference the answer, so they died with a `NullPointerException` that a packaged WAR never raised.
//!
//! So the jars are gathered into one folder under the Tomcat base and that folder is mounted whole
//! (`DirResourceSet`), which Tomcat can list. Hard links where the disk allows — no copy of a large
//! dependency set — and a copy where it does not.

use std::path::Path;

use bennu_tomcat::prelude::{Mount, MountKind};

use super::forward;

const WEB_LIB_DIR: &str = "web-lib";
const WEB_LIB_MOUNT: &str = "/WEB-INF/lib";

/// `mounts` with every `/WEB-INF/lib/<jar>` mount replaced by one listable folder under `base`.
/// Other mounts (the classes) pass through unchanged; with no jar mounts nothing is created.
pub(super) fn stage(base: &Path, mounts: Vec<Mount>) -> Result<Vec<Mount>, String> {
    let is_jar = |m: &Mount| m.kind == MountKind::File && m.web_app_mount.starts_with("/WEB-INF/lib/");
    if !mounts.iter().any(is_jar) {
        return Ok(mounts);
    }
    let dir = base.join(WEB_LIB_DIR);
    // Rebuilt on every launch: a dependency dropped from the project must not linger in the application.
    if dir.exists() {
        std::fs::remove_dir_all(&dir).map_err(|e| format!("Could not clear {}: {e}", dir.display()))?;
    }
    std::fs::create_dir_all(&dir).map_err(|e| format!("Could not create {}: {e}", dir.display()))?;

    let mut out = Vec::with_capacity(mounts.len());
    for mount in mounts {
        if !is_jar(&mount) {
            out.push(mount);
            continue;
        }
        let from = Path::new(&mount.base);
        let Some(name) = from.file_name() else { continue };
        let to = dir.join(name);
        // Two classpath entries with one file name: the first wins, as it does on the classpath.
        if to.exists() {
            continue;
        }
        std::fs::hard_link(from, &to)
            .or_else(|_| std::fs::copy(from, &to).map(|_| ()))
            .map_err(|e| format!("Could not stage {}: {e}", from.display()))?;
    }
    out.push(Mount { kind: MountKind::Dir, base: forward(&dir), web_app_mount: WEB_LIB_MOUNT.into(), class_loader_only: false });
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("bennu-weblib-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn jar_mount(path: &Path) -> Mount {
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        Mount { kind: MountKind::File, base: forward(path), web_app_mount: format!("/WEB-INF/lib/{name}"), class_loader_only: false }
    }

    #[test]
    fn jars_become_one_listable_folder_and_classes_pass_through() {
        let d = temp("stage");
        let (a, b) = (d.join("a.jar"), d.join("b.jar"));
        std::fs::write(&a, b"a").unwrap();
        std::fs::write(&b, b"b").unwrap();
        let classes = Mount { kind: MountKind::Dir, base: forward(&d), web_app_mount: "/WEB-INF/classes".into(), class_loader_only: false };

        let out = stage(&d.join("base"), vec![classes, jar_mount(&a), jar_mount(&b)]).unwrap();

        assert_eq!(out.len(), 2, "the classes and one folder, no per-jar mounts: {out:?}");
        assert_eq!(out[0].web_app_mount, "/WEB-INF/classes");
        assert_eq!(out[1].web_app_mount, "/WEB-INF/lib");
        assert!(out[1].kind == MountKind::Dir);
        assert!(d.join("base/web-lib/a.jar").is_file() && d.join("base/web-lib/b.jar").is_file());
    }

    #[test]
    fn a_dropped_dependency_does_not_linger_and_no_jars_creates_nothing() {
        let d = temp("rebuild");
        let (base, a) = (d.join("base"), d.join("a.jar"));
        std::fs::create_dir_all(base.join("web-lib")).unwrap();
        std::fs::write(base.join("web-lib/old.jar"), b"old").unwrap();
        std::fs::write(&a, b"a").unwrap();

        stage(&base, vec![jar_mount(&a)]).unwrap();
        assert!(!base.join("web-lib/old.jar").exists());

        let fresh = d.join("fresh");
        assert!(stage(&fresh, Vec::new()).unwrap().is_empty());
        assert!(!fresh.exists());
    }
}
