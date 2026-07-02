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
//!   - **Tiles files**: `*tiles*.xml` under `src/main/webapp`.

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

    WebInputs { struts_roots, resource_roots, spring_files, tiles_files }
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
        std::fs::write(web.join("tiles.xml"), "<tiles-definitions/>").unwrap();

        let inputs = discover_web_inputs(&dir);
        assert_eq!(inputs.struts_roots.len(), 2, "struts.xml + plugin fragment");
        assert_eq!(inputs.spring_files.len(), 1, "only the <beans xml");
        assert_eq!(inputs.tiles_files.len(), 1);
        assert!(inputs.resource_roots.iter().any(|p| p.ends_with("resources")));

        let _ = std::fs::remove_dir_all(&dir);
    }
}
