//! Per-project JDK detection (docs §5 #22, §10).
//!
//! Resolves the Java language level for a project, in priority order:
//!
//! 1. an explicit **override** (the config's `jdk_overrides` for this root),
//! 2. `maven.compiler.source` property,
//! 3. `maven.compiler.target` property,
//! 4. the `maven-compiler-plugin` `<source>` / `<target>`,
//! 5. a `<toolchains>` presence (reported as "toolchains" — the exact JDK there is a
//!    later resolution; Phase 0 records that a toolchain governs it),
//! 6. nothing → `None` (unknown; the FE offers an override).
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
    if let Some(v) = pom.property("maven.compiler.source").filter(|s| !s.is_empty()) {
        return Some(JdkInfo { version: v.to_string(), source: "maven.compiler.source".to_string() });
    }
    if let Some(v) = pom.property("maven.compiler.target").filter(|s| !s.is_empty()) {
        return Some(JdkInfo { version: v.to_string(), source: "maven.compiler.target".to_string() });
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
}
