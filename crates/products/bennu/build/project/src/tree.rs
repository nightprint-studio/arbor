//! The project file tree (docs §5 #16).
//!
//! Builds a [`TreeNode`] rooted at a directory. Lazy-friendly: [`build`] takes a
//! `max_depth` so a large legacy tree (1200+ files — docs §8) isn't materialised in
//! one shot; the FE fetches deeper levels on expand. Noise dirs (`target`, `.git`,
//! `node_modules`) are skipped. Entries are sorted dirs-first then by name, the
//! conventional tree order.
//!
//! Every node also carries **what it is to the project**: hidden by convention, and
//! ignored by git (see [`crate::ignored`]). Both are marks, not filters — the tree
//! greys them out and keeps showing them, because a stale ignored artifact you cannot
//! see is a stale artifact you cannot explain.

use std::path::Path;

use bennu_proto::prelude::TreeNode;

use crate::error::ProjectError;
use crate::ignored::IgnoreStack;

/// Directories never walked. Not a "hidden" list: these are build output and machinery
/// with five-figure file counts, and materialising them would cost more than the rest
/// of the tree put together. Everything git merely *ignores* is walked and marked.
///
/// Public because the tree's filesystem watcher has to agree with it: a change to an entry the
/// tree never lists is a reload that shows nothing new.
pub const SKIP_DIRS: [&str; 7] =
    ["target", ".git", "node_modules", ".idea", ".gradle", ".venv", "__pycache__"];

/// Build output that is only noise **next to a build manifest**. `build`, `dist` and `out` are also
/// perfectly good Java package names (`com.acme.build`), so they are skipped only where they sit
/// beside a `pom.xml` / `Cargo.toml` / … — i.e. where they can only be the tool's output.
const OUTPUT_DIRS: [&str; 3] = ["build", "dist", "out"];

/// Files whose presence makes a sibling [`OUTPUT_DIRS`] entry build output.
const MANIFESTS: [&str; 5] = ["pom.xml", "Cargo.toml", "build.gradle", "build.gradle.kts", "package.json"];

/// The deepest a tree is ever walked, whatever the caller asks for.
const MAX_DEPTH: usize = 64;

/// The most nodes one tree holds. A directory of generated files can have five-figure counts that
/// no caller wants materialised; past the cap the walk stops and the tree says so.
pub const MAX_NODES: usize = 20_000;

/// Name of the marker node appended to the root when [`MAX_NODES`] was hit.
pub const TRUNCATED_MARKER: &str = "… tree truncated (too many entries)";

/// Build the tree rooted at `root`, descending at most `max_depth` levels. A
/// directory at the depth limit is returned with empty `children` (the FE re-requests
/// it). `root` must be a directory.
///
/// Bounded in three ways: depth (clamped to 64), node count ([`MAX_NODES`], marked by a trailing
/// [`TRUNCATED_MARKER`] child of the root) and links — a symlink or junction is listed but never
/// followed, so a link back up the tree cannot recurse until the path is too long.
pub fn build(root: &Path, max_depth: usize) -> Result<TreeNode, ProjectError> {
    if !root.is_dir() {
        return Err(ProjectError::NotADirectory(root.display().to_string()));
    }
    // Seeded from the repository root, so a lazy expansion deep inside the tree still
    // honours the `.gitignore` at the top — where the rules that matter live.
    let mut ignores = IgnoreStack::at(root);
    let root_ignored = ignores.is_ignored(root, true);
    let mut remaining = MAX_NODES;
    let mut node = build_node(root, max_depth.min(MAX_DEPTH), &mut ignores, root_ignored, &mut remaining);
    if remaining == 0 {
        node.children.push(truncation_marker(root));
    }
    Ok(node)
}

fn truncation_marker(root: &Path) -> TreeNode {
    TreeNode {
        name: TRUNCATED_MARKER.to_string(),
        path: root.join(TRUNCATED_MARKER).display().to_string(),
        is_dir: false,
        children: Vec::new(),
        hidden: false,
        ignored: true,
        root_tag: String::new(),
    }
}

