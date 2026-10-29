//! What a tool may touch — and when it has to ask.
//!
//! Every tool that names a path goes through [`Policy::check`] first, and nothing else decides. The
//! answer is one of three: **allow**, **ask** (put the question to the user and act only on a yes), or
//! **deny** (and say why, so the model can choose another way) — deny being for what no answer can
//! change: editing switched off, a secret, the repository's own files. The rules are deliberately few and
//! readable, because a policy nobody can hold in their head is a policy nobody trusts:
//!
//! 1. **Where.** The [`Scope`] sets the ground: this file, this project, every open project, or anywhere
//!    on disk. Inside it is allowed. **Outside it, it asks — once.** One prompt decides whether the folder
//!    is opened for *reading* or for *reading and writing*, and the answer is remembered for the session
//!    as a [`Grant`]; a read-only grant asks again, as an upgrade, the first time something wants to
//!    write there. Under `Disk` there is no outside to ask about. The user's extra folders and the Temp
//!    folders are always inside, whatever the scope.
//! 2. **What.** Changing anything (write, create, delete) needs edits switched on. **Deleting** follows
//!    [`Policy::check_counted`]: free in the assistant's own Temp area in every mode; free inside the
//!    scope when the mode applies edits (`delete_unasked`) and the call is not past
//!    [`MAX_UNASKED_DELETE_FILES`]; otherwise it asks — always in Manual, always outside the scope, and
//!    always for the project folder itself.
//! 3. **Secrets and the repository's own files.** Environment files, keys and certificates are never
//!    written, and reading one asks; nothing under `.git` is ever written.
//!
//! The check is on the **lexical** path: `.` and `..` are resolved without touching the disk, so
//! `project/../other` is `other`, outside, whatever it looks like. A host that follows symbolic links
//! must resolve them *before* asking — a link out of the project is a path out of the project.

use std::path::{Component, Path, PathBuf};

/// The most files one delete call may remove without a question, even where deleting is otherwise free.
/// A directory delete counts the files inside it.
pub const MAX_UNASKED_DELETE_FILES: usize = 50;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Scope {
    /// Only the file the user is in.
    File,
    /// Every file of the project.
    Project,
    /// Every project open in the workspace.
    Workspace,
    /// Anywhere on disk — asking before anything outside the workspace.
    Disk,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Op {
    Read,
    /// List a folder, or search under it.
    List,
    Write,
    Create,
    Delete,
}

impl Op {
    fn changes(self) -> bool {
        matches!(self, Op::Write | Op::Create | Op::Delete)
    }
}

/// A question for the user. A plain one is yes or no; one with a `grant` is a request for access to a
/// folder, and its answer is one of three — no, reading, or reading and writing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ask {
    /// What to ask, in a sentence.
    pub message: String,
    /// The folder the access request is about; `None` for a plain confirmation (a deletion).
    pub grant: Option<PathBuf>,
    /// The question is about writing in a folder the user opened for reading only, rather than about reaching it.
    pub upgrade: bool,
}

impl Ask {
    fn confirm(message: String) -> Self {
        Ask { message, grant: None, upgrade: false }
    }

    fn access(message: String, folder: PathBuf) -> Self {
        Ask { message, grant: Some(folder), upgrade: false }
    }
}

/// What the user allowed for one folder, for the rest of the session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Grant {
    pub root: PathBuf,
    /// Reading and writing; otherwise reading only.
    pub write: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Decision {
    Allow,
    /// The user has to say yes first.
    Ask(Ask),
    /// Not permitted. The text says why, for the model as much as for the user.
    Deny(String),
}

impl Decision {
    pub fn is_allowed(&self) -> bool {
        matches!(self, Decision::Allow)
    }
}

