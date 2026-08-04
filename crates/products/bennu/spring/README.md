# bennu-spring

Spring support for Bennu, as a **framework extension** — the first implementation of the
[`bennu-ext`](../ext) seam, and the template for the ones after it.

```rust
use bennu_spring::prelude::*;
use bennu_ext::prelude::*;

let ext = SpringExtension::new();
ext.reindex(&ProjectScan { root, java, xml, resources, descriptors });

ext.gutter(&ctx);              // bean / inject / endpoint marks, usage counts in a yaml
ext.navigate(&ctx, offset);    // ${key} → application.yml, @Qualifier → the bean
ext.completions(&ctx, offset); // property keys, from the jars and from the project
ext.catalog("beans");          // the Beans panel
```

## What it knows

| Piece | Source |
|---|---|
| **Beans** | `@Service`/`@Component`/`@Repository`/`@Controller`/`@RestController`/`@Configuration` (+ JSR-330 `@Named`), `@Bean` factory methods, and `<bean>` XML with its `parent=` chain |
| **Injection points** | annotated fields and setters, constructor parameters — including the implicit single constructor *and* the one Lombok generates from `final` fields |
| **Endpoints** | `@RequestMapping` + the five shorthands, class path joined with method path |
| **Configuration** | `application*.properties` / `.yml` flattened into one dotted key space |
| **Bound properties** | every `@ConfigurationProperties` field and the *full* key it binds — through nesting, maps (`…<key>…`), collections (`…[0]…`) and `@Name` renames |
| **Expressions** | `${…}` and `#{…}` inside annotation strings, XML attributes **and property-file values**, via [`bennu-spel`](../spel) |
| **The property vocabulary** | `META-INF/spring-configuration-metadata.json` out of the dependency jars — every key Spring and the project's libraries accept, with type, default, prose and deprecation. A curated table ([`builtin_meta`](src/builtin_meta.rs)) stands in until the jars are resolved |
| **Environment overrides** | a key → the variable that overrides it, by Spring's own three rules (dot → `_`, dash **removed**, uppercase) |

## Design notes worth knowing before changing it

**It parses Java itself** ([`scan`](src/scan.rs)) instead of using `bennu-java`. Annotations
— with their argument spans, on methods and on parameters — are the entire substance of
Spring, and teaching the shared symbol model to carry all of that would put
framework-shaped data in the core for one consumer's benefit. It is also what lets this
crate become a WASM module later without a rewrite.

**Not every file is parsed.** A legacy tree has a thousand-plus sources and almost none
mention Spring. Selection is three rounds: files that mention anything Spring-shaped (a
cheap `contains`), files named by an XML `<bean class=>` (a plain POJO whose setters the
`<property>` check needs), and one round of unresolved supertypes.

**An annotation is identified by origin, not by name** ([`known`](src/known.rs)). `@Service` is not
a reserved word: anyone can declare `com.acme.Service`, and matching the simple name would register
a bean that does not exist — then navigate to it, count it, and offer it as an injection candidate.
So each annotation is resolved through the file's imports in the compiler's own order: a qualified
use, then a single-type import, then an on-demand import of an expected package, and otherwise
nothing (a bare name with no import can only be a type in the same package). This is also the only
thing that tells `lombok.Value` from
`org.springframework.beans.factory.annotation.Value`. Meta-annotations are not followed — a
project's `@MyService` meta-annotated `@Service` is missed, which loses a bean rather than
inventing one.

**The model answers, the buffer positions.** Every editor query re-parses the buffer for
spans and consults the project model for answers. Using the model for spans would navigate
to where a symbol used to be; re-deriving the project per keystroke would cost the scan.

**The user picks which `application.yml` is "the" one.** There is no way to know from
sources which profile is running — that is a launch argument — so `PropertySources` takes
the choice (persisted per project) and resolves: pinned file, then profile-less files, then
everything else.

## Every diagnostic is gated, on purpose

Under-report rather than risk a false positive (docs §7). Each check earns its right to
speak:

