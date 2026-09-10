//! `new_module` domain — adding a Maven module to a reactor.
//!
//! ## What makes this more than `mkdir`
//!
//! A module is not a directory; it is a directory **that its parent knows about**. Creating the
//! folder and the pom and stopping there produces a module that is invisible to `mvn`, builds
//! nothing, and appears in no IDE — and it looks completely correct on disk, which is why the
//! mistake survives so long. So the parent's `<modules>` is written in the same operation, and the
//! parent's packaging with it: a pom that lists modules has to say `pom`, and one that does not is
//! a build that fails at the first `mvn install` with a message about packaging.
//!
//! ## Inheritance is the default, not an option
//!
//! The coordinates a module could repeat, it does not. A module that writes its own `<version>` is
//! one that will drift from its parent at the next release — and the reason so many legacy reactors
//! need the version bumped in eleven files is that eleven modules each wrote one down. So a
//! `<groupId>` or a `<version>` is written **only** when it genuinely differs from what the parent
//! gives, and the caller is shown what will be inherited before it decides.
//!
//! ## What it refuses
//!
//! - A directory that already exists with something in it. Scaffolding over somebody's work is not
//!   recoverable from a dialog.
//! - A parent that has **sources of its own**. Making it an aggregator means `packaging=pom`, and a
//!   `pom` module builds no jar — so the module that was there would silently stop producing one.
//!   That is a decision for a person, not a side effect of adding a module beside it.

use std::path::{Path, PathBuf};

use bennu_core::prelude::BennuState;
use bennu_deps::prelude::parse_pom;
use bennu_maven::prelude::reactor;
use bennu_pomedit::prelude::{
    add_module, apply, module_pom, packaging_of, set_packaging, source_dirs, valid_artifact_id,
    ModuleSpec, ParentCoords,
};
use serde::{Deserialize, Serialize};

/// Args naming a project root.
#[derive(Deserialize)]
pub struct RootArgs {
    /// Absolute path to the project root.
    pub root: String,
}

/// One pom a new module could be added under.
#[derive(Debug, Clone, Serialize)]
pub struct ModuleParent {
    /// Absolute path of the pom, forward-slashed.
    pub pom: String,
    /// Absolute path of its directory, forward-slashed — where the new module's folder goes.
    pub dir: String,
    /// Project-relative directory, empty for the root. What the picker's rows say.
    pub relative: String,
    /// The coordinates a child would inherit.
    pub group_id: String,
    pub artifact_id: String,
    pub version: String,
    /// `jar` · `war` · `pom`.
    pub packaging: String,
    /// Already an aggregator — adding a module changes nothing about it.
    pub aggregator: bool,
    /// Set when this pom **cannot** become one without silently disabling what it builds: it has
    /// sources of its own. The dialog offers it greyed, with this as the reason.
    pub blocked: Option<String>,
}

/// What the New Module dialog needs to be smart: every pom in the reactor, with what a child of it
/// would inherit.
#[derive(Debug, Clone, Default, Serialize)]
pub struct NewModuleContext {
    /// Outermost first — the root's own pom is the first row and the usual answer.
    pub parents: Vec<ModuleParent>,
}

/// The poms a new module could be added under, and what each one gives a child.
///
/// Follows the reactor's `<modules>` declarations rather than walking the tree: a `samples/`
/// directory with a pom of its own is not part of this build, and offering it as a parent would
/// produce a module nothing ever compiles.
#[arbor_rpc::handler]
fn bennu_new_module_context(
    _ctx: &BennuState,
    args: RootArgs,
) -> Result<NewModuleContext, String> {
    Ok(module_context(Path::new(&args.root)))
}

/// The pure half — what the handler answers, without a state nobody reads.
fn module_context(root: &Path) -> NewModuleContext {
    let parents = reactor(root)
        .into_iter()
        .map(|(dir, pom)| {
            let packaging = if pom.packaging.is_empty() { "jar".into() } else { pom.packaging.clone() };
            let aggregator = packaging == "pom";
            ModuleParent {
                pom: slash(&dir.join("pom.xml")),
                dir: slash(&dir),
                relative: relative_of(root, &dir),
                group_id: pom.effective_group().to_string(),
                artifact_id: pom.artifact_id.clone(),
                version: pom.effective_version().to_string(),
                packaging,
                aggregator,
                blocked: (!aggregator && has_sources(&dir)).then(|| {
                    format!(
                        "{} builds its own sources. Adding a module here would make it \
                         <packaging>pom</packaging>, and a pom module produces no jar.",
                        pom.display_name()
                    )
                }),
            }
        })
        .collect();
    NewModuleContext { parents }
}

