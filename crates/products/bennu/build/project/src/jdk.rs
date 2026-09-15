//! Per-project JDK detection (docs §5 #22, §10).
//!
//! Resolves the Java language level for a project, in priority order:
//!
//! 1. an explicit **override** (the config's `jdk_overrides` for this root),
//! 2. `maven.compiler.release` property — the modern single knob, and the one that *wins* when
//!    present, because `javac --release` overrides `-source`/`-target` outright,
//! 3. `maven.compiler.source` property,
//! 4. `maven.compiler.target` property,
//! 5. `java.version` property,
//! 6. the `maven-compiler-plugin` `<release>` / `<source>` / `<target>`,
//! 7. a `<toolchains>` presence (reported as "toolchains" — the exact JDK there is a
//!    later resolution; Phase 0 records that a toolchain governs it),
//! 8. nothing → `None` (unknown; the FE offers an override).
//!
//! ## Multi-module: the reactor is read, not only its root
//!
//! An aggregator pom very often declares no level at all — it exists to list `<modules>` — while
//! each module declares its own. Reading only the root then answered `None` on a project where
//! every module says Java 21, and the backend fell back to its JDK-8 default: `Records require
//! Java 16, but the project targets Java 8`, on code that compiles.
//!
//! So the modules are read too, and the **highest** level any of them declares wins. The index has
//! one language level and the choice is not symmetric: too low invents errors in the module that
//! legitimately uses newer syntax, while too high can only stay silent about an older module using
//! something it should not. A wrong accusation costs more than a missed one — the same trade the
//! rest of the validation makes.
//!
//! ⚠️ `java.version` (5) is not a Maven property — it is the **Spring Boot parent's**
//! convention, and the parent's own `pluginManagement` wires it into the compiler. A Boot
//! project therefore very often declares its level *only* that way. Missing it meant `detect`
//! answering `None`, the backend falling back to its JDK-8 default, and a Java 21 project
//! being told "Records require Java 16, but the project targets Java 8" — a wrong answer
//! delivered with total confidence, on correct code.
//!
//! The version string is reported as declared (`"1.8"` / `"8"` / `"17"`). Multi-JDK
//! selection (rt.jar 8 vs jimage 9+) keys off this string in `bennu-classpath`.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use bennu_proto::prelude::JdkInfo;

use crate::pom::Pom;

/// How deep a reactor is walked looking for a declared level. Aggregators nest two or three deep in
/// the wild; past that the cost is pom reads for an answer nobody is waiting on.
const MAX_DEPTH: usize = 4;

/// Resolve the JDK for the project rooted at `root`, given its pom and any explicit
/// per-project override. `override_version` wins over everything.
///
/// `root` is the directory the pom was read from — the modules are read relative to it. See the
/// module doc for why the reactor is consulted at all and why the highest level wins.
pub fn detect(root: &Path, pom: &Pom, override_version: Option<&str>) -> Option<JdkInfo> {
    if let Some(v) = override_version.filter(|s| !s.is_empty()) {
        return Some(JdkInfo { version: v.to_string(), source: "override".to_string() });
    }
    // The root's own answer is preferred at equal levels: it is the one a reader would look at, and
    // naming a module in the status bar for a level the root also declares would be noise.
    let mut best: Option<(u32, JdkInfo)> = declared(pom).and_then(rank);
    let mut seen: HashSet<PathBuf> = HashSet::new();
    seen.insert(canonical(root));
    walk_modules(root, pom, 1, &mut seen, &mut best);
    if let Some((_, info)) = best {
        return Some(info);
    }
    // Nothing numeric anywhere. A toolchain still tells the reader that something governs the
    // version even though this layer cannot say which — but only as a last resort, since it names
    // no level at all.
    if pom.has_toolchains {
        return Some(JdkInfo { version: "toolchains".to_string(), source: "toolchains".to_string() });
    }
    None
}

/// The level the pom in `dir` declares **on its own** — no reactor walk, no inheritance, no
/// toolchain fallback.
///
/// For a caller that has a file and wants the level of the module it belongs to: walk up from the
/// file's directory and take the first answer, which is exactly how Maven's own inheritance reads.
/// `None` means this pom says nothing, so the question passes to its parent directory.
pub fn module_level(dir: &Path) -> Option<JdkInfo> {
    let xml = std::fs::read_to_string(dir.join("pom.xml")).ok()?;
    declared(&crate::pom::parse(&xml))
}

