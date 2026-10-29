//! Spring Initializr (`start.spring.io`).
//!
//! The form is built from the service's own metadata (`GET /` with the
//! `application/vnd.initializr.v2.2+json` media type), so a new Boot version or a new dependency
//! appears without a Bennu release. Creation downloads `/starter.zip`; every query value passes
//! through a character allow-list first, and the URL is built with proper percent-encoding.
//!
//! The metadata shape (`type`, `language`, `bootVersion`, `packaging`, `javaVersion`,
//! `dependencies` as groups of `{id,name,description}`, text defaults for `groupId`, `version`,
//! `description`) is taken from the Initializr documentation; the parser is tolerant — a missing
//! block degrades that field, not the form.

use reqwest::Url;
use serde_json::Value;

use super::http::SPRING_HOST;
use super::model::{Field, Opt, Plan, PlanCx, Problem, Source};
use super::names;

pub const META_URL: &str = "https://start.spring.io";
pub const META_ACCEPT: &str = "application/vnd.initializr.v2.2+json";

/// `{ "values": [ { "id", "name" } ], "default" }` → options and the default id.
fn option_list(meta: &Value, key: &str) -> (Vec<Opt>, String) {
    let block = meta.get(key);
    let default = block.and_then(|b| b.get("default")).and_then(Value::as_str).unwrap_or("").to_string();
    let opts = block
        .and_then(|b| b.get("values"))
        .and_then(Value::as_array)
        .map(|vs| {
            vs.iter()
                .filter_map(|v| {
                    let id = v.get("id")?.as_str()?;
                    let name = v.get("name").and_then(Value::as_str).unwrap_or(id);
                    Some(Opt::new(id, name))
                })
                .collect()
        })
        .unwrap_or_default();
    (opts, default)
}

/// Project types that produce a project (the metadata also lists build-file-only types).
fn project_types(meta: &Value) -> (Vec<Opt>, String) {
    let (mut opts, default) = option_list(meta, "type");
    let formats: Vec<(String, bool)> = meta
        .pointer("/type/values")
        .and_then(Value::as_array)
        .map(|vs| {
            vs.iter()
                .map(|v| {
                    let id = v.get("id").and_then(Value::as_str).unwrap_or("").to_string();
                    let format = v.pointer("/tags/format").and_then(Value::as_str);
                    (id.clone(), format == Some("project") || (format.is_none() && id.ends_with("-project")))
                })
                .collect()
        })
        .unwrap_or_default();
    opts.retain(|o| formats.iter().any(|(id, keep)| *id == o.value && *keep));
    (opts, default)
}

fn text_default(meta: &Value, key: &str, fallback: &str) -> String {
    meta.get(key).and_then(|b| b.get("default")).and_then(Value::as_str).unwrap_or(fallback).to_string()
}

fn dependency_options(meta: &Value) -> Vec<Opt> {
    let mut out = Vec::new();
    let groups = meta.pointer("/dependencies/values").and_then(Value::as_array);
    for group in groups.into_iter().flatten() {
        let heading = group.get("name").and_then(Value::as_str).unwrap_or("Other");
        for dep in group.get("values").and_then(Value::as_array).into_iter().flatten() {
            let Some(id) = dep.get("id").and_then(Value::as_str) else { continue };
            let name = dep.get("name").and_then(Value::as_str).unwrap_or(id);
            out.push(Opt {
                value: id.into(),
                label: name.into(),
                description: dep.get("description").and_then(Value::as_str).map(str::to_string),
                group: Some(heading.into()),
            });
        }
    }
    out
}

