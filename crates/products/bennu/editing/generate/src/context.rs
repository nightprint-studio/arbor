//! What a generator is told about where it runs: the project's dependencies and Java level, the class
//! at the caret, the Spring endpoints of the app, and the shapes of the types they mention.
//!
//! Plain data, filled by the backend from the index — the generators never read a file, so every one of
//! them is exercised in tests with a hand-built context.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// `major.minor.patch`, read tolerantly from `3.2.1`, `6.1.0-M1`, `2.7.18.RELEASE`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Version {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
}

impl Version {
    pub fn new(major: u32, minor: u32, patch: u32) -> Self {
        Version { major, minor, patch }
    }

    pub fn parse(text: &str) -> Option<Version> {
        let mut parts = text.trim().split(|c: char| !c.is_ascii_digit()).filter(|p| !p.is_empty());
        let major = parts.next()?.parse().ok()?;
        let minor = parts.next().and_then(|p| p.parse().ok()).unwrap_or(0);
        let patch = parts.next().and_then(|p| p.parse().ok()).unwrap_or(0);
        Some(Version { major, minor, patch })
    }

    pub fn at_least(&self, major: u32, minor: u32) -> bool {
        (self.major, self.minor) >= (major, minor)
    }
}

/// The project a generator writes for.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ProjectInfo {
    /// The Java level; `0` while unknown.
    pub java: u32,
    /// Dependencies by `group:artifact`, with their version.
    pub versions: BTreeMap<String, String>,
    /// The classpath has been resolved. Until then nothing is reported missing.
    pub resolved: bool,
}

impl ProjectInfo {
    pub fn new(java: u32, deps: &[(&str, &str)]) -> Self {
        ProjectInfo {
            java,
            versions: deps.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect(),
            resolved: !deps.is_empty(),
        }
    }

    /// A dependency named by `group:artifact`, or by its artifact alone.
    pub fn has(&self, name: &str) -> bool {
        self.find(name).is_some()
    }

    pub fn version(&self, name: &str) -> Option<Version> {
        self.find(name).and_then(|v| Version::parse(v))
    }

    fn find(&self, name: &str) -> Option<&String> {
        let name = name.trim().to_ascii_lowercase();
        self.versions.iter().find_map(|(id, version)| {
            let id = id.to_ascii_lowercase();
            let artifact = id.rsplit(':').next().unwrap_or(&id);
            (id == name || artifact == name).then_some(version)
        })
    }

    /// The Java level, assuming a current one while it is unknown.
    pub fn java_level(&self) -> u32 {
        if self.java == 0 { 17 } else { self.java }
    }

    pub fn spring_boot(&self) -> Option<Version> {
        self.version("spring-boot")
    }

    pub fn spring_security(&self) -> Option<Version> {
        self.version("spring-security-web").or_else(|| self.version("spring-security-core"))
    }

    pub fn lombok(&self) -> bool {
        self.has("lombok")
    }

    /// `jakarta.*` rather than `javax.*` for the servlet and persistence APIs.
    pub fn jakarta(&self) -> bool {
        if self.spring_boot().is_some_and(|v| v.major >= 3) {
            return true;
        }
        // Jakarta EE 8 artifacts (persistence 2.x, servlet 4.x) still use the javax namespace.
        for (artifact, first_jakarta) in [("jakarta.persistence-api", 3), ("jakarta.servlet-api", 5), ("jakarta.annotation-api", 2)] {
            if let Some(v) = self.version(artifact) {
                return v.major >= first_jakarta;
            }
        }
        if self.has("javax.persistence-api") || self.has("javax.servlet-api") || self.has("hibernate-jpa-2.1-api") {
            return false;
        }
        if self.spring_boot().is_some_and(|v| v.major == 2) {
            return false;
        }
        self.java_level() >= 17
    }

    /// `"jakarta"` or `"javax"`.
    pub fn ee(&self) -> &'static str {
        if self.jakarta() { "jakarta" } else { "javax" }
    }

    /// Records need Java 16.
    pub fn records(&self) -> bool {
        self.java_level() >= 16
    }

    /// Dependencies from `artifacts` the classpath lacks — empty while it is unresolved.
    pub fn missing(&self, artifacts: &[&str]) -> Vec<String> {
        if !self.resolved {
            return Vec::new();
        }
        artifacts.iter().filter(|a| !self.has(a.rsplit(':').next().unwrap_or(a))).map(|a| a.to_string()).collect()
    }
}

/// One instance field of a class.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FieldInfo {
    pub name: String,
    /// As written: `List<String>`.
    pub type_name: String,
    #[serde(default)]
    pub is_final: bool,
    #[serde(default)]
    pub id: bool,
    /// Annotation simple names.
    #[serde(default)]
    pub annotations: Vec<String>,
    #[serde(default)]
    pub getter: Option<String>,
    #[serde(default)]
    pub setter: Option<String>,
}

impl FieldInfo {
    pub fn new(name: &str, type_name: &str) -> Self {
        FieldInfo { name: name.to_string(), type_name: type_name.to_string(), ..Default::default() }
    }

    pub fn has_annotation(&self, name: &str) -> bool {
        self.annotations.iter().any(|a| a == name)
    }
}

/// A class, record, enum or interface.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ClassInfo {
    pub name: String,
    pub package: String,
    /// `class` | `record` | `interface` | `enum`.
    pub kind: String,
    #[serde(default)]
    pub annotations: Vec<String>,
    #[serde(default)]
    pub fields: Vec<FieldInfo>,
    /// Enum constants.
    #[serde(default)]
    pub constants: Vec<String>,
    /// Method names it declares.
    #[serde(default)]
    pub methods: Vec<String>,
    #[serde(default)]
    pub superclass: Option<String>,
}

