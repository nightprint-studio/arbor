//! Plain Maven and plain Gradle projects, generated locally from the data files in `templates/`.
//!
//! No build tool is needed to create one — only to build it — which is why these two never show up
//! disabled. The Java-flavoured fields (group, version, JDK level, package) are shared with the
//! web-app generators through [`java_fields`] / [`java_vars`].

use super::model::{Field, Opt, Plan, PlanCx, Problem};
use super::names;
use super::render::{render_set, TemplateSet, Vars};

pub const JDK_LEVELS: &[&str] = &["8", "11", "17", "21", "25"];

/// The fields every Java generator that writes its own build file shares.
pub fn java_fields() -> Vec<Field> {
    vec![
        Field::text("group", "Group", "com.example").hint("Maven groupId, reversed domain style"),
        Field::text("version", "Version", "1.0-SNAPSHOT"),
        Field::select(
            "java",
            "JDK level",
            "21",
            JDK_LEVELS.iter().map(|v| Opt::new(v, &format!("Java {v}"))).collect(),
        )
        .hint("The language level the project compiles at"),
        Field::text("package", "Package", "")
            .placeholder("derived from group and name")
            .hint("Leave empty to use the group plus the project name")
            .advanced(),
    ]
}

/// Validated template variables for the shared fields, plus `artifact`, `title` and `class`.
pub fn java_vars(cx: &PlanCx) -> Result<Vars, Problem> {
    let group = cx.opts.str("group");
    names::group_id(group).map_err(|m| Problem::new("group", m))?;
    let version = cx.opts.str("version");
    names::version(version).map_err(|m| Problem::new("version", m))?;
    let java = cx.opts.str("java");
    if !JDK_LEVELS.contains(&java) {
        return Err(Problem::new("java", "Pick one of the listed JDK levels"));
    }
    let package = match cx.opts.str("package") {
        "" => format!("{group}.{}", names::package_segment(cx.name)),
        p => p.to_string(),
    };
    names::java_package(&package).map_err(|m| Problem::new("package", m))?;

    let mut v = Vars::new();
    v.set("group", group)
        .set("version", version)
        .set("java", java)
        .flag("legacy", java == "8")
        .set("package", package.clone())
        .set("package_path", package.replace('.', "/"))
        .set("artifact", cx.name)
        .set("title", names::title_case(cx.name))
        .set("class", "Main")
        .flag("git", cx.git);
    Ok(v)
}

fn build_fields(extra: Vec<Field>) -> Vec<Field> {
    let mut f = extra;
    f.extend(java_fields());
    f.push(Field::toggle("sample", "Add sample code", true).hint("A Main class and, with JUnit, a first test"));
    f.push(Field::toggle("junit", "JUnit 5", true).hint("Adds the test dependency and wires the test runner"));
    f
}

pub fn maven_fields() -> Vec<Field> {
    build_fields(Vec::new())
}

pub fn gradle_fields() -> Vec<Field> {
    build_fields(vec![Field::select(
        "dsl",
        "Gradle DSL",
        "kotlin",
        vec![Opt::new("kotlin", "Kotlin (build.gradle.kts)"), Opt::new("groovy", "Groovy (build.gradle)")],
    )])
}

const POM: TemplateSet = &[("pom.xml", include_str!("templates/maven/pom.xml.tpl"))];

/// The files whose layout is the same for Maven and Gradle; only the build file differs.
const SHARED: TemplateSet = &[
    ("{{#if sample}}src/main/java/{{package_path}}/{{class}}.java{{/if}}", include_str!("templates/java/Main.java.tpl")),
    ("{{#if junit_sample}}src/test/java/{{package_path}}/{{class}}Test.java{{/if}}", include_str!("templates/java/MainTest.java.tpl")),
    ("{{#unless sample}}src/main/java/.gitkeep{{/unless}}", ""),
    ("{{#unless sample}}src/test/java/.gitkeep{{/unless}}", ""),
    ("{{#if git}}.gitignore{{/if}}", include_str!("templates/java/gitignore.tpl")),
    ("README.md", include_str!("templates/java/README.md.tpl")),
];

