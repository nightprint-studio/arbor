//! Which tree entries git ignores.
//!
//! The project tree **shows** ignored files — it marks them. That is the whole reason
//! this is a matcher and not a filter: an IDE that hides `target/` and `*.class` hides
//! the answer to "why isn't my change being picked up", and the one thing you want in
//! that moment is to see the stale artifact sitting there greyed out.
//!
//! Gitignore is a stack, not a file: a `.gitignore` applies to its own directory and
//! everything below it, a deeper one overrides a shallower one, and a `!negation` can
//! bring an entry back. [`IgnoreStack`] is that stack, pushed and popped along the same
//! recursion the tree walk already does, so each `.gitignore` is read once per walk
//! rather than once per entry.
//!
//! The stack is seeded from the **repository** root and not from the walk root: a lazy
//! expansion of `web/src/main/java` still has to honour the `.gitignore` at the top of
//! the repo, which is where the interesting rules live.

use std::path::{Path, PathBuf};

use ignore::gitignore::{Gitignore, GitignoreBuilder};

/// The `.gitignore` files in scope at the current depth, shallowest first.
///
/// Empty when the walk root is not inside a git repository, in which case nothing is
/// ignored — which is the honest answer, not a fallback.
pub struct IgnoreStack {
    matchers: Vec<Gitignore>,
}

impl IgnoreStack {
    /// The stack in effect *around* `root`: the user's global excludes, the repo's
    /// `.git/info/exclude`, then every `.gitignore` from the repository root down to
    /// `root`'s parent. `root`'s own is pushed by the walk's first
    /// [`enter`](Self::enter), so a directory is never matched against a file whose
    /// patterns only describe what is inside it.
    pub fn at(root: &Path) -> Self {
        let mut matchers = Vec::new();
        let (global, _) = Gitignore::global();
        if !global.is_empty() {
            matchers.push(global);
        }

        let Some(repo) = repo_root(root) else {
            return Self { matchers };
        };
        if let Some(m) = build(&repo, &repo.join(".git").join("info").join("exclude")) {
            matchers.push(m);
        }

        // Repo root → walk root's parent, shallowest first: `ancestors()` yields the
        // other way, and `skip(1)` drops `root` itself.
        let mut chain: Vec<&Path> =
            root.ancestors().skip(1).take_while(|p| p.starts_with(&repo)).collect();
        chain.reverse();
        for dir in chain {
            push_dir_into(&mut matchers, dir);
        }
        Self { matchers }
    }

    /// Enter `dir`, adding its `.gitignore` if it has one. Returns whether anything was
    /// pushed, which is what the caller hands back to [`pop`](Self::pop) — balancing the
    /// stack by return value rather than by re-testing for the file keeps the two sides
    /// of the recursion from disagreeing.
    pub fn enter(&mut self, dir: &Path) -> bool {
        let before = self.matchers.len();
        push_dir_into(&mut self.matchers, dir);
        self.matchers.len() != before
    }

    /// Leave a directory previously entered. `pushed` is [`enter`](Self::enter)'s answer.
    pub fn pop(&mut self, pushed: bool) {
        if pushed {
            self.matchers.pop();
        }
    }

    /// Whether git ignores `path`.
    ///
    /// Deepest matcher wins, and within one file the last matching pattern wins — both
    /// are git's own precedence, and the second one is the `ignore` crate's job. A
    /// `!negation` therefore un-ignores, which is why this cannot be a simple "any
    /// matcher says yes".
    pub fn is_ignored(&self, path: &Path, is_dir: bool) -> bool {
        for m in self.matchers.iter().rev() {
            match m.matched(path, is_dir) {
                ignore::Match::Ignore(_) => return true,
                ignore::Match::Whitelist(_) => return false,
                ignore::Match::None => {}
            }
        }
        false
    }
}

fn push_dir_into(matchers: &mut Vec<Gitignore>, dir: &Path) {
    if let Some(m) = build(dir, &dir.join(".gitignore")) {
        matchers.push(m);
    }
}

