//! A test class for the class at the caret, shaped by what it is: a controller gets a MockMvc slice test
//! with a call per known endpoint, a service a Mockito unit test, a repository a `@DataJpaTest`, anything
//! else a plain JUnit class.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::code::JavaFile;
use crate::context::{ClassInfo, EndpointInfo, FieldInfo, GenContext};
use crate::names::{camel, java_string};
use crate::output::{parse_options, require_class_name, GenFile, Output, SourceRoot};
use crate::registry::{Generator, GeneratorMeta, Target};

pub struct TestSkeleton;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct TestOptions {
    /// `auto` | `controller` | `service` | `repository` | `plain`
    pub kind: String,
    /// `5` | `4`
    pub junit: String,
    /// `assertj` | `junit`
    pub assertions: String,
    /// Methods to write a test for; empty means every public method known.
    pub methods: Vec<String>,
    pub class_name: String,
}

impl Default for TestOptions {
    fn default() -> Self {
        TestOptions { kind: "auto".into(), junit: "5".into(), assertions: "assertj".into(), methods: Vec::new(), class_name: String::new() }
    }
}

impl Generator for TestSkeleton {
    fn meta(&self) -> GeneratorMeta {
        GeneratorMeta {
            id: "test-skeleton",
            title: "Generate test class",
            description: "A MockMvc, Mockito, @DataJpaTest or plain JUnit test skeleton for the class at the caret, with a test per method.",
            icon: "FlaskConical",
            group: "Test",
            language: "java",
            targets: &[Target::Class, Target::Java],
            related: &["junit-jupiter", "mockito-core", "spring-boot-starter-test"],
        }
    }

    fn defaults(&self, ctx: &GenContext) -> Value {
        let mut o = TestOptions::default();
        let p = &ctx.project;
        if p.has("junit") && !p.has("junit-jupiter") && !p.has("junit-jupiter-api") {
            o.junit = "4".into();
        }
        if !p.has("assertj-core") && p.resolved {
            o.assertions = "junit".into();
        }
        if let Some(c) = &ctx.class {
            o.methods = c.methods.clone();
        }
        serde_json::to_value(o).unwrap_or(Value::Null)
    }

    fn generate(&self, options: &Value, ctx: &GenContext) -> Result<Output, String> {
        let o: TestOptions = parse_options(options)?;
        let class = ctx.class.as_ref().ok_or("Place the caret in the class to test")?;
        let kind = resolve_kind(&o, class);
        let name = if o.class_name.trim().is_empty() { format!("{}Test", class.name) } else { o.class_name.trim().to_string() };
        require_class_name(&name, "Class name")?;
        let methods: Vec<&String> = class.methods.iter().filter(|m| (o.methods.is_empty() || o.methods.contains(m)) && !is_noise(m)).collect();

        let mut f = JavaFile::new(&class.package);
        let ctx_ = Ctx { o: &o, class, kind: &kind, ctx, junit5: o.junit != "4" };
        ctx_.imports(&mut f);
        ctx_.write(&mut f, &name, &methods);
        let mut out = Output::file(GenFile::java(SourceRoot::Test, &class.package, &name, f.render()));
        if kind == "controller" {
            let known = ctx.endpoints.iter().filter(|e| e.controller == class.name).count();
            if known == 0 {
                out = out.note("No request mappings were found for this controller — the tests are plain placeholders.");
            }
        }
        if ctx.project.resolved && !ctx.project.has("spring-boot-starter-test") && kind != "plain" && kind != "service" {
            out = out.note("The Spring test slices need `spring-boot-starter-test`.");
        }
        let needs: &[&str] = match kind.as_str() {
            "service" => &["org.mockito:mockito-core"],
            "controller" | "repository" => &["org.springframework.boot:spring-boot-starter-test"],
            _ => &[],
        };
        Ok(out.needs(ctx.project.missing(needs)))
    }
}

fn is_noise(method: &str) -> bool {
    matches!(method, "equals" | "hashCode" | "toString" | "main")
}