const GRADLE_KTS: TemplateSet = &[
    ("build.gradle.kts", include_str!("templates/gradle/build.gradle.kts.tpl")),
    ("settings.gradle.kts", include_str!("templates/gradle/settings.gradle.kts.tpl")),
];
const GRADLE_GROOVY: TemplateSet = &[
    ("build.gradle", include_str!("templates/gradle/build.gradle.tpl")),
    ("settings.gradle", include_str!("templates/gradle/settings.gradle.tpl")),
];

const JUNIT_DEP: &str = "        <dependency>\n            <groupId>org.junit.jupiter</groupId>\n            <artifactId>junit-jupiter</artifactId>\n            <version>5.11.3</version>\n            <scope>test</scope>\n        </dependency>";
const SUREFIRE: &str = "        <plugins>\n            <plugin>\n                <artifactId>maven-surefire-plugin</artifactId>\n                <version>3.5.2</version>\n            </plugin>\n        </plugins>";

fn java_summary(build: &str, cx: &PlanCx) -> Vec<String> {
    let mut s = vec![format!("{build} project “{}”", cx.name)];
    s.push(format!("Java {}, package {}", cx.opts.str("java"), vars_package(cx)));
    if cx.opts.flag("sample") {
        s.push("Sample Main class".into());
    }
    if cx.opts.flag("junit") {
        s.push("JUnit 5 test setup".into());
    }
    s
}

fn vars_package(cx: &PlanCx) -> String {
    match cx.opts.str("package") {
        "" => format!("{}.{}", cx.opts.str("group"), names::package_segment(cx.name)),
        p => p.into(),
    }
}

fn finish(cx: &PlanCx, mut vars: Vars, set: Vec<TemplateSet>, build: &str, build_cmd: &str, run_cmd: &str) -> Result<Plan, Problem> {
    vars.set("description", format!("A {build} project."))
        .set("build_cmd", build_cmd)
        .set("run_cmd", run_cmd)
        .flag("junit_sample", cx.opts.flag("junit") && cx.opts.flag("sample"))
        .flag("sample", cx.opts.flag("sample"))
        .flag("junit", cx.opts.flag("junit"));
    let mut files = Vec::new();
    for s in set {
        files.extend(render_set(s, &vars).map_err(|m| Problem::new("name", m))?);
    }
    let summary = java_summary(build, cx);
    let open: Vec<String> = if cx.opts.flag("sample") {
        vec![format!("src/main/java/{}/Main.java", vars_package(cx).replace('.', "/"))]
    } else {
        Vec::new()
    };
    let mut plan = Plan::files(files, &[], summary);
    plan.open = open;
    plan.open.push("README.md".into());
    Ok(plan)
}

pub fn plan_maven(cx: &PlanCx) -> Result<Plan, Problem> {
    let mut vars = java_vars(cx)?;
    vars.set("packaging", "jar");
    vars.set("deps", if cx.opts.flag("junit") { JUNIT_DEP } else { "" });
    vars.set("build", if cx.opts.flag("junit") { SUREFIRE } else { "" });
    let run = if cx.opts.flag("sample") {
        format!("mvn compile exec:java -Dexec.mainClass={}.Main", vars_package(cx))
    } else {
        String::new()
    };
    finish(cx, vars, vec![POM, SHARED], "Maven", "mvn verify", &run)
}

