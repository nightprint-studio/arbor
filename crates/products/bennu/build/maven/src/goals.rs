//! The build half of a pom — what a tool window can offer to run.
//!
//! Two vocabularies live here: Maven's own lifecycle, and the plugins a pom configures. They are
//! in the backend rather than in the panel for the reason `bennu-cargo`'s command table is —
//! a frontend carrying its own copy of a vocabulary eventually offers something the backend then
//! does not run, and a button that silently does nothing is the worst kind.
//!
//! **Nothing here runs Maven.** A goal's *existence* is read out of the pom; whether it works is
//! answered by running it, which is what pressing it does. That split is why this file is cheap
//! enough to call on every panel open: it is a parse, not a build.

use serde::Serialize;

use crate::doc::Doc;

/// One phase of a Maven lifecycle, in the order Maven runs them.
#[derive(Debug, Clone, Copy, Serialize)]
pub struct Phase {
    /// What is passed on the command line.
    pub id: &'static str,
    /// What it does, in the fewest words that are still true.
    pub hint: &'static str,
    /// Which lifecycle it belongs to — `clean`, `default` or `site`. The panel groups by it, and
    /// it is the reason `clean` sits above `validate` rather than being the first phase of the
    /// same sequence: they are two lifecycles, and `mvn clean install` runs both.
    pub lifecycle: &'static str,
}

/// The phases Maven's three built-in lifecycles are made of, in run order.
///
/// Not every phase — the default lifecycle has twenty-three and nobody invokes
/// `process-test-resources` by hand. These are the ones that are *bindings people use*, which is
/// the same set IntelliJ's Maven window lists, and the set a build is actually driven from.
pub const LIFECYCLE: &[Phase] = &[
    Phase { id: "clean", hint: "Delete target/", lifecycle: "clean" },
    Phase { id: "validate", hint: "Check the project is well-formed", lifecycle: "default" },
    Phase { id: "compile", hint: "Compile the source", lifecycle: "default" },
    Phase { id: "test", hint: "Run the unit tests", lifecycle: "default" },
    Phase { id: "package", hint: "Build the JAR/WAR", lifecycle: "default" },
    Phase { id: "verify", hint: "Run the checks on the package", lifecycle: "default" },
    Phase { id: "install", hint: "Install into the local repository", lifecycle: "default" },
    Phase { id: "site", hint: "Generate the project site", lifecycle: "site" },
    Phase { id: "deploy", hint: "Publish to the remote repository", lifecycle: "default" },
];

/// A plugin a pom configures, and the goals it names.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct PomPlugin {
    pub group_id: String,
    pub artifact_id: String,
    /// As written — a `${property}` stays a `${property}`, because that is what the pom says and
    /// this file resolves nothing.
    pub version: String,
    /// The goal prefix Maven would accept on a command line (`compiler` for
    /// `maven-compiler-plugin`), when the artifactId follows one of the two conventional shapes.
    /// Empty otherwise, and then a goal has to be invoked fully qualified.
    pub prefix: String,
    /// The goals its `<execution>`s bind, deduplicated, in document order. Empty for a plugin that
    /// only carries `<configuration>` — which is most of them, and is not a defect: those run
    /// because a lifecycle phase calls them, and the phase above is how you run them.
    pub goals: Vec<String>,
    /// True when it was found under `<pluginManagement>` and nowhere else: configured *for* the
    /// modules rather than bound in this one. Worth saying, because pressing a goal on it runs
    /// nothing here.
    pub managed: bool,
    /// Byte offset of the `<plugin>` tag — what opens the pom at the right line.
    pub offset: usize,
    /// 1-based line of that tag.
    pub line: u32,
}

