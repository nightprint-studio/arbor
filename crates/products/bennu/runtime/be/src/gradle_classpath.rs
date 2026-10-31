//! The dependency tier for a Gradle project — which jars the symbol index loads.
//!
//! The Maven resolver reads the poms and `~/.m2` directly and asks Maven only when that comes up
//! short. Gradle cannot be read that way: the *transitive* closure of a Gradle build depends on its
//! resolution rules (capabilities, platforms, substitutions, constraints), and the cache layout
//! names a coordinate without letting you walk from one artifact to what it needs. So the order is
//! reversed:
//!
//! 1. **Gradle itself**, asked through a tiny init script that prints every source set's
//!    compile and runtime classpath. Seconds on a warm daemon, and exactly what the compiler would
//!    be handed.
//! 2. **The Gradle cache, read directly**, when Gradle cannot be run (no JDK, no wrapper, offline
//!    distribution) — only the dependencies the build files *declare*, which is a partial answer and
//!    is reported as one rather than passed off as the classpath.
//!
//! The result is cached by [`crate::dep_classpath`] exactly as the Maven one is, keyed on the build
//! scripts' timestamps.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use arbor_process_ext::prelude::NoWindowExt;

use crate::dep_classpath::ResolvedList;

/// A cold Gradle (distribution download, daemon start, dependency download) can take minutes; past
/// this it is hung, not slow.
const TIMEOUT: Duration = Duration::from_secs(600);

/// Marks a classpath line in Gradle's output, so a plugin's own `println` cannot be mistaken for one.
const MARK: &str = "BENNU-CP|";

/// The init script. Registers `bennuPrintClasspath` in every project and prints one line per file:
/// `BENNU-CP|<project path>|<source set>|<absolute path>`.
///
/// Registered in `afterEvaluate` because the `java` plugin may be applied by the project's own script
/// after the init script ran; a project without source sets (a pure aggregator) prints nothing.
const INIT_SCRIPT: &str = r#"allprojects {
    afterEvaluate { p ->
        p.tasks.register('bennuPrintClasspath') {
            doLast {
                def sets = null
                def javaExt = p.extensions.findByName('java')
                if (javaExt != null && javaExt.hasProperty('sourceSets')) { sets = javaExt.sourceSets }
                if (sets == null) { sets = p.extensions.findByName('sourceSets') }
                if (sets == null) { return }
                sets.each { ss ->
                    def files = [] as LinkedHashSet
                    try { files.addAll(ss.compileClasspath.files) } catch (Throwable t) { }
                    try { files.addAll(ss.runtimeClasspath.files) } catch (Throwable t) { }
                    files.each { f -> println "BENNU-CP|" + p.path + "|" + ss.name + "|" + f.absolutePath }
                }
            }
        }
    }
}
"#;

/// Where the init script lives. Rewritten when its text differs, so an upgrade of Bennu that changes
/// the script reaches everyone without a manual cleanup.
fn init_script_path() -> Result<PathBuf, String> {
    let dir = arbor_core::prelude::bennu_data_dir().join("gradle");
    std::fs::create_dir_all(&dir).map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
    let path = dir.join("bennu-classpath.init.gradle");
    let current = std::fs::read_to_string(&path).unwrap_or_default();
    if current != INIT_SCRIPT {
        std::fs::write(&path, INIT_SCRIPT).map_err(|e| format!("cannot write {}: {e}", path.display()))?;
    }
    Ok(path)
}

/// Resolve from scratch: Gradle first, the cache second. See the module doc.
pub(crate) fn resolve_fresh(root: &Path) -> Result<ResolvedList, String> {
    match run_classpath(root, false) {
        Ok(jars) if !jars.is_empty() => {
            eprintln!(
                "bennu-be: Gradle classpath resolved for {} ({} jars)",
                root.display(),
                jars.len()
            );
            Ok(ResolvedList { jars, source: "gradle", cacheable: true, ..ResolvedList::default() })
        }
        Ok(_) => declared_only(root, "Gradle resolved no dependency jars".to_string()),
        Err(reason) => declared_only(root, reason),
    }
}

/// The fallback: the jars the build files declare, found in the Gradle cache.
///
/// Never cacheable. A failure is not a result — it is re-tried on the next open, which costs one
/// spawn on a machine without Gradle and heals a transient failure for free.
fn declared_only(root: &Path, reason: String) -> Result<ResolvedList, String> {
    let project = bennu_gradle::prelude::read(root);
    let found = bennu_gradle::prelude::resolve_declared(&project);
    if found.jars.is_empty() {
        return Err(format!("{reason}. No declared dependency was found in the Gradle cache either."));
    }
    let jars: Vec<String> = found.jars.iter().map(|p| p.display().to_string()).collect();
    let missing = found.missing.len();
    let partial = Some(format!(
        "Gradle could not be run ({reason}), so the classpath is only the {} dependencies the build files declare, \
         read from the Gradle cache{}. Transitive dependencies are missing: types from them will read as \
         unresolved until Gradle resolves the build.",
        jars.len(),
        if missing > 0 { format!(" ({missing} declared ones are not downloaded yet)") } else { String::new() }
    ));
    Ok(ResolvedList { jars, partial, source: "gradle-cache", cacheable: false, ..ResolvedList::default() })
}