/// Args for [`bennu_new_module`].
#[derive(Deserialize)]
pub struct NewModuleArgs {
    /// Absolute path to the project root.
    pub root: String,
    /// Absolute path of the parent pom the module is added under.
    pub parent_pom: String,
    /// The module's artifactId, and — unless `dir_name` says otherwise — its directory name.
    pub artifact_id: String,
    /// The directory to create, when it should differ from the artifactId. One path segment.
    #[serde(default)]
    pub dir_name: String,
    /// Written only when it differs from what the parent gives; empty means inherit.
    #[serde(default)]
    pub group_id: String,
    /// Likewise.
    #[serde(default)]
    pub version: String,
    /// `jar` · `war` · `pom`.
    #[serde(default)]
    pub packaging: String,
    /// `<name>`, omitted when empty.
    #[serde(default)]
    pub name: String,
}

/// What was created.
#[derive(Debug, Clone, Default, Serialize)]
pub struct NewModuleResult {
    /// The module's directory, forward-slashed.
    pub module_dir: String,
    /// Its pom, forward-slashed — what the caller opens.
    pub pom: String,
    /// Every directory created, so the tree can be told what appeared.
    pub created: Vec<String>,
    /// Whether the parent's packaging had to be changed to `pom`.
    pub parent_became_aggregator: bool,
}

/// Create the module: its directory, its pom, its source folders, and its entry in the parent.
///
/// Every refusal happens **before** anything is written. A half-created module — a folder with no
/// pom, or a pom the parent does not list — is worse than no module, because it looks like one.
#[arbor_rpc::handler]
fn bennu_new_module(_ctx: &BennuState, args: NewModuleArgs) -> Result<NewModuleResult, String> {
    create_module(&args)
}

/// The pure half — every refusal and every write, with nothing from the session in it.
fn create_module(args: &NewModuleArgs) -> Result<NewModuleResult, String> {
    let artifact_id = args.artifact_id.trim();
    if !valid_artifact_id(artifact_id) {
        return Err(format!(
            "`{artifact_id}` is not usable as an artifactId and a folder name — letters, digits, \
             `-`, `_` and `.`, not starting with `.` or `-`"
        ));
    }
    let dir_name = if args.dir_name.trim().is_empty() { artifact_id } else { args.dir_name.trim() };
    if !valid_artifact_id(dir_name) {
        return Err(format!("`{dir_name}` is not usable as a folder name"));
    }

    let parent_pom = PathBuf::from(&args.parent_pom);
    let parent_dir = parent_pom
        .parent()
        .ok_or_else(|| format!("{} has no directory", args.parent_pom))?
        .to_path_buf();
    let parent_xml = std::fs::read_to_string(&parent_pom)
        .map_err(|e| format!("could not read {}: {e}", args.parent_pom))?;
    let parent = parse_pom(&parent_xml);

    let parent_packaging = packaging_of(&parent_xml);
    if parent_packaging != "pom" && has_sources(&parent_dir) {
        return Err(format!(
            "{} builds its own sources, so it cannot also aggregate modules: listing one requires \
             <packaging>pom</packaging>, and a pom module produces no jar. Add the module under \
             the parent instead.",
            parent.display_name()
        ));
    }

    let module_dir = parent_dir.join(dir_name);
    if module_dir.exists() && std::fs::read_dir(&module_dir).map(|d| d.count()).unwrap_or(1) > 0 {
        return Err(format!("{} already exists and is not empty", slash(&module_dir)));
    }

    let packaging =
        if args.packaging.trim().is_empty() { "jar".to_string() } else { args.packaging.trim().to_string() };
    let spec = ModuleSpec {
        artifact_id: artifact_id.to_string(),
        // Written only when it really differs — the whole point of a parent.
        group_id: differing(&args.group_id, parent.effective_group()),
        version: differing(&args.version, parent.effective_version()),
        packaging: packaging.clone(),
        name: args.name.trim().to_string(),
        parent: Some(ParentCoords {
            group_id: parent.effective_group().to_string(),
            artifact_id: parent.artifact_id.clone(),
            version: parent.effective_version().to_string(),
            // The module sits directly under its parent, which is Maven's default — so nothing.
            relative_path: String::new(),
        }),
    };

    // ── nothing above this line has written anything ─────────────────────────

    let mut created = Vec::new();
    std::fs::create_dir_all(&module_dir)
        .map_err(|e| format!("could not create {}: {e}", slash(&module_dir)))?;
    created.push(slash(&module_dir));
    for dir in source_dirs(&packaging) {
        let path = module_dir.join(dir);
        std::fs::create_dir_all(&path)
            .map_err(|e| format!("could not create {}: {e}", slash(&path)))?;
        created.push(slash(&path));
    }

    let pom_path = module_dir.join("pom.xml");
    std::fs::write(&pom_path, module_pom(&spec))
        .map_err(|e| format!("could not write {}: {e}", slash(&pom_path)))?;
    created.push(slash(&pom_path));

    // The parent last: until it lists the module, nothing is committed to that has to be undone.
    let mut edits = Vec::new();
    if let Some(edit) = add_module(&parent_xml, dir_name) {
        edits.push(edit);
    }
    let became_aggregator = parent_packaging != "pom";
    if became_aggregator {
        if let Some(edit) = set_packaging(&parent_xml, "pom") {
            edits.push(edit);
        }
    }
    if !edits.is_empty() {
        std::fs::write(&parent_pom, apply(&parent_xml, &edits))
            .map_err(|e| format!("could not write {}: {e}", args.parent_pom))?;
    }

    Ok(NewModuleResult {
        module_dir: slash(&module_dir),
        pom: slash(&pom_path),
        created,
        parent_became_aggregator: became_aggregator,
    })
}