impl PomPlugin {
    /// How this plugin's `goal` is spelled on a command line.
    ///
    /// The prefix form when the artifactId follows Maven's own naming convention, and the fully
    /// qualified form otherwise — never a guess. `mvn compiler:compile` and
    /// `mvn org.acme:funny-plugin:1.2:frobnicate` are both correct; a prefix invented for the
    /// second would resolve to a different plugin or to none.
    pub fn invocation(&self, goal: &str) -> String {
        if !self.prefix.is_empty() {
            return format!("{}:{}", self.prefix, goal);
        }
        match self.version.is_empty() {
            true => format!("{}:{}:{}", self.group_id, self.artifact_id, goal),
            false => format!("{}:{}:{}:{}", self.group_id, self.artifact_id, self.version, goal),
        }
    }
}

/// The goal prefix Maven resolves an artifactId to, for the two conventional shapes.
///
/// Documented convention, not a heuristic: `maven-${prefix}-plugin` is reserved for plugins under
/// `org.apache.maven.plugins`, and `${prefix}-maven-plugin` is the form everyone else uses. An
/// artifactId shaped like neither has a prefix only its own `plugin.xml` knows, and that lives
/// inside the jar — so the answer here is "no prefix", and the caller writes the coordinates out.
fn prefix_of(artifact_id: &str) -> String {
    if let Some(rest) = artifact_id.strip_prefix("maven-") {
        if let Some(prefix) = rest.strip_suffix("-plugin") {
            return prefix.to_string();
        }
    }
    if let Some(prefix) = artifact_id.strip_suffix("-maven-plugin") {
        return prefix.to_string();
    }
    String::new()
}

/// Every plugin this pom configures — `<build>`, `<build><pluginManagement>`, and the same two
/// inside every `<profile>`.
///
/// Deduplicated by coordinate, first mention winning, so a plugin managed at the top and bound
/// lower down appears once, as bound. Profiles are included without asking which are active: a
/// panel that hid a plugin because a profile is off would be hiding the plugin somebody is looking
/// for, and the row says nothing about whether it will run today.
pub fn plugins(source: &str) -> Vec<PomPlugin> {
    let doc = Doc::new(source);
    let Some(root) = doc.root() else { return Vec::new() };
    if doc.name(root) != "project" {
        return Vec::new();
    }

    let mut out: Vec<PomPlugin> = Vec::new();
    for build in builds(&doc, root) {
        collect_from_build(&doc, build, &mut out);
    }
    out
}

/// Every `<build>` element in the document — the project's own, plus one per profile.
fn builds(doc: &Doc<'_>, root: usize) -> Vec<usize> {
    let mut out = Vec::new();
    if let Some(b) = doc.child(root, "build") {
        out.push(b);
    }
    if let Some(profiles) = doc.child(root, "profiles") {
        for profile in doc.children(profiles) {
            if doc.name(profile) != "profile" {
                continue;
            }
            if let Some(b) = doc.child(profile, "build") {
                out.push(b);
            }
        }
    }
    out
}

/// The bound plugins of one `<build>`, then its managed ones. Bound first so a plugin that is both
/// keeps the mention that says it actually runs.
fn collect_from_build(doc: &Doc<'_>, build: usize, out: &mut Vec<PomPlugin>) {
    if let Some(plugins) = doc.child(build, "plugins") {
        collect_plugins(doc, plugins, false, out);
    }
    if let Some(management) = doc.child(build, "pluginManagement") {
        if let Some(plugins) = doc.child(management, "plugins") {
            collect_plugins(doc, plugins, true, out);
        }
    }
}

fn collect_plugins(doc: &Doc<'_>, plugins: usize, managed: bool, out: &mut Vec<PomPlugin>) {
    for plugin in doc.children(plugins) {
        if doc.name(plugin) != "plugin" {
            continue;
        }
        let artifact_id = doc.child(plugin, "artifactId").map(|i| doc.text(i)).unwrap_or_default();
        if artifact_id.is_empty() {
            continue;
        }
        // Maven's own default, and the reason a pom may name only the artifactId.
        let group_id = doc
            .child(plugin, "groupId")
            .map(|i| doc.text(i))
            .filter(|g| !g.is_empty())
            .unwrap_or("org.apache.maven.plugins");
        if out.iter().any(|p| p.group_id == group_id && p.artifact_id == artifact_id) {
            continue;
        }
        let offset = doc.start(plugin);
        out.push(PomPlugin {
            group_id: group_id.to_string(),
            artifact_id: artifact_id.to_string(),
            version: doc.child(plugin, "version").map(|i| doc.text(i)).unwrap_or_default().to_string(),
            prefix: prefix_of(artifact_id),
            goals: goals_of(doc, plugin),
            managed,
            offset,
            line: line_of(doc.source, offset),
        });
    }
}

