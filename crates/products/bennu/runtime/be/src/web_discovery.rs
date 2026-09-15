//! Discover the config-graph inputs for a project: which struts roots, which Spring
//! bean XMLs, which Tiles files, and the classpath resource roots for `<include>`
//! resolution. `bennu-web` parses+resolves; this be-layer helper is the filesystem walk
//! it deliberately does NOT do (it stays a leaf that depends only on `bennu-index`).
//!
//! Heuristics match the real Entando/jAPS layout validated on PortaleAppalti:
//!   - **struts roots**: `struts.xml`, any `*-struts-plugin.xml`, `eldasoft-struts.xml`
//!     under `src/main/resources` (each plugin fragment is its own include root on the
//!     Entando classpath merge);
//!   - **resource roots**: `src/main/resources` + `src/main/webapp/WEB-INF`;
//!   - **Spring bean files**: every `.xml` whose text has a `<beans` root, project-wide;
//!   - **Tiles files**: `*tiles*.xml` under `src/main/webapp`;
//!   - **Validation files**: `*-validation.xml`, project-wide (next to the action class).

use std::path::{Path, PathBuf};

use bennu_web::prelude::WebInputs;

/// Build [`WebInputs`] for the project rooted at `root` by walking its module tree.
pub fn discover_web_inputs(root: &Path) -> WebInputs {
    let resources = root.join("src/main/resources");
    let webapp = root.join("src/main/webapp");

    let struts_roots = find_files(&resources, &|n| {
        n == "struts.xml" || n.ends_with("-struts-plugin.xml") || n == "eldasoft-struts.xml"
    });

    let resource_roots = vec![resources.clone(), webapp.join("WEB-INF")];

    // Spring bean files: any xml with a `<beans` root, across the whole project.
    let spring_files = find_files(root, &|n| n.ends_with(".xml"))
        .into_iter()
        .filter(|p| std::fs::read_to_string(p).map(|t| t.contains("<beans")).unwrap_or(false))
        .collect();

    let tiles_files = find_files(&webapp, &|n| n.contains("tiles") && n.ends_with(".xml"));

    // Validation rulesets: `<Action>-validation.xml`, project-wide (they sit next to the
    // action class under src/main/java or mirror the package under src/main/resources).
    let validation_files = find_files(root, &|n| n.ends_with("-validation.xml"));

    // MyBatis mapper XMLs: any `.xml` whose ROOT is `<mapper namespace=…>`, project-wide
    // (they live under src/main/resources mirroring the interface package, sometimes
    // src/main/java). The parser doubles as the sniff — it returns `None`/empty for a
    // non-`<mapper>` root — so there's no separate heuristic to drift.
    let mapper_files = find_files(root, &|n| n.ends_with(".xml"))
        .into_iter()
        .filter(|p| {
            bennu_web::prelude::parse_mybatis_file(p).map(|m| !m.mappers.is_empty()).unwrap_or(false)
        })
        .collect();

    WebInputs {
        struts_roots,
        resource_roots,
        spring_files,
        tiles_files,
        validation_files,
        mapper_files,
    }
}

/// Discover the project's JSP-family files (`*.jsp` / `*.jspf` / `*.tag` / `*.tagx`),
/// project-wide. Used by action find-usages (which JSPs reference a given `<action>`).
pub fn discover_jsp_files(root: &Path) -> Vec<PathBuf> {
    find_files(root, &|n| is_jsp_family(n))
}

/// Whether a file NAME is a JSP-family page (`.jsp` / `.jspf` / `.tag` / `.tagx`), case-insensitive.
pub fn is_jsp_family(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    n.ends_with(".jsp") || n.ends_with(".jspf") || n.ends_with(".tag") || n.ends_with(".tagx")
}

/// Candidate **webapp source** directories, relative to a project root — the folder that holds the
/// JSPs + `WEB-INF` (Maven's `src/main/webapp` first, then the common legacy layouts). Ordered by
/// precedence; the FIRST that exists on disk is the project's webapp root.
pub const WEBAPP_DIR_CANDIDATES: &[&str] =
    &["src/main/webapp", "web", "WebContent", "webapp", "src/webapp", "WebRoot"];

/// Every existing webapp source directory under `root` (in precedence order). Empty when the
/// project has no recognizable webapp layout.
pub fn webapp_dirs(root: &Path) -> Vec<PathBuf> {
    WEBAPP_DIR_CANDIDATES.iter().map(|b| root.join(b)).filter(|p| p.is_dir()).collect()
}

/// The project's PRIMARY webapp source directory (the first existing candidate) — where its JSPs
/// live. `None` when the project isn't a web app (no `src/main/webapp` &co.).
pub fn source_webapp_dir(root: &Path) -> Option<PathBuf> {
    webapp_dirs(root).into_iter().next()
}

/// Recursively collect files under `dir` whose file name matches `matcher`, skipping
/// `target` / `.git` / hidden dirs. A missing `dir` yields an empty vec (non-fatal).
fn find_files(dir: &Path, matcher: &dyn Fn(&str) -> bool) -> Vec<PathBuf> {
    let mut out = Vec::new();
    collect(dir, matcher, &mut out);
    out
}

fn collect(dir: &Path, matcher: &dyn Fn(&str) -> bool, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for e in entries.flatten() {
        let path = e.path();
        let name = e.file_name().to_string_lossy().into_owned();
        if path.is_dir() {
            if name == "target" || name == ".git" || name.starts_with('.') {
                continue;
            }
            collect(&path, matcher, out);
        } else if matcher(&name) {
            out.push(path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discovers_struts_spring_tiles_by_layout() {
        let dir = std::env::temp_dir().join(format!("bennu-webdisc-{}", std::process::id()));
        let res = dir.join("src/main/resources/com/x");
        let web = dir.join("src/main/webapp/WEB-INF");
        std::fs::create_dir_all(&res).unwrap();
        std::fs::create_dir_all(&web).unwrap();
        std::fs::write(res.join("struts.xml"), "<struts/>").unwrap();
        std::fs::write(res.join("foo-struts-plugin.xml"), "<struts/>").unwrap();
        std::fs::write(res.join("applicationContext.xml"), "<beans></beans>").unwrap();
        std::fs::write(res.join("not-a-bean.xml"), "<other/>").unwrap();
        std::fs::write(res.join("LoginAction-validation.xml"), "<validators/>").unwrap();
        std::fs::write(web.join("tiles.xml"), "<tiles-definitions/>").unwrap();
        std::fs::write(
            res.join("FooMapper.xml"),
            r#"<mapper namespace="com.x.FooMapper"><select id="a">x</select></mapper>"#,
        )
        .unwrap();

        let inputs = discover_web_inputs(&dir);
        assert_eq!(inputs.struts_roots.len(), 2, "struts.xml + plugin fragment");
        assert_eq!(inputs.spring_files.len(), 1, "only the <beans xml");
        assert_eq!(inputs.tiles_files.len(), 1);
        assert_eq!(inputs.validation_files.len(), 1, "the -validation.xml file");
        // only the `<mapper namespace=…>` root matches — not-a-bean.xml / tiles.xml don't.
        assert_eq!(inputs.mapper_files.len(), 1, "the <mapper namespace root");
        assert!(inputs.resource_roots.iter().any(|p| p.ends_with("resources")));

        let _ = std::fs::remove_dir_all(&dir);
    }
}