/// `value` when it says something the parent does not already say, else empty (inherit).
fn differing(value: &str, inherited: &str) -> String {
    let value = value.trim();
    if value.is_empty() || value == inherited {
        String::new()
    } else {
        value.to_string()
    }
}

/// Whether this directory builds sources of its own — the test for "can it become an aggregator".
fn has_sources(dir: &Path) -> bool {
    dir.join("src/main/java").is_dir() || dir.join("src/test/java").is_dir()
}

fn slash(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn relative_of(root: &Path, dir: &Path) -> String {
    dir.strip_prefix(root).map(|p| slash(p)).unwrap_or_else(|_| slash(dir))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    const AGGREGATOR: &str = r#"<project>
  <modelVersion>4.0.0</modelVersion>
  <groupId>com.acme</groupId>
  <artifactId>portale</artifactId>
  <version>2.4.0</version>
  <packaging>pom</packaging>
  <modules>
    <module>core</module>
  </modules>
</project>"#;

    fn temp_root(label: &str) -> PathBuf {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        let dir =
            std::env::temp_dir().join(format!("bennu-newmod-{label}-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn args(root: &Path, artifact: &str) -> NewModuleArgs {
        NewModuleArgs {
            root: slash(root),
            parent_pom: slash(&root.join("pom.xml")),
            artifact_id: artifact.into(),
            dir_name: String::new(),
            group_id: String::new(),
            version: String::new(),
            packaging: "jar".into(),
            name: String::new(),
        }
    }

    #[test]
    fn a_module_is_created_and_its_parent_told_about_it() {
        let root = temp_root("create");
        std::fs::write(root.join("pom.xml"), AGGREGATOR).unwrap();
        std::fs::create_dir_all(root.join("core")).unwrap();

        let out = create_module(&args(&root, "portale-api")).unwrap();

        assert!(root.join("portale-api/pom.xml").is_file());
        assert!(root.join("portale-api/src/main/java").is_dir());
        assert!(root.join("portale-api/src/test/resources").is_dir());
        assert!(!out.parent_became_aggregator);

        // The parent lists it — without which the module builds for nobody.
        let parent = std::fs::read_to_string(root.join("pom.xml")).unwrap();
        assert!(parent.contains("<module>portale-api</module>"));
        assert!(parent.contains("<module>core</module>"), "and keeps the ones it had");

        // And the module inherits rather than repeating.
        let pom = std::fs::read_to_string(root.join("portale-api/pom.xml")).unwrap();
        assert!(pom.contains("<artifactId>portale-api</artifactId>"));
        assert!(pom.contains("<artifactId>portale</artifactId>"), "the parent");
        assert_eq!(pom.matches("<version>").count(), 1, "only the parent's");

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_coordinate_that_differs_is_written_and_one_that_does_not_is_not() {
        let root = temp_root("coords");
        std::fs::write(root.join("pom.xml"), AGGREGATOR).unwrap();

        let mut a = args(&root, "tools");
        a.group_id = "com.acme.tools".into();
        // The same version the parent gives: saying it again is what makes a reactor drift.
        a.version = "2.4.0".into();
        create_module(&a).unwrap();

        let pom = std::fs::read_to_string(root.join("tools/pom.xml")).unwrap();
        assert!(pom.contains("<groupId>com.acme.tools</groupId>"));
        assert_eq!(pom.matches("<version>").count(), 1, "the parent's, not a repeat");

        let _ = std::fs::remove_dir_all(&root);
    }

    /// A plain jar project gets its first module: it has to become an aggregator, and it is told.
    #[test]
    fn a_plain_pom_becomes_an_aggregator_and_says_so() {
        let root = temp_root("aggregate");
        std::fs::write(
            root.join("pom.xml"),
            "<project>\n  <groupId>com.acme</groupId>\n  <artifactId>app</artifactId>\n  <version>1.0</version>\n</project>",
        )
        .unwrap();

        let out = create_module(&args(&root, "core")).unwrap();
        assert!(out.parent_became_aggregator);

        let parent = std::fs::read_to_string(root.join("pom.xml")).unwrap();
        assert!(parent.contains("<packaging>pom</packaging>"));
        assert!(parent.contains("<module>core</module>"));

        let _ = std::fs::remove_dir_all(&root);
    }

    /// The refusal that matters: turning a module that builds a jar into an aggregator would stop
    /// it building one, silently.
    #[test]
    fn a_parent_with_sources_of_its_own_is_refused() {
        let root = temp_root("sources");
        std::fs::write(
            root.join("pom.xml"),
            "<project><groupId>com.acme</groupId><artifactId>app</artifactId><version>1.0</version></project>",
        )
        .unwrap();
        std::fs::create_dir_all(root.join("src/main/java")).unwrap();

        let err = create_module(&args(&root, "core")).unwrap_err();
        assert!(err.contains("cannot also aggregate"), "{err}");
        assert!(!root.join("core").exists(), "and nothing was written");

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn an_occupied_directory_is_refused_before_anything_is_written() {
        let root = temp_root("occupied");
        std::fs::write(root.join("pom.xml"), AGGREGATOR).unwrap();
        std::fs::create_dir_all(root.join("core/src")).unwrap();

        let err = create_module(&args(&root, "core")).unwrap_err();
        assert!(err.contains("already exists"), "{err}");
        assert!(!root.join("core/pom.xml").exists());

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_name_that_is_not_a_folder_name_is_refused() {
        let root = temp_root("badname");
        std::fs::write(root.join("pom.xml"), AGGREGATOR).unwrap();
        let mut a = args(&root, "not a name");
        a.artifact_id = "not a name".into();
        assert!(create_module(&a).is_err());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn the_parents_offered_are_the_reactors_own() {
        let root = temp_root("context");
        std::fs::write(root.join("pom.xml"), AGGREGATOR).unwrap();
        let core = root.join("core");
        std::fs::create_dir_all(&core).unwrap();
        std::fs::write(
            core.join("pom.xml"),
            "<project><parent><groupId>com.acme</groupId><artifactId>portale</artifactId><version>2.4.0</version></parent><artifactId>core</artifactId></project>",
        )
        .unwrap();
        // Not listed in <modules>, so not part of this build — and not a parent to offer.
        let samples = root.join("samples");
        std::fs::create_dir_all(&samples).unwrap();
        std::fs::write(samples.join("pom.xml"), AGGREGATOR).unwrap();

        let ctx = module_context(&root);
        let names: Vec<&str> = ctx.parents.iter().map(|p| p.artifact_id.as_str()).collect();
        assert_eq!(names, ["portale", "core"]);
        assert_eq!(ctx.parents[0].version, "2.4.0");
        // `core` inherits its groupId and version, and the picker shows what a child would get.
        assert_eq!(ctx.parents[1].group_id, "com.acme");
        assert_eq!(ctx.parents[1].version, "2.4.0");

        let _ = std::fs::remove_dir_all(&root);
    }
}