/// The goals a plugin's `<executions>` name, deduplicated, in document order.
fn goals_of(doc: &Doc<'_>, plugin: usize) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let Some(executions) = doc.child(plugin, "executions") else { return out };
    for execution in doc.children(executions) {
        if doc.name(execution) != "execution" {
            continue;
        }
        let Some(goals) = doc.child(execution, "goals") else { continue };
        for goal in doc.children(goals) {
            if doc.name(goal) != "goal" {
                continue;
            }
            let text = doc.text(goal);
            if !text.is_empty() && !out.iter().any(|g| g == text) {
                out.push(text.to_string());
            }
        }
    }
    out
}

/// A profile the pom declares.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct PomProfile {
    pub id: String,
    /// True when the pom marks it `<activation><activeByDefault>true`. Not "active now": every
    /// other activation Maven supports is a fact about the machine or the command line, and a
    /// panel that claimed to know which of those hold would be wrong on somebody's CI.
    pub active_by_default: bool,
    /// The module directory that declares it, relative to the project root — filled in by the
    /// caller, which is the only thing that knows where the pom was.
    pub module: String,
}

/// The profiles a pom declares, in document order.
pub fn profiles(source: &str) -> Vec<PomProfile> {
    let doc = Doc::new(source);
    let Some(root) = doc.root() else { return Vec::new() };
    if doc.name(root) != "project" {
        return Vec::new();
    }
    let Some(profiles) = doc.child(root, "profiles") else { return Vec::new() };
    let mut out = Vec::new();
    for profile in doc.children(profiles) {
        if doc.name(profile) != "profile" {
            continue;
        }
        let id = doc.child(profile, "id").map(|i| doc.text(i)).unwrap_or_default();
        if id.is_empty() {
            continue;
        }
        let active_by_default = doc
            .child(profile, "activation")
            .and_then(|a| doc.child(a, "activeByDefault"))
            .map(|i| doc.text(i) == "true")
            .unwrap_or(false);
        out.push(PomProfile { id: id.to_string(), active_by_default, module: String::new() });
    }
    out
}

/// 1-based line holding `offset`.
fn line_of(source: &str, offset: usize) -> u32 {
    let end = offset.min(source.len());
    source[..end].bytes().filter(|b| *b == b'\n').count() as u32 + 1
}

#[cfg(test)]
mod tests {
    use super::*;

    const POM: &str = r#"<project>
  <artifactId>demo</artifactId>
  <build>
    <pluginManagement>
      <plugins>
        <plugin>
          <artifactId>maven-surefire-plugin</artifactId>
          <version>3.2.5</version>
        </plugin>
      </plugins>
    </pluginManagement>
    <plugins>
      <plugin>
        <artifactId>maven-compiler-plugin</artifactId>
        <version>3.11.0</version>
        <configuration><source>1.8</source></configuration>
      </plugin>
      <plugin>
        <groupId>org.codehaus.mojo</groupId>
        <artifactId>build-helper-maven-plugin</artifactId>
        <executions>
          <execution>
            <goals>
              <goal>add-source</goal>
              <goal>add-source</goal>
              <goal>parse-version</goal>
            </goals>
          </execution>
        </executions>
      </plugin>
    </plugins>
  </build>
</project>
"#;

    #[test]
    fn a_document_that_is_not_a_pom_has_no_plugins() {
        assert!(plugins("<beans><plugin><artifactId>x</artifactId></plugin></beans>").is_empty());
    }