impl ClassInfo {
    pub fn new(name: &str, package: &str, fields: Vec<FieldInfo>) -> Self {
        ClassInfo { name: name.to_string(), package: package.to_string(), kind: "class".to_string(), fields, ..Default::default() }
    }

    pub fn fqn(&self) -> String {
        if self.package.is_empty() { self.name.clone() } else { format!("{}.{}", self.package, self.name) }
    }

    pub fn is_record(&self) -> bool {
        self.kind == "record"
    }

    pub fn is_enum(&self) -> bool {
        self.kind == "enum"
    }

    pub fn has_annotation(&self, name: &str) -> bool {
        self.annotations.iter().any(|a| a == name)
    }

    pub fn has_method(&self, name: &str) -> bool {
        self.methods.iter().any(|m| m == name)
    }
}

/// One parameter of a handler method.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EndpointParam {
    /// The name it is addressed by: `@RequestParam("q")`'s, else the parameter's.
    pub name: String,
    pub type_text: String,
    /// `path` | `query` | `body` | `header` | `cookie` | `part` | `model` | `arg`.
    pub binding: String,
    #[serde(default = "yes")]
    pub required: bool,
}

fn yes() -> bool {
    true
}

/// A Spring request mapping, as the index knows it.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EndpointInfo {
    /// `GET /orders/{id}`.
    pub id: String,
    /// HTTP methods; empty accepts any.
    pub methods: Vec<String>,
    pub path: String,
    /// Simple name of the controller class.
    pub controller: String,
    pub handler: String,
    pub return_type: String,
    #[serde(default)]
    pub produces: String,
    /// The media types the handler accepts, as values (`application/json`).
    #[serde(default)]
    pub consumes: String,
    #[serde(default)]
    pub params: Vec<EndpointParam>,
}

impl EndpointInfo {
    /// The HTTP method this endpoint is called with (`GET` when the mapping accepts any).
    pub fn method(&self) -> &str {
        self.methods.first().map(String::as_str).unwrap_or("GET")
    }

    pub fn params_bound(&self, binding: &str) -> Vec<&EndpointParam> {
        self.params.iter().filter(|p| p.binding == binding).collect()
    }
}

/// Everything a generator may read.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GenContext {
    pub project: ProjectInfo,
    /// The class at the caret, when the caret is in one.
    pub class: Option<ClassInfo>,
    /// The file the caret is in.
    pub file: Option<String>,
    /// The package new classes go under by default: the application's, else the caret file's.
    pub package: String,
    /// Spring request mappings of the project.
    pub endpoints: Vec<EndpointInfo>,
    /// Shapes of the types the endpoints (or the options) mention, by simple name.
    pub types: BTreeMap<String, ClassInfo>,
    /// A name the application is known by (`shop`) — a project name, used for defaults like a base-URL property.
    pub app_name: String,
}

impl GenContext {
    /// `package` + `.sub`, or `sub` alone in the default package.
    pub fn package_for(&self, sub: &str) -> String {
        match (self.package.is_empty(), sub.is_empty()) {
            (true, _) => sub.to_string(),
            (false, true) => self.package.clone(),
            (false, false) => format!("{}.{sub}", self.package),
        }
    }

    pub fn class_name(&self) -> Option<&str> {
        self.class.as_ref().map(|c| c.name.as_str())
    }

    pub fn endpoint(&self, id: &str) -> Option<&EndpointInfo> {
        self.endpoints.iter().find(|e| e.id == id)
    }

    /// Whether a JSON-ish `type_text` names a type whose shape is known.
    pub fn shape_of(&self, type_text: &str) -> Option<&ClassInfo> {
        self.types.get(crate::names::simple_name(type_text))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_are_read_tolerantly() {
        assert_eq!(Version::parse("3.2.1"), Some(Version::new(3, 2, 1)));
        assert_eq!(Version::parse("6.1.0-M1"), Some(Version::new(6, 1, 0)));
        assert_eq!(Version::parse("2.7.18.RELEASE"), Some(Version::new(2, 7, 18)));
        assert_eq!(Version::parse("5"), Some(Version::new(5, 0, 0)));
        assert_eq!(Version::parse("SNAPSHOT"), None);
    }

    #[test]
    fn boot_three_is_jakarta_and_boot_two_is_javax() {
        let modern = ProjectInfo::new(17, &[("org.springframework.boot:spring-boot", "3.2.1")]);
        let legacy = ProjectInfo::new(8, &[("org.springframework.boot:spring-boot", "2.7.18")]);
        assert!(modern.jakarta());
        assert!(!legacy.jakarta());
        assert_eq!(legacy.ee(), "javax");
    }

    #[test]
    fn an_explicit_javax_api_beats_the_java_level() {
        let project = ProjectInfo::new(21, &[("javax.servlet:javax.servlet-api", "4.0.1")]);
        assert!(!project.jakarta());
    }

    #[test]
    fn nothing_is_missing_until_the_classpath_is_resolved() {
        let unresolved = ProjectInfo::default();
        assert!(unresolved.missing(&["org.springframework.boot:spring-boot-starter-security"]).is_empty());
        let resolved = ProjectInfo::new(17, &[("org.springframework.boot:spring-boot-starter-web", "3.2.1")]);
        assert_eq!(
            resolved.missing(&["org.springframework.boot:spring-boot-starter-security", "org.springframework.boot:spring-boot-starter-web"]),
            vec!["org.springframework.boot:spring-boot-starter-security".to_string()]
        );
    }

    #[test]
    fn package_for_joins_the_default_package() {
        let ctx = GenContext { package: "com.acme".into(), ..Default::default() };
        assert_eq!(ctx.package_for("config"), "com.acme.config");
        assert_eq!(GenContext::default().package_for("config"), "config");
    }
}
