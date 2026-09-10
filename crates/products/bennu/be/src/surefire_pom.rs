//! `surefire_pom` domain — what the reactor's poms say about running tests, and the one edit that
//! changes it.
//!
//! ## Why a domain of its own
//!
//! Because the reading and the writing must not drift. The panel warns "every run here runs
//! `TestSuite`" from one rule, and the button that fixes it rewrites what another rule found; if
//! the two ever disagree the button appears and does nothing, or worse, does something to a
//! `<test>` the warning was not about. So both go through [`bennu_pomedit`], which locates the
//! element **structurally**, and this module owns only the part that is genuinely about the
//! filesystem: which poms to look in, and writing the file back.
//!
//! ## The walk
//!
//! Module poms, not an effective pom. A `<configuration>` inherited through `<pluginManagement>`
//! is the same problem for the user and lives in the text of one of these files; resolving an
//! effective pom would mean running Maven, which is seconds of waiting for a question asked
//! before every run.

use std::path::{Path, PathBuf};

use bennu_core::prelude::BennuState;
use bennu_pomedit::prelude::{
    apply, convert_pinned_suite, forkcount_zero_in, surefire_test_in, SurefireTest,
};
use serde::{Deserialize, Serialize};

/// Matches the reactor depth the rest of the classpath work walks.
const MAX_DEPTH: usize = 6;

/// Every `pom.xml` in the reactor under `root`, outermost first.
///
/// `target/`, dot-directories and `node_modules` are skipped: none of them holds a pom anybody
/// configures, and `target/` holds copies that would be found twice.
fn reactor_poms(root: &Path) -> Vec<PathBuf> {
    fn walk(dir: &Path, depth_left: usize, out: &mut Vec<PathBuf>) {
        let pom = dir.join("pom.xml");
        if pom.is_file() {
            out.push(pom);
        }
        if depth_left == 0 {
            return;
        }
        let Ok(entries) = std::fs::read_dir(dir) else { return };
        for entry in entries.flatten() {
            let Ok(ft) = entry.file_type() else { continue };
            if !ft.is_dir() {
                continue;
            }
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if name == "target" || name.starts_with('.') || name == "node_modules" {
                continue;
            }
            walk(&entry.path(), depth_left - 1, out);
        }
    }
    let mut out = Vec::new();
    walk(root, MAX_DEPTH, &mut out);
    out
}

/// What the poms under `root` write for Surefire's `<test>` — the first one that writes anything.
pub(crate) fn surefire_pinned_test(root: &Path) -> Option<SurefireTest> {
    reactor_poms(root)
        .iter()
        .filter_map(|pom| std::fs::read_to_string(pom).ok())
        .find_map(|xml| surefire_test_in(&xml))
}

/// Whether any pom under `root` pins Surefire's `<forkCount>` to zero — tests in Maven's own JVM,
/// so there is no fork to put a debug agent on.
pub(crate) fn forkcount_zero(root: &Path) -> bool {
    reactor_poms(root)
        .iter()
        .filter_map(|pom| std::fs::read_to_string(pom).ok())
        .any(|xml| forkcount_zero_in(&xml))
}

/// Args for the two handlers here.
#[derive(Deserialize)]
pub struct RootArgs {
    /// Absolute path to the project root.
    pub root: String,
}

/// The literal `<test>` this project's pom pins on the Surefire plugin, when it pins one — the
/// answer to "will running one test actually run one test here".
///
/// `None` for every project that does not, which is nearly all of them, and also for the property
/// spellings: those are steerable, and a warning about them would be a warning about the case that
/// works. Read before anything is run, because the whole point is to say it before the click
/// rather than after the wrong run.
#[arbor_rpc::handler]
fn bennu_test_selection_pinned(
    _ctx: &BennuState,
    args: RootArgs,
) -> Result<Option<String>, String> {
    Ok(match surefire_pinned_test(Path::new(&args.root)) {
        Some(SurefireTest::Literal(pinned)) => Some(pinned),
        _ => None,
    })
}

/// One pom the conversion would rewrite, described before anything is written.
#[derive(Debug, Clone, Serialize)]
pub struct SuiteConversionFile {
    /// Absolute path of the pom, forward-slashed.
    pub pom: String,
    /// The literal that is pinned today, and becomes the property's default.
    pub suite: String,
    /// The property that will steer the selection from now on.
    pub property: String,
    /// The pom's text after the rewrite — what the confirmation shows, so nobody is asked to
    /// approve an edit to a build file sight unseen.
    pub preview: String,
}

/// What the conversion comes to across the reactor.
#[derive(Debug, Clone, Default, Serialize)]
pub struct SuiteConversionPlan {
    pub files: Vec<SuiteConversionFile>,
}

/// Plan the conversion: every pom under `root` whose Surefire `<test>` is pinned to a literal,
/// with the text it would have afterwards.
///
/// Writes nothing. Split from [`bennu_apply_suite_property`] because a change to a build file is
/// one the user has to be able to look at first — the whole point of this being a button rather
/// than something the editor does on your behalf.
#[arbor_rpc::handler]
fn bennu_plan_suite_property(
    _ctx: &BennuState,
    args: RootArgs,
) -> Result<SuiteConversionPlan, String> {
    Ok(SuiteConversionPlan { files: plan_for(Path::new(&args.root)) })
}