/// The level this ONE pom declares, and which key said so — the ladder in the module doc, minus the
/// toolchain fallback (which names no level and so cannot be compared with one).
fn declared(pom: &Pom) -> Option<JdkInfo> {
    // Properties, in the order javac's own flags resolve: `--release` beats `-source`/`-target`,
    // and `java.version` is the Spring Boot parent's alias for whichever of them it wires up.
    for key in ["maven.compiler.release", "maven.compiler.source", "maven.compiler.target", "java.version"] {
        if let Some(v) = pom.property(key).filter(|s| !s.is_empty()) {
            return Some(JdkInfo { version: v.to_string(), source: key.to_string() });
        }
    }
    for value in [&pom.compiler_release, &pom.compiler_source, &pom.compiler_target] {
        if let Some(v) = value.as_deref().filter(|s| !s.is_empty()) {
            return Some(JdkInfo { version: v.to_string(), source: "compiler-plugin".to_string() });
        }
    }
    None
}

/// A declared level paired with the number it compares as. `None` for a version this cannot read as
/// a number — an unexpanded `${java.version}`, say, which must not be allowed to win a comparison
/// it has no place in.
fn rank(info: JdkInfo) -> Option<(u32, JdkInfo)> {
    numeric(&info.version).map(|n| (n, info))
}

/// `"1.8"` → 8, `"8"` → 8, `"21"` → 21. `None` for anything else, including a property that was
/// never expanded.
fn numeric(version: &str) -> Option<u32> {
    version.strip_prefix("1.").unwrap_or(version).trim().parse().ok()
}

/// Read every module's pom, depth-first, keeping the highest level found.
fn walk_modules(
    dir: &Path,
    pom: &Pom,
    depth: usize,
    seen: &mut HashSet<PathBuf>,
    best: &mut Option<(u32, JdkInfo)>,
) {
    if depth > MAX_DEPTH {
        return;
    }
    for module in &pom.modules {
        let module = module.trim();
        if module.is_empty() {
            continue;
        }
        // Maven allows a module to name the pom file itself as well as its directory.
        let base = dir.join(module);
        let (module_dir, path) = if base.is_dir() {
            (base.clone(), base.join("pom.xml"))
        } else if base.is_file() {
            (base.parent().map(Path::to_path_buf).unwrap_or_else(|| dir.to_path_buf()), base.clone())
        } else {
            continue;
        };
        if !seen.insert(canonical(&module_dir)) {
            continue; // a cycle, or the same module listed twice
        }
        let Ok(xml) = std::fs::read_to_string(&path) else { continue };
        let child = crate::pom::parse(&xml);
        if let Some((level, info)) = declared(&child).and_then(rank) {
            if best.as_ref().is_none_or(|(top, _)| level > *top) {
                *best = Some((
                    level,
                    // Named by the module it came from: "21" alone in the status bar, on a project
                    // whose root pom says nothing, is an answer with no visible origin.
                    JdkInfo { version: info.version, source: format!("{module}: {}", info.source) },
                ));
            }
        }
        walk_modules(&module_dir, &child, depth + 1, seen, best);
    }
}

/// A path in a form two spellings of the same directory agree on, for the visited set. Falls back to
/// the path as given when it cannot be canonicalized — a module that does not exist is skipped
/// before this, so the fallback only ever covers a permissions oddity.
fn canonical(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}

#[cfg(test)]
mod reactor_tests {
    use super::*;
    use std::fs;

    /// A throwaway reactor on disk: `(relative pom path, xml)`.
    struct Tree(PathBuf);

    impl Tree {
        fn new(files: &[(&str, &str)]) -> Self {
            use std::sync::atomic::{AtomicU64, Ordering};
            static N: AtomicU64 = AtomicU64::new(0);
            let root = std::env::temp_dir().join(format!(
                "bennu-reactor-{}-{}",
                std::process::id(),
                N.fetch_add(1, Ordering::Relaxed)
            ));
            let _ = fs::remove_dir_all(&root);
            for (rel, xml) in files {
                let path = root.join(rel);
                fs::create_dir_all(path.parent().expect("a parent")).expect("mkdir");
                fs::write(&path, xml).expect("write");
            }
            Tree(root)
        }

        /// What `detect` answers for this reactor, reading the root pom the way `open_maven` does.
        fn jdk(&self) -> Option<JdkInfo> {
            let xml = fs::read_to_string(self.0.join("pom.xml")).expect("root pom");
            detect(&self.0, &crate::pom::parse(&xml), None)
        }
    }

    impl Drop for Tree {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn aggregator(modules: &[&str]) -> String {
        let list: String = modules.iter().map(|m| format!("<module>{m}</module>")).collect();
        format!("<project><artifactId>root</artifactId><modules>{list}</modules></project>")
    }

