//! Legacy-style Java web applications, generated locally: a Servlet + JSP WAR (`javax` for Tomcat 9
//! and older containers, `jakarta` for Tomcat 10.1+) and a Struts 1 application.
//!
//! These are the shapes Bennu's framework support (JSP, taglibs, Struts navigation) is built for,
//! so the output is deliberately plain: a `web.xml`, a JSP, one servlet or one action with its
//! form and `struts-config.xml`, and a Maven `pom.xml` with `war` packaging. The files are data
//! under `templates/webapp` and `templates/struts`; only the dependency lists are assembled here,
//! because they depend on the chosen flavour.

use serde_json::json;

use super::maven_gradle::{java_fields, java_vars};
use super::model::{Field, Opt, Plan, PlanCx, Problem};
use super::render::{render_set, TemplateSet, Vars};

const POM: &str = include_str!("templates/maven/pom.xml.tpl");
const JAVA_GITIGNORE: &str = include_str!("templates/java/gitignore.tpl");
const JAVA_README: &str = include_str!("templates/java/README.md.tpl");

const SERVLET: TemplateSet = &[
    ("pom.xml", POM),
    ("src/main/webapp/WEB-INF/web.xml", include_str!("templates/webapp/web.xml.tpl")),
    ("src/main/webapp/index.jsp", include_str!("templates/webapp/index.jsp.tpl")),
    ("{{#if sample}}src/main/java/{{package_path}}/HelloServlet.java{{/if}}", include_str!("templates/webapp/HelloServlet.java.tpl")),
    ("{{#unless sample}}src/main/java/.gitkeep{{/unless}}", ""),
    ("{{#if git}}.gitignore{{/if}}", JAVA_GITIGNORE),
    ("README.md", JAVA_README),
];

const STRUTS: TemplateSet = &[
    ("pom.xml", POM),
    ("src/main/webapp/WEB-INF/web.xml", include_str!("templates/struts/web.xml.tpl")),
    ("src/main/webapp/WEB-INF/struts-config.xml", include_str!("templates/struts/struts-config.xml.tpl")),
    ("src/main/webapp/index.jsp", include_str!("templates/struts/index.jsp.tpl")),
    ("src/main/webapp/WEB-INF/jsp/hello.jsp", include_str!("templates/struts/hello.jsp.tpl")),
    ("src/main/java/{{package_path}}/HelloForm.java", include_str!("templates/struts/HelloForm.java.tpl")),
    ("src/main/java/{{package_path}}/HelloAction.java", include_str!("templates/struts/HelloAction.java.tpl")),
    ("src/main/resources/MessageResources.properties", include_str!("templates/struts/MessageResources.properties.tpl")),
    ("{{#if git}}.gitignore{{/if}}", JAVA_GITIGNORE),
    ("README.md", JAVA_README),
];

fn with_default_java(mut fields: Vec<Field>, level: &str) -> Vec<Field> {
    for f in &mut fields {
        if f.key == "java" {
            f.default = json!(level);
        }
    }
    fields
}

pub fn servlet_fields() -> Vec<Field> {
    let mut f = vec![Field::select(
        "flavor",
        "Servlet API",
        "javax",
        vec![
            Opt::described("javax", "javax.servlet (Servlet 4.0)", "Tomcat 9 and older containers"),
            Opt::described("jakarta", "jakarta.servlet (Servlet 6.0)", "Tomcat 10.1+ and Jakarta EE 10"),
        ],
    )];
    f.extend(with_default_java(java_fields(), "17"));
    f.push(Field::toggle("sample", "Add a sample servlet", true));
    f
}

pub fn struts_fields() -> Vec<Field> {
    with_default_java(java_fields(), "8")
}

fn dep(group: &str, artifact: &str, version: &str, scope: Option<&str>) -> String {
    let scope = scope.map_or(String::new(), |s| format!("\n            <scope>{s}</scope>"));
    format!(
        "        <dependency>\n            <groupId>{group}</groupId>\n            <artifactId>{artifact}</artifactId>\n            <version>{version}</version>{scope}\n        </dependency>"
    )
}

fn war_build(artifact: &str) -> String {
    format!(
        "        <finalName>{artifact}</finalName>\n        <plugins>\n            <plugin>\n                <artifactId>maven-war-plugin</artifactId>\n                <version>3.4.0</version>\n                <configuration>\n                    <failOnMissingWebXml>false</failOnMissingWebXml>\n                </configuration>\n            </plugin>\n        </plugins>"
    )
}