/// Whether `name`, a real directory under `parent`, is noise the tree never walks.
///
/// Public so the other whole-project walkers (TODO scan, …) skip exactly what the tree skips.
pub fn is_noise_dir(parent: &Path, name: &str) -> bool {
    SKIP_DIRS.contains(&name)
        || (OUTPUT_DIRS.contains(&name) && MANIFESTS.iter().any(|m| parent.join(m).is_file()))
}

/// The children of `path`, noise removed and ordered dirs-first then case-insensitively by name.
///
/// Typed by `file_type()` rather than `path.is_dir()`: it does not follow links (so a `target`
/// *file* is kept — only a directory of that name is noise), and it is free where `is_dir` is a stat.
fn sorted_children(path: &Path) -> Vec<(std::fs::DirEntry, bool)> {
    let Ok(entries) = std::fs::read_dir(path) else { return Vec::new() };
    let mut kids: Vec<_> = entries
        .flatten()
        .filter_map(|e| {
            let real_dir = e.file_type().ok()?.is_dir();
            let name = e.file_name().to_string_lossy().into_owned();
            (!(real_dir && is_noise_dir(path, &name))).then_some((e, real_dir))
        })
        .collect();
    kids.sort_by(|(a, ad), (b, bd)| {
        bd.cmp(ad).then_with(|| {
            a.file_name().to_string_lossy().to_lowercase().cmp(&b.file_name().to_string_lossy().to_lowercase())
        })
    });
    kids
}

/// `inherited` is whether an ancestor is already ignored. Git has no way to un-ignore a
/// file under an ignored directory (it never descends into one), so this short-circuits
/// the match instead of asking again at every level.
///
/// `remaining` is the node budget shared by the whole walk; a node is only built while it is above zero.
fn build_node(
    path: &Path,
    depth_left: usize,
    ignores: &mut IgnoreStack,
    inherited: bool,
    remaining: &mut usize,
) -> TreeNode {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string());
    let is_dir = path.is_dir();
    let mut children = Vec::new();

    if is_dir && depth_left > 0 {
        let pushed = ignores.enter(path);
        for (kid, real_dir) in sorted_children(path) {
            if *remaining == 0 {
                break;
            }
            *remaining -= 1;
            let kid_path = kid.path();
            let kid_is_dir = real_dir || kid_path.is_dir();
            let kid_ignored = inherited || ignores.is_ignored(&kid_path, kid_is_dir);
            // A link to a directory is shown as a directory but not entered: only a real directory
            // can be the start of a cycle-free walk.
            let kid_depth = if real_dir { depth_left - 1 } else { 0 };
            children.push(build_node(&kid_path, kid_depth, ignores, kid_ignored, remaining));
        }
        ignores.pop(pushed);
    }

    TreeNode {
        hidden: is_hidden(path, &name),
        ignored: inherited,
        root_tag: if is_dir { String::new() } else { root_tag(path, &name) },
        name,
        path: path.display().to_string(),
        is_dir,
        children,
    }
}

/// The most bytes read from an XML to find its root element.
///
/// A root element is within the first few hundred bytes of every XML that exists — after it come
/// the declaration, a DOCTYPE and the licence comment somebody's generator writes. This bounds the
/// cost of being wrong about that on a file that turns out to be a megabyte of one line.
const ROOT_TAG_BYTES: usize = 4096;