/// A matcher for `file`, rooted at `base` (the directory its patterns are relative to).
/// `None` when the file is absent or holds nothing that matches.
fn build(base: &Path, file: &Path) -> Option<Gitignore> {
    if !file.is_file() {
        return None;
    }
    let mut b = GitignoreBuilder::new(base);
    // A malformed line is reported as an error alongside a usable matcher; the rest of
    // the file still applies, which is also what git does.
    let _ = b.add(file);
    let m = b.build().ok()?;
    (!m.is_empty()).then_some(m)
}

/// The nearest ancestor of `start` (inclusive) holding a `.git`, or `None` outside a
/// repository. `.git` is a *file* in a linked worktree and a directory otherwise, so
/// this tests existence rather than kind.
fn repo_root(start: &Path) -> Option<PathBuf> {
    start.ancestors().find(|p| p.join(".git").exists()).map(Path::to_path_buf)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A temp dir that cleans itself up, so these tests need no dev-dependency.
    struct TempDir(PathBuf);
    impl TempDir {
        fn new(tag: &str) -> Self {
            let mut p = std::env::temp_dir();
            p.push(format!("bennu-ignored-{tag}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&p);
            std::fs::create_dir_all(&p).unwrap();
            Self(p)
        }
        fn path(&self) -> &Path {
            &self.0
        }
        fn write(&self, rel: &str, body: &str) -> PathBuf {
            let p = self.0.join(rel);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(&p, body).unwrap();
            p
        }
        fn dir(&self, rel: &str) -> PathBuf {
            let p = self.0.join(rel);
            std::fs::create_dir_all(&p).unwrap();
            p
        }
    }
    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn repo(tag: &str) -> TempDir {
        let t = TempDir::new(tag);
        t.dir(".git");
        t
    }

    /// The stack as the tree walk holds it once it has entered `dir` — [`IgnoreStack::at`]
    /// seeds what is *above* the walk root, and the walk pushes the root's own on entry.
    fn walking(dir: &Path) -> IgnoreStack {
        let mut st = IgnoreStack::at(dir);
        st.enter(dir);
        st
    }

    #[test]
    fn no_repository_ignores_nothing() {
        let t = TempDir::new("norepo");
        let f = t.write("build.log", "");
        // No `.git` anywhere above a temp dir, so only the user's global excludes are in
        // scope — and those don't name this file.
        assert!(!walking(t.path()).is_ignored(&f, false));
    }

    #[test]
    fn root_gitignore_applies() {
        let t = repo("root");
        t.write(".gitignore", "*.log\n");
        let f = t.write("build.log", "");
        assert!(walking(t.path()).is_ignored(&f, false));
    }

    #[test]
    fn negation_wins_within_a_file() {
        let t = repo("neg");
        t.write(".gitignore", "*.log\n!keep.log\n");
        assert!(!walking(t.path()).is_ignored(&t.write("keep.log", ""), false));
        assert!(walking(t.path()).is_ignored(&t.write("drop.log", ""), false));
    }

    #[test]
    fn deeper_gitignore_overrides_shallower() {
        let t = repo("deep");
        t.write(".gitignore", "*.txt\n");
        t.write("sub/.gitignore", "!notes.txt\n");
        let mut st = walking(t.path());
        let sub = t.dir("sub");
        let pushed = st.enter(&sub);
        assert!(pushed, "sub/.gitignore should have entered the stack");
        assert!(!st.is_ignored(&t.write("sub/notes.txt", ""), false));
        st.pop(pushed);
        // Back at the root the negation is out of scope again.
        assert!(st.is_ignored(&t.write("notes.txt", ""), false));
    }

    #[test]
    fn seeded_from_the_repository_root_not_the_walk_root() {
        let t = repo("seed");
        t.write(".gitignore", "generated/\n");
        let walk = t.dir("web/src");
        let gen = t.dir("web/src/generated");
        // The walk starts three levels down, and the rule lives at the top of the repo.
        assert!(IgnoreStack::at(&walk).is_ignored(&gen, true));
    }

    #[test]
    fn directory_rules_need_the_dir_flag() {
        let t = repo("dirflag");
        t.write(".gitignore", "out/\n");
        let out = t.dir("out");
        let st = walking(t.path());
        assert!(st.is_ignored(&out, true));
        // The same name as a FILE is not what `out/` matches.
        assert!(!st.is_ignored(&t.path().join("out"), false));
    }
}