fn plan_for(root: &Path) -> Vec<SuiteConversionFile> {
    reactor_poms(root)
        .into_iter()
        .filter_map(|pom| {
            let xml = std::fs::read_to_string(&pom).ok()?;
            let plan = convert_pinned_suite(&xml)?;
            Some(SuiteConversionFile {
                pom: pom.to_string_lossy().replace('\\', "/"),
                suite: plan.suite,
                property: plan.property,
                preview: apply(&xml, &plan.edits),
            })
        })
        .collect()
}

/// What was written.
#[derive(Debug, Clone, Default, Serialize)]
pub struct SuiteConversionResult {
    /// The poms actually rewritten, forward-slashed.
    pub written: Vec<String>,
}

/// Apply the conversion planned by [`bennu_plan_suite_property`].
///
/// Re-plans rather than taking the preview back from the caller: the file may have been edited
/// between the two calls, and writing text computed from a version that is no longer there is how
/// an editor silently reverts somebody's work. If the pin is gone by then, so is the edit — the
/// result simply lists nothing.
#[arbor_rpc::handler]
fn bennu_apply_suite_property(
    _ctx: &BennuState,
    args: RootArgs,
) -> Result<SuiteConversionResult, String> {
    let mut written = Vec::new();
    for file in plan_for(Path::new(&args.root)) {
        std::fs::write(&file.pom, &file.preview)
            .map_err(|e| format!("could not write {}: {e}", file.pom))?;
        written.push(file.pom);
    }
    Ok(SuiteConversionResult { written })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    const PINNED: &str = r#"<project>
  <modelVersion>4.0.0</modelVersion>
  <artifactId>app</artifactId>
  <build>
    <plugins>
      <plugin>
        <artifactId>maven-surefire-plugin</artifactId>
        <configuration><test>TestSuite</test></configuration>
      </plugin>
    </plugins>
  </build>
</project>"#;

    /// A directory nothing else is using. Named by a counter rather than by the clock: two tests
    /// running in parallel on a coarse macOS clock got the same nanosecond, and one overwrote the
    /// other's pom.
    fn temp_root(label: &str) -> PathBuf {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("bennu-surefire-{label}-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn a_pinned_pom_is_found_and_converted() {
        let root = temp_root("convert");
        std::fs::write(root.join("pom.xml"), PINNED).unwrap();

        assert_eq!(surefire_pinned_test(&root), Some(SurefireTest::Literal("TestSuite".into())));

        let plan = plan_for(&root);
        assert_eq!(plan.len(), 1);
        assert_eq!(plan[0].property, "test");
        assert_eq!(plan[0].suite, "TestSuite");
        assert!(plan[0].preview.contains("<test>${test}</test>"));

        std::fs::write(&plan[0].pom, &plan[0].preview).unwrap();
        // The project is steerable now, and the pin is gone — so the warning that sent the user
        // here stops being shown, which is the only visible proof the button worked.
        assert_eq!(surefire_pinned_test(&root), Some(SurefireTest::Property("test".into())));
        assert!(plan_for(&root).is_empty());

        let _ = std::fs::remove_dir_all(&root);
    }

    /// The pin is regularly in the parent of the module that has the tests, which is why the walk
    /// reads every pom rather than the root's.
    #[test]
    fn a_pin_in_a_nested_module_is_found() {
        let root = temp_root("nested");
        std::fs::write(root.join("pom.xml"), "<project><artifactId>parent</artifactId></project>")
            .unwrap();
        let child = root.join("core");
        std::fs::create_dir_all(&child).unwrap();
        std::fs::write(child.join("pom.xml"), PINNED).unwrap();

        let plan = plan_for(&root);
        assert_eq!(plan.len(), 1);
        assert!(plan[0].pom.ends_with("core/pom.xml"));

        let _ = std::fs::remove_dir_all(&root);
    }

    /// `target/` holds a copy of the pom, and converting it would be converting nothing while
    /// reporting a file changed.
    #[test]
    fn a_copy_under_target_is_not_a_pom_anybody_configures() {
        let root = temp_root("target");
        std::fs::write(root.join("pom.xml"), "<project><artifactId>a</artifactId></project>")
            .unwrap();
        let built = root.join("target").join("classes");
        std::fs::create_dir_all(&built).unwrap();
        std::fs::write(built.join("pom.xml"), PINNED).unwrap();

        assert!(plan_for(&root).is_empty());
        assert_eq!(surefire_pinned_test(&root), None);

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_project_that_pins_nothing_plans_nothing() {
        let root = temp_root("clean");
        std::fs::write(root.join("pom.xml"), "<project><artifactId>a</artifactId></project>")
            .unwrap();
        assert!(plan_for(&root).is_empty());
        assert!(!forkcount_zero(&root));
        let _ = std::fs::remove_dir_all(&root);
    }
}
