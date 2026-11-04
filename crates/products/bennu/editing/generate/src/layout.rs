//! Where a generated file lands: the module's `src/main/java` / `src/test/java` trees, found from the
//! file the caret is in (or the project root), and the package's folders beneath them.

use crate::output::{GenFile, SourceRoot};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceRoots {
    pub main: String,
    pub test: String,
}

fn slashes(path: &str) -> String {
    path.replace('\\', "/")
}

/// The source roots of the module `file` belongs to; without a file (or outside a standard layout), of `project_root`.
pub fn source_roots(project_root: &str, file: Option<&str>) -> SourceRoots {
    let module = file
        .map(slashes)
        .and_then(|f| ["/src/main/java", "/src/test/java"].iter().find_map(|marker| f.find(marker).map(|at| f[..at].to_string())))
        .unwrap_or_else(|| slashes(project_root).trim_end_matches('/').to_string());
    SourceRoots { main: format!("{module}/src/main/java"), test: format!("{module}/src/test/java") }
}

/// The full path of `file`; `beside` is the file the caret is in, for output that sits next to it.
pub fn path_of(roots: &SourceRoots, file: &GenFile, beside: Option<&str>) -> String {
    let base = match file.root {
        SourceRoot::Main => roots.main.clone(),
        SourceRoot::Test => roots.test.clone(),
        SourceRoot::Beside => beside.map(slashes).and_then(|b| b.rsplit_once('/').map(|(dir, _)| dir.to_string())).unwrap_or_default(),
    };
    let package = file.package.replace('.', "/");
    match (base.is_empty(), package.is_empty()) {
        (true, _) => file.name.clone(),
        (false, true) => format!("{base}/{}", file.name),
        (false, false) => format!("{base}/{package}/{}", file.name),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(root: SourceRoot, package: &str) -> GenFile {
        GenFile { root, package: package.into(), name: "A.java".into(), text: String::new(), language: "java" }
    }

    #[test]
    fn roots_come_from_the_module_of_the_open_file() {
        let roots = source_roots(r"C:\p", Some(r"C:\p\api\src\main\java\com\acme\A.java"));
        assert_eq!(roots, SourceRoots { main: "C:/p/api/src/main/java".into(), test: "C:/p/api/src/test/java".into() });
    }

    #[test]
    fn a_test_file_has_the_same_module() {
        let roots = source_roots("/p", Some("/p/app/src/test/java/a/ATest.java"));
        assert_eq!(roots.main, "/p/app/src/main/java");
    }

    #[test]
    fn without_a_file_the_project_root_is_the_module() {
        let roots = source_roots("/p/", None);
        assert_eq!(roots.main, "/p/src/main/java");
        assert_eq!(roots.test, "/p/src/test/java");
    }

    #[test]
    fn paths_follow_the_package_and_the_tree() {
        let roots = source_roots("/p", None);
        assert_eq!(path_of(&roots, &file(SourceRoot::Main, "com.acme.config"), None), "/p/src/main/java/com/acme/config/A.java");
        assert_eq!(path_of(&roots, &file(SourceRoot::Test, "com.acme"), None), "/p/src/test/java/com/acme/A.java");
        assert_eq!(path_of(&roots, &file(SourceRoot::Main, ""), None), "/p/src/main/java/A.java");
        assert_eq!(path_of(&roots, &file(SourceRoot::Beside, ""), Some("/p/web/ui.ts")), "/p/web/A.java");
    }
}