pub fn plan_gradle(cx: &PlanCx) -> Result<Plan, Problem> {
    let vars = java_vars(cx)?;
    let kotlin = match cx.opts.str("dsl") {
        "kotlin" => true,
        "groovy" => false,
        _ => return Err(Problem::new("dsl", "Pick Kotlin or Groovy")),
    };
    let run = if cx.opts.flag("sample") { "gradle run" } else { "" };
    finish(cx, vars, vec![if kotlin { GRADLE_KTS } else { GRADLE_GROOVY }, SHARED], "Gradle", "gradle build", run)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::new_project::model::{Opts, Source};
    use serde_json::{json, Map, Value};

    fn cx<'a>(name: &'a str, fields: &[Field], given: Value, git: bool) -> PlanCx<'a> {
        let map: Map<String, Value> = given.as_object().cloned().unwrap_or_default();
        PlanCx { name, opts: Opts::resolve(fields, &map), git }
    }

    fn files(plan: &Plan) -> Vec<(&str, &str)> {
        match &plan.source {
            Source::Files(f) => f.iter().map(|g| (g.path.as_str(), g.content.as_str())).collect(),
            _ => panic!("expected files"),
        }
    }

    #[test]
    fn maven_defaults() {
        let f = maven_fields();
        let plan = plan_maven(&cx("my-app", &f, json!({}), true)).unwrap();
        let fs = files(&plan);
        let paths: Vec<_> = fs.iter().map(|(p, _)| *p).collect();
        assert!(paths.contains(&"pom.xml"));
        assert!(paths.contains(&"src/main/java/com/example/myapp/Main.java"));
        assert!(paths.contains(&"src/test/java/com/example/myapp/MainTest.java"));
        assert!(paths.contains(&".gitignore"));
        let pom = fs.iter().find(|(p, _)| *p == "pom.xml").unwrap().1;
        assert!(pom.contains("<artifactId>my-app</artifactId>"));
        assert!(pom.contains("<maven.compiler.release>21</maven.compiler.release>"));
        assert!(pom.contains("junit-jupiter") && pom.contains("maven-surefire-plugin"));
        assert!(!pom.contains("{{"));
        assert_eq!(plan.open[0], "src/main/java/com/example/myapp/Main.java");
    }

    #[test]
    fn maven_legacy_without_sample_or_git() {
        let f = maven_fields();
        let plan =
            plan_maven(&cx("old", &f, json!({"java": "8", "sample": false, "junit": false}), false)).unwrap();
        let fs = files(&plan);
        let pom = fs.iter().find(|(p, _)| *p == "pom.xml").unwrap().1;
        assert!(pom.contains("<maven.compiler.source>8</maven.compiler.source>"));
        assert!(!pom.contains("junit") && !pom.contains("<dependencies>"));
        let paths: Vec<_> = fs.iter().map(|(p, _)| *p).collect();
        assert!(paths.contains(&"src/main/java/.gitkeep"));
        assert!(!paths.contains(&".gitignore"));
    }

    #[test]
    fn gradle_dsls() {
        let f = gradle_fields();
        let kts = plan_gradle(&cx("g", &f, json!({}), false)).unwrap();
        let paths: Vec<_> = files(&kts).iter().map(|(p, _)| p.to_string()).collect();
        assert!(paths.contains(&"build.gradle.kts".to_string()));
        assert!(paths.contains(&"settings.gradle.kts".to_string()));
        let groovy = plan_gradle(&cx("g", &f, json!({"dsl": "groovy"}), false)).unwrap();
        let g = files(&groovy);
        let build = g.iter().find(|(p, _)| *p == "build.gradle").unwrap().1;
        assert!(build.contains("mainClass = 'com.example.g.Main'"));
    }

    #[test]
    fn bad_values_name_the_field() {
        let f = maven_fields();
        assert_eq!(plan_maven(&cx("x", &f, json!({"group": "a..b"}), false)).unwrap_err().field, "group");
        assert_eq!(plan_maven(&cx("x", &f, json!({"java": "99"}), false)).unwrap_err().field, "java");
        assert_eq!(plan_maven(&cx("x", &f, json!({"package": "1bad"}), false)).unwrap_err().field, "package");
    }
}
