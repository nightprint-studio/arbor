//! Quarkus (`code.quarkus.io`).
//!
//! The extension picker comes from `GET /api/extensions` (an array of `{id, name, description,
//! category, order, …}`); creation downloads `GET /d` with the usual query (`g`, `a`, `v`, `b`,
//! `j`, repeated `e` for extensions, `nc` to leave the starter code out). The extension `id` — the
//! `groupId:artifactId` coordinate — is what is sent back, since it is the one stable name.
//!
//! The endpoint and parameter names are from the code.quarkus.io API documentation; the parser is
//! tolerant, and every value is checked against a character allow-list before it is put in a URL.

use reqwest::Url;
use serde_json::Value;

use super::http::QUARKUS_HOST;
use super::model::{Field, Opt, Plan, PlanCx, Problem, Source};
use super::names;

pub const EXTENSIONS_URL: &str = "https://code.quarkus.io/api/extensions";
const ZIP_URL: &str = "https://code.quarkus.io/d";
const JAVA_LEVELS: &[&str] = &["17", "21", "25"];

/// Extension options from the `/api/extensions` array, ordered by the service's own `order`
/// (the most common first) and then name.
pub fn extension_options(list: &Value) -> Vec<Opt> {
    let mut rows: Vec<(i64, Opt)> = list
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|e| {
            let id = e.get("id")?.as_str()?;
            let name = e.get("name").and_then(Value::as_str).unwrap_or(id);
            let order = e.get("order").and_then(Value::as_i64).unwrap_or(i64::MAX);
            Some((
                order,
                Opt {
                    value: id.into(),
                    label: name.into(),
                    description: e.get("description").and_then(Value::as_str).map(str::to_string),
                    group: Some(e.get("category").and_then(Value::as_str).unwrap_or("Other").into()),
                },
            ))
        })
        .collect();
    rows.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.label.cmp(&b.1.label)));
    rows.into_iter().map(|(_, o)| o).collect()
}

/// The Quarkus form; `extensions` is empty until the list has been fetched.
pub fn fields(extensions: Vec<Opt>) -> Vec<Field> {
    vec![
        Field::select(
            "build",
            "Build tool",
            "MAVEN",
            vec![
                Opt::new("MAVEN", "Maven"),
                Opt::new("GRADLE", "Gradle (Groovy DSL)"),
                Opt::new("GRADLE_KOTLIN_DSL", "Gradle (Kotlin DSL)"),
            ],
        ),
        Field::text("group", "Group", "org.acme"),
        Field::text("version", "Version", "1.0.0-SNAPSHOT"),
        Field::select(
            "java",
            "Java",
            "21",
            JAVA_LEVELS.iter().map(|v| Opt::new(v, &format!("Java {v}"))).collect(),
        ),
        Field::toggle("starter_code", "Include starter code", true).hint("A sample resource and its test"),
        Field::multi("extensions", "Extensions", extensions),
    ]
}

fn is_extension_id(s: &str) -> bool {
    !s.is_empty() && s.len() <= 120 && s.chars().all(|c| c.is_ascii_alphanumeric() || ".:-_".contains(c))
}

pub fn plan_quarkus(cx: &PlanCx) -> Result<Plan, Problem> {
    let o = &cx.opts;
    let build = o.str("build");
    if !["MAVEN", "GRADLE", "GRADLE_KOTLIN_DSL"].contains(&build) {
        return Err(Problem::new("build", "Pick Maven or Gradle"));
    }
    let java = o.str("java");
    if !JAVA_LEVELS.contains(&java) {
        return Err(Problem::new("java", "Pick one of the listed Java levels"));
    }
    names::group_id(o.str("group")).map_err(|m| Problem::new("group", m))?;
    names::version(o.str("version")).map_err(|m| Problem::new("version", m))?;
    let extensions = o.list("extensions");
    if extensions.len() > 100 || !extensions.iter().all(|e| is_extension_id(e)) {
        return Err(Problem::new("extensions", "An extension id isn't valid"));
    }

    let mut params: Vec<(&str, String)> = vec![
        ("g", o.str("group").into()),
        ("a", cx.name.into()),
        ("v", o.str("version").into()),
        ("b", build.into()),
        ("j", java.into()),
    ];
    params.extend(extensions.iter().map(|e| ("e", e.clone())));
    if !o.flag("starter_code") {
        params.push(("nc", "true".into()));
    }
    let url = Url::parse_with_params(ZIP_URL, &params)
        .map_err(|e| Problem::new("name", format!("could not build the request: {e}")))?;

    let summary = vec![
        format!("Quarkus project “{}” from {QUARKUS_HOST}", cx.name),
        format!("{} · Java {java}", o.str("build").to_ascii_lowercase().replace('_', " ")),
        if extensions.is_empty() { "No extensions selected".into() } else { format!("{} extensions", extensions.len()) },
    ];
    Ok(Plan {
        source: Source::Download { url: url.to_string() },
        post: Vec::new(),
        open: vec!["README.md".into()],
        summary,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::new_project::model::Opts;
    use serde_json::{json, Map};

    fn plan(given: Value) -> Result<Plan, Problem> {
        let f = fields(Vec::new());
        let map: Map<String, Value> = given.as_object().cloned().unwrap_or_default();
        plan_quarkus(&PlanCx { name: "svc", opts: Opts::resolve(&f, &map), git: false })
    }

    #[test]
    fn extensions_are_ordered_and_grouped() {
        let list = json!([
            {"id": "io.quarkus:quarkus-b", "name": "Beta", "category": "Web", "order": 2},
            {"id": "io.quarkus:quarkus-a", "name": "Alpha", "category": "Data", "order": 2, "description": "d"},
            {"id": "io.quarkus:quarkus-first", "name": "First", "order": 1},
            {"name": "no id"}
        ]);
        let opts = extension_options(&list);
        let labels: Vec<_> = opts.iter().map(|o| o.label.as_str()).collect();
        assert_eq!(labels, ["First", "Alpha", "Beta"]);
        assert_eq!(opts[0].group.as_deref(), Some("Other"));
        assert_eq!(opts[1].description.as_deref(), Some("d"));
    }

    #[test]
    fn the_url_repeats_e_for_each_extension() {
        let p = plan(json!({"extensions": ["io.quarkus:quarkus-rest", "io.quarkus:quarkus-hibernate-orm"], "starter_code": false})).unwrap();
        let Source::Download { url } = &p.source else { panic!() };
        assert!(url.starts_with("https://code.quarkus.io/d?"));
        assert_eq!(url.matches("&e=").count(), 2);
        for needle in ["g=org.acme", "a=svc", "b=MAVEN", "j=21", "nc=true", "io.quarkus%3Aquarkus-rest"] {
            assert!(url.contains(needle), "{needle} in {url}");
        }
    }

    #[test]
    fn starter_code_on_sends_no_nc() {
        let p = plan(json!({})).unwrap();
        let Source::Download { url } = &p.source else { panic!() };
        assert!(!url.contains("nc="));
    }

    #[test]
    fn bad_values_are_refused() {
        assert_eq!(plan(json!({"build": "ANT"})).unwrap_err().field, "build");
        assert_eq!(plan(json!({"java": "7"})).unwrap_err().field, "java");
        assert_eq!(plan(json!({"extensions": ["a&b"]})).unwrap_err().field, "extensions");
    }
}