fn war_vars(cx: &PlanCx, deps: Vec<String>, what: &str) -> Result<Vars, Problem> {
    let mut v = java_vars(cx)?;
    v.set("packaging", "war")
        .set("deps", deps.join("\n"))
        .set("build", war_build(cx.name))
        .set("description", format!("A {what} web application."))
        .set("build_cmd", "mvn package")
        .set("run_cmd", format!("# copy target/{}.war into Tomcat's webapps folder", cx.name))
        .flag("sample", cx.opts.flag("sample"));
    Ok(v)
}

fn render(cx: &PlanCx, vars: &Vars, set: TemplateSet, summary: Vec<String>, open: &[&str]) -> Result<Plan, Problem> {
    let files = render_set(set, vars).map_err(|m| Problem::new("name", m))?;
    let _ = cx;
    Ok(Plan::files(files, open, summary))
}

pub fn plan_servlet(cx: &PlanCx) -> Result<Plan, Problem> {
    let jakarta = match cx.opts.str("flavor") {
        "javax" => false,
        "jakarta" => true,
        _ => return Err(Problem::new("flavor", "Pick javax or jakarta")),
    };
    if jakarta && cx.opts.str("java") == "8" {
        return Err(Problem::new("java", "Jakarta EE 10 needs Java 11 or newer"));
    }
    let deps = if jakarta {
        vec![
            dep("jakarta.servlet", "jakarta.servlet-api", "6.0.0", Some("provided")),
            dep("jakarta.servlet.jsp", "jakarta.servlet.jsp-api", "3.1.1", Some("provided")),
            dep("jakarta.servlet.jsp.jstl", "jakarta.servlet.jsp.jstl-api", "3.0.0", None),
            dep("org.glassfish.web", "jakarta.servlet.jsp.jstl", "3.0.1", None),
        ]
    } else {
        vec![
            dep("javax.servlet", "javax.servlet-api", "4.0.1", Some("provided")),
            dep("javax.servlet.jsp", "javax.servlet.jsp-api", "2.3.3", Some("provided")),
            dep("javax.servlet", "jstl", "1.2", None),
        ]
    };
    let mut v = war_vars(cx, deps, "Servlet and JSP")?;
    if jakarta {
        v.set("ns", "jakarta")
            .set("xmlns", "https://jakarta.ee/xml/ns/jakartaee")
            .set("xsd", "https://jakarta.ee/xml/ns/jakartaee/web-app_6_0.xsd")
            .set("servlet_version", "6.0")
            .set("jstl_uri", "jakarta.tags.core");
    } else {
        v.set("ns", "javax")
            .set("xmlns", "http://xmlns.jcp.org/xml/ns/javaee")
            .set("xsd", "http://xmlns.jcp.org/xml/ns/javaee/web-app_4_0.xsd")
            .set("servlet_version", "4.0")
            .set("jstl_uri", "http://java.sun.com/jsp/jstl/core");
    }
    let mut open = vec!["src/main/webapp/index.jsp"];
    let servlet_path;
    if cx.opts.flag("sample") {
        let pkg = v_package(cx);
        servlet_path = format!("src/main/java/{}/HelloServlet.java", pkg.replace('.', "/"));
        open.insert(0, &servlet_path);
    }
    render(
        cx,
        &v,
        SERVLET,
        vec![
            format!("Servlet + JSP web app “{}” (WAR)", cx.name),
            format!("{} API, Java {}", if jakarta { "jakarta.servlet 6.0" } else { "javax.servlet 4.0" }, cx.opts.str("java")),
            "web.xml, index.jsp with JSTL, pom.xml with war packaging".into(),
        ],
        &open,
    )
}

