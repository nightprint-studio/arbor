//! `settings.gradle` / `settings.gradle.kts` — which subprojects the build has.
//!
//! This is the file that makes Gradle a *multi-project* build: `include ':api', ':web:client'`
//! names the subprojects, `rootProject.name` names the whole, and a handful of less common
//! statements move a project's directory or add a version catalog. It is the Gradle counterpart of
//! a pom's `<modules>`, and it is read the same way the rest of this crate reads a script — the
//! declarative statements only.

use std::collections::BTreeMap;

use crate::script::{literals, Script};

/// What a settings script declares.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Settings {
    /// `rootProject.name`; empty when the script does not set it (the directory name then applies).
    pub root_name: String,
    /// Every included project path (`:api`, `:web:client`), in declaration order, with the
    /// intermediate paths a nested include implies (`:web` for `:web:client`).
    pub includes: Vec<String>,
    /// Projects whose directory is not the one their path implies — `project(':x').projectDir = …`
    /// and `includeFlat`. Value is the directory relative to the root.
    pub project_dirs: BTreeMap<String, String>,
    /// `includeBuild("…")` — composite builds, as written.
    pub include_builds: Vec<String>,
    /// `(catalog name, file relative to the root)` for catalogs declared in `versionCatalogs {}`.
    pub catalogs: Vec<(String, String)>,
}

/// `a`, `:a`, `a:b` → `:a`, `:a`, `:a:b`.
pub fn project_path(raw: &str) -> String {
    let t = raw.trim();
    if t.starts_with(':') {
        t.to_string()
    } else {
        format!(":{t}")
    }
}

impl Settings {
    pub fn parse(text: &str) -> Settings {
        let script = Script::new(text);
        let mut out = Settings::default();

        for at in script.find_word("rootProject") {
            let end = script.statement_end(at);
            let stmt = script.text(at..end);
            if !stmt.trim_start().starts_with("rootProject.name") {
                continue;
            }
            if let Some(name) = stmt.split_once('=').and_then(|(_, rhs)| literals(rhs).into_iter().next()) {
                out.root_name = name.text;
            }
        }

        for at in script.find_word("include") {
            let end = script.statement_end(at);
            for lit in literals(script.text(at..end)) {
                out.add_include(&lit.text);
            }
        }
        for at in script.find_word("includeFlat") {
            let end = script.statement_end(at);
            for lit in literals(script.text(at..end)) {
                let path = project_path(&lit.text);
                out.add_include(&path);
                out.project_dirs.insert(path, format!("../{}", lit.text.trim().trim_start_matches(':')));
            }
        }
        for at in script.find_word("includeBuild") {
            let end = script.statement_end(at);
            if let Some(lit) = literals(script.text(at..end)).into_iter().next() {
                out.include_builds.push(lit.text);
            }
        }

        out.read_project_dirs(&script);
        out.read_catalogs(&script);
        out
    }

    /// Record `path` and every ancestor path it implies, once each.
    fn add_include(&mut self, raw: &str) {
        let path = project_path(raw);
        let mut prefix = String::new();
        for segment in path.split(':').filter(|s| !s.is_empty()) {
            prefix.push(':');
            prefix.push_str(segment);
            if !self.includes.contains(&prefix) {
                self.includes.push(prefix.clone());
            }
        }
    }

    /// `project(':x').projectDir = file('…')` / `File(rootDir, '…')` / `new File('…')`.
    fn read_project_dirs(&mut self, script: &Script) {
        let code = script.text(script.all());
        let mut from = 0;
        while let Some(rel) = code[from..].find(".projectDir") {
            let at = from + rel;
            from = at + ".projectDir".len();
            let line_start = code[..at].rfind('\n').map(|p| p + 1).unwrap_or(0);
            let head = &code[line_start..at];
            let Some(path_lit) = literals(head).into_iter().next() else { continue };
            let end = script.statement_end(at);
            let rhs = match code.get(from..end).and_then(|s| s.split_once('=')) {
                Some((_, rhs)) => rhs,
                None => continue,
            };
            // The last literal is the relative part in every spelling of "a file under here".
            let Some(dir) = literals(rhs).into_iter().last() else { continue };
            self.project_dirs.insert(project_path(&path_lit.text), dir.text);
        }
    }

    /// `versionCatalogs { create("libs") { from(files("gradle/libs.versions.toml")) } }`.
    fn read_catalogs(&mut self, script: &Script) {
        for block in script.blocks(script.all()) {
            match block.head.as_str() {
                "versionCatalogs" => self.read_catalog_block(script, block.body()),
                // `dependencyResolutionManagement { versionCatalogs { … } }` nests one deeper.
                "dependencyResolutionManagement" => {
                    for inner in script.blocks(block.body()) {
                        if inner.head == "versionCatalogs" {
                            self.read_catalog_block(script, inner.body());
                        }
                    }
                }
                _ => {}
            }
        }
    }

    fn read_catalog_block(&mut self, script: &Script, body: std::ops::Range<usize>) {
        for catalog in script.blocks(body) {
            let name = match literals(&catalog.head).into_iter().next() {
                Some(l) => l.text,
                None => continue,
            };
            let text = script.text(catalog.body());
            let Some(files_at) = text.find("files(") else { continue };
            if let Some(file) = literals(&text[files_at..]).into_iter().next() {
                self.catalogs.push((name, file.text));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn groovy_and_kotlin_includes_read_the_same() {
        let groovy = Settings::parse("rootProject.name = 'shop'\ninclude ':api', 'web:client'\n");
        let kotlin = Settings::parse("rootProject.name = \"shop\"\ninclude(\":api\", \"web:client\")\n");
        assert_eq!(groovy, kotlin);
        assert_eq!(groovy.root_name, "shop");
        assert_eq!(groovy.includes, [":api", ":web", ":web:client"]);
    }

    #[test]
    fn a_commented_include_is_not_an_include() {
        let s = Settings::parse("// include 'old'\ninclude 'new'\n/* include 'older' */");
        assert_eq!(s.includes, [":new"]);
    }

    #[test]
    fn include_continues_across_lines() {
        let s = Settings::parse("include 'a',\n        'b'\n");
        assert_eq!(s.includes, [":a", ":b"]);
    }

    #[test]
    fn a_moved_project_dir_is_recorded() {
        let s = Settings::parse("include ':legacy'\nproject(':legacy').projectDir = new File(rootDir, 'old/legacy')\n");
        assert_eq!(s.project_dirs.get(":legacy").map(String::as_str), Some("old/legacy"));
    }

    #[test]
    fn a_custom_catalog_is_found() {
        let s = Settings::parse(
            "dependencyResolutionManagement {\n  versionCatalogs {\n    create(\"tools\") {\n      from(files(\"gradle/tools.versions.toml\"))\n    }\n  }\n}\n",
        );
        assert_eq!(s.catalogs, [("tools".to_string(), "gradle/tools.versions.toml".to_string())]);
    }
}
