//! How much the agent decides for itself: three modes, and the one narrow way the last of them may widen its
//! own reach.
//!
//! * **Manual** — every edit is a proposal; anything outside the granted scope is asked about.
//! * **Accept edits** — edits inside the scope are applied at once; anything outside is asked about.
//! * **Autonomous** — like accept edits, but a *file the build itself names* may be opened without asking.
//!
//! The project's **own build and test commands** (`bennu_build`, `bennu_test_run`) follow the mode too, through
//! [`must_ask_own_command`]: asked on every call in manual, once per run in accept-edits, never in autonomous.
//! They run code the project supplies, which is why they ask at all; they are the checks a working agent
//! repeats dozens of times, which is why asking each time is friction with little information in it.
//! Arbitrary commands (`run_start`, scripts, `cargo run`) are not covered and always ask.
//!
//! That last rule is the whole of the autonomy, and it is deliberately small. A build or check that the
//! assistant ran reported an error in a file; fixing the error means editing that file; asking the user to
//! open a file the compiler just pointed at is friction with no information in it. So [`decide_grant`]
//! says yes to *that file, for that run* — inside the project the user is in, never in output or
//! dependency folders, never a secret, never past a cap — and no to everything else, which then goes
//! through the ordinary prompt. What the function does not touch at all: deletions (always ask), the
//! `.git` folder, secrets, anything outside the workspace, and starting processes.
//!
//! Pure on purpose: the decision is a table of inputs to a verdict, so every row of it is a test.

use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::access::{file_name, has_component, is_secret_file, normalize, same, within, Op, Scope};

/// Files one run may open for itself. Past this the rest ask, so a build with hundreds of errors cannot
/// quietly turn into write access to the whole project.
pub const MAX_AUTO_GRANTS: usize = 25;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccessMode {
    Manual,
    AcceptEdits,
    Autonomous,
}

impl AccessMode {
    /// A mode from its saved word. `auto` — what the two-way switch called the middle one — still reads as
    /// accept-edits, so a saved choice survives; anything unknown is the safe one.
    pub fn parse(word: &str) -> Self {
        match word.trim().to_ascii_lowercase().as_str() {
            "accept_edits" | "accept-edits" | "auto" => AccessMode::AcceptEdits,
            "autonomous" => AccessMode::Autonomous,
            _ => AccessMode::Manual,
        }
    }

    /// The word that is saved and sent.
    pub fn id(self) -> &'static str {
        match self {
            AccessMode::Manual => "manual",
            AccessMode::AcceptEdits => "accept_edits",
            AccessMode::Autonomous => "autonomous",
        }
    }

    /// Edits are written as they are made rather than held for review.
    pub fn applies_edits(self) -> bool {
        self != AccessMode::Manual
    }

    /// Whether [`decide_grant`] can ever say yes.
    pub fn may_raise_access(self) -> bool {
        self == AccessMode::Autonomous
    }
}

/// The tools that run the project's own build or tests: nothing the model typed, only what the project defines.
pub const OWN_COMMANDS: [&str; 2] = ["bennu_build", "bennu_test_run"];

pub fn is_own_command(tool: &str) -> bool {
    OWN_COMMANDS.contains(&tool)
}

/// Whether a call to the project's own build or test command must be confirmed first.
///
/// `approved_this_run` is whether the user already said yes to one in this run (accept-edits remembers it
/// for the run, not for the conversation: a new message is a new run and asks again).
pub fn must_ask_own_command(mode: AccessMode, approved_this_run: bool) -> bool {
    match mode {
        AccessMode::Manual => true,
        AccessMode::AcceptEdits => !approved_this_run,
        AccessMode::Autonomous => false,
    }
}

/// The saved word for a mode, normalised: what a config file is rewritten with.
pub fn canonical_mode_word(word: &str) -> &'static str {
    AccessMode::parse(word).id()
}

