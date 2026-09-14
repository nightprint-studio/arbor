//! The extension — what a host registers.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};

use bennu_ext::prelude::{
    ExtEntry, ExtGutterMark, ExtHover, ExtMemory, ExtStat, ExtTarget, FileCtx, FrameworkExtension,
    ProjectScan,
};
use bennu_proto::prelude::{CapabilitySet, CompletionItem, Diagnostic};

use crate::model::{JaxRsModel, ResourceMethod};
use crate::{checks, index, intel};

#[derive(Default)]
pub struct JaxRsExtension {
    model: RwLock<Arc<JaxRsModel>>,
    scanned: AtomicBool,
}

impl JaxRsExtension {
    pub fn new() -> Self {
        Self::default()
    }

    fn model(&self) -> Arc<JaxRsModel> {
        self.model.read().map(|m| Arc::clone(&m)).unwrap_or_default()
    }

    /// The buffer's resource methods and the model they were read against — `None` for a file this
    /// extension has no business with.
    fn buffer(&self, ctx: &FileCtx<'_>) -> Option<(Arc<JaxRsModel>, Vec<ResourceMethod>)> {
        if ctx.extension() != "java" {
            return None;
        }
        let model = self.model();
        if !index::relevant(&model, ctx.source) {
            return None;
        }
        let methods = index::read_buffer(&model, &ctx.path_str(), ctx.source)?;
        Some((model, methods))
    }
}

