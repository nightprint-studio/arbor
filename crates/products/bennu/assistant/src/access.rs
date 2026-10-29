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
//! 2. **What.** Changing anything (write, create, delete) needs edits switched on. **Deleting always
//!    asks**, in every scope and every folder, and is not something a setting can turn off.
//! 3. **Secrets and the repository's own files.** Environment files, keys and certificates are never
//!    written, and reading one asks; nothing under `.git` is ever written.
//!
//! The check is on the **lexical** path: `.` and `..` are resolved without touching the disk, so
//! `project/../other` is `other`, outside, whatever it looks like. A host that follows symbolic links
//! must resolve them *before* asking — a link out of the project is a path out of the project.

use std::path::{Component, Path, PathBuf};

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
}

impl Ask {
    fn confirm(message: String) -> Self {
        Ask { message, grant: None }
    }

    fn access(message: String, folder: PathBuf) -> Self {
        Ask { message, grant: Some(folder) }
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
    /// What the user has opened beyond the scope, folder by folder, while answering prompts.
    pub grants: Vec<Grant>,
}

impl Policy {
    /// Record the answer to an access prompt. Granting a folder again replaces what was there, so a
    /// read-only grant upgraded to read-write is one entry, not two.
    pub fn grant(&mut self, root: PathBuf, write: bool) {
        let root = normalize(&root);
        self.grants.retain(|g| !same(&g.root, &root));
        self.grants.push(Grant { root, write });
    }

    pub fn check(&self, path: &Path, op: Op) -> Decision {
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
        if !self.in_always_allowed(&path) && !self.in_scope(&path, op) && self.scope != Scope::Disk {
            match self.grants.iter().find(|g| within(&path, &g.root)) {
                // Opened for reading, and now something wants to write: the one case that asks again.
                Some(g) if op.changes() && !g.write => {
                    return Decision::Ask(Ask::access(
                        format!("Allow writing in `{}`? You opened it for reading only.", g.root.display()),
                        g.root.clone(),
                    ));
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
            return Decision::Ask(Ask::confirm(format!("Delete `{}`? It stays in Local History.", path.display())));
        }
        if matches!(op, Op::Read) && is_secret_file(&path) {
            return Decision::Ask(Ask::confirm(format!(
                "`{}` may contain secrets. Let the assistant read it?",
                file_name(&path)
            )));
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

    fn scope_words(&self) -> &'static str {
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
fn within(path: &Path, root: &Path) -> bool {
    let (p, r) = (parts(path), parts(&normalize(root)));
    !r.is_empty() && p.len() >= r.len() && p[..r.len()] == r[..]
}

fn same(a: &Path, b: &Path) -> bool {
    parts(a) == parts(&normalize(b))
}

fn has_component(path: &Path, name: &str) -> bool {
    path.components().any(|c| fold(c.as_os_str()) == name)
}

/// The folder an access request is really about: a listing's own folder, otherwise the one the file is in.
fn folder_of(path: &Path, op: Op) -> PathBuf {
    if op == Op::List {
        return path.to_path_buf();
    }
    path.parent().map(Path::to_path_buf).unwrap_or_else(|| path.to_path_buf())
}

fn file_name(path: &Path) -> String {
    path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
}

/// Environment files, private keys and certificates — the files whose content is the secret.
fn is_secret_file(path: &Path) -> bool {
    let name = file_name(path).to_lowercase();
    name == ".env"
        || name.starts_with(".env.")
        || name.ends_with(".pem")
        || name.ends_with(".key")
        || name.ends_with(".p12")
        || name.ends_with(".pfx")
        || name.starts_with("id_rsa")
        || name.starts_with("id_ed25519")
        || name == "credentials"
        || name == ".npmrc"
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
            grants: Vec::new(),
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
        // Deleting is its own question whatever was granted.
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
        // Still not a secret, and still not a deletion without a yes.
        assert!(matches!(check(Scope::Disk, "elsewhere/.env", Op::Write), Decision::Deny(_)));
        assert!(matches!(check(Scope::Disk, "elsewhere/x.txt", Op::Delete), Decision::Ask(_)));
    }

    #[test]
    fn deleting_always_asks_in_every_scope() {
        for scope in [Scope::File, Scope::Project, Scope::Workspace, Scope::Disk] {
            // Asked, in every scope. (Under `File` it is first asked about the folder — the file scope
            // covers one file for reading and writing and nothing else — and a deletion is a plain
            // confirmation under every other.)
            let d = check(scope, "work/app/src/A.java", Op::Delete);
            assert!(matches!(d, Decision::Ask(_)), "{scope:?}: {d:?}");
        }
        // And inside the Temp folders, where nothing else asks.
        assert!(matches!(check(Scope::Project, "tmp/scratch.txt", Op::Delete), Decision::Ask(_)));
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