/// Why a file was not opened without asking. Never shown as an error: the ordinary prompt follows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refusal {
    /// The mode does not raise its own access.
    NotAutonomous,
    /// Only reading and writing a file are ever granted; listing, creating and deleting are not.
    Operation,
    /// Not inside the project the user is in.
    OutsideProject,
    /// Inside another project of the workspace, which the conversation was not given.
    OtherProject,
    /// No build or check this run made named it.
    NotNamedByBuild,
    Secret,
    /// The repository's own `.git` files.
    RepositoryInternals,
    /// Under a folder that holds build output or fetched code (the folder's name).
    GeneratedFolder(String),
    /// A manifest or lockfile: what the project depends on is the user's to change.
    DependencyFile,
    /// The run has already opened [`MAX_AUTO_GRANTS`] files.
    CapReached,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    /// Open this file, read and write, for the rest of the run.
    Grant,
    /// Nothing to grant: the conversation's reach already covers it.
    AlreadyInScope,
    /// Go on to the ordinary prompt.
    Ask(Refusal),
}

/// Everything the decision depends on.
#[derive(Debug, Clone)]
pub struct GrantQuery<'a> {
    pub mode: AccessMode,
    /// The reach the conversation was given.
    pub scope: Scope,
    pub op: Op,
    pub path: &'a Path,
    /// The files that builds and checks of this run reported an error in.
    pub diagnostics: &'a [PathBuf],
    /// The project the user is in.
    pub project_root: &'a Path,
    /// The other projects of the workspace in play.
    pub other_projects: &'a [PathBuf],
    /// Files this run has already opened (revoked ones count: the cap is on the run's behaviour).
    pub used: usize,
}

/// Folders whose contents are produced or fetched, not written: fixing the source is the cure.
const GENERATED_FOLDERS: [&str; 14] = [
    "target", "build", "out", "dist", "node_modules", "generated", "generated-sources", "generated-test-sources",
    ".gradle", ".m2", ".cargo", "vendor", "__pycache__", ".svelte-kit",
];

/// Manifests and lockfiles of the ecosystems Bennu knows.
const DEPENDENCY_FILES: [&str; 22] = [
    "pom.xml", "build.gradle", "build.gradle.kts", "settings.gradle", "settings.gradle.kts", "gradle.properties",
    "cargo.toml", "cargo.lock", "package.json", "package-lock.json", "yarn.lock", "pnpm-lock.yaml", "go.mod",
    "go.sum", "requirements.txt", "pipfile", "pipfile.lock", "pyproject.toml", "gemfile", "gemfile.lock",
    "composer.json", "composer.lock",
];

/// Decide whether the file may be opened without asking.
pub fn decide_grant(q: &GrantQuery) -> Verdict {
    if !q.mode.may_raise_access() {
        return Verdict::Ask(Refusal::NotAutonomous);
    }
    if !matches!(q.op, Op::Read | Op::Write) {
        return Verdict::Ask(Refusal::Operation);
    }
    if !q.path.is_absolute() {
        return Verdict::Ask(Refusal::OutsideProject);
    }
    let path = normalize(q.path);
    if !within(&path, q.project_root) {
        // Another project of the workspace is reached by the scope the user chose, not by a build error.
        if q.other_projects.iter().any(|r| within(&path, r)) {
            return if q.scope >= Scope::Workspace { Verdict::AlreadyInScope } else { Verdict::Ask(Refusal::OtherProject) };
        }
        return Verdict::Ask(Refusal::OutsideProject);
    }
    if !q.diagnostics.iter().any(|d| same(d, &path)) {
        return Verdict::Ask(Refusal::NotNamedByBuild);
    }
    if is_secret_file(&path) {
        return Verdict::Ask(Refusal::Secret);
    }
    if has_component(&path, ".git") {
        return Verdict::Ask(Refusal::RepositoryInternals);
    }
    if let Some(folder) = generated_folder(&path, q.project_root) {
        return Verdict::Ask(Refusal::GeneratedFolder(folder));
    }
    if DEPENDENCY_FILES.contains(&file_name(&path).to_ascii_lowercase().as_str()) {
        return Verdict::Ask(Refusal::DependencyFile);
    }
    if q.used >= MAX_AUTO_GRANTS {
        return Verdict::Ask(Refusal::CapReached);
    }
    Verdict::Grant
}