| Check | Only when |
|---|---|
| `<property name=>` names nothing | the bean's class is a project type whose property set is **known complete** — an unresolved supertype or an unmodelled `@Accessors` turns it off |
| `class=` doesn't exist | the class's **package is one the project declares** — a `org.springframework.*` class we've never seen is not our business |
| `ref=` names no bean | the id **looks like a typo** of one that does exist — a bean can legitimately come from a jar or a parent context |
| `${key}` unresolved | the project has property files, the placeholder has **no default**, and **another key in the same namespace exists** — which separates a typo from a value supplied at launch |

That last guard is the one to keep in mind: `${server.port}` in a project that declares no
`server.*` stays silent, because the honest answer is "it probably comes from the
environment".

## Layout

| File | Holds |
|---|---|
| `scan.rs` | Spring's relevance markers — the pass itself is [`bennu-facts`](../facts) |
| `known.rs` | the package table: which annotation is *actually* Spring's, resolved through the imports |
| `config_props.rs` | walking each `@ConfigurationProperties` root down to the key every field binds |
| `model.rs` | `SpringModel` + the bean/endpoint/injection types + name conventions |
| `beans.rs` | bean registry, injection points, the type index with `properties_complete` |
| `endpoints.rs` | request mappings, path joining |
| `props.rs` | `.properties` / `.yml` parsing, flattening, lookup precedence |
| `metadata.rs` | `spring-configuration-metadata.json` out of the dependency jars → the documented vocabulary |
| `builtin_meta.rs` | the curated fallback table, for a project whose jars have not been resolved |
| `env.rs` | a key → the environment variable that overrides it, in each form you might paste it into |
| `xml.rs` | bean XML parsing with byte ranges + `attribute_at` |
| `highlight.rs` | expression → coloured spans (shared by every file kind) |
| `java_intel.rs` | the editor's answers for a `.java` buffer |
| `xml_intel.rs` | the editor's answers for a bean XML buffer |
| `props_intel.rs` | the editor's answers for an `application*.yml` / `.properties` buffer |
| `library_beans.rs` | beans declared **inside an allowlisted dependency**, read from bytecode — their own tier, never merged into `SpringModel` |
| `ext.rs` | `FrameworkExtension` impl: file selection, model ownership, routing |

### Why library beans are a separate tier

A bean declared in a jar is a **declaration Spring may or may not act on**. Boot's whole model is
auto-configuration gated by `@ConditionalOnMissingBean` / `@ConditionalOnClass` /
`@ConditionalOnProperty`, and deciding what is actually registered is Spring's own condition
evaluator — `@ConditionalOnMissingBean` depends on the entire bean set *and* on registration order,
so nothing short of running it gives a true answer.

So `LibraryBean` is deliberately not a `BeanDef`: it carries the conditions that gate it, it is
grouped by the artifact it came from, and it takes no part in injection-candidate matching or in any
diagnostic. Merging the two would turn `known.rs`'s house rule — *a bean that does not exist,
navigated to and counted in a panel, is a confident lie* — into a lie told thousands of times.

Which dependencies are read at all is the `LibraryBeanAllowlist`, empty by default. It is not only a
volume control: the artifacts anyone allowlists in practice are their own shared modules, whose beans
are plain `@Service` / `@Configuration` and unconditional. Boot's conditional ones only appear if
somebody deliberately asks for them.

## What moved out

Three times now this crate has grown something that turned out not to be about Spring, and
each time the second consumer was the signal to extract rather than to copy:

- **[`bennu-facts`](../facts)** — the annotation-shaped tree-sitter scan and the rule that
  resolves `@Service` through the file's imports the way the compiler would. JPA needed both
  unchanged.
- **[`bennu-complete`](../complete)** — the caret's token, the prefix rule, the de-duplicated
  capped candidate list, and the discipline that separates ghost text from a guess. XML needed
  all of it unchanged.

What stays is the **policy**: which markers make a file worth parsing, which packages an
annotation may come from, which keys exist and what they mean.
