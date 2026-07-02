# bennu-web

The Bennu **web-config graph**. Models the web-config control flow as a first-class
language (the XML *is* the source of truth — 880 `<action>`, 0 annotations in the
reference Entando project — docs §2, §8), then emits string-keyed records + relations
onto the [`bennu-index`](../index) seam for the integration to ingest and resolve.

## What it parses

- **Struts2 / XWork** (`struts` module) — parse a `struts.xml` / `*-struts-plugin.xml`
  root and **follow `<include file="classpath/name.xml">`** across the project resource
  tree (Entando merges ~69 fragments pulled by classpath-name; in a vendored install
  they're on disk, docs §8 #3). Extract `<package namespace>` + `<action name method
  class>` + `<result name type>`. Include cycles / diamonds are de-duplicated; an include
  not found on disk (would come from a dependency jar) is **reported, never fatal**.
- **Spring bean-XML** (`spring` module) — `<bean id class parent>` → an **id→FQCN map**,
  walking `parent=` chains. This is the load-bearing C1 join (docs §10 C1):
  `<action class="beanId">` carries a Spring **bean-id**, not an FQCN, so JSP→action
  resolution goes *through* here.
- **Apache Tiles** (`tiles` module) — `<definition name template>` (+ `extends` +
  `<put-attribute name="body">`) → the JSP rendered. In this codebase 96/97 defs carry
  the per-action view in the `body` put-attribute and inherit the layout via `extends`,
  so resolution prefers `template=`, else the `body` JSP, else walks `extends` up to the
  parent layout (docs §8 #2).

## Wildcards & Tiles → candidate edges (never a false "missing")

Wildcard action names (`*`) and `{1}` backref methods/results are pervasive (155 + 128
in the reference project). They're kept as **patterns**, and every edge from a wildcard
action — plus every Tiles-indirected / backref result — is marked `inferred`. The
`WildcardPattern` matcher answers "does this concrete action plausibly match?" for
candidate navigation. Per docs §7/§8 the "action inesistente" diagnostic must **never**
emit an exact missing verdict when a wildcard/Tiles/computed path could match.

## Public API (via the prelude)

Call sites use `bennu_web::prelude::…`:

- `build_web_graph(&WebInputs) -> (WebConfigGraph, BuildReport)` — parse everything;
  the caller (`bennu-project`) supplies the discovered file lists + classpath resource
  roots.
- `resolve_action_class(&graph, action_qname) -> Option<String>` — the C1 chain:
  action → bean-id → real FQCN.
- `resolve_action_view(&graph, action_qname) -> Option<String>` — the view chain:
  action → `<result type="tiles">` → Tiles def → JSP.
- Records: `ActionRecord`, `ResultRecord`, `BeanRecord`, `TilesDefRecord`, plus
  `Relation` / `RelKind` (with `RelKind::into_index()` mapping onto the index'
  `RelationKind`) and `action_source()` / `bean_source()` for the `Source` tag.
- Helpers: `WildcardPattern`, `resolve_bean_map`, `resolve_tiles_view`, `relations_of`.
- `bean_class_value_spans(xml_text, fqcn) -> Vec<BeanClassSpan>` — the exact byte spans of
  every `<bean class="fqcn">` attribute value (matched exactly), for the class-rename
  config-aware edit (docs §5 #10). A Struts `<action class="beanId">` is a bean-id, not an
  FQCN, so it is correctly never matched.

## The ingestion seam

The emitted records are **string-keyed** (action qualified name, bean-id, FQCN, Tiles
def name, JSP path). The integration turns each into a `bennu_index` `Symbol`
(`Source::StrutsAction` / `SpringBean` / `TldTag`) and each edge into a `Relation`
(`ActionToClass` / `BeanIdToImpl` / `ResultToView` / `ActionToResult`), resolving the
string keys to `Symbol.id`s. This crate owns the *shape* of the graph, not the index.

## Not handled (honest limits)

- **Includes from dependency jars** on a non-vendored install — this crate only resolves
  `<include>` against on-disk resource roots; jar-resident fragments need a classpath
  resource-index (docs §8 #3). Missing includes are reported in `BuildReport`.
- **Struts convention-plugin** (`@Action`/`@Namespace` annotations) — not used in the
  reference project (100% explicit XML); annotation-driven actions are out of scope here.
- **Computed / reflection action names**, dynamic `jsp:include page="%{…}"`, and
  `wp:`/showlet view composition assembled at runtime from DB-stored config — represented
  elsewhere as unresolved-with-expression, not by this parser.
- **Interceptor stacks, `validation.xml`, result params** beyond the view target — parsed
  structure is limited to what action/bean/tiles navigation needs.
- **TLD tags** (`Source::TldTag`) — the seam variant exists; TLD parsing lands with the
  JSP taglib work, not in this module.

## Dependency

`roxmltree` (pure-Rust read-only XML DOM) with `allow_dtd` on — the Struts/Tiles
fragments declare a remote `<!DOCTYPE>` that roxmltree tolerates but never fetches.