#[derive(Debug, Clone)]
pub struct Policy {
    pub scope: Scope,
    /// The file the user is in — what `Scope::File` means.
    pub file: Option<PathBuf>,
    pub project_root: Option<PathBuf>,
    pub workspace_roots: Vec<PathBuf>,
    /// Folders the user added by name. Allowed in any scope.
    pub extra_folders: Vec<PathBuf>,
    /// The system Temp folders, allowed (in any scope) while `allow_temp` is on.
    pub temp_dirs: Vec<PathBuf>,
    pub allow_temp: bool,
    pub allow_edits: bool,
    /// Deleting inside the scope needs no yes: true in the modes that apply edits as they are made.
    pub delete_unasked: bool,
    /// What the user has opened beyond the scope, folder by folder, while answering prompts.
    pub grants: Vec<Grant>,
    /// Single files the run opened for itself (see [`crate::autonomy`]), read and write. Unlike `grants`
    /// they are never remembered: a policy is built per run, so they end with it.
    pub run_files: Vec<PathBuf>,
}

impl Policy {
    /// Open one file for reading and writing for the rest of this run.
    pub fn grant_file(&mut self, file: &Path) {
        let file = normalize(file);
        if !self.run_files.iter().any(|f| same(f, &file)) {
            self.run_files.push(file);
        }
    }

    /// Close a file opened with [`Self::grant_file`]. Returns whether it was open.
    pub fn revoke_file(&mut self, file: &Path) -> bool {
        let before = self.run_files.len();
        self.run_files.retain(|f| !same(f, file));
        self.run_files.len() != before
    }

    fn run_granted(&self, path: &Path, op: Op) -> bool {
        matches!(op, Op::Read | Op::Write) && self.run_files.iter().any(|f| same(path, f))
    }

    /// Record the answer to an access prompt. Granting a folder again replaces what was there, so a
    /// read-only grant upgraded to read-write is one entry, not two.
    pub fn grant(&mut self, root: PathBuf, write: bool) {
        let root = normalize(&root);
        self.grants.retain(|g| !same(&g.root, &root));
        self.grants.push(Grant { root, write });
    }

    pub fn check(&self, path: &Path, op: Op) -> Decision {
        self.check_counted(path, op, 1)
    }

    /// Like [`Self::check`], for an operation that touches `files` files at once — a delete of a folder
    /// counts what is inside it, because the cap on unasked deletes is about how much goes in one call.
    pub fn check_counted(&self, path: &Path, op: Op, files: usize) -> Decision {
        if !path.is_absolute() {
            return Decision::Deny(format!("`{}` is not an absolute path", path.display()));
        }
        let path = normalize(path);

        // What no answer can change, wherever it is.
        if op.changes() {
            if !self.allow_edits {
                return Decision::Deny("editing is switched off".to_string());
            }
            if has_component(&path, ".git") {
                return Decision::Deny("the repository's own `.git` files are never changed here".to_string());
            }
            if is_secret_file(&path) {
                return Decision::Deny(format!("`{}` holds secrets and is never written", file_name(&path)));
            }
        }

        // Where it is.
        if !self.in_always_allowed(&path) && !self.in_scope(&path, op) && !self.run_granted(&path, op) && self.scope != Scope::Disk {
            match self.grants.iter().find(|g| within(&path, &g.root)) {
                // Opened for reading, and now something wants to write: the one case that asks again.
                Some(g) if op.changes() && !g.write => {
                    let ask = Ask::access(format!("Allow writing in `{}`? You opened it for reading only.", g.root.display()), g.root.clone());
                    return Decision::Ask(Ask { upgrade: true, ..ask });
                }
                Some(_) => {}
                None => {
                    let folder = folder_of(&path, op);
                    return Decision::Ask(Ask::access(
                        format!(
                            "`{}` is outside what you allowed ({}). Open `{}` for reading, or for reading and writing?",
                            path.display(),
                            self.scope_words(),
                            folder.display()
                        ),
                        folder,
                    ));
                }
            }
        }

        // What it does there.
        if op == Op::Delete {
            return self.delete_decision(&path, files);
        }
        if matches!(op, Op::Read) && is_secret_file(&path) {
            return Decision::Ask(Ask::confirm(format!(
                "`{}` may contain secrets. Let Imber read it?",
                file_name(&path)
            )));
        }
        Decision::Allow
    }

    /// Whether `path` is in the assistant's own Temp area (and that area is switched on).
    pub fn is_temp(&self, path: &Path) -> bool {
        self.allow_temp && self.temp_dirs.iter().any(|t| within(path, t))
    }