    #[test]
    fn the_bound_plugins_and_the_managed_ones_are_told_apart() {
        let found = plugins(POM);
        let compiler = found.iter().find(|p| p.artifact_id == "maven-compiler-plugin").unwrap();
        let surefire = found.iter().find(|p| p.artifact_id == "maven-surefire-plugin").unwrap();
        assert!(!compiler.managed, "declared under <plugins> — it runs here");
        assert!(surefire.managed, "declared only under <pluginManagement> — it does not");
    }

    #[test]
    fn a_plugin_with_no_group_gets_mavens_own() {
        let found = plugins(POM);
        let compiler = found.iter().find(|p| p.artifact_id == "maven-compiler-plugin").unwrap();
        assert_eq!(compiler.group_id, "org.apache.maven.plugins");
    }

    /// The same goal bound twice is one row: a panel that listed it twice would be reporting the
    /// pom's shape rather than what can be run.
    #[test]
    fn the_goals_are_the_executions_own_without_repeats() {
        let found = plugins(POM);
        let helper = found.iter().find(|p| p.artifact_id == "build-helper-maven-plugin").unwrap();
        assert_eq!(helper.goals, vec!["add-source".to_string(), "parse-version".to_string()]);
    }

    /// A plugin that only carries `<configuration>` binds nothing — and that is not a defect, it is
    /// what "the lifecycle calls it" looks like in a pom.
    #[test]
    fn a_plugin_that_only_configures_binds_no_goals() {
        let found = plugins(POM);
        let compiler = found.iter().find(|p| p.artifact_id == "maven-compiler-plugin").unwrap();
        assert!(compiler.goals.is_empty());
    }

    #[test]
    fn both_conventional_shapes_yield_a_prefix_and_nothing_else_does() {
        assert_eq!(prefix_of("maven-compiler-plugin"), "compiler");
        assert_eq!(prefix_of("build-helper-maven-plugin"), "build-helper");
        // Neither shape: the prefix lives in the jar's own plugin.xml, so there is no honest answer.
        assert_eq!(prefix_of("frobnicator"), "");
        // `maven-plugin` alone is not `maven-${x}-plugin` — the empty middle must not become "".
        assert_eq!(prefix_of("maven--plugin"), "");
    }

    #[test]
    fn a_goal_without_a_prefix_is_written_out_in_full() {
        let unconventional = PomPlugin {
            group_id: "org.acme".into(),
            artifact_id: "frobnicator".into(),
            version: "1.2".into(),
            ..PomPlugin::default()
        };
        assert_eq!(unconventional.invocation("frob"), "org.acme:frobnicator:1.2:frob");
        let conventional = PomPlugin { prefix: "compiler".into(), ..PomPlugin::default() };
        assert_eq!(conventional.invocation("compile"), "compiler:compile");
    }

    #[test]
    fn a_plugin_inside_a_profile_is_listed_too() {
        let pom = r#"<project><profiles><profile><id>ci</id><build><plugins>
            <plugin><artifactId>jacoco-maven-plugin</artifactId></plugin>
        </plugins></build></profile></profiles></project>"#;
        let found = plugins(pom);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].prefix, "jacoco");
    }

    #[test]
    fn a_profile_is_read_with_whether_the_pom_turns_it_on_itself() {
        let pom = r#"<project><profiles>
            <profile><id>ci</id></profile>
            <profile><id>dev</id><activation><activeByDefault>true</activeByDefault></activation></profile>
            <profile><activation/></profile>
        </profiles></project>"#;
        let found = profiles(pom);
        // The third declares no id, so there is nothing to pass to `-P` and nothing to show.
        assert_eq!(found.len(), 2);
        assert!(!found[0].active_by_default);
        assert!(found[1].active_by_default);
    }

    #[test]
    fn the_line_is_where_the_plugin_tag_is() {
        let found = plugins(POM);
        let surefire = found.iter().find(|p| p.artifact_id == "maven-surefire-plugin").unwrap();
        assert_eq!(POM.lines().nth(surefire.line as usize - 1).unwrap().trim(), "<plugin>");
    }
}