/// The name of an XML file's root element, or empty.
///
/// **Why the tree reads files at all.** Because a name is not evidence: Tomcat's per-application
/// context is `context.xml` inside a `.war` and `<appname>.xml` under `conf/Catalina/localhost`,
/// and a Struts module configuration is called whatever `struts.configuration.files` says. An icon
/// keyed on the name is right for the conventional spelling and silent for every other one.
///
/// The cost is one bounded read per XML per tree build — not per keystroke, and not for anything
/// that is not an XML. The prologue is skipped rather than parsed: the declaration, comments and
/// the DOCTYPE all come before the root element and none of them is it.
fn root_tag(path: &Path, name: &str) -> String {
    if !name.rsplit('.').next().is_some_and(|e| e.eq_ignore_ascii_case("xml")) {
        return String::new();
    }
    let Ok(bytes) = read_head(path, ROOT_TAG_BYTES) else { return String::new() };
    first_element(&String::from_utf8_lossy(&bytes))
}

/// The first `ROOT_TAG_BYTES` of a file, or fewer when it is shorter.
fn read_head(path: &Path, limit: usize) -> std::io::Result<Vec<u8>> {
    use std::io::Read;
    let mut file = std::fs::File::open(path)?;
    let mut buffer = vec![0u8; limit];
    let read = file.read(&mut buffer)?;
    buffer.truncate(read);
    Ok(buffer)
}

/// The first real element's name — skipping the declaration, comments and the DOCTYPE.
///
/// Written out rather than handed to the XML scanner because this runs over every XML in the tree
/// and wants to stop at the first tag; a scan of the whole head to use its first entry would be
/// doing the work twice for the same answer.
fn first_element(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() {
        if bytes[i] != b'<' {
            i += 1;
            continue;
        }
        let rest = &text[i..];
        // The three things that are not the root element, each skipped past its own terminator.
        if let Some(skip) = rest.strip_prefix("<!--") {
            match skip.find("-->") {
                Some(end) => i += 4 + end + 3,
                None => return String::new(),
            }
            continue;
        }
        if rest.starts_with("<?") || rest.starts_with("<!") {
            match rest.find('>') {
                Some(end) => i += end + 1,
                None => return String::new(),
            }
            continue;
        }
        // A real tag. Its name ends at whitespace, `/` or `>`; the prefix is kept off, because
        // `<beans:beans>` and `<beans>` are the same document to anybody asking this.
        let name: String = rest[1..]
            .chars()
            .take_while(|c| !c.is_whitespace() && *c != '>' && *c != '/')
            .collect();
        return name.rsplit(':').next().unwrap_or(&name).to_string();
    }
    String::new()
}

/// Whether the platform considers the entry hidden.
///
/// A leading dot everywhere, plus the real attribute on Windows — where `.env` is a
/// perfectly visible file and `Thumbs.db` is not, and the name says neither.
fn is_hidden(path: &Path, name: &str) -> bool {
    if name.starts_with('.') && name != "." && name != ".." {
        return true;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const FILE_ATTRIBUTE_HIDDEN: u32 = 0x2;
        if let Ok(md) = path.metadata() {
            return md.file_attributes() & FILE_ATTRIBUTE_HIDDEN != 0;
        }
    }
    #[cfg(not(windows))]
    let _ = path;
    false
}

#[cfg(test)]
mod tests {
    /// The reason the tree reads XML heads at all: a name is not evidence, and the root element
    /// is the same in every spelling of the file.
    #[test]
    fn the_root_element_is_found_past_everything_that_comes_before_it() {
        assert_eq!(first_element("<Context path=\"/app\"/>"), "Context");
        assert_eq!(
            first_element("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<struts>"),
            "struts"
        );
        assert_eq!(
            first_element(
                "<?xml version=\"1.0\"?>\n<!-- a licence\n  spanning lines -->\n\
                 <!DOCTYPE struts PUBLIC \"-//Apache//DTD\" \"http://x/struts-2.1.dtd\">\n\
                 <struts>\n  <package/>\n</struts>"
            ),
            "struts"
        );
        // A namespaced root is the same document to anybody asking this.
        assert_eq!(first_element("<beans:beans xmlns:beans=\"x\">"), "beans");
        assert_eq!(first_element("<web-app>"), "web-app");
    }

