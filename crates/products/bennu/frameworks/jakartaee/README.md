# bennu-jakartaee

Jakarta EE / Java EE support for Bennu, as a **framework extension** of the [`bennu-ext`](../ext)
seam: CDI and EJB beans, the injection points they satisfy, and the servlets and filters a web
module deploys.

```rust
use bennu_jakartaee::prelude::*;
use bennu_ext::prelude::*;

let ext = JakartaEeExtension::new();
ext.applies(&caps);                     // caps.jakarta_ee — and remembers whether Spring is there
ext.reindex(&ProjectScan { java, xml, ..ProjectScan::empty(root) });

ext.gutter(&ctx);             // `inject` on injection points, `bean` on beans and producers
ext.navigate(&ctx, offset);   // injection point → the beans that may satisfy it
ext.catalog("beans");         // the CDI beans panel (`jakartaee.beans`)
ext.catalog("endpoints");     // servlet and filter URL patterns, beside every other framework's
```

Not to be confused with [`bennu-jakarta`](../jakarta), which is Bean Validation.

## The idea

`@Inject OrderService orders;` says what it wants and nothing about what it gets. The container
decides at deployment, from every bean in every archive, and when the answer is none or two the
application does not start. This crate reads what the container reads — bean-defining annotations,
producers, qualifiers, the archive's discovery mode — and puts the answer beside the field.

## The model

| Piece | Source |
|---|---|
| **Types** | every type read, supertypes resolved through the imports into *project*, *library* or *unresolved* ([`types`](src/types.rs)) — the third answer is what keeps a half-read hierarchy from becoming a report |
| **Discovery mode** | `beans.xml` (`all` / `annotated` / `none`; modeless = `all` unless it declares CDI 4; none at all = `annotated`; disagreement = unknown) ([`archive`](src/archive.rs)) |
| **Beans** | classes with a bean-defining annotation (scopes, `@Model`, EJB kinds, decorators, interceptors, project stereotypes) — *explicit*; plain classes with a usable constructor — *possible*; `@Produces` methods and fields ([`beans`](src/beans.rs)) |
| **Qualifiers** | `@Named` with its defaults, project annotations meta-annotated `@Qualifier`, `@Default`/`@Any` — and *unclassified* for a library annotation nobody can read ([`qualifiers`](src/qualifiers.rs)) |
| **Injection points** | `@Inject` fields, constructor and initializer parameters; `@EJB` fields ([`inject`](src/inject.rs)) |
| **Web components** | `@WebServlet` / `@WebFilter` and `web.xml` mappings, per module, merged as Tomcat merges them ([`web`](src/web.rs)) |

`@Singleton` is resolved twice — `javax.inject` is a pseudo-scope, `javax.ejb` a session bean — and
only CDI's `@Produces` is a producer, never JAX-RS's ([`known`](src/known.rs)).

### Which files are read

Rounds, each widening by what the last showed to matter: files mentioning a platform package; files
named after a supertype or injected type; files that *mention* an injected project type or a known
implementation of one (in an `all` archive any implementor is a bean, and it cannot implement a type
without naming it); files using a project stereotype. A scan cut short marks the model incomplete,
and nothing is ever reported unsatisfied from it ([`model`](src/model.rs)).

### The model answers, the buffer positions

Each editor query lays the live buffer over the model ([`Model::buffer`](src/model.rs)): its types,
beans and injection points replace the ones read from disk.

## Checks

Matching answers *yes*, *no* or *maybe* ([`matching`](src/matching.rs)). "Unsatisfied" needs every
bean to be a certain *no*; "ambiguous" needs two certain *yes*es.

| Code | Severity | When |
|---|---|---|
| `jakartaee.unsatisfied-injection` | warning | a managed class's point of a **project** type that no bean, possible bean or producer can satisfy |
| `jakartaee.ambiguous-injection` | warning | two or more explicit, non-EJB beans certainly match |
| `jakartaee.final-inject-field` | error | `@Inject` on a `final` field |
| `jakartaee.static-inject-field` | error | `@Inject` on a `static` field |
| `jakartaee.unproxyable-bean` | error | a normal-scoped bean that is final, has a non-private final instance method, or has no non-private no-argument constructor |
| `jakartaee.invalid-ejb-class` | error | an EJB annotation on an interface, enum, record, abstract or final class |
| `jakartaee.servlet-not-a-servlet` | error | `@WebServlet` on a class whose fully read hierarchy has no servlet in it |
| `jakartaee.invalid-url-pattern` | error | a pattern Tomcat — the most lenient container — refuses |
| `jakartaee.duplicate-url-pattern` | error | two servlet names on one pattern in one module, annotations and `web.xml` alike (reported in both files) |

## What it will not judge

Every injection check goes quiet when:

- **Spring** wires beans in the project — its extension answers `@Inject` (the EJB, proxy and servlet checks still run);
- a **portable extension** is implemented, an `ejb-jar.xml` declares beans, or a producer's type could be a project type nobody read;
- the project is **Quarkus** (ArC has its own rules; only the constructor rule of proxyability is relaxed for it);
- the discovery mode is **unknown**, or the scan did not finish;
- the type is **generic**, an array, a library type (unsatisfied) or a container built-in;
- an `@Alternative`, `@Priority`, `@Specializes` or `@Typed` bean of the type exists anywhere — which alternatives a deployment enables is not in the source;
- a qualifier or a possible bean carries an annotation nobody can classify, or a scope this crate does not catalogue (`@ViewScoped`, a library's own);
- the point is `@EJB` (resolved by business interface and deployment lookups) or its owner is not container-managed.

Library bean archives, beans added at deployment, `@Inherited` qualifiers on library base classes,
JSF managed beans and `@Nonbinding` members are not modelled; each of them only ever costs a report,
never invents one.

## Layout

| File | Holds |
|---|---|
| `known.rs` | the package table, both `@Singleton`s, hover prose |
| `text.rs` | lines, module roots, erasure, whole-word mentions |
| `types.rs` | units, modifiers as tokens, name resolution, type rows, ancestry, the buffer overlay |
| `archive.rs` | the discovery mode |
| `qualifiers.rs` | annotation classification |
| `beans.rs` / `inject.rs` | beans and producers / injection points |
| `matching.rs` | fits, candidates, resolution and its gates |
| `web.rs` | servlets, filters, `web.xml`, pattern rules, clashes |
| `model.rs` | the read rounds, project-wide flags, `Buffer` |
| `checks.rs` / `intel.rs` / `catalog.rs` | diagnostics / gutter, hover, go-to, completion / panel rows |
| `ext.rs` | the `FrameworkExtension` impl |

## Public API

Through the [`prelude`](src/prelude.rs): `JakartaEeExtension`, the nine `CODE_*` constants, `Model`,
`Bean`, `BeanOrigin`, `InjectionPoint`, `InjectKind`, `Qualifiers`, `CustomQualifier`, `TypeRef`,
`Fit`, `Resolution`, `ArchiveMode` + `discovery_mode`, and `WebComponent`, `WebKind`, `UrlPattern`,
`parse_web_xml`, `pattern_problem`.
