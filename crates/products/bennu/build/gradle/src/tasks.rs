//! The tasks a Gradle project offers.
//!
//! `gradle tasks --all` is the authoritative list and costs a JVM start, which a tool window that
//! has to draw its first row cannot pay. So the panel is built from what the build *says*: the
//! tasks the applied plugins are known to add, plus the ones the script declares itself. A plugin
//! this table has never heard of (a company convention plugin) contributes nothing here, and the
//! panel's "Run a task…" row is how its tasks are reached.

use serde::Serialize;

use crate::build_file::BuildInfo;

/// One runnable task.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Task {
    /// What goes on the command line.
    pub id: String,
    /// What it does, in the fewest words that are still true.
    pub hint: String,
    /// `build` · `verification` · `application` · `documentation` · `publishing` · `help` · `other`.
    pub group: String,
}

fn task(id: &str, hint: &str, group: &str) -> Task {
    Task { id: id.to_string(), hint: hint.to_string(), group: group.to_string() }
}

/// The tasks every project has, whatever it applies.
fn help_tasks() -> Vec<Task> {
    vec![
        task("tasks", "List the tasks of this project", "help"),
        task("dependencies", "Print the dependency tree", "help"),
        task("properties", "Print the project's properties", "help"),
        task("projects", "List the subprojects", "help"),
    ]
}

/// What the Java base and `java` plugins add.
fn java_tasks() -> Vec<Task> {
    vec![
        task("clean", "Delete the build directory", "build"),
        task("classes", "Compile the main sources", "build"),
        task("assemble", "Build every archive, run no tests", "build"),
        task("build", "Assemble and test", "build"),
        task("jar", "Package the classes as a jar", "build"),
        task("test", "Run the unit tests", "verification"),
        task("check", "Run every verification task", "verification"),
        task("javadoc", "Generate the API documentation", "documentation"),
    ]
}

/// The tasks the applied plugins add to a project, then the ones its script declares.
///
/// `jvm_build`: the build as a whole has Java projects. The root of a multi-project build usually
/// applies nothing itself and still answers to `gradle build` — Gradle runs a task in every project
/// that has it — so the root gets the Java set whenever any project does.
pub fn tasks_for(info: &BuildInfo, is_jvm: bool, jvm_build: bool) -> Vec<Task> {
    let mut out = Vec::new();
    if is_jvm || jvm_build {
        out.extend(java_tasks());
    }
    if info.applies("application") {
        out.push(task("run", "Run the application's main class", "application"));
        out.push(task("installDist", "Install the distribution locally", "application"));
        out.push(task("distZip", "Zip the distribution", "application"));
    }
    if info.applies("war") {
        out.push(task("war", "Package the web application", "build"));
    }
    if info.applies("org.springframework.boot") {
        out.push(task("bootRun", "Run the Spring Boot application", "application"));
        out.push(task("bootJar", "Package the executable jar", "build"));
        out.push(task("bootBuildImage", "Build an OCI image", "build"));
    }
    if info.applies("io.quarkus") {
        out.push(task("quarkusDev", "Run in Quarkus dev mode", "application"));
        out.push(task("quarkusBuild", "Build the Quarkus application", "build"));
    }
    if info.applies("maven-publish") {
        out.push(task("publishToMavenLocal", "Publish to ~/.m2", "publishing"));
        out.push(task("publish", "Publish to the configured repositories", "publishing"));
    }
    for declared in &info.tasks {
        if !out.iter().any(|t| &t.id == declared) {
            out.push(task(declared, "Declared in the build script", "other"));
        }
    }
    out.extend(help_tasks());
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::build_file::Env;
    use std::collections::BTreeMap;

    fn info(text: &str) -> BuildInfo {
        let none = BTreeMap::new();
        BuildInfo::parse(text, &Env { properties: &none, inherited: &none, catalogs: &[] })
    }

    fn ids(tasks: &[Task]) -> Vec<&str> {
        tasks.iter().map(|t| t.id.as_str()).collect()
    }

    #[test]
    fn plugins_add_their_own_tasks() {
        let i = info("plugins { id 'java'; id 'application'; id 'org.springframework.boot' version '3.1.0' }");
        let t = tasks_for(&i, true, true);
        for expected in ["build", "test", "run", "bootRun"] {
            assert!(ids(&t).contains(&expected), "{expected} missing");
        }
    }

    #[test]
    fn a_plain_root_still_answers_to_build_when_a_subproject_is_java() {
        let i = info("");
        assert!(ids(&tasks_for(&i, false, true)).contains(&"build"));
        assert!(!ids(&tasks_for(&i, false, false)).contains(&"build"));
    }

    #[test]
    fn declared_tasks_are_listed_once() {
        let i = info("tasks.register(\"docs\") { }\ntasks.register(\"test\") { }");
        let t = tasks_for(&i, true, true);
        assert_eq!(ids(&t).iter().filter(|id| **id == "test").count(), 1);
        assert!(ids(&t).contains(&"docs"));
    }
}