fn resolve_kind(o: &TestOptions, class: &ClassInfo) -> String {
    if o.kind != "auto" {
        return o.kind.clone();
    }
    if class.has_annotation("RestController") || class.has_annotation("Controller") {
        "controller".into()
    } else if class.has_annotation("Repository") || class.superclass.as_deref().is_some_and(|s| s.contains("Repository")) || class.kind == "interface" && class.name.ends_with("Repository") {
        "repository".into()
    } else if class.has_annotation("Service") || class.has_annotation("Component") || !dependencies(class).is_empty() {
        "service".into()
    } else {
        "plain".into()
    }
}

/// Injected collaborators: final fields and `@Autowired`/`@Inject`/`@Resource` ones.
fn dependencies(class: &ClassInfo) -> Vec<&FieldInfo> {
    class.fields.iter().filter(|f| f.is_final || f.has_annotation("Autowired") || f.has_annotation("Inject") || f.has_annotation("Resource")).collect()
}

struct Ctx<'a> {
    o: &'a TestOptions,
    class: &'a ClassInfo,
    kind: &'a str,
    ctx: &'a GenContext,
    junit5: bool,
}

impl Ctx<'_> {
    fn imports(&self, f: &mut JavaFile) {
        let im = &mut f.imports;
        if self.junit5 {
            im.add("org.junit.jupiter.api.Test");
        } else {
            im.add("org.junit.Test");
        }
        if self.o.assertions == "assertj" {
            im.add_static("org.assertj.core.api.Assertions.assertThat");
        } else if self.junit5 {
            im.add_static("org.junit.jupiter.api.Assertions.assertNotNull");
        } else {
            im.add_static("org.junit.Assert.assertNotNull");
        }
        match self.kind {
            "controller" => {
                im.add("org.springframework.beans.factory.annotation.Autowired");
                im.add("org.springframework.boot.test.autoconfigure.web.servlet.WebMvcTest");
                im.add("org.springframework.test.web.servlet.MockMvc");
                im.add_static("org.springframework.test.web.servlet.request.MockMvcRequestBuilders.*");
                im.add_static("org.springframework.test.web.servlet.result.MockMvcResultMatchers.status");
                if !dependencies(self.class).is_empty() {
                    im.add(self.mock_bean_import());
                }
                if !self.junit5 {
                    im.add("org.junit.runner.RunWith");
                    im.add("org.springframework.test.context.junit4.SpringRunner");
                }
            }
            "service" => {
                im.add("org.mockito.InjectMocks");
                im.add("org.mockito.Mock");
                if self.junit5 {
                    im.add("org.junit.jupiter.api.extension.ExtendWith");
                    im.add("org.mockito.junit.jupiter.MockitoExtension");
                } else {
                    im.add("org.junit.runner.RunWith");
                    im.add("org.mockito.junit.MockitoJUnitRunner");
                }
            }
            "repository" => {
                im.add("org.springframework.beans.factory.annotation.Autowired");
                im.add("org.springframework.boot.test.autoconfigure.orm.jpa.DataJpaTest");
                if !self.junit5 {
                    im.add("org.junit.runner.RunWith");
                    im.add("org.springframework.test.context.junit4.SpringRunner");
                }
            }
            _ => {}
        }
    }

    /// `@MockitoBean` from Spring Framework 6.2 (Boot 3.4), `@MockBean` before.
    fn mock_bean(&self) -> &'static str {
        let boot = self.ctx.project.spring_boot();
        if boot.is_some_and(|v| v.at_least(3, 4)) { "MockitoBean" } else { "MockBean" }
    }

    fn mock_bean_import(&self) -> &'static str {
        if self.mock_bean() == "MockitoBean" {
            "org.springframework.test.context.bean.override.mockito.MockitoBean"
        } else {
            "org.springframework.boot.test.mock.mockito.MockBean"
        }
    }

    fn write(&self, f: &mut JavaFile, name: &str, methods: &[&String]) {
        let class = self.class;
        let var = camel(&class.name);
        let kind = self.kind;
        match (kind, self.junit5) {
            ("controller", true) => {
                f.body.line(format!("@WebMvcTest({}.class)", class.name));
            }
            ("controller", false) => {
                f.body.line("@RunWith(SpringRunner.class)");
                f.body.line(format!("@WebMvcTest({}.class)", class.name));
            }
            ("service", true) => {
                f.body.line("@ExtendWith(MockitoExtension.class)");
            }
            ("service", false) => {
                f.body.line("@RunWith(MockitoJUnitRunner.class)");
            }
            ("repository", true) => {
                f.body.line("@DataJpaTest");
            }
            ("repository", false) => {
                f.body.line("@RunWith(SpringRunner.class)");
                f.body.line("@DataJpaTest");
            }
            _ => {}
        }
        let visibility = if self.junit5 { "" } else { "public " };
        let public_class = if self.junit5 { "" } else { "public " };
        f.body.block(format!("{public_class}class {name}"), |l| {
            match kind {
                "controller" => {
                    l.line("@Autowired");
                    l.line("private MockMvc mockMvc;");
                    for dep in dependencies(class) {
                        l.blank();
                        l.line(format!("@{}", self.mock_bean()));
                        l.line(format!("private {} {};", dep.type_name, dep.name));
                    }
                }
                "service" => {
                    for dep in dependencies(class) {
                        l.line("@Mock");
                        l.line(format!("private {} {};", dep.type_name, dep.name));
                        l.blank();
                    }
                    l.line("@InjectMocks");
                    l.line(format!("private {} {var};", class.name));
                }
                "repository" => {
                    l.line("@Autowired");
                    l.line(format!("private {} {var};", class.name));
                }
                _ => {
                    l.line(format!("private final {0} {var} = new {0}();", class.name));
                }
            }
            if kind == "controller" {
                let endpoints: Vec<&EndpointInfo> = self.ctx.endpoints.iter().filter(|e| e.controller == class.name).collect();
                if !endpoints.is_empty() {
                    for e in endpoints {
                        l.blank();
                        self.mock_mvc_test(l, e, visibility);
                    }
                    return;
                }
            }
            if methods.is_empty() {
                l.blank();
                l.line("@Test");
                l.block(format!("{visibility}void {}Works()", var), |l| {
                    l.line("// TODO");
                });
                return;
            }
            for m in methods {
                l.blank();
                l.line("@Test");
                l.block(format!("{visibility}void {m}_shouldWork()"), |l| {
                    l.line("// Arrange");
                    l.line("// Act");
                    l.line("// Assert");
                    if self.o.assertions == "assertj" {
                        l.line(format!("assertThat({var}).isNotNull();"));
                    } else {
                        l.line(format!("assertNotNull({var});"));
                    }
                });
            }
        });
    }

    fn mock_mvc_test(&self, l: &mut crate::code::Lines, e: &EndpointInfo, visibility: &str) {
        let verb = e.method().to_lowercase();
        let name = format!("{}_returnsOk", e.handler);
        let path_values: Vec<String> = e.params_bound("path").iter().map(|p| placeholder(&p.type_text)).collect();
        let mut uri = format!("{}({}", verb, java_string(&e.path));
        for v in &path_values {
            uri.push_str(&format!(", {v}"));
        }
        uri.push(')');
        l.line("@Test");
        l.block(format!("{visibility}void {name}() throws Exception"), |l| {
            l.line("// TODO: stub the collaborators this call reaches");
            let mut request = vec![format!("mockMvc.perform({uri}")];
            for q in e.params_bound("query").iter().filter(|q| q.required) {
                request.push(format!("        .param({}, {})", java_string(&q.name), java_string("1")));
            }
            for h in e.params_bound("header").iter().filter(|h| h.required) {
                request.push(format!("        .header({}, {})", java_string(&h.name), java_string("value")));
            }
            if e.params.iter().any(|p| p.binding == "body") {
                request.push("        .contentType(\"application/json\")".to_string());
                request.push("        .content(\"{}\")".to_string());
            }
            if let Some(last) = request.last_mut() {
                last.push(')');
            }
            for line in request {
                l.line(line);
            }
            l.line("        .andExpect(status().isOk());");
        });
    }
}