/// The first output or dependency folder between the project root and the file. Compared below the root, so
/// a project that itself lives under a folder called `build` is not shut out of its own files.
fn generated_folder(path: &Path, project_root: &Path) -> Option<String> {
    let depth = normalize(project_root).components().count();
    let comps: Vec<_> = path.components().collect();
    // Between the root and the file name: the file itself may be called `build` without being in a build folder.
    comps
        .get(depth..comps.len().saturating_sub(1))?
        .iter()
        .filter_map(|c| c.as_os_str().to_str())
        .find(|name| GENERATED_FOLDERS.iter().any(|g| g.eq_ignore_ascii_case(name)))
        .map(str::to_string)
}

/// The sentence for the ordinary prompt when the file is one the build named: why the assistant is asking,
/// so the question is not a surprise.
pub fn build_error_ask(path: &Path, folder: &Path, scope_words: &str) -> String {
    format!(
        "The build reports a compile error in `{}`, which is outside what you allowed ({scope_words}). Open `{}` so Imber can fix it — for reading, or for reading and writing?",
        file_name(path),
        folder.display()
    )
}

/// The line the thread shows when a file was opened on its own.
pub fn grant_reason(path: &Path) -> String {
    format!("Access granted to {} because the build reports an error there", file_name(path))
}

/// The files with an error in the answer of a tool that reports problems, as absolute normalised paths.
/// `root` resolves the relative ones (Cargo names files from the project root). Warnings do not count: only
/// an error is a reason to edit a file nobody asked about. Other tools name no files.
pub fn diagnostic_files(tool: &str, result: &Value, root: &Path) -> Vec<PathBuf> {
    let at = |file: &str| -> Option<PathBuf> {
        let file = file.trim();
        if file.is_empty() {
            return None;
        }
        let p = Path::new(file);
        Some(normalize(&if p.is_absolute() { p.to_path_buf() } else { root.join(p) }))
    };
    let is_error = |p: &Value| p.get("severity").and_then(Value::as_str) == Some("error");
    let list = |key: &str| result.get(key).and_then(Value::as_array).cloned().unwrap_or_default();
    let mut out: Vec<PathBuf> = Vec::new();
    match tool {
        "bennu_build" => {
            for p in list("problems").iter().filter(|p| is_error(*p)) {
                out.extend(p.get("file").and_then(Value::as_str).and_then(at));
            }
        }
        "bennu_check_file" => {
            if list("problems").iter().any(|p| is_error(p)) {
                out.extend(result.get("file").and_then(Value::as_str).and_then(at));
            }
        }
        "bennu_problems" => {
            for f in list("files") {
                let has_error = f.get("problems").and_then(Value::as_array).is_some_and(|ps| ps.iter().any(|p| is_error(p)));
                if has_error {
                    out.extend(f.get("file").and_then(Value::as_str).and_then(at));
                }
            }
        }
        _ => {}
    }
    out
}

/// A file a run opened for itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileGrant {
    pub id: String,
    pub file: PathBuf,
    pub revoked: bool,
}

/// What a run has opened for itself, in order. Lives as long as the run.
#[derive(Debug, Default)]
pub struct RunGrants {
    items: Vec<FileGrant>,
}

impl RunGrants {
    /// How many files the run has opened, revoked ones included — what the cap counts.
    pub fn used(&self) -> usize {
        self.items.len()
    }