    /// The delete rule, once the path is known to be reachable and writable (`path` is normalized):
    /// the Temp area is free in every mode; inside the scope it is free where the mode applies edits;
    /// the project folder itself and anything past the file cap always ask; outside the scope asks.
    fn delete_decision(&self, path: &Path, files: usize) -> Decision {
        let ask = |why: String| Decision::Ask(Ask::confirm(format!("Delete `{}`? {why} It stays in Local History.", path.display())));
        let is_root = self
            .project_root
            .iter()
            .chain(&self.workspace_roots)
            .chain(&self.extra_folders)
            .chain(&self.temp_dirs)
            .chain(self.grants.iter().map(|g| &g.root))
            .any(|r| same(path, r));
        if is_root {
            return ask("This is a whole project or allowed folder, not a file in it.".into());
        }
        if self.is_temp(path) {
            return Decision::Allow;
        }
        let inside = self.in_scope(path, Op::Delete) || self.extra_folders.iter().any(|f| within(path, f));
        if !inside {
            return ask("It is outside what you allowed.".into());
        }
        if !self.delete_unasked {
            return ask(String::new());
        }
        if files > MAX_UNASKED_DELETE_FILES {
            return ask(format!("It removes {files} files."));
        }
        Decision::Allow
    }

    fn in_always_allowed(&self, path: &Path) -> bool {
        self.extra_folders.iter().any(|f| within(path, f))
            || (self.allow_temp && self.temp_dirs.iter().any(|t| within(path, t)))
    }

    fn in_scope(&self, path: &Path, op: Op) -> bool {
        match self.scope {
            Scope::File => {
                // The one file, and nothing about folders: no listing, no creating beside it.
                matches!(op, Op::Read | Op::Write) && self.file.as_deref().is_some_and(|f| same(path, f))
            }
            Scope::Project => self.project_root.as_deref().is_some_and(|r| within(path, r)),
            Scope::Workspace | Scope::Disk => {
                self.project_root.as_deref().is_some_and(|r| within(path, r))
                    || self.workspace_roots.iter().any(|r| within(path, r))
            }
        }
    }

    pub fn scope_words(&self) -> &'static str {
        match self.scope {
            Scope::File => "this file only",
            Scope::Project => "this project",
            Scope::Workspace => "the projects in this workspace",
            Scope::Disk => "anywhere on disk",
        }
    }
}

/// `.` and `..` resolved without the filesystem. A `..` above the root stays at the root, which is where
/// the operating system would leave it too.
pub fn normalize(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for c in path.components() {
        match c {
            Component::ParentDir => {
                // Never pop the prefix or the root.
                if matches!(out.components().next_back(), Some(Component::Normal(_))) {
                    out.pop();
                }
            }
            Component::CurDir => {}
            other => out.push(other.as_os_str()),
        }
    }
    out
}

/// Case-insensitive on Windows, where `C:\Proj` and `c:\proj` are one place.
fn fold(s: &std::ffi::OsStr) -> String {
    let text = s.to_string_lossy();
    if cfg!(windows) { text.to_lowercase() } else { text.into_owned() }
}

fn parts(path: &Path) -> Vec<String> {
    path.components().map(|c| fold(c.as_os_str())).collect()
}

/// Whether `path` is `root` or something inside it — compared by whole components, so `/proj-other` is
/// not inside `/proj`.
pub(crate) fn within(path: &Path, root: &Path) -> bool {
    let (p, r) = (parts(path), parts(&normalize(root)));
    !r.is_empty() && p.len() >= r.len() && p[..r.len()] == r[..]
}

pub(crate) fn same(a: &Path, b: &Path) -> bool {
    parts(&normalize(a)) == parts(&normalize(b))
}

pub(crate) fn has_component(path: &Path, name: &str) -> bool {
    path.components().any(|c| fold(c.as_os_str()) == name)
}

/// The folder an access request is really about: a listing's own folder, otherwise the one the file is in.
fn folder_of(path: &Path, op: Op) -> PathBuf {
    if op == Op::List {
        return path.to_path_buf();
    }
    path.parent().map(Path::to_path_buf).unwrap_or_else(|| path.to_path_buf())
}