    fn module(level: &str) -> String {
        format!(
            "<project><artifactId>m</artifactId><properties>             <maven.compiler.release>{level}</maven.compiler.release></properties></project>"
        )
    }

    /// The reported bug: an aggregator that declares no level, three modules that all say 21, and
    /// the answer was `None` — so the backend fell back to Java 8 and reported records as a syntax
    /// error on a project that compiles.
    #[test]
    fn an_aggregator_with_no_level_takes_it_from_its_modules() {
        let tree = Tree::new(&[
            ("pom.xml", &aggregator(&["api", "service", "web"])),
            ("api/pom.xml", &module("21")),
            ("service/pom.xml", &module("21")),
            ("web/pom.xml", &module("21")),
        ]);
        let jdk = tree.jdk().expect("the modules declare a level");
        assert_eq!(jdk.version, "21");
        // And it says WHERE it came from: "21" with no visible origin is an answer nobody can check.
        assert!(jdk.source.contains("maven.compiler.release"), "{}", jdk.source);
    }

    /// One index, one language level. Too low invents errors in the module that legitimately uses
    /// newer syntax; too high can only stay silent about an older one.
    #[test]
    fn the_highest_level_in_the_reactor_wins() {
        let tree = Tree::new(&[
            ("pom.xml", &aggregator(&["old", "new"])),
            ("old/pom.xml", &module("1.8")),
            ("new/pom.xml", &module("21")),
        ]);
        assert_eq!(tree.jdk().expect("a level").version, "21");
    }

    /// `1.8` and `8` are the same level written two ways, and comparing them as strings makes `8`
    /// look bigger than `21`.
    #[test]
    fn the_old_spelling_compares_as_the_number_it_is() {
        let tree = Tree::new(&[
            ("pom.xml", &aggregator(&["a", "b"])),
            ("a/pom.xml", &module("1.8")),
            ("b/pom.xml", &module("11")),
        ]);
        assert_eq!(tree.jdk().expect("a level").version, "11");
    }

    /// The root's own answer is what a reader looks at, so at equal levels it is not replaced by a
    /// module's — the status bar should not name a module for something the root also says.
    #[test]
    fn the_root_keeps_its_own_answer_at_the_same_level() {
        let tree = Tree::new(&[
            (
                "pom.xml",
                "<project><artifactId>root</artifactId>                 <properties><maven.compiler.release>17</maven.compiler.release></properties>                 <modules><module>api</module></modules></project>",
            ),
            ("api/pom.xml", &module("17")),
        ]);
        let jdk = tree.jdk().expect("a level");
        assert_eq!(jdk.version, "17");
        assert_eq!(jdk.source, "maven.compiler.release", "the root's own key, not a module's");
    }

    /// But a module that really does ask for more overrides it — that module's sources are the ones
    /// that would fail to parse.
    #[test]
    fn a_module_above_the_root_still_wins() {
        let tree = Tree::new(&[
            (
                "pom.xml",
                "<project><artifactId>root</artifactId>                 <properties><maven.compiler.release>17</maven.compiler.release></properties>                 <modules><module>api</module></modules></project>",
            ),
            ("api/pom.xml", &module("21")),
        ]);
        let jdk = tree.jdk().expect("a level");
        assert_eq!(jdk.version, "21");
        assert!(jdk.source.starts_with("api:"), "{}", jdk.source);
    }

    /// A nested aggregator is a reactor too.
    #[test]
    fn a_module_that_is_itself_an_aggregator_is_walked_through() {
        let tree = Tree::new(&[
            ("pom.xml", &aggregator(&["group"])),
            ("group/pom.xml", &aggregator(&["leaf"])),
            ("group/leaf/pom.xml", &module("21")),
        ]);
        assert_eq!(tree.jdk().expect("a level").version, "21");
    }

    /// A module listed but not on disk is skipped, not an error — a reactor mid-checkout is a
    /// normal state, and the level the other modules declare is still the right answer.
    #[test]
    fn a_missing_module_is_skipped() {
        let tree = Tree::new(&[
            ("pom.xml", &aggregator(&["gone", "here"])),
            ("here/pom.xml", &module("21")),
        ]);
        assert_eq!(tree.jdk().expect("a level").version, "21");
    }

    /// Nothing anywhere is still `None`: the FE offers an override, which is better than a guess.
    #[test]
    fn a_reactor_that_declares_nothing_is_still_unknown() {
        let tree = Tree::new(&[
            ("pom.xml", &aggregator(&["api"])),
            ("api/pom.xml", "<project><artifactId>api</artifactId></project>"),
        ]);
        assert!(tree.jdk().is_none());
    }