    /// Record a file and give back the id the thread and a revoke refer to it by.
    pub fn add(&mut self, file: &Path) -> String {
        let id = format!("g{}", self.items.len() + 1);
        self.items.push(FileGrant { id: id.clone(), file: normalize(file), revoked: false });
        id
    }

    /// Mark one revoked. `None` when there is no such grant or it was revoked already.
    pub fn revoke(&mut self, id: &str) -> Option<PathBuf> {
        let item = self.items.iter_mut().find(|g| g.id == id && !g.revoked)?;
        item.revoked = true;
        Some(item.file.clone())
    }

    /// The ids still in force.
    pub fn active_ids(&self) -> Vec<String> {
        self.items.iter().filter(|g| !g.revoked).map(|g| g.id.clone()).collect()
    }
}

/// The files of builds and checks a run has seen errors in, without repeats.
#[derive(Debug, Default)]
pub struct Diagnostics {
    files: Vec<PathBuf>,
}

impl Diagnostics {
    pub fn note(&mut self, files: impl IntoIterator<Item = PathBuf>) {
        for f in files {
            if !self.files.iter().any(|k| same(k, &f)) {
                self.files.push(f);
            }
        }
    }

    pub fn files(&self) -> &[PathBuf] {
        &self.files
    }

    pub fn names(&self, path: &Path) -> bool {
        self.files.iter().any(|k| same(k, path))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn p(s: &str) -> PathBuf {
        if cfg!(windows) { PathBuf::from(format!("C:\\{}", s.replace('/', "\\"))) } else { PathBuf::from(format!("/{s}")) }
    }

    struct Fixture {
        diagnostics: Vec<PathBuf>,
        others: Vec<PathBuf>,
        root: PathBuf,
    }

    impl Fixture {
        fn new(named: &[&str]) -> Self {
            Fixture { diagnostics: named.iter().map(|n| p(n)).collect(), others: vec![p("work/lib")], root: p("work/app") }
        }

        fn verdict(&self, mode: AccessMode, scope: Scope, op: Op, path: &str, used: usize) -> Verdict {
            let path = p(path);
            decide_grant(&GrantQuery {
                mode, scope, op, path: &path, diagnostics: &self.diagnostics, project_root: &self.root, other_projects: &self.others, used,
            })
        }

        fn autonomous(&self, path: &str) -> Verdict {
            self.verdict(AccessMode::Autonomous, Scope::Project, Op::Write, path, 0)
        }
    }

    #[test]
    fn manual_asks_before_every_build_and_test_run() {
        assert!(must_ask_own_command(AccessMode::Manual, false));
        assert!(must_ask_own_command(AccessMode::Manual, true), "a yes is not remembered in manual");
    }

    #[test]
    fn accept_edits_asks_once_per_run() {
        assert!(must_ask_own_command(AccessMode::AcceptEdits, false));
        assert!(!must_ask_own_command(AccessMode::AcceptEdits, true), "after the first yes the run is not asked again");
    }

    #[test]
    fn autonomous_never_asks_for_the_projects_own_commands() {
        assert!(!must_ask_own_command(AccessMode::Autonomous, false));
        assert!(!must_ask_own_command(AccessMode::Autonomous, true));
    }

    #[test]
    fn only_the_build_and_the_test_run_are_the_projects_own_commands() {
        assert!(is_own_command("bennu_build") && is_own_command("bennu_test_run"));
        for other in ["run_start", "bennu_cargo_run", "bennu_npm_run_script", "bennu_run", "send_request", "bennu_check_file"] {
            assert!(!is_own_command(other), "{other} runs something the model chose, or is not a command at all");
        }
    }

    #[test]
    fn a_file_the_build_named_inside_the_project_is_granted() {
        let f = Fixture::new(&["work/app/src/B.java"]);
        assert_eq!(f.autonomous("work/app/src/B.java"), Verdict::Grant);
        assert_eq!(f.verdict(AccessMode::Autonomous, Scope::File, Op::Read, "work/app/src/B.java", 0), Verdict::Grant, "even under the narrowest scope");
    }

    #[test]
    fn only_autonomous_ever_raises_its_own_access() {
        let f = Fixture::new(&["work/app/src/B.java"]);
        for mode in [AccessMode::Manual, AccessMode::AcceptEdits] {
            assert_eq!(f.verdict(mode, Scope::Project, Op::Write, "work/app/src/B.java", 0), Verdict::Ask(Refusal::NotAutonomous));
        }
    }

    #[test]
    fn a_file_nobody_reported_is_not_granted() {
        let f = Fixture::new(&["work/app/src/B.java"]);
        assert_eq!(f.autonomous("work/app/src/C.java"), Verdict::Ask(Refusal::NotNamedByBuild));
        assert_eq!(Fixture::new(&[]).autonomous("work/app/src/B.java"), Verdict::Ask(Refusal::NotNamedByBuild));
    }

    #[test]
    fn only_reading_and_writing_are_granted() {
        let f = Fixture::new(&["work/app/src/B.java"]);
        for op in [Op::Delete, Op::Create, Op::List] {
            assert_eq!(f.verdict(AccessMode::Autonomous, Scope::Project, op, "work/app/src/B.java", 0), Verdict::Ask(Refusal::Operation), "{op:?}");
        }
    }

    #[test]
    fn another_project_is_in_scope_only_when_the_conversation_was_given_the_workspace() {
        let f = Fixture::new(&["work/lib/Other.java"]);
        assert_eq!(f.verdict(AccessMode::Autonomous, Scope::Project, Op::Write, "work/lib/Other.java", 0), Verdict::Ask(Refusal::OtherProject));
        assert_eq!(f.verdict(AccessMode::Autonomous, Scope::File, Op::Write, "work/lib/Other.java", 0), Verdict::Ask(Refusal::OtherProject));
        assert_eq!(f.verdict(AccessMode::Autonomous, Scope::Workspace, Op::Write, "work/lib/Other.java", 0), Verdict::AlreadyInScope);
        assert_eq!(f.verdict(AccessMode::Autonomous, Scope::Disk, Op::Write, "work/lib/Other.java", 0), Verdict::AlreadyInScope);
    }

    #[test]
    fn nothing_outside_the_workspace_is_ever_granted() {
        let f = Fixture::new(&["elsewhere/X.java", "work/app/../secret/X.java"]);
        for scope in [Scope::File, Scope::Project, Scope::Workspace, Scope::Disk] {
            assert_eq!(f.verdict(AccessMode::Autonomous, scope, Op::Write, "elsewhere/X.java", 0), Verdict::Ask(Refusal::OutsideProject), "{scope:?}");
        }
        // `..` cannot climb out of the project into a sibling.
        assert_eq!(f.autonomous("work/app/../secret/X.java"), Verdict::Ask(Refusal::OutsideProject));
        // A folder that merely starts with the project's name is not the project.
        assert_eq!(f.autonomous("work/app-other/X.java"), Verdict::Ask(Refusal::OutsideProject));
        assert_eq!(decide_grant(&GrantQuery {
            mode: AccessMode::Autonomous, scope: Scope::Project, op: Op::Write, path: Path::new("src/B.java"),
            diagnostics: &[], project_root: &p("work/app"), other_projects: &[], used: 0,
        }), Verdict::Ask(Refusal::OutsideProject), "a relative path is not a place");
    }

    #[test]
    fn secrets_are_never_granted_even_when_the_build_names_them() {
        let f = Fixture::new(&["work/app/.env", "work/app/server.pem", "work/app/.npmrc", "work/app/.pypirc", "work/app/release.jks", "work/app/id_rsa"]);
        for name in [".env", "server.pem", ".npmrc", ".pypirc", "release.jks", "id_rsa"] {
            assert_eq!(f.autonomous(&format!("work/app/{name}")), Verdict::Ask(Refusal::Secret), "{name}");
        }
    }

    #[test]
    fn output_and_fetched_folders_are_never_granted() {
        let f = Fixture::new(&[
            "work/app/target/debug/build/x/out.rs", "work/app/build/Gen.java", "work/app/out/A.class", "work/app/dist/app.js",
            "work/app/node_modules/lib/index.js", "work/app/src/generated/Dto.java", "work/app/.git/config",
        ]);
        for (path, folder) in [
            ("work/app/target/debug/build/x/out.rs", "target"), ("work/app/build/Gen.java", "build"), ("work/app/out/A.class", "out"),
            ("work/app/dist/app.js", "dist"), ("work/app/node_modules/lib/index.js", "node_modules"), ("work/app/src/generated/Dto.java", "generated"),
        ] {
            assert_eq!(f.autonomous(path), Verdict::Ask(Refusal::GeneratedFolder(folder.into())), "{path}");
        }
        assert_eq!(f.autonomous("work/app/.git/config"), Verdict::Ask(Refusal::RepositoryInternals));
    }

    #[test]
    fn a_project_that_lives_under_a_folder_called_build_is_not_shut_out_of_its_own_files() {
        let mut f = Fixture::new(&["build/app/src/B.java"]);
        f.root = p("build/app");
        assert_eq!(f.autonomous("build/app/src/B.java"), Verdict::Grant);
    }

    #[test]
    fn dependency_manifests_and_lockfiles_are_never_granted() {
        let names = ["pom.xml", "Cargo.toml", "Cargo.lock", "package.json", "package-lock.json", "build.gradle", "build.gradle.kts", "go.mod", "pnpm-lock.yaml"];
        let named: Vec<String> = names.iter().map(|n| format!("work/app/{n}")).collect();
        let f = Fixture::new(&named.iter().map(String::as_str).collect::<Vec<_>>());
        for n in names {
            assert_eq!(f.autonomous(&format!("work/app/{n}")), Verdict::Ask(Refusal::DependencyFile), "{n}");
        }
        // A nested module's manifest is the same thing.
        let nested = Fixture::new(&["work/app/core/pom.xml"]);
        assert_eq!(nested.autonomous("work/app/core/pom.xml"), Verdict::Ask(Refusal::DependencyFile));
    }

    #[test]
    fn the_cap_stops_the_twenty_sixth_file() {
        let f = Fixture::new(&["work/app/src/B.java"]);
        assert_eq!(f.verdict(AccessMode::Autonomous, Scope::Project, Op::Write, "work/app/src/B.java", MAX_AUTO_GRANTS - 1), Verdict::Grant);
        assert_eq!(f.verdict(AccessMode::Autonomous, Scope::Project, Op::Write, "work/app/src/B.java", MAX_AUTO_GRANTS), Verdict::Ask(Refusal::CapReached));
    }

    #[cfg(windows)]
    #[test]
    fn windows_compares_the_build_path_without_regard_to_case() {
        let f = Fixture::new(&["WORK/APP/SRC/B.JAVA"]);
        assert_eq!(f.autonomous("work/app/src/b.java"), Verdict::Grant);
    }

    #[test]
    fn modes_read_the_old_and_the_new_word_and_write_the_new_one() {
        assert_eq!(AccessMode::parse("auto"), AccessMode::AcceptEdits);
        assert_eq!(AccessMode::parse("accept_edits"), AccessMode::AcceptEdits);
        assert_eq!(AccessMode::parse("Autonomous"), AccessMode::Autonomous);
        assert_eq!(AccessMode::parse("manual"), AccessMode::Manual);
        assert_eq!(AccessMode::parse(""), AccessMode::Manual);
        assert_eq!(AccessMode::parse("nonsense"), AccessMode::Manual, "unknown means the safe one");
        assert_eq!(canonical_mode_word("auto"), "accept_edits");
        for mode in [AccessMode::Manual, AccessMode::AcceptEdits, AccessMode::Autonomous] {
            assert_eq!(AccessMode::parse(mode.id()), mode, "the saved word reads back");
        }
        assert!(!AccessMode::Manual.applies_edits() && AccessMode::AcceptEdits.applies_edits() && AccessMode::Autonomous.applies_edits());
    }

    #[test]
    fn a_build_names_the_files_with_errors_and_not_those_with_warnings() {
        let root = p("work/app");
        let build = json!({"problems": [
            {"severity": "error", "file": "src/a.rs", "message": "x"},
            {"severity": "warning", "file": "src/w.rs", "message": "y"},
            {"severity": "error", "file": p("work/lib/b.rs").to_string_lossy(), "message": "z"},
            {"severity": "error", "message": "no file"},
        ]});
        let got = diagnostic_files("bennu_build", &build, &root);
        assert_eq!(got, vec![p("work/app/src/a.rs"), p("work/lib/b.rs")]);
    }

    #[test]
    fn a_check_and_a_problems_list_name_their_files_only_when_there_is_an_error() {
        let root = p("work/app");
        let clean = json!({"file": "src/A.java", "problems": [{"severity": "warning"}]});
        assert!(diagnostic_files("bennu_check_file", &clean, &root).is_empty());
        let broken = json!({"file": "src/A.java", "problems": [{"severity": "error"}]});
        assert_eq!(diagnostic_files("bennu_check_file", &broken, &root), vec![p("work/app/src/A.java")]);

        let list = json!({"files": [
            {"file": "src/A.java", "problems": [{"severity": "error"}]},
            {"file": "src/B.java", "problems": [{"severity": "warning"}]},
        ]});
        assert_eq!(diagnostic_files("bennu_problems", &list, &root), vec![p("work/app/src/A.java")]);
        assert!(diagnostic_files("bennu_read_file", &list, &root).is_empty(), "other tools name no files");
    }

    #[test]
    fn a_path_in_a_report_cannot_climb_out_of_the_project_by_looking_relative() {
        let root = p("work/app");
        let got = diagnostic_files("bennu_build", &json!({"problems": [{"severity": "error", "file": "../lib/b.rs"}]}), &root);
        assert_eq!(got, vec![p("work/lib/b.rs")], "resolved to where it really is, so the grant function refuses it");
    }

    #[test]
    fn the_ledger_counts_every_file_and_revokes_each_once() {
        let mut g = RunGrants::default();
        let a = g.add(&p("work/app/A.java"));
        let b = g.add(&p("work/app/B.java"));
        assert_ne!(a, b);
        assert_eq!(g.used(), 2);
        assert_eq!(g.revoke(&a), Some(p("work/app/A.java")));
        assert_eq!(g.revoke(&a), None, "once");
        assert_eq!(g.revoke("g99"), None);
        assert_eq!(g.used(), 2, "a revoked grant still counts toward the cap");
        assert_eq!(g.active_ids(), vec![b]);
    }

    #[test]
    fn diagnostics_are_remembered_without_repeats() {
        let mut d = Diagnostics::default();
        d.note([p("work/app/A.java"), p("work/app/A.java")]);
        d.note([p("work/app/./A.java"), p("work/app/B.java")]);
        assert_eq!(d.files().len(), 2);
        assert!(d.names(&p("work/app/B.java")) && !d.names(&p("work/app/C.java")));
    }

    #[test]
    fn the_prompt_for_a_build_error_says_so() {
        let text = build_error_ask(&p("work/app/src/B.java"), &p("work/app/src"), "this file only");
        assert!(text.contains("compile error") && text.contains("`B.java`") && text.contains("this file only"), "{text}");
        assert_eq!(grant_reason(&p("work/app/src/B.java")), "Access granted to B.java because the build reports an error there");
    }
}
