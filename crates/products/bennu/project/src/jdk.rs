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
//! ⚠️ `java.version` (5) is not a Maven property — it is the **Spring Boot parent's**
//! convention, and the parent's own `pluginManagement` wires it into the compiler. A Boot
//! project therefore very often declares its level *only* that way. Missing it meant `detect`
//! answering `None`, the backend falling back to its JDK-8 default, and a Java 21 project
//! being told "Records require Java 16, but the project targets Java 8" — a wrong answer
//! delivered with total confidence, on correct code.
//!
//! The version string is reported as declared (`"1.8"` / `"8"` / `"17"`). Multi-JDK
//! selection (rt.jar 8 vs jimage 9+) keys off this string in `bennu-classpath`.

use bennu_proto::prelude::JdkInfo;

use crate::pom::Pom;

/// Resolve the JDK for the project rooted at `root`, given its pom and any explicit
/// per-project override. `override_version` wins over everything.
pub fn detect(pom: &Pom, override_version: Option<&str>) -> Option<JdkInfo> {
    if let Some(v) = override_version.filter(|s| !s.is_empty()) {
        return Some(JdkInfo { version: v.to_string(), source: "override".to_string() });
    }
    // Properties, in the order javac's own flags resolve: `--release` beats `-source`/`-target`,
    // and `java.version` is the Spring Boot parent's alias for whichever of them it wires up.
    for key in ["maven.compiler.release", "maven.compiler.source", "maven.compiler.target", "java.version"] {
        if let Some(v) = pom.property(key).filter(|s| !s.is_empty()) {
            return Some(JdkInfo { version: v.to_string(), source: key.to_string() });
        }
    }
    if let Some(v) = pom.compiler_release.as_deref().filter(|s| !s.is_empty()) {
        return Some(JdkInfo { version: v.to_string(), source: "compiler-plugin".to_string() });
    }
    if let Some(v) = pom.compiler_source.as_deref().filter(|s| !s.is_empty()) {
        return Some(JdkInfo { version: v.to_string(), source: "compiler-plugin".to_string() });
    }
    if let Some(v) = pom.compiler_target.as_deref().filter(|s| !s.is_empty()) {
        return Some(JdkInfo { version: v.to_string(), source: "compiler-plugin".to_string() });
    }
    if pom.has_toolchains {
        // The concrete JDK behind a toolchain is resolved later; Phase 0 records that
        // a toolchain governs the version rather than guessing one.
        return Some(JdkInfo { version: "toolchains".to_string(), source: "toolchains".to_string() });
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pom;

    #[test]
    fn override_wins() {
        let p = pom::parse(
            "<project><properties><maven.compiler.source>1.8\
             </maven.compiler.source></properties></project>",
        );
        let jdk = detect(&p, Some("17")).unwrap();
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
        let jdk = detect(&p, None).unwrap();
        assert_eq!(jdk.version, "1.8");
        assert_eq!(jdk.source, "maven.compiler.source");
    }

    #[test]
    fn none_when_nothing_declared() {
        let p = pom::parse("<project></project>");
        assert!(detect(&p, None).is_none());
    }

    /// The reported bug: a Spring Boot pom declares its level ONLY as `<java.version>`, and
    /// detection answered `None` — so the backend fell back to JDK 8 and told a Java 21 project
    /// that records need Java 16.
    #[test]
    fn reads_spring_boot_java_version() {
        let p = pom::parse(
            "<project><properties><java.version>21</java.version></properties></project>",
        );
        let jdk = detect(&p, None).expect("java.version must resolve a level");
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
        let jdk = detect(&p, None).unwrap();
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
        assert_eq!(detect(&p, None).unwrap().version, "11");
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
        let jdk = detect(&p, None).unwrap();
        assert_eq!(jdk.version, "17");
        assert_eq!(jdk.source, "compiler-plugin");
    }
}