pub fn plan_struts(cx: &PlanCx) -> Result<Plan, Problem> {
    let deps = vec![
        dep("org.apache.struts", "struts-core", "1.3.10", None),
        dep("org.apache.struts", "struts-taglib", "1.3.10", None),
        dep("org.apache.struts", "struts-extras", "1.3.10", None),
        dep("javax.servlet", "javax.servlet-api", "3.1.0", Some("provided")),
        dep("javax.servlet.jsp", "javax.servlet.jsp-api", "2.3.3", Some("provided")),
        dep("javax.servlet", "jstl", "1.2", None),
    ];
    let v = war_vars(cx, deps, "Struts 1")?;
    let pkg = v_package(cx);
    let action = format!("src/main/java/{}/HelloAction.java", pkg.replace('.', "/"));
    render(
        cx,
        &v,
        STRUTS,
        vec![
            format!("Struts 1.3 web app “{}” (WAR)", cx.name),
            "struts-config.xml with a form bean, an action and a forward".into(),
            "ActionServlet mapped to *.do, MessageResources.properties".into(),
        ],
        &[&action, "src/main/webapp/WEB-INF/struts-config.xml"],
    )
}

/// The package the shared Java fields resolve to (the explicit one, or group + project name).
fn v_package(cx: &PlanCx) -> String {
    match cx.opts.str("package") {
        "" => format!("{}.{}", cx.opts.str("group"), super::names::package_segment(cx.name)),
        p => p.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::new_project::model::{Opts, Source};
    use serde_json::{Map, Value};

    fn plan(f: fn(&PlanCx) -> Result<Plan, Problem>, fields: Vec<Field>, given: Value) -> Result<Plan, Problem> {
        let map: Map<String, Value> = given.as_object().cloned().unwrap_or_default();
        f(&PlanCx { name: "portal", opts: Opts::resolve(&fields, &map), git: true })
    }

    fn files(p: &Plan) -> Vec<(String, String)> {
        match &p.source {
            Source::Files(f) => f.iter().map(|g| (g.path.clone(), g.content.clone())).collect(),
            _ => panic!(),
        }
    }

    fn get<'a>(fs: &'a [(String, String)], path: &str) -> &'a str {
        &fs.iter().find(|(p, _)| p == path).unwrap_or_else(|| panic!("{path} missing")).1
    }

    #[test]
    fn servlet_javax_defaults() {
        let p = plan(plan_servlet, servlet_fields(), json!({})).unwrap();
        let fs = files(&p);
        let pom = get(&fs, "pom.xml");
        assert!(pom.contains("<packaging>war</packaging>") && pom.contains("javax.servlet-api") && pom.contains("<finalName>portal</finalName>"));
        assert!(get(&fs, "src/main/webapp/WEB-INF/web.xml").contains("version=\"4.0\""));
        assert!(get(&fs, "src/main/java/com/example/portal/HelloServlet.java").contains("import javax.servlet.http.HttpServlet;"));
        assert!(get(&fs, "src/main/webapp/index.jsp").contains("http://java.sun.com/jsp/jstl/core"));
        assert_eq!(p.open[0], "src/main/java/com/example/portal/HelloServlet.java");
        for (path, content) in &fs {
            assert!(!content.contains("{{"), "{path}");
        }
    }

    #[test]
    fn servlet_jakarta_and_the_java_8_conflict() {
        let p = plan(plan_servlet, servlet_fields(), json!({"flavor": "jakarta", "sample": false})).unwrap();
        let fs = files(&p);
        assert!(get(&fs, "src/main/webapp/WEB-INF/web.xml").contains("jakarta.ee/xml/ns/jakartaee"));
        assert!(get(&fs, "src/main/webapp/index.jsp").contains("jakarta.tags.core"));
        assert!(fs.iter().any(|(p, _)| p == "src/main/java/.gitkeep"));
        assert_eq!(plan(plan_servlet, servlet_fields(), json!({"flavor": "jakarta", "java": "8"})).unwrap_err().field, "java");
    }

    #[test]
    fn struts_files_agree_on_the_package() {
        let p = plan(plan_struts, struts_fields(), json!({"group": "it.acme"})).unwrap();
        let fs = files(&p);
        assert!(get(&fs, "src/main/webapp/WEB-INF/struts-config.xml").contains("type=\"it.acme.portal.HelloAction\""));
        assert!(get(&fs, "src/main/java/it/acme/portal/HelloAction.java").starts_with("package it.acme.portal;"));
        assert!(get(&fs, "pom.xml").contains("struts-core"));
        assert!(get(&fs, "pom.xml").contains("<maven.compiler.source>8</maven.compiler.source>"), "Struts defaults to Java 8");
        assert!(get(&fs, "src/main/webapp/WEB-INF/web.xml").contains("org.apache.struts.action.ActionServlet"));
        for (path, content) in &fs {
            assert!(!content.contains("{{"), "{path}");
        }
    }
}