/// A literal that fits a path variable of the given type.
fn placeholder(ty: &str) -> String {
    match ty.trim() {
        "Long" | "long" => "1L".to_string(),
        "Integer" | "int" => "1".to_string(),
        "UUID" => "java.util.UUID.randomUUID()".to_string(),
        _ => java_string("1"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::*;

    fn service() -> GenContext {
        let mut ctx = sample_context();
        let mut deps = FieldInfo::new("repository", "OrderRepository");
        deps.is_final = true;
        let mut class = ClassInfo::new("OrderService", "com.acme.shop.service", vec![deps]);
        class.annotations = vec!["Service".into()];
        class.methods = vec!["place".into(), "cancel".into(), "toString".into()];
        ctx.class = Some(class);
        ctx
    }

    #[test]
    fn a_service_gets_mocks_inject_mocks_and_a_test_per_method() {
        let out = TestSkeleton.generate(&serde_json::json!({}), &service()).unwrap();
        let file = &out.files[0];
        assert_eq!(file.name, "OrderServiceTest.java");
        assert_eq!(file.root, SourceRoot::Test);
        assert_eq!(file.package, "com.acme.shop.service");
        let text = &file.text;
        assert!(text.contains("@ExtendWith(MockitoExtension.class)"), "{text}");
        assert!(text.contains("@Mock\n    private OrderRepository repository;"), "{text}");
        assert!(text.contains("@InjectMocks\n    private OrderService orderService;"), "{text}");
        assert!(text.contains("void place_shouldWork() {"), "{text}");
        assert!(text.contains("void cancel_shouldWork() {"), "{text}");
        assert!(!text.contains("toString_shouldWork"), "{text}");
        assert!(text.contains("import static org.assertj.core.api.Assertions.assertThat;"), "{text}");
    }

    #[test]
    fn a_controller_calls_each_known_endpoint_with_mockmvc() {
        let mut ctx = sample_context();
        let mut class = ClassInfo::new("OrderController", "com.acme.shop.api", Vec::new());
        class.annotations = vec!["RestController".into()];
        ctx.class = Some(class);
        let out = TestSkeleton.generate(&serde_json::json!({}), &ctx).unwrap();
        let text = &out.files[0].text;
        assert!(text.contains("@WebMvcTest(OrderController.class)"), "{text}");
        assert!(text.contains("mockMvc.perform(get(\"/api/orders/{id}\", 1L)"), "{text}");
        assert!(text.contains(".param(\"status\", \"1\")"), "{text}");
        assert!(text.contains(".header(\"X-Tenant\", \"value\")"), "{text}");
        assert!(text.contains("void create_returnsOk() throws Exception"), "{text}");
        assert!(text.contains("@MockitoBean") || text.contains("@MockBean") || !text.contains("@Mock"), "{text}");
    }

    #[test]
    fn junit_4_uses_runners_and_public_methods() {
        let out = TestSkeleton.generate(&serde_json::json!({ "junit": "4" }), &service()).unwrap();
        let text = &out.files[0].text;
        assert!(text.contains("@RunWith(MockitoJUnitRunner.class)"), "{text}");
        assert!(text.contains("public class OrderServiceTest"), "{text}");
        assert!(text.contains("public void place_shouldWork()"), "{text}");
        assert!(text.contains("import org.junit.Test;"), "{text}");
    }

    #[test]
    fn a_repository_is_a_data_jpa_test_and_plain_classes_are_new_ed() {
        let mut ctx = sample_context();
        let mut repo = ClassInfo::new("OrderRepository", "p", Vec::new());
        repo.kind = "interface".into();
        ctx.class = Some(repo);
        let out = TestSkeleton.generate(&serde_json::json!({}), &ctx).unwrap();
        assert!(out.files[0].text.contains("@DataJpaTest"), "{}", out.files[0].text);
        let mut plain = sample_context();
        plain.class = Some(ClassInfo::new("Helper", "p", vec![FieldInfo::new("x", "int")]));
        let out = TestSkeleton.generate(&serde_json::json!({}), &plain).unwrap();
        assert!(out.files[0].text.contains("private final Helper helper = new Helper();"), "{}", out.files[0].text);
    }

    #[test]
    fn no_class_is_an_error() {
        let mut ctx = sample_context();
        ctx.class = None;
        assert!(TestSkeleton.generate(&serde_json::json!({}), &ctx).is_err());
    }
}