/// Ask Gradle for the classpaths of every project. `refresh`: `--refresh-dependencies`, the
/// download button's flag.
///
/// Err carries Gradle's own reason, because "0 jars" has to be diagnosable from the UI.
pub(crate) fn run_classpath(root: &Path, refresh: bool) -> Result<Vec<String>, String> {
    let init = init_script_path()?;
    let launcher = crate::gradle::find_gradle_launcher(root);

    let mut cmd = Command::new(&launcher);
    cmd.current_dir(root)
        .arg("--console=plain")
        .arg("-q")
        .arg("--init-script")
        .arg(&init)
        // A project with the configuration cache switched on rejects the task's `project` access.
        .arg("-Dorg.gradle.configuration-cache=false");
    if refresh {
        cmd.arg("--refresh-dependencies");
    } else if !crate::dep_classpath::auto_download() {
        cmd.arg("--offline");
    }
    cmd.arg("bennuPrintClasspath");
    cmd.no_window();

    let output = bennu_classpath::prelude::run_bounded(cmd, TIMEOUT)
        .map_err(|e| format!("spawn gradle ({launcher}): {e}"))?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let jars = parse_classpath_output(&stdout, root);
    if jars.is_empty() && !output.status.success() {
        return Err(failure_reason(&String::from_utf8_lossy(&output.stderr)));
    }
    Ok(jars)
}

/// The jars out of the init script's output, in first-seen order and without duplicates.
///
/// Only files that exist and end in `.jar`: class directories belong to projects whose sources the
/// index reads anyway, and a sibling project's own `build/libs/*.jar` would present the same types
/// twice.
fn parse_classpath_output(stdout: &str, root: &Path) -> Vec<String> {
    let root_prefix = root.to_string_lossy().replace('\\', "/");
    let mut out: Vec<String> = Vec::new();
    for line in stdout.lines() {
        let Some(rest) = line.trim().strip_prefix(MARK) else { continue };
        let Some(path) = rest.splitn(3, '|').nth(2) else { continue };
        if !path.ends_with(".jar") {
            continue;
        }
        let normal = path.replace('\\', "/");
        if normal.starts_with(&root_prefix) && normal.contains("/build/") {
            continue;
        }
        if Path::new(path).is_file() && !out.iter().any(|p| p == path) {
            out.push(path.to_string());
        }
    }
    out
}

/// Gradle's own words for what went wrong — the lines after `* What went wrong:` — else its tail.
pub(crate) fn failure_reason(stderr: &str) -> String {
    let lines: Vec<&str> = stderr.lines().collect();
    if let Some(at) = lines.iter().position(|l| l.contains("What went wrong")) {
        let reason: Vec<&str> =
            lines[at + 1..].iter().map(|l| l.trim()).take_while(|l| !l.starts_with('*')).filter(|l| !l.is_empty()).take(3).collect();
        if !reason.is_empty() {
            return reason.join(" ");
        }
    }
    let tail: Vec<&str> = lines.iter().map(|l| l.trim()).filter(|l| !l.is_empty()).rev().take(3).collect();
    if tail.is_empty() {
        "Gradle failed without saying why. Run the build in a terminal to see".to_string()
    } else {
        tail.into_iter().rev().collect::<Vec<_>>().join(" ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_marked_existing_jars_outside_the_build_dirs_are_kept() {
        let dir = std::env::temp_dir().join(format!("bennu-gradle-cp-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("lib/build/libs")).unwrap();
        let outside = dir.join("cache.jar");
        let own = dir.join("lib/build/libs/lib.jar");
        std::fs::write(&outside, "").unwrap();
        std::fs::write(&own, "").unwrap();
        let out = format!(
            "noise\nBENNU-CP|:a|main|{0}\nBENNU-CP|:a|test|{0}\nBENNU-CP|:a|main|{1}\nBENNU-CP|:a|main|{2}/classes\nBENNU-CP|:a|main|{2}/gone.jar\n",
            outside.display(),
            own.display(),
            dir.display()
        );
        let jars = parse_classpath_output(&out, &dir);
        assert_eq!(jars, [outside.display().to_string()]);
    }

    #[test]
    fn a_failure_quotes_what_went_wrong() {
        let err = "FAILURE: Build failed\n\n* What went wrong:\nCould not resolve org.x:y:1.\n> Cannot resolve\n\n* Try:\nrun with --stacktrace\n";
        assert_eq!(failure_reason(err), "Could not resolve org.x:y:1. > Cannot resolve");
        assert!(failure_reason("").contains("terminal"));
    }
}