    #[test]
    fn a_head_with_no_element_in_it_answers_nothing_rather_than_guessing() {
        assert_eq!(first_element(""), "");
        assert_eq!(first_element("<?xml version=\"1.0\"?>"), "");
        // An unterminated comment: everything after it is inside the comment, so there is no root.
        assert_eq!(first_element("<!-- never closed <struts>"), "");
        assert_eq!(first_element("not xml at all"), "");
    }

    use super::*;

    struct TempDir(std::path::PathBuf);
    impl TempDir {
        fn new(tag: &str) -> Self {
            let mut p = std::env::temp_dir();
            p.push(format!("bennu-tree-{tag}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&p);
            std::fs::create_dir_all(&p).unwrap();
            Self(p)
        }
        fn write(&self, rel: &str, body: &str) {
            let p = self.0.join(rel);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, body).unwrap();
        }
        fn dir(&self, rel: &str) {
            std::fs::create_dir_all(self.0.join(rel)).unwrap();
        }
    }
    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn find<'a>(node: &'a TreeNode, name: &str) -> Option<&'a TreeNode> {
        if node.name == name {
            return Some(node);
        }
        node.children.iter().find_map(|c| find(c, name))
    }

    #[test]
    fn marks_ignored_entries_without_hiding_them() {
        let t = TempDir::new("mark");
        t.dir(".git");
        t.write(".gitignore", "*.class\ndist/\n");
        t.write("Main.java", "");
        t.write("Main.class", "");
        t.write("dist/app.jar", "");

        let tree = build(&t.0, 8).unwrap();
        assert!(!find(&tree, "Main.java").unwrap().ignored);
        assert!(find(&tree, "Main.class").unwrap().ignored, "gitignored file is marked");
        assert!(find(&tree, "dist").unwrap().ignored, "gitignored dir is marked");
        assert!(
            find(&tree, "app.jar").unwrap().ignored,
            "an entry under an ignored dir inherits the mark — git never descends to \
             re-decide"
        );
    }

    #[test]
    fn marks_dotfiles_hidden() {
        let t = TempDir::new("hidden");
        t.write(".editorconfig", "");
        t.write("pom.xml", "");
        let tree = build(&t.0, 4).unwrap();
        assert!(find(&tree, ".editorconfig").unwrap().hidden);
        assert!(!find(&tree, "pom.xml").unwrap().hidden);
    }

    #[test]
    fn skip_dirs_are_not_walked() {
        let t = TempDir::new("skip");
        t.write("target/classes/A.class", "");
        t.write("src/A.java", "");
        let tree = build(&t.0, 8).unwrap();
        assert!(find(&tree, "target").is_none());
        assert!(find(&tree, "A.java").is_some());
    }

    #[test]
    fn a_file_named_like_a_noise_dir_is_kept() {
        let t = TempDir::new("targetfile");
        t.write("target", "not a directory");
        let tree = build(&t.0, 2).unwrap();
        assert!(find(&tree, "target").is_some());
    }

    #[test]
    fn output_dirs_are_noise_only_beside_a_manifest() {
        let t = TempDir::new("output");
        t.write("pom.xml", "");
        t.write("build/x.class", "");
        t.write("src/build/Pkg.java", "");
        let tree = build(&t.0, 8).unwrap();
        // The tool's output at the project root is skipped; a `build` package under src is source.
        assert!(find(&tree, "x.class").is_none());
        assert!(find(&tree, "Pkg.java").is_some());
    }

    #[test]
    fn the_node_budget_stops_the_walk_and_says_so() {
        let t = TempDir::new("cap");
        for i in 0..5 {
            t.write(&format!("f{i}.txt"), "");
        }
        let mut remaining = 2;
        let mut ignores = IgnoreStack::at(&t.0);
        let node = build_node(&t.0, 4, &mut ignores, false, &mut remaining);
        assert_eq!(node.children.len(), 2);
        assert_eq!(remaining, 0);
    }
}
