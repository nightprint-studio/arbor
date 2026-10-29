//! The list of generators: one row per project type, in one table.
//!
//! Adding a generator is adding a row here plus its module — the wizard renders the form from the
//! row's `fields`, and Create/preview go through its `plan`. Nothing in the frontend or the
//! handlers knows a generator by name.

use serde::Serialize;
use serde_json::Value;

use super::model::{Field, GeneratorInfo, NameRule, Plan, PlanCx, Problem};
use super::tools::{missing_reason, ToolStatus};
use super::{maven_gradle, node, quarkus, rust, spring, webapp};

pub struct Gen {
    pub id: &'static str,
    pub category: &'static str,
    pub label: &'static str,
    pub description: &'static str,
    pub icon: &'static str,
    pub name_rule: NameRule,
    pub requires: &'static [&'static str],
    pub remote: bool,
    /// The form with static options. Remote generators return the skeleton (keys and defaults);
    /// the real options arrive through `bennu_np_fields`.
    pub fields: fn() -> Vec<Field>,
    pub plan: fn(&PlanCx) -> Result<Plan, Problem>,
}

#[derive(Serialize)]
pub struct Category {
    pub id: &'static str,
    pub label: &'static str,
}

pub const CATEGORIES: &[Category] = &[
    Category { id: "java", label: "Java" },
    Category { id: "rust", label: "Rust" },
    Category { id: "node", label: "Node" },
];

const NPM: &[&str] = &["node", "npm", "npx"];

fn spring_skeleton() -> Vec<Field> {
    spring::fields_from_metadata(&Value::Null)
}
fn quarkus_skeleton() -> Vec<Field> {
    quarkus::fields(Vec::new())
}

pub static GENERATORS: &[Gen] = &[
    // ── Java ──
    Gen { id: "spring", category: "java", label: "Spring Initializr", description: "Spring Boot from start.spring.io — pick the version, language and dependencies.", icon: "spring", name_rule: NameRule::Artifact, requires: &[], remote: true, fields: spring_skeleton, plan: spring::plan_spring },
    Gen { id: "quarkus", category: "java", label: "Quarkus", description: "Quarkus from code.quarkus.io — Maven or Gradle, with extensions.", icon: "quarkus", name_rule: NameRule::Artifact, requires: &[], remote: true, fields: quarkus_skeleton, plan: quarkus::plan_quarkus },
    Gen { id: "maven", category: "java", label: "Maven", description: "A plain Maven project: pom.xml, source folders, an optional sample and JUnit.", icon: "maven", name_rule: NameRule::Artifact, requires: &[], remote: false, fields: maven_gradle::maven_fields, plan: maven_gradle::plan_maven },
    Gen { id: "gradle", category: "java", label: "Gradle", description: "A plain Gradle project, Kotlin or Groovy DSL, with an optional sample and JUnit.", icon: "gradle", name_rule: NameRule::Artifact, requires: &[], remote: false, fields: maven_gradle::gradle_fields, plan: maven_gradle::plan_gradle },
    Gen { id: "servlet", category: "java", label: "Servlet + JSP web app", description: "A WAR for Tomcat: web.xml, a JSP with JSTL and a servlet, javax or jakarta.", icon: "web", name_rule: NameRule::Artifact, requires: &[], remote: false, fields: webapp::servlet_fields, plan: webapp::plan_servlet },
    Gen { id: "struts", category: "java", label: "Struts 1 web app", description: "A legacy Struts 1.3 WAR: ActionServlet, struts-config.xml, a form, an action and JSPs.", icon: "web", name_rule: NameRule::Artifact, requires: &[], remote: false, fields: webapp::struts_fields, plan: webapp::plan_struts },
    // ── Rust ──
    Gen { id: "rust_bin", category: "rust", label: "Binary", description: "A Cargo binary crate with src/main.rs.", icon: "rust", name_rule: NameRule::Crate, requires: &[], remote: false, fields: rust::bin_fields, plan: rust::plan_bin },
    Gen { id: "rust_lib", category: "rust", label: "Library", description: "A Cargo library crate with a first unit test.", icon: "rust", name_rule: NameRule::Crate, requires: &[], remote: false, fields: rust::lib_fields, plan: rust::plan_lib },
    Gen { id: "rust_workspace", category: "rust", label: "Workspace", description: "A Cargo workspace with a core library and an app binary sharing one version and edition.", icon: "rust", name_rule: NameRule::Crate, requires: &[], remote: false, fields: rust::workspace_fields, plan: rust::plan_workspace },
    Gen { id: "rust_bevy", category: "rust", label: "Bevy game", description: "A Bevy app: camera, a sprite, a component, a system and a plugin; dev profile and assets folder ready.", icon: "bevy", name_rule: NameRule::Crate, requires: &[], remote: false, fields: rust::bevy_fields, plan: rust::plan_bevy },
    Gen { id: "rust_cli", category: "rust", label: "Command-line app (clap)", description: "A CLI with argument parsing through clap's derive API.", icon: "rust", name_rule: NameRule::Crate, requires: &[], remote: false, fields: rust::cli_fields, plan: rust::plan_cli },
    Gen { id: "rust_axum", category: "rust", label: "Web service (axum)", description: "An async HTTP service on axum and tokio with two routes.", icon: "rust", name_rule: NameRule::Crate, requires: &[], remote: false, fields: rust::axum_fields, plan: rust::plan_axum },
    Gen { id: "rust_wasm", category: "rust", label: "WASM library", description: "A wasm-bindgen cdylib with a browser page that loads it.", icon: "rust", name_rule: NameRule::Crate, requires: &[], remote: false, fields: rust::wasm_fields, plan: rust::plan_wasm },
    // ── Node ──
    Gen { id: "node_vite", category: "node", label: "Vite", description: "create-vite: React, Vue, Svelte, Solid, Preact, Lit or vanilla, with or without TypeScript.", icon: "vite", name_rule: NameRule::Npm, requires: NPM, remote: false, fields: node::vite_fields, plan: node::plan_vite },
    Gen { id: "node_angular", category: "node", label: "Angular", description: "The Angular CLI's `ng new`.", icon: "angular", name_rule: NameRule::Npm, requires: NPM, remote: false, fields: node::angular_fields, plan: node::plan_angular },
    Gen { id: "node_svelte", category: "node", label: "SvelteKit", description: "`sv create`: minimal, demo or library.", icon: "svelte", name_rule: NameRule::Npm, requires: NPM, remote: false, fields: node::svelte_fields, plan: node::plan_svelte },
    Gen { id: "node_vue", category: "node", label: "Vue", description: "create-vue with router, Pinia, Vitest, ESLint and Prettier options.", icon: "vue", name_rule: NameRule::Npm, requires: NPM, remote: false, fields: node::vue_fields, plan: node::plan_vue },
    Gen { id: "node_next", category: "node", label: "Next.js", description: "create-next-app with App Router, Tailwind and ESLint options.", icon: "next", name_rule: NameRule::Npm, requires: NPM, remote: false, fields: node::next_fields, plan: node::plan_next },
    Gen { id: "node_package", category: "node", label: "Node package", description: "A plain package: package.json, an entry file and, optionally, TypeScript.", icon: "node", name_rule: NameRule::Npm, requires: &[], remote: false, fields: node::package_fields, plan: node::plan_package },
];