/// The whole Spring form from the service metadata.
pub fn fields_from_metadata(meta: &Value) -> Vec<Field> {
    let (types, type_default) = project_types(meta);
    let (langs, lang_default) = option_list(meta, "language");
    let (boots, boot_default) = option_list(meta, "bootVersion");
    let (packs, pack_default) = option_list(meta, "packaging");
    let (javas, java_default) = option_list(meta, "javaVersion");
    let pick = |default: &str, opts: &[Opt], fallback: &str| {
        if opts.iter().any(|o| o.value == default) { default.to_string() } else { opts.first().map_or(fallback.into(), |o| o.value.clone()) }
    };
    let boot_label = |o: Opt| if o.value.contains("SNAPSHOT") || o.value.contains("-M") || o.value.contains("RC") { Opt { label: format!("{} (preview)", o.label), ..o } } else { o };
    let boots: Vec<Opt> = boots.into_iter().map(boot_label).collect();

    vec![
        Field::select("type", "Project", &pick(&type_default, &types, "maven-project"), types.clone()),
        Field::select("language", "Language", &pick(&lang_default, &langs, "java"), langs.clone()),
        Field::select("boot", "Spring Boot", &pick(&boot_default, &boots, ""), boots.clone()),
        Field::select("packaging", "Packaging", &pick(&pack_default, &packs, "jar"), packs.clone()),
        Field::select("java", "Java", &pick(&java_default, &javas, "21"), javas.clone()),
        Field::text("group", "Group", &text_default(meta, "groupId", "com.example")),
        Field::text("version", "Version", &text_default(meta, "version", "0.0.1-SNAPSHOT")),
        Field::text("description", "Description", &text_default(meta, "description", "Demo project for Spring Boot")).advanced(),
        Field::text("package", "Package", "")
            .placeholder("derived from group and name")
            .advanced(),
        Field::multi("dependencies", "Dependencies", dependency_options(meta)),
    ]
}

fn is_id(s: &str, extra: &str) -> bool {
    !s.is_empty() && s.len() <= 64 && s.chars().all(|c| c.is_ascii_alphanumeric() || extra.contains(c))
}