    /// An override is the user's word and is not put to a vote.
    #[test]
    fn an_override_beats_every_module() {
        let tree = Tree::new(&[
            ("pom.xml", &aggregator(&["api"])),
            ("api/pom.xml", &module("21")),
        ]);
        let xml = fs::read_to_string(tree.0.join("pom.xml")).expect("root pom");
        let jdk = detect(&tree.0, &crate::pom::parse(&xml), Some("11")).expect("the override");
        assert_eq!(jdk.version, "11");
        assert_eq!(jdk.source, "override");
    }

    /// A property that was never expanded is not a level, and must not win a comparison it has no
    /// place in — `${java.version}` sorting above `21` would be a level nobody declared.
    #[test]
    fn an_unexpanded_property_is_not_a_level() {
        let tree = Tree::new(&[
            ("pom.xml", &aggregator(&["a", "b"])),
            ("a/pom.xml", &module("${java.version}")),
            ("b/pom.xml", &module("21")),
        ]);
        assert_eq!(tree.jdk().expect("a level").version, "21");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pom;

    /// A directory with no modules on disk — every single-pom case below is about the pom alone.
    const NO_TREE: &str = "/nonexistent-bennu-test";

    #[test]
    fn override_wins() {
        let p = pom::parse(
            "<project><properties><maven.compiler.source>1.8\
             </maven.compiler.source></properties></project>",
        );
        let jdk = detect(Path::new(NO_TREE), &p, Some("17")).unwrap();
        assert_eq!(jdk.version, "17");
        assert_eq!(jdk.source, "override");
    }

    #[test]
    fn reads_maven_compiler_source() {
        let p = pom::parse(
            "<project><properties><maven.compiler.source>1.8\
             </maven.compiler.source><maven.compiler.target>1.8\
             </maven.compiler.target></properties></project>",
        );
        let jdk = detect(Path::new(NO_TREE), &p, None).unwrap();
        assert_eq!(jdk.version, "1.8");
        assert_eq!(jdk.source, "maven.compiler.source");
    }

    #[test]
    fn none_when_nothing_declared() {
        let p = pom::parse("<project></project>");
        assert!(detect(Path::new(NO_TREE), &p, None).is_none());
    }

    /// The reported bug: a Spring Boot pom declares its level ONLY as `<java.version>`, and
    /// detection answered `None` — so the backend fell back to JDK 8 and told a Java 21 project
    /// that records need Java 16.
    #[test]
    fn reads_spring_boot_java_version() {
        let p = pom::parse(
            "<project><properties><java.version>21</java.version></properties></project>",
        );
        let jdk = detect(Path::new(NO_TREE), &p, None).expect("java.version must resolve a level");
        assert_eq!(jdk.version, "21");
        assert_eq!(jdk.source, "java.version");
    }

    /// `--release` is javac's override of `-source`/`-target`, so it wins over both.
    #[test]
    fn release_wins_over_source_and_target() {
        let p = pom::parse(
            "<project><properties>\
               <maven.compiler.source>1.8</maven.compiler.source>\
               <maven.compiler.target>1.8</maven.compiler.target>\
               <maven.compiler.release>17</maven.compiler.release>\
             </properties></project>",
        );
        let jdk = detect(Path::new(NO_TREE), &p, None).unwrap();
        assert_eq!(jdk.version, "17");
        assert_eq!(jdk.source, "maven.compiler.release");
    }

    /// An explicit `maven.compiler.*` outranks `java.version`: the Boot property is a
    /// convention the parent wires up, so a project that sets both meant the specific one.
    #[test]
    fn explicit_compiler_property_outranks_java_version() {
        let p = pom::parse(
            "<project><properties>\
               <java.version>21</java.version>\
               <maven.compiler.source>11</maven.compiler.source>\
             </properties></project>",
        );
        assert_eq!(detect(Path::new(NO_TREE), &p, None).unwrap().version, "11");
    }

    /// The plugin's own `<release>` counts too, for a pom that configures it directly.
    #[test]
    fn reads_the_compiler_plugin_release_element() {
        let p = pom::parse(
            "<project><build><plugins><plugin>\
               <artifactId>maven-compiler-plugin</artifactId>\
               <configuration><release>17</release></configuration>\
             </plugin></plugins></build></project>",
        );
        let jdk = detect(Path::new(NO_TREE), &p, None).unwrap();
        assert_eq!(jdk.version, "17");
        assert_eq!(jdk.source, "compiler-plugin");
    }
}