impl FrameworkExtension for JaxRsExtension {
    fn id(&self) -> &'static str {
        "jaxrs"
    }

    fn display_name(&self) -> &'static str {
        "JAX-RS"
    }

    fn applies(&self, caps: &CapabilitySet) -> bool {
        caps.jaxrs
    }

    fn reindex(&self, scan: &ProjectScan<'_>) {
        let model = index::build(scan);
        if let Ok(mut slot) = self.model.write() {
            *slot = Arc::new(model);
        }
        self.scanned.store(true, Ordering::Release);
    }

    fn is_ready(&self) -> bool {
        self.scanned.load(Ordering::Acquire)
    }

    fn diagnostics(&self, ctx: &FileCtx<'_>) -> Vec<Diagnostic> {
        let Some((model, methods)) = self.buffer(ctx) else { return Vec::new() };
        checks::diagnostics(&ctx.path_str(), &methods, &model, self.scanned.load(Ordering::Acquire))
    }

    fn completions(&self, ctx: &FileCtx<'_>, offset: usize) -> Vec<CompletionItem> {
        let Some((model, methods)) = self.buffer(ctx) else { return Vec::new() };
        intel::completions(&methods, &model.ctx.prefix, ctx.source, offset)
    }

    fn hover(&self, ctx: &FileCtx<'_>, offset: usize) -> Option<ExtHover> {
        let (model, methods) = self.buffer(ctx)?;
        intel::hover(&methods, &model.ctx.prefix, offset)
    }

    fn navigate(&self, ctx: &FileCtx<'_>, offset: usize) -> Vec<ExtTarget> {
        let Some((_, methods)) = self.buffer(ctx) else { return Vec::new() };
        intel::navigate(&methods, offset)
    }

    fn gutter(&self, ctx: &FileCtx<'_>) -> Vec<ExtGutterMark> {
        let Some((model, methods)) = self.buffer(ctx) else { return Vec::new() };
        intel::gutter(&methods, &model.ctx.prefix)
    }

    /// The bare `endpoints` kind: the registry unions it with Spring's and Struts', so the panel is
    /// about URLs rather than about whichever framework registered first.
    fn catalog(&self, kind: &str) -> Vec<ExtEntry> {
        if kind != "endpoints" {
            return Vec::new();
        }
        intel::catalog(&self.model())
    }

    fn stats(&self) -> Vec<ExtStat> {
        vec![ExtStat {
            label: "JAX-RS routes".to_string(),
            value: self.model().routes().count(),
            catalog: Some("endpoints".to_string()),
        }]
    }

    fn memory(&self) -> Vec<ExtMemory> {
        let model = self.model();
        let held: usize = model.ctx.interfaces.iter().map(|u| u.text.len()).sum();
        vec![
            ExtMemory::counted("JAX-RS resource methods", model.methods.len()),
            ExtMemory::text("Annotated interface sources", model.ctx.interfaces.len(), held),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::checks::*;
    use bennu_ext::prelude::ScannedFile;
    use std::path::{Path, PathBuf};

    const ROOT: &str = "/p/src/main/java/com/acme/";

    fn file(name: &str, text: &str) -> ScannedFile {
        ScannedFile { path: PathBuf::from(format!("{ROOT}{name}")), text: text.to_string() }
    }

    fn indexed(java: &[ScannedFile], xml: &[ScannedFile]) -> JaxRsExtension {
        let ext = JaxRsExtension::new();
        ext.reindex(&ProjectScan { java, xml, ..ProjectScan::empty(Path::new("/p")) });
        ext
    }

    fn ctx(f: &ScannedFile) -> FileCtx<'_> {
        FileCtx { path: &f.path, source: &f.text }
    }

    fn codes(ext: &JaxRsExtension, f: &ScannedFile) -> Vec<String> {
        ext.diagnostics(&ctx(f)).into_iter().map(|d| d.code).collect()
    }

    const APP: &str = "package com.acme;\nimport jakarta.ws.rs.ApplicationPath;\nimport jakarta.ws.rs.core.Application;\n@ApplicationPath(\"/api\")\npublic class App extends Application {}\n";

    const ORDERS: &str = "package com.acme;\n\
        import jakarta.ws.rs.*;\n\
        @Path(\"orders\")\n\
        @Produces(\"application/json\")\n\
        public class OrderResource {\n\
        @GET @Path(\"{id}\")\n\
        public Order find(@PathParam(\"id\") long id, @QueryParam(\"q\") @DefaultValue(\"x\") String query) { return null; }\n\
        }\n";

    /// A resource with every route-level problem a test below might want, one at a time.
    fn resource(class_path: &str, body: &str) -> String {
        format!(
            "package com.acme;\nimport javax.ws.rs.*;\nimport javax.ws.rs.core.*;\nimport javax.ws.rs.container.*;\n{class_path}\npublic class R {{\n{body}\n}}\n"
        )
    }

    // ── the catalog ──────────────────────────────────────────────────────────

    #[test]
    fn a_route_is_one_row_shaped_like_springs() {
        let ext = indexed(&[file("App.java", APP), file("OrderResource.java", ORDERS)], &[]);
        let rows = ext.catalog("endpoints");
        assert_eq!(rows.len(), 1, "{rows:#?}");
        let r = &rows[0];
        assert_eq!(r.id, "GET /api/orders/{id}");
        assert_eq!(r.primary, "/api/orders/{id}");
        assert_eq!(r.secondary, "OrderResource#find");
        assert_eq!(r.kind, "GET");
        assert_eq!(r.file.as_deref(), Some("/p/src/main/java/com/acme/OrderResource.java"));
        assert_eq!(r.offset, ORDERS.find("find(").map(Some).unwrap());
        assert_eq!(r.line, Some(7));
        assert_eq!(r.tags, ["Order", "application/json"]);
        let kids: Vec<(&str, &str, &str, &str)> = r
            .children
            .iter()
            .map(|c| (c.id.as_str(), c.primary.as_str(), c.secondary.as_str(), c.kind.as_str()))
            .collect();
        assert_eq!(kids, [("id", "id", "long", "path"), ("query", "q", "String", "query")]);
        assert!(r.children[0].tags.is_empty());
        assert_eq!(r.children[1].tags, ["optional"], "@DefaultValue");

        assert!(ext.catalog("jaxrs.endpoints").is_empty(), "the registry strips the namespace, not this");
        assert!(ext.catalog("beans").is_empty());
        let stat = &ext.stats()[0];
        assert_eq!((stat.label.as_str(), stat.value, stat.catalog.as_deref()), ("JAX-RS routes", 1, Some("endpoints")));
    }

    #[test]
    fn a_resteasy_servlet_mapping_prefixes_a_javax_resource() {
        let xml = ScannedFile {
            path: PathBuf::from("/p/src/main/webapp/WEB-INF/web.xml"),
            text: "<web-app><servlet><servlet-name>rest</servlet-name><servlet-class>org.jboss.resteasy.plugins.server.servlet.HttpServletDispatcher</servlet-class></servlet>\
                   <servlet-mapping><servlet-name>rest</servlet-name><url-pattern>/rest/*</url-pattern></servlet-mapping></web-app>"
                .to_string(),
        };
        let ext = indexed(&[file("OrderResource.java", &ORDERS.replace("jakarta", "javax"))], &[xml]);
        assert_eq!(ext.catalog("endpoints")[0].id, "GET /rest/orders/{id}");
    }

    #[test]
    fn several_application_paths_tag_the_rows_instead_of_guessing() {
        let admin = APP.replace("/api", "/admin").replace("class App", "class Admin");
        let ext = indexed(&[file("App.java", APP), file("Admin.java", &admin), file("OrderResource.java", ORDERS)], &[]);
        let r = &ext.catalog("endpoints")[0];
        assert_eq!(r.primary, "/orders/{id}");
        assert!(r.tags.iter().any(|t| t == "application path unknown"), "{:?}", r.tags);
    }

    #[test]
    fn a_locator_is_a_row_and_what_it_returns_is_not_joined() {
        let src = resource(
            "@Path(\"customers\")",
            "  @Path(\"{id}/orders\") public Orders orders() { return null; }\n}\nclass Orders {\n  @GET public String list() { return null; }",
        );
        let ext = indexed(&[file("R.java", &src)], &[]);
        let rows = ext.catalog("endpoints");
        assert_eq!(rows.iter().map(|r| r.id.as_str()).collect::<Vec<_>>(), ["LOCATOR /customers/{id}/orders"]);
    }

    #[test]
    fn a_project_verb_declared_with_http_method_is_a_verb() {
        let verb = file("Lock.java", "package com.acme;\nimport javax.ws.rs.HttpMethod;\n@HttpMethod(\"LOCK\") public @interface Lock {}\n");
        let src = resource("@Path(\"files\")", "  @Lock public void lock() {}");
        let ext = indexed(&[verb, file("R.java", &src)], &[]);
        assert_eq!(ext.catalog("endpoints")[0].id, "LOCK /files");
    }

    #[test]
    fn a_projects_own_path_annotation_is_not_jaxrs() {
        let src = "package com.acme;\nimport com.acme.web.Path;\nimport com.acme.web.GET;\nimport javax.ws.rs.core.Response;\n@Path(\"x\") public class R { @GET Response m() { return null; } }\n";
        let f = file("R.java", src);
        let ext = indexed(std::slice::from_ref(&f), &[]);
        assert!(ext.catalog("endpoints").is_empty());
        assert!(codes(&ext, &f).is_empty(), "not even the non-public warning");
        assert!(ext.gutter(&ctx(&f)).is_empty());
    }

    #[test]
    fn an_annotated_interface_routes_to_the_class_that_implements_it() {
        let api = file(
            "OrderApi.java",
            "package com.acme;\nimport javax.ws.rs.*;\n@Path(\"orders\")\npublic interface OrderApi {\n  @GET @Path(\"{id}\") String find(@PathParam(\"id\") long id);\n}\n",
        );
        let imp = file("OrderService.java", "package com.acme;\npublic class OrderService implements OrderApi {\n  public String find(long id) { return null; }\n}\n");
        let ext = indexed(&[api.clone(), imp.clone()], &[]);
        let rows = ext.catalog("endpoints");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].secondary, "OrderService#find");
        assert!(rows[0].file.as_deref().is_some_and(|f| f.ends_with("OrderService.java")));
        let marks = ext.gutter(&ctx(&imp));
        assert_eq!((marks.len(), marks[0].line), (1, 3), "the implementation gets the mark");
        assert!(ext.gutter(&ctx(&api)).is_empty());

        let alone = indexed(std::slice::from_ref(&api), &[]);
        assert_eq!(alone.catalog("endpoints")[0].secondary, "OrderApi#find", "no implementation: the interface");
    }

    // ── gutter, hover, completion, navigation ─────────────────────────────────

    #[test]
    fn the_route_is_the_gutter_tooltip_and_the_hover_title() {
        let ext = indexed(&[file("App.java", APP), file("OrderResource.java", ORDERS)], &[]);
        let f = file("OrderResource.java", ORDERS);
        let marks = ext.gutter(&ctx(&f));
        assert_eq!(marks.len(), 1);
        assert_eq!((marks[0].line, marks[0].kind.as_str(), marks[0].tooltip.as_str()), (7, "endpoint", "GET /api/orders/{id}"));
        assert!(marks[0].targets.is_empty());

        let h = ext.hover(&ctx(&f), ORDERS.find("@GET").unwrap() + 2).expect("hover on the verb");
        assert_eq!(h.title, "GET /api/orders/{id}");
        assert_eq!(h.signature, "OrderResource#find");
        assert!(h.doc.contains("Produces: application/json"), "{}", h.doc);
        assert!(ext.hover(&ctx(&f), ORDERS.find("@Path(\"{id}\")").unwrap() + 1).is_some(), "and on the method @Path");
        assert!(ext.hover(&ctx(&f), ORDERS.find("long id").unwrap()).is_none());
    }

    #[test]
    fn inside_a_path_param_the_routes_variables_are_offered_over_the_string() {
        let ext = indexed(&[file("App.java", APP), file("OrderResource.java", ORDERS)], &[]);
        let src = ORDERS.replace("@PathParam(\"id\")", "@PathParam(\"i\")");
        let f = file("OrderResource.java", &src);
        let start = src.find("@PathParam(\"").unwrap() + "@PathParam(\"".len();
        let items = ext.completions(&ctx(&f), start + 1);
        assert_eq!(items.iter().map(|i| i.label.as_str()).collect::<Vec<_>>(), ["id"]);
        assert_eq!((items[0].replace_start, items[0].replace_end), (Some(start), Some(start + 1)));
        assert_eq!(items[0].kind, "path-variable");
        assert!(ext.completions(&ctx(&f), src.find("@QueryParam(\"").unwrap() + 13).is_empty(), "not in a query param");
    }

    #[test]
    fn a_path_param_navigates_to_the_variable_it_binds() {
        let ext = indexed(&[file("OrderResource.java", ORDERS)], &[]);
        let f = file("OrderResource.java", ORDERS);
        let at = ORDERS.find("@PathParam(\"id\")").unwrap() + "@PathParam(\"".len() + 1;
        let targets = ext.navigate(&ctx(&f), at);
        assert_eq!(targets.len(), 1);
        let path_at = ORDERS.find("@Path(\"{id}\")").unwrap();
        assert_eq!(targets[0].offset, path_at + "@Path(\"{".len());
        assert_eq!(&ORDERS[targets[0].offset..targets[0].offset + 2], "id");
        assert_eq!(targets[0].label, "{id}");
    }

    // ── unknown path params ───────────────────────────────────────────────────

    #[test]
    fn a_path_param_the_route_does_not_declare_is_reported_on_its_string() {
        let src = resource(
            "@Path(\"orders/{orderId}\")",
            "  @GET @Path(\"items/{itemId: [0-9]+}\") public String get(@PathParam(\"orderId\") String o, @PathParam(\"itemId\") String i, @PathParam(\"id\") String bad) { return null; }",
        );
        let f = file("R.java", &src);
        let ext = indexed(std::slice::from_ref(&f), &[]);
        let found = ext.diagnostics(&ctx(&f));
        assert_eq!(found.iter().map(|d| d.code.as_str()).collect::<Vec<_>>(), [CODE_UNKNOWN_PATH_PARAM]);
        assert_eq!(&src[found[0].start..found[0].end], "id");
        assert_eq!(found[0].severity, "warning");
    }

    #[test]
    fn a_path_param_is_not_judged_where_the_route_is_not_known() {
        let bad = "  @GET @Path(\"{x}\") public String get(@PathParam(\"id\") String id) { return null; }";
        // No class-level @Path: a sub-resource, whose variables come from a locator.
        let sub = file("R.java", &resource("", bad));
        assert!(codes(&indexed(std::slice::from_ref(&sub), &[]), &sub).is_empty());
        // A class path that is not a literal.
        let opaque = file("R.java", &resource("@Path(Paths.ORDERS)", bad));
        assert!(codes(&indexed(std::slice::from_ref(&opaque), &[]), &opaque).is_empty());
        // A locator somewhere declares `{id}`, and may be what reaches this class.
        let root = file("R.java", &resource("@Path(\"x\")", bad));
        let locator = file(
            "Customers.java",
            "package com.acme;\nimport javax.ws.rs.*;\n@Path(\"customers\") public class Customers { @Path(\"{id}/orders\") public Object orders() { return null; } }\n",
        );
        assert!(codes(&indexed(&[root.clone(), locator], &[]), &root).is_empty());
        // Before the first scan nothing is known about locators at all.
        assert!(codes(&JaxRsExtension::new(), &root).is_empty());
        // And with the project read and no locator, it does speak.
        assert_eq!(codes(&indexed(std::slice::from_ref(&root), &[]), &root), [CODE_UNKNOWN_PATH_PARAM]);
    }

    #[test]
    fn an_interface_with_two_implementations_is_not_judged() {
        let api = file(
            "OrderApi.java",
            "package com.acme;\nimport javax.ws.rs.*;\n@Path(\"orders\")\npublic interface OrderApi {\n  @GET String find(@PathParam(\"id\") long id);\n}\n",
        );
        let a = file("A.java", "package com.acme;\npublic class A implements OrderApi { public String find(long id) { return null; } }\n");
        let b = file("B.java", "package com.acme;\npublic class B implements OrderApi { public String find(long id) { return null; } }\n");
        let ext = indexed(&[api.clone(), a], &[]);
        assert_eq!(codes(&ext, &api), [CODE_UNKNOWN_PATH_PARAM], "one implementation: the pair is resolved");
        let ext = indexed(&[api.clone(), file("A.java", "package com.acme;\npublic class A implements OrderApi { public String find(long id) { return null; } }\n"), b], &[]);
        assert!(codes(&ext, &api).is_empty(), "two: which one is deployed is not in the source");
    }

    // ── duplicate routes ──────────────────────────────────────────────────────

    fn orders_class(name: &str, class_path: &str, method: &str, extra: &str) -> ScannedFile {
        file(
            &format!("{name}.java"),
            &format!(
                "package com.acme;\nimport javax.ws.rs.*;\n@Path(\"{class_path}\")\npublic class {name} {{\n  @GET @Path(\"{method}\") {extra} public String handle{name}(@PathParam(\"id\") String id) {{ return null; }}\n}}\n"
            ),
        )
    }

    #[test]
    fn two_methods_for_one_route_are_reported_on_both() {
        let a = orders_class("A", "orders", "{id}", "");
        let b = orders_class("B", "/orders", "/{orderId}", "");
        let ext = indexed(&[a.clone(), b.clone()], &[]);
        let on_a = ext.diagnostics(&ctx(&a));
        let clash = on_a.iter().find(|d| d.code == CODE_DUPLICATE_ROUTE).expect("a clash on A");
        assert_eq!(clash.severity, "error");
        assert_eq!(&a.text[clash.start..clash.end], "handleA");
        assert!(clash.message.contains("B#handleB"), "{}", clash.message);
        assert!(codes(&ext, &b).contains(&CODE_DUPLICATE_ROUTE.to_string()));
    }

    #[test]
    fn routes_that_differ_in_what_the_runtime_matches_on_do_not_clash() {
        let a = orders_class("A", "orders", "{id}", "");
        let silent = |b: ScannedFile| {
            let ext = indexed(&[a.clone(), b], &[]);
            let found = codes(&ext, &a);
            assert!(!found.contains(&CODE_DUPLICATE_ROUTE.to_string()), "{found:?}");
        };
        silent(orders_class("B", "orders", "{id}", "@Produces(\"application/xml\")"));
        silent(orders_class("B", "orders", "{id: [0-9]+}", ""));
        silent(orders_class("B", "orders/", "{id}", ""));
        silent(orders_class("B", "orders/{id}", "", ""));
        // Another module is another deployment.
        let mut other = orders_class("B", "orders", "{id}", "");
        other.path = PathBuf::from("/q/src/main/java/com/acme/B.java");
        silent(other);
        // A client interface mirroring the server resource.
        silent(file(
            "B.java",
            "package com.acme;\nimport javax.ws.rs.*;\n@Path(\"orders\")\npublic interface B {\n  @GET @Path(\"{id}\") String find(@PathParam(\"id\") String id);\n}\n",
        ));
    }

    #[test]
    fn a_clash_across_applications_or_hand_registered_classes_is_not_claimed() {
        let a = orders_class("A", "orders", "{id}", "");
        let b = orders_class("B", "orders", "{id}", "");
        let app = |name: &str, body: &str| {
            file(
                &format!("{name}.java"),
                &format!("package com.acme;\nimport javax.ws.rs.ApplicationPath;\nimport javax.ws.rs.core.Application;\n@ApplicationPath(\"api\") public class {name} extends Application {{ {body} }}\n"),
            )
        };
        let two_apps = indexed(&[a.clone(), b.clone(), app("One", ""), app("Two", "")], &[]);
        assert!(!codes(&two_apps, &a).contains(&CODE_DUPLICATE_ROUTE.to_string()));
        let listed = indexed(&[a.clone(), b.clone(), app("One", "public java.util.Set<Class<?>> getClasses() { return null; }")], &[]);
        assert!(!codes(&listed, &a).contains(&CODE_DUPLICATE_ROUTE.to_string()));
        let one_app = indexed(&[a.clone(), b, app("One", "")], &[]);
        assert!(codes(&one_app, &a).contains(&CODE_DUPLICATE_ROUTE.to_string()), "one application does clash");
    }

    // ── entities and visibility ───────────────────────────────────────────────

    #[test]
    fn a_second_entity_is_an_error_and_injected_parameters_are_not_entities() {
        let src = resource(
            "@Path(\"o\")",
            "  @POST public void save(Order a, @Valid Order b, @Context UriInfo u, @Suspended AsyncResponse r, @FormDataParam(\"f\") InputStream in) {}",
        );
        let f = file("R.java", &src);
        let found = JaxRsExtension::new().diagnostics(&ctx(&f));
        assert_eq!(found.iter().map(|d| d.code.as_str()).collect::<Vec<_>>(), [CODE_SEVERAL_ENTITY_PARAMS]);
        assert_eq!(&src[found[0].start..found[0].end], "b");

        let fine = file("R.java", &resource("@Path(\"o\")", "  @POST public void save(Order a, @Context UriInfo u, @Suspended AsyncResponse r) {}"));
        assert!(codes(&JaxRsExtension::new(), &fine).is_empty());
    }

    #[test]
    fn a_body_on_a_get_is_weak_and_on_a_post_is_nothing() {
        let get = file("R.java", &resource("@Path(\"o\")", "  @GET public String find(Filter filter, @QueryParam(\"q\") String q) { return null; }"));
        let found = JaxRsExtension::new().diagnostics(&ctx(&get));
        assert_eq!(found.iter().map(|d| (d.code.as_str(), d.severity.as_str())).collect::<Vec<_>>(), [(CODE_GET_WITH_BODY, "weak")]);
        let post = file("R.java", &resource("@Path(\"o\")", "  @POST public String find(Filter filter) { return null; }"));
        assert!(codes(&JaxRsExtension::new(), &post).is_empty());
        let query = file("R.java", &resource("@Path(\"o\")", "  @GET public String find(@QueryParam(\"q\") String q) { return null; }"));
        assert!(codes(&JaxRsExtension::new(), &query).is_empty());
    }

    #[test]
    fn a_non_public_resource_method_is_reported_and_an_interface_method_never_is() {
        let src = resource("@Path(\"o\")", "  @GET String list() { return null; }\n  @GET @Path(\"x\") public String ok() { return null; }");
        let f = file("R.java", &src);
        let found = JaxRsExtension::new().diagnostics(&ctx(&f));
        assert_eq!(found.iter().map(|d| d.code.as_str()).collect::<Vec<_>>(), [CODE_NON_PUBLIC_RESOURCE_METHOD]);
        assert_eq!(&src[found[0].start..found[0].end], "list");

        let api = file("Api.java", "package com.acme;\nimport javax.ws.rs.*;\n@Path(\"o\") interface Api { @GET String list(); }\n");
        assert!(codes(&JaxRsExtension::new(), &api).is_empty());
    }
}
