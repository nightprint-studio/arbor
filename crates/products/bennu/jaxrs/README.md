# bennu-jaxrs

JAX-RS support for Bennu, as a **framework extension** of the [`bennu-ext`](../ext) seam: every
route a project's resources answer, in the shared Endpoints panel beside Spring and Struts, and the
mistakes the runtime only reports when a request arrives — or by refusing to deploy.

```rust
use bennu_jaxrs::prelude::*;
use bennu_ext::prelude::*;

let ext = JaxRsExtension::new();
ext.reindex(&ProjectScan { java, xml, ..ProjectScan::empty(root) });

ext.catalog("endpoints");       // one row per route — unioned with Spring's and Struts' by the registry
ext.gutter(&ctx);               // an `endpoint` mark per resource method, the route as tooltip
ext.hover(&ctx, offset);        // on @GET / @Path: the full route, produces, consumes
ext.completions(&ctx, offset);  // inside @PathParam("…"): the route's variables
ext.navigate(&ctx, offset);     // @PathParam("id") → the {id} in the @Path it binds
ext.diagnostics(&ctx);          // the five checks below
```

## The idea

The URL a request hits is written in three places — `@ApplicationPath("api")` (or a `web.xml`
servlet mapping), `@Path("orders")` on the class, `@Path("{id}")` on the method — and appears nowhere
as one string. Every question about a route starts with joining them by hand. This crate joins them
once, and answers from the result.

## The model

| Piece | Source |
|---|---|
| **Resource classes** | a class-level `@Path`; an abstract class is never a root |
| **Resource methods** | `@GET` `@POST` `@PUT` `@DELETE` `@PATCH` `@HEAD` `@OPTIONS`, and any project annotation meta-annotated `@HttpMethod("X")` |
| **Sub-resource locators** | a `@Path` method with no verb — a `LOCATOR` row; what it returns is *not* joined, since the runtime decides that per request |
| **Interface inheritance** | an annotated interface with exactly one implementing class in the project: the class serves the routes, with the interface's annotations — unless the class method carries a JAX-RS annotation of its own, which discards them, as the runtime does |
| **Application path** | one distinct `@ApplicationPath`; else a `web.xml` mapping of the Jersey / RESTEasy servlet, or of a servlet named after an `Application` subclass, to `/x/*`; several distinct values leave routes unprefixed and tagged `application path unknown` |
| **Deployed set** | classpath scanning by default; `ResourceConfig.packages(…)` or the Jersey `packages` init-param narrows it; `getClasses()` / `register(…)` makes it unknown |
| **Parameters** | `@PathParam` path · `@QueryParam` query · `@FormParam` form · `@HeaderParam` header · `@CookieParam` cookie · `@MatrixParam` matrix · `@BeanParam` bean · `@Context` / `@Suspended` context · unannotated → body; `@DefaultValue` → optional |
| **Media types** | `@Produces` / `@Consumes`, method over class |

Every annotation is resolved through the file's imports ([`bennu-facts`](../facts)'s origin rule), for
`jakarta.ws.rs` and `javax.ws.rs` alike. That is what keeps CDI's `@Produces` — a producer method —
out of the Endpoints panel, and a project's own `@Path` out of everything.

## The checks

| Code | Severity | Reports |
|---|---|---|
| `jaxrs.unknown-path-param` | warning | `@PathParam("x")` where the full route has no `{x}` — the parameter is null on every request |
| `jaxrs.duplicate-route` | error | two resource methods with the same verb, class path, method path (variable names ignored, regexes compared as text), produces and consumes — Jersey refuses the deployment |
| `jaxrs.several-entity-params` | error | a second unannotated parameter — a request has one body |
| `jaxrs.non-public-resource-method` | warning | a verb-annotated method that is not `public` — the runtime skips it without a word |
| `jaxrs.get-with-body` | weak | an entity parameter on `@GET` / `@HEAD` |

The last three read one method and speak from the first keystroke. The first two read the project and
wait for the first scan.

## What it will not judge

Under-report rather than risk a false positive (docs §7). Each of these makes the relevant check
silent:

- **A path that is not one literal** — `@Path(Paths.BASE + "/{id}")`. The scan sees the `"/{id}"`
  inside it; reading that as the path would check parameters against half a template.
- **A class with no class-level `@Path`** — it may be a sub-resource, whose variables come from a
  locator. And **any name a locator's route declares**, since nothing says statically which class a
  locator returns.
- **An interface implemented more than once**, or re-annotated by its implementation: which
  annotations are in effect is not in the source.
- **An entity-looking parameter with an unknown annotation** — Jersey's `@FormDataParam`, RESTEasy's
  `@MultipartForm`, Dropwizard's `@Auth` all supply it from elsewhere. Only parameters with no
  annotation, or only validation / documentation ones, count.
- **Clashes** involving a route declared on an interface (usually a REST client mirroring the
  server), across build modules, between two applications, under hand-written registration, with a
  constant as the media type against its string value, or between two different regex texts.

## Layout

| File | Holds |
|---|---|
| `known.rs` | the annotation table, verb and binding resolution, reading a literal with certainty |
| `paths.rs` | joining, template variables, route identity |
| `model.rs` | `ResourceMethod`, `Param`, `PathSpec`, the project `Context` |
| `prefix.rs` | `@ApplicationPath`, `web.xml`, and what the application deploys |
| `extract.rs` | resource methods out of one parsed file, interface pairing |
| `index.rs` | the model from a scan; one buffer re-read against it |
| `checks.rs` | the diagnostics |
| `intel.rs` | catalog rows, gutter, hover, completion, navigation |
| `ext.rs` | the `FrameworkExtension` impl |

## Public API

Through `bennu_jaxrs::prelude`: `JaxRsExtension`, the five `CODE_*` constants, the model types
(`JaxRsModel`, `ResourceMethod`, `Param`, `PathSpec`, `Lit`, `Site`, `AppPrefix`, `JaxRsContext`,
`Implementor`, `Locators`, `Unit`, `Src`), `build_model` / `read_buffer` / `mentions_jaxrs`,
`extract_resource_methods` / `ExtractEnv`, `applications` / `Applications` / `Registration`,
`join_route` / `route_key` / `path_templates` / `Template`, and `CustomVerb` / `JAXRS_ANNOTATIONS` /
`JAXRS_VERBS`.