pub(crate) fn file_name(path: &Path) -> String {
    path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
}

/// Environment files, private keys, certificates, keystores and package-registry credentials — the files
/// whose content is the secret.
pub(crate) fn is_secret_file(path: &Path) -> bool {
    let name = file_name(path).to_lowercase();
    name == ".env"
        || name.starts_with(".env.")
        || name.ends_with(".pem")
        || name.ends_with(".key")
        || name.ends_with(".p12")
        || name.ends_with(".pfx")
        || name.ends_with(".jks")
        || name.ends_with(".keystore")
        || name.starts_with("id_rsa")
        || name.starts_with("id_dsa")
        || name.starts_with("id_ecdsa")
        || name.starts_with("id_ed25519")
        || name == "credentials"
        || name == "credentials.json"
        || name == ".npmrc"
        || name == ".pypirc"
        || name == ".netrc"
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(s: &str) -> PathBuf {
        // A rooted path on whichever platform the tests run: `C:` on Windows, `/` elsewhere.
        if cfg!(windows) { PathBuf::from(format!("C:\\{}", s.replace('/', "\\"))) } else { PathBuf::from(format!("/{s}")) }
    }

    fn policy(scope: Scope) -> Policy {
        Policy {
            scope,
            file: Some(p("work/app/src/A.java")),
            project_root: Some(p("work/app")),
            workspace_roots: vec![p("work/app"), p("work/lib")],
            extra_folders: vec![p("scratch/notes")],
            temp_dirs: vec![p("tmp")],
            allow_temp: true,
            allow_edits: true,
            delete_unasked: true,
            grants: Vec::new(),
            run_files: Vec::new(),
        }
    }

    fn check(scope: Scope, path: &str, op: Op) -> Decision {
        policy(scope).check(&p(path), op)
    }

    #[test]
    fn inside_the_project_reading_and_editing_are_allowed() {
        assert_eq!(check(Scope::Project, "work/app/src/B.java", Op::Read), Decision::Allow);
        assert_eq!(check(Scope::Project, "work/app/src/B.java", Op::Write), Decision::Allow);
        assert_eq!(check(Scope::Project, "work/app/src/New.java", Op::Create), Decision::Allow);
        assert_eq!(check(Scope::Project, "work/app/src", Op::List), Decision::Allow);
    }

    #[test]
    fn outside_the_scope_it_asks_once_naming_the_folder() {
        let d = check(Scope::Project, "work/lib/Other.java", Op::Read);
        match d {
            Decision::Ask(ask) => {
                assert_eq!(ask.grant, Some(p("work/lib")), "the question is about the folder, not the one file");
                assert!(ask.message.contains("outside what you allowed"), "{}", ask.message);
            }
            other => panic!("expected a question, got {other:?}"),
        }
    }

    #[test]
    fn listing_a_folder_asks_about_that_folder() {
        match check(Scope::Project, "elsewhere/data", Op::List) {
            Decision::Ask(ask) => assert_eq!(ask.grant, Some(p("elsewhere/data"))),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn a_read_grant_opens_the_folder_for_reading_and_asks_again_to_write() {
        let mut policy = policy(Scope::Project);
        policy.grant(p("work/lib"), false);
        assert_eq!(policy.check(&p("work/lib/Other.java"), Op::Read), Decision::Allow);
        assert_eq!(policy.check(&p("work/lib/deep/er/X.java"), Op::Read), Decision::Allow, "the folder and everything under it");
        match policy.check(&p("work/lib/Other.java"), Op::Write) {
            Decision::Ask(ask) => {
                assert_eq!(ask.grant, Some(p("work/lib")));
                assert!(ask.message.contains("reading only"), "{}", ask.message);
            }
            other => panic!("{other:?}"),
        }
        // A sibling folder is a different question.
        assert!(matches!(policy.check(&p("work/other/X.java"), Op::Read), Decision::Ask(_)));
    }

    #[test]
    fn a_write_grant_opens_it_for_both_and_replaces_the_read_one() {
        let mut policy = policy(Scope::Project);
        policy.grant(p("work/lib"), false);
        policy.grant(p("work/lib"), true);
        assert_eq!(policy.grants.len(), 1, "an upgrade is one entry, not two");
        assert_eq!(policy.check(&p("work/lib/Other.java"), Op::Write), Decision::Allow);
        assert_eq!(policy.check(&p("work/lib/New.java"), Op::Create), Decision::Allow);
        // A granted folder is outside the scope: deleting there still asks.
        assert!(matches!(policy.check(&p("work/lib/Other.java"), Op::Delete), Decision::Ask(_)));
    }

    #[test]
    fn a_grant_does_not_unlock_what_no_answer_can() {
        let mut policy = policy(Scope::Project);
        policy.grant(p("work/lib"), true);
        assert!(matches!(policy.check(&p("work/lib/.env"), Op::Write), Decision::Deny(_)));
        assert!(matches!(policy.check(&p("work/lib/.git/config"), Op::Write), Decision::Deny(_)));
    }

    #[test]
    fn with_edits_off_an_outside_write_is_denied_rather_than_asked() {
        let mut policy = policy(Scope::Project);
        policy.allow_edits = false;
        assert!(matches!(policy.check(&p("work/lib/Other.java"), Op::Write), Decision::Deny(_)));
        assert!(matches!(policy.check(&p("work/lib/Other.java"), Op::Read), Decision::Ask(_)));
    }

    #[test]
    fn a_folder_that_merely_starts_with_the_projects_name_is_not_inside_it() {
        assert!(matches!(check(Scope::Project, "work/app-other/A.java", Op::Read), Decision::Ask(_)));
    }

    #[test]
    fn dot_dot_cannot_climb_out() {
        // It is asked about as the place it really is, not the place it looks like.
        let sneaky = p("work/app/../lib/Other.java");
        match policy(Scope::Project).check(&sneaky, Op::Read) {
            Decision::Ask(ask) => assert_eq!(ask.grant, Some(p("work/lib"))),
            other => panic!("{other:?}"),
        }
        let up_to_root = p("work/app/../../../../etc/passwd");
        assert!(matches!(policy(Scope::Project).check(&up_to_root, Op::Read), Decision::Ask(_)));
    }

    #[test]
    fn a_relative_path_is_refused_outright() {
        assert!(matches!(policy(Scope::Project).check(Path::new("src/A.java"), Op::Read), Decision::Deny(_)));
    }

    #[test]
    fn the_file_scope_is_one_file_and_nothing_around_it() {
        assert_eq!(check(Scope::File, "work/app/src/A.java", Op::Read), Decision::Allow);
        assert_eq!(check(Scope::File, "work/app/src/A.java", Op::Write), Decision::Allow);
        // Anything else is outside what the scope covers, so it asks.
        assert!(matches!(check(Scope::File, "work/app/src/B.java", Op::Read), Decision::Ask(_)));
        assert!(matches!(check(Scope::File, "work/app/src", Op::List), Decision::Ask(_)));
        assert!(matches!(check(Scope::File, "work/app/src/New.java", Op::Create), Decision::Ask(_)));
    }

    #[test]
    fn the_workspace_reaches_its_other_projects() {
        assert_eq!(check(Scope::Workspace, "work/lib/Other.java", Op::Read), Decision::Allow);
        assert!(matches!(check(Scope::Workspace, "elsewhere/x.txt", Op::Read), Decision::Ask(_)));
    }

    #[test]
    fn with_the_whole_disk_there_is_no_outside_to_ask_about() {
        assert_eq!(check(Scope::Disk, "elsewhere/x.txt", Op::Read), Decision::Allow);
        assert_eq!(check(Scope::Disk, "elsewhere/x.txt", Op::Write), Decision::Allow);
        // Still not a secret, and a deletion outside the workspace still asks.
        assert!(matches!(check(Scope::Disk, "elsewhere/.env", Op::Write), Decision::Deny(_)));
        assert!(matches!(check(Scope::Disk, "elsewhere/x.txt", Op::Delete), Decision::Ask(_)));
    }

    fn delete_check(unasked: bool, scope: Scope, path: &str, files: usize) -> Decision {
        let mut policy = policy(scope);
        policy.delete_unasked = unasked;
        policy.check_counted(&p(path), Op::Delete, files)
    }

    #[test]
    fn the_temp_area_is_free_to_delete_in_every_mode() {
        for unasked in [false, true] {
            assert_eq!(delete_check(unasked, Scope::Project, "tmp/scratch.txt", 1), Decision::Allow, "unasked={unasked}");
            assert_eq!(delete_check(unasked, Scope::File, "tmp/dir", 500), Decision::Allow, "no cap in the agent's own area");
        }
        // Switched off, it is just another folder outside the scope.
        let mut off = policy(Scope::Project);
        off.allow_temp = false;
        assert!(matches!(off.check(&p("tmp/scratch.txt"), Op::Delete), Decision::Ask(_)));
    }

    #[test]
    fn inside_the_scope_a_delete_is_free_where_the_mode_applies_edits_and_asks_in_manual() {
        assert_eq!(delete_check(true, Scope::Project, "work/app/src/A.java", 1), Decision::Allow);
        assert_eq!(delete_check(true, Scope::Workspace, "work/lib/Other.java", 1), Decision::Allow);
        assert_eq!(delete_check(true, Scope::Project, "scratch/notes/todo.md", 1), Decision::Allow, "an extra folder is inside");
        for scope in [Scope::File, Scope::Project, Scope::Workspace, Scope::Disk] {
            assert!(matches!(delete_check(false, scope, "work/app/src/A.java", 1), Decision::Ask(_)), "manual {scope:?}");
        }
    }

    #[test]
    fn outside_the_scope_a_delete_asks_in_every_mode() {
        for unasked in [false, true] {
            assert!(matches!(delete_check(unasked, Scope::Disk, "elsewhere/x.txt", 1), Decision::Ask(_)));
            assert!(matches!(delete_check(unasked, Scope::Project, "work/lib/Other.java", 1), Decision::Ask(_)));
        }
        // The file scope covers one file for reading and writing, not for deleting.
        assert!(matches!(delete_check(true, Scope::File, "work/app/src/A.java", 1), Decision::Ask(_)));
    }

    #[test]
    fn the_project_folder_itself_always_asks() {
        for scope in [Scope::Project, Scope::Workspace, Scope::Disk] {
            let d = delete_check(true, scope, "work/app", 1);
            match d {
                Decision::Ask(ask) => assert!(ask.message.contains("whole project"), "{}", ask.message),
                other => panic!("{scope:?}: {other:?}"),
            }
        }
        assert!(matches!(delete_check(true, Scope::Workspace, "work/lib", 1), Decision::Ask(_)));
        assert!(matches!(delete_check(true, Scope::Project, "tmp", 1), Decision::Ask(_)), "the Temp root itself");
    }

    #[test]
    fn a_big_delete_asks_even_where_deleting_is_free() {
        assert_eq!(delete_check(true, Scope::Project, "work/app/gen", MAX_UNASKED_DELETE_FILES), Decision::Allow);
        match delete_check(true, Scope::Project, "work/app/gen", MAX_UNASKED_DELETE_FILES + 1) {
            Decision::Ask(ask) => assert!(ask.message.contains("51 files"), "{}", ask.message),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn secrets_and_git_files_are_denied_not_deleted_in_any_mode() {
        for unasked in [false, true] {
            assert!(matches!(delete_check(unasked, Scope::Project, "work/app/.env", 1), Decision::Deny(_)));
            assert!(matches!(delete_check(unasked, Scope::Project, "work/app/.git/HEAD", 1), Decision::Deny(_)));
        }
    }

    #[test]
    fn switching_edits_off_leaves_it_able_to_read_only() {
        let mut policy = policy(Scope::Project);
        policy.allow_edits = false;
        assert_eq!(policy.check(&p("work/app/src/B.java"), Op::Read), Decision::Allow);
        assert!(matches!(policy.check(&p("work/app/src/B.java"), Op::Write), Decision::Deny(_)));
        assert!(matches!(policy.check(&p("work/app/src/N.java"), Op::Create), Decision::Deny(_)));
    }

    #[test]
    fn the_temp_and_extra_folders_are_open_in_any_scope() {
        assert_eq!(check(Scope::File, "tmp/out.log", Op::Read), Decision::Allow);
        assert_eq!(check(Scope::Project, "tmp/out.log", Op::Create), Decision::Allow);
        assert_eq!(check(Scope::Project, "scratch/notes/todo.md", Op::Write), Decision::Allow);
    }

    #[test]
    fn temp_can_be_switched_off_and_the_extra_folders_stay() {
        let mut policy = policy(Scope::Project);
        policy.allow_temp = false;
        assert!(matches!(policy.check(&p("tmp/out.log"), Op::Read), Decision::Ask(_)));
        assert_eq!(policy.check(&p("scratch/notes/todo.md"), Op::Read), Decision::Allow);
    }

    #[test]
    fn secrets_are_never_written_and_reading_one_asks() {
        for name in ["work/app/.env", "work/app/.env.local", "work/app/server.pem", "work/app/id_rsa"] {
            assert!(matches!(check(Scope::Project, name, Op::Write), Decision::Deny(_)), "{name}");
            assert!(matches!(check(Scope::Project, name, Op::Read), Decision::Ask(_)), "{name}");
        }
        assert_eq!(check(Scope::Project, "work/app/environment.md", Op::Read), Decision::Allow);
    }

    #[test]
    fn the_repositorys_own_files_are_never_changed() {
        assert!(matches!(check(Scope::Project, "work/app/.git/config", Op::Write), Decision::Deny(_)));
        assert!(matches!(check(Scope::Project, "work/app/.git/hooks/pre-commit", Op::Create), Decision::Deny(_)));
        assert_eq!(check(Scope::Project, "work/app/.git/HEAD", Op::Read), Decision::Allow);
    }

    #[test]
    fn a_secret_inside_temp_is_still_a_secret() {
        assert!(matches!(check(Scope::Project, "tmp/.env", Op::Write), Decision::Deny(_)));
    }

    #[test]
    fn a_file_opened_for_the_run_is_read_and_write_and_nothing_around_it() {
        let mut policy = policy(Scope::File);
        let granted = p("work/app/src/B.java");
        assert!(matches!(policy.check(&granted, Op::Write), Decision::Ask(_)));
        policy.grant_file(&granted);
        assert_eq!(policy.check(&granted, Op::Read), Decision::Allow);
        assert_eq!(policy.check(&granted, Op::Write), Decision::Allow);
        // Neighbours, listings, creation and deletion are not part of it.
        assert!(matches!(policy.check(&p("work/app/src/C.java"), Op::Write), Decision::Ask(_)));
        assert!(matches!(policy.check(&p("work/app/src"), Op::List), Decision::Ask(_)));
        assert!(matches!(policy.check(&granted, Op::Delete), Decision::Ask(_)));
        assert!(policy.revoke_file(&granted));
        assert!(matches!(policy.check(&granted, Op::Write), Decision::Ask(_)));
        assert!(!policy.revoke_file(&granted), "revoking twice finds nothing");
    }

    #[test]
    fn a_file_opened_for_the_run_does_not_unlock_a_secret() {
        let mut policy = policy(Scope::Project);
        let env = p("work/app/.env");
        policy.grant_file(&env);
        assert!(matches!(policy.check(&env, Op::Write), Decision::Deny(_)));
    }

    #[test]
    fn registry_credentials_and_keystores_are_secrets() {
        for name in [".pypirc", "release.jks", "app.keystore", "credentials.json", "id_ecdsa"] {
            assert!(is_secret_file(&p(&format!("work/app/{name}"))), "{name}");
        }
    }

    #[test]
    fn normalize_resolves_without_the_disk() {
        assert_eq!(normalize(&p("a/./b/../c")), p("a/c"));
        assert_eq!(normalize(&p("a/../../..")), p(""));
    }

    #[cfg(windows)]
    #[test]
    fn windows_paths_compare_without_regard_to_case() {
        let mut policy = policy(Scope::Project);
        policy.project_root = Some(PathBuf::from("C:\\Work\\App"));
        assert_eq!(policy.check(Path::new("c:\\work\\app\\src\\A.java"), Op::Read), Decision::Allow);
    }
}