/// The `starter.zip` URL for these options.
pub fn plan_spring(cx: &PlanCx) -> Result<Plan, Problem> {
    let o = &cx.opts;
    for (key, extra) in [("type", "-"), ("language", "-"), ("packaging", "-"), ("java", "")] {
        if !is_id(o.str(key), extra) {
            return Err(Problem::new(key, "Pick one of the listed values"));
        }
    }
    let boot = o.str("boot");
    if !boot.is_empty() && !is_id(boot, ".-") {
        return Err(Problem::new("boot", "Pick one of the listed Spring Boot versions"));
    }
    names::group_id(o.str("group")).map_err(|m| Problem::new("group", m))?;
    names::version(o.str("version")).map_err(|m| Problem::new("version", m))?;
    let description = o.str("description");
    if description.chars().count() > 300 || description.chars().any(char::is_control) {
        return Err(Problem::new("description", "At most 300 characters, on one line"));
    }
    let package = o.str("package");
    if !package.is_empty() {
        names::java_package(package).map_err(|m| Problem::new("package", m))?;
    }
    let deps = o.list("dependencies");
    if deps.len() > 100 || deps.iter().any(|d| !is_id(d, ".-")) {
        return Err(Problem::new("dependencies", "A dependency id isn't valid"));
    }

    let mut params: Vec<(&str, String)> = vec![
        ("type", o.str("type").into()),
        ("language", o.str("language").into()),
        ("groupId", o.str("group").into()),
        ("artifactId", cx.name.into()),
        ("name", cx.name.into()),
        ("description", description.into()),
        ("packaging", o.str("packaging").into()),
        ("javaVersion", o.str("java").into()),
    ];
    if !boot.is_empty() {
        params.push(("bootVersion", boot.into()));
    }
    if !package.is_empty() {
        params.push(("packageName", package.into()));
    }
    if !deps.is_empty() {
        params.push(("dependencies", deps.join(",")));
    }
    let url = Url::parse_with_params(&format!("{META_URL}/starter.zip"), &params)
        .map_err(|e| Problem::new("name", format!("could not build the request: {e}")))?;

    let pkg = if package.is_empty() { format!("{}.{}", o.str("group"), names::package_segment(cx.name)) } else { package.into() };
    let (dir, ext) = match o.str("language") {
        "kotlin" => ("kotlin", "kt"),
        "groovy" => ("groovy", "groovy"),
        _ => ("java", "java"),
    };
    let main_class = format!("src/main/{dir}/{}/{}Application.{ext}", pkg.replace('.', "/"), names::class_name(cx.name));
    let mut summary = vec![
        format!("Spring Boot project “{}” from {SPRING_HOST}", cx.name),
        format!("{} · {} · Java {}", o.str("type"), o.str("language"), o.str("java")),
    ];
    summary.push(if deps.is_empty() { "No dependencies selected".into() } else { format!("{} dependencies: {}", deps.len(), deps.join(", ")) });
    Ok(Plan {
        source: Source::Download { url: url.to_string() },
        post: Vec::new(),
        open: vec![main_class, "HELP.md".into()],
        summary,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::new_project::model::Opts;
    use serde_json::{json, Map};

    fn meta() -> Value {
        json!({
            "type": {"default": "maven-project", "values": [
                {"id": "maven-project", "name": "Maven", "tags": {"format": "project", "build": "maven"}},
                {"id": "maven-build", "name": "Maven POM", "tags": {"format": "build"}},
                {"id": "gradle-project-kotlin", "name": "Gradle - Kotlin", "tags": {"format": "project"}}
            ]},
            "language": {"default": "java", "values": [{"id": "java", "name": "Java"}, {"id": "kotlin", "name": "Kotlin"}]},
            "bootVersion": {"default": "3.5.0", "values": [{"id": "3.5.1-SNAPSHOT", "name": "3.5.1 (SNAPSHOT)"}, {"id": "3.5.0", "name": "3.5.0"}]},
            "packaging": {"default": "jar", "values": [{"id": "jar", "name": "Jar"}, {"id": "war", "name": "War"}]},
            "javaVersion": {"default": "17", "values": [{"id": "21", "name": "21"}, {"id": "17", "name": "17"}]},
            "groupId": {"default": "com.example"},
            "version": {"default": "0.0.1-SNAPSHOT"},
            "description": {"default": "Demo project for Spring Boot"},
            "dependencies": {"values": [
                {"name": "Web", "values": [{"id": "web", "name": "Spring Web", "description": "MVC"}]},
                {"name": "SQL", "values": [{"id": "data-jpa", "name": "Spring Data JPA"}]}
            ]}
        })
    }

    fn cx_for(name: &str, given: Value) -> Result<Plan, Problem> {
        let fields = fields_from_metadata(&meta());
        let map: Map<String, Value> = given.as_object().cloned().unwrap_or_default();
        plan_spring(&PlanCx { name, opts: Opts::resolve(&fields, &map), git: false })
    }

    #[test]
    fn metadata_becomes_fields() {
        let f = fields_from_metadata(&meta());
        let get = |k: &str| f.iter().find(|x| x.key == k).unwrap();
        let types: Vec<_> = get("type").options.iter().map(|o| o.value.as_str()).collect();
        assert_eq!(types, ["maven-project", "gradle-project-kotlin"], "build-only types are dropped");
        assert_eq!(get("boot").default, json!("3.5.0"));
        assert!(get("boot").options[0].label.contains("preview"));
        assert_eq!(get("java").default, json!("17"));
        let deps = &get("dependencies").options;
        assert_eq!(deps.len(), 2);
        assert_eq!(deps[0].group.as_deref(), Some("Web"));
        assert_eq!(deps[0].description.as_deref(), Some("MVC"));
    }

    #[test]
    fn empty_metadata_does_not_panic() {
        let f = fields_from_metadata(&json!({}));
        assert!(f.iter().any(|x| x.key == "dependencies"));
    }

    #[test]
    fn the_url_carries_the_options_encoded() {
        let plan = cx_for("my-app", json!({"dependencies": ["web", "data-jpa"], "description": "A & B"})).unwrap();
        let Source::Download { url } = &plan.source else { panic!() };
        assert!(url.starts_with("https://start.spring.io/starter.zip?"));
        for needle in ["type=maven-project", "artifactId=my-app", "groupId=com.example", "dependencies=web%2Cdata-jpa", "description=A+%26+B", "bootVersion=3.5.0", "javaVersion=17"] {
            assert!(url.contains(needle), "{needle} in {url}");
        }
        assert!(!url.contains("packageName"));
        assert_eq!(plan.open[0], "src/main/java/com/example/myapp/MyAppApplication.java");
    }

    #[test]
    fn hostile_values_are_refused() {
        assert_eq!(cx_for("x", json!({"type": "a&b=c"})).unwrap_err().field, "type");
        assert_eq!(cx_for("x", json!({"dependencies": ["web", "../x"]})).unwrap_err().field, "dependencies");
        assert_eq!(cx_for("x", json!({"group": "a b"})).unwrap_err().field, "group");
        assert_eq!(cx_for("x", json!({"description": "line\nbreak"})).unwrap_err().field, "description");
    }

    #[test]
    fn kotlin_opens_a_kotlin_file() {
        let plan = cx_for("svc", json!({"language": "kotlin"})).unwrap();
        assert_eq!(plan.open[0], "src/main/kotlin/com/example/svc/SvcApplication.kt");
    }
}