pub fn find(id: &str) -> Option<&'static Gen> {
    GENERATORS.iter().find(|g| g.id == id)
}

/// The catalog rows with availability worked out against the installed tools.
pub fn infos(tools: &[ToolStatus]) -> Vec<GeneratorInfo> {
    GENERATORS
        .iter()
        .map(|g| {
            let requires: Vec<String> = g.requires.iter().map(|s| (*s).to_string()).collect();
            let reason = missing_reason(&requires, tools);
            GeneratorInfo {
                id: g.id.into(),
                category: g.category.into(),
                label: g.label.into(),
                description: g.description.into(),
                icon: g.icon.into(),
                name_rule: g.name_rule,
                requires,
                remote: g.remote,
                available: reason.is_none(),
                reason,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn ids_are_unique_and_categories_exist() {
        let mut seen = HashSet::new();
        for g in GENERATORS {
            assert!(seen.insert(g.id), "duplicate id {}", g.id);
            assert!(CATEGORIES.iter().any(|c| c.id == g.category), "{} has an unknown category", g.id);
        }
    }

    #[test]
    fn every_form_has_unique_keys_and_defaults_of_the_right_type() {
        for g in GENERATORS {
            let fields = (g.fields)();
            let mut keys = HashSet::new();
            for f in &fields {
                assert!(keys.insert(f.key.clone()), "{}: duplicate key {}", g.id, f.key);
                let ok = match f.kind {
                    super::super::model::FieldKind::Text | super::super::model::FieldKind::Select => f.default.is_string(),
                    super::super::model::FieldKind::Toggle => f.default.is_boolean(),
                    super::super::model::FieldKind::Multi => f.default.is_array(),
                };
                assert!(ok, "{}: default of {} has the wrong type", g.id, f.key);
                if let Some(w) = &f.show_when {
                    assert!(fields.iter().any(|x| x.key == w.key), "{}: show_when names a missing field", g.id);
                }
            }
        }
    }

    #[test]
    fn local_generators_with_defaults_produce_a_valid_plan() {
        use crate::new_project::model::{Opts, Source};
        for g in GENERATORS.iter().filter(|g| !g.remote && g.requires.is_empty()) {
            let fields = (g.fields)();
            let cx = PlanCx { name: "demo-app", opts: Opts::resolve(&fields, &Default::default()), git: true };
            let plan = (g.plan)(&cx).unwrap_or_else(|p| panic!("{}: {} ({})", g.id, p.message, p.field));
            let Source::Files(files) = plan.source else { panic!("{} should write files", g.id) };
            assert!(!files.is_empty(), "{}", g.id);
            let mut paths = HashSet::new();
            for f in &files {
                assert!(paths.insert(f.path.clone()), "{}: duplicate file {}", g.id, f.path);
                assert!(!f.path.contains("{{") && !f.content.contains("{{"), "{}: {} has a leftover tag", g.id, f.path);
            }
            assert!(!plan.summary.is_empty(), "{}", g.id);
        }
    }

    #[test]
    fn unavailable_generators_say_why() {
        let none: Vec<ToolStatus> = Vec::new();
        let rows = infos(&none);
        let vite = rows.iter().find(|r| r.id == "node_vite").unwrap();
        assert!(!vite.available && vite.reason.as_deref().unwrap().contains("`node`"));
        assert!(rows.iter().find(|r| r.id == "maven").unwrap().available);
    }
}
