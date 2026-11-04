//! Typed, bounded views of what the framework extensions know about a project, for the AI clients
//! (MCP) and the Imber assistant alike — both reach them through the same `#[handler(mcp(...))]`.
//!
//! `bennu_catalog` already lists *rows* for the editor's panels; those rows are display-shaped (a
//! badge, a few tags). These tools answer the questions an assistant actually asks — "what URLs
//! does this serve, taking what?", "who injects this bean?", "how are these entities related?" —
//! straight from the extension models, so nothing is parsed twice and the JSON shape is stable.
//!
//! Every tool takes the project `root`, filters before it limits, and ends a cut list with a
//! `note` saying how many rows were left out: a list that quietly ends early reads as complete.

mod beans;
mod conflicts;
mod endpoints;
mod entities;
mod jobs;
mod properties;

use bennu_core::prelude::BennuState;
use serde::{Deserialize, Serialize};

use self::beans::{BeanFilter, BeanRow};
use self::endpoints::{EndpointFilter, EndpointRow};
use self::entities::{EntityFilter, EntityRow};
use self::jobs::{JobFilter, JobRow};
use self::properties::{PropertyFilter, PropertyRow};

const DEFAULT_LIMIT: usize = 200;
const MAX_LIMIT: usize = 2_000;

/// A bounded list: the rows that fit, how many matched, and a note when some were left out.
#[derive(Debug, Serialize)]
pub struct Listing<T> {
    /// How many rows matched the filters, before the limit.
    pub total: usize,
    pub items: Vec<T>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

/// Cut `rows` to `limit` (default [`DEFAULT_LIMIT`], at most [`MAX_LIMIT`]) and say so when it cut.
fn page<T>(mut rows: Vec<T>, limit: Option<usize>, empty_note: &str) -> Listing<T> {
    let total = rows.len();
    let limit = limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT);
    let note = if total == 0 {
        Some(empty_note.to_string())
    } else if total > limit {
        rows.truncate(limit);
        Some(format!("Showing {limit} of {total} — narrow the filters or raise `limit` (max {MAX_LIMIT}) to see the rest."))
    } else {
        None
    };
    Listing { total, items: rows, note }
}

/// Case-insensitive substring match; an absent filter matches everything.
fn has(haystack: &str, needle: &Option<String>) -> bool {
    match needle.as_deref().filter(|n| !n.is_empty()) {
        None => true,
        Some(n) => haystack.to_lowercase().contains(&n.to_lowercase()),
    }
}

const NO_FRAMEWORK: &str = "Nothing found. Either the project uses none of the frameworks Bennu reads, or its index is still building (check bennu_index_stats).";

/// Args for [`bennu_endpoints`].
#[derive(Deserialize, schemars::JsonSchema)]
pub struct EndpointsArgs {
    /// Absolute path to the project root.
    pub root: String,
    /// Keep routes whose path contains this text (`/orders`).
    #[serde(default)]
    pub path: Option<String>,
    /// Keep routes answering this HTTP method (`GET`, `POST`…). A route with no method answers all of them.
    #[serde(default)]
    pub method: Option<String>,
    /// Keep routes whose controller class or handler method name contains this text.
    #[serde(default)]
    pub class: Option<String>,
    /// Keep one framework's routes: `spring`, `jaxrs`, `servlet` or `struts`.
    #[serde(default)]
    pub framework: Option<String>,
    /// At most this many rows (default 200, max 2000).
    #[serde(default)]
    pub limit: Option<usize>,
}

/// List the HTTP endpoints a project serves: Spring MVC / WebFlux request mappings, JAX-RS resources,
/// servlets and Struts actions, in one shape.
///
/// Each row has the HTTP `methods` (empty = any), the full `path` (class-level and method-level
/// mappings already joined), the handler (`class`, `handler`, `returns`), what it `produces` /
/// `consumes`, and — for Spring — every parameter with where it binds from (`path`, `query`, `body`,
/// `header`…). Use it to answer "which controller serves this URL", "what does this endpoint take",
/// or to see the shape of an unfamiliar API before reading source. Struts actions list their
/// interceptors and results under `details`: in that stack the behaviour lives in XML.
///
/// Filters narrow before the limit applies. For the editor's own catalogue rows use `bennu_catalog`.
#[arbor_rpc::handler(mcp(
    title = "List the HTTP endpoints a project serves",
    safety = read,
))]
fn bennu_endpoints(_ctx: &BennuState, args: EndpointsArgs) -> Result<Listing<EndpointRow>, String> {
    let filter = EndpointFilter { path: args.path, method: args.method, class: args.class, framework: args.framework };
    Ok(page(endpoints::collect(&args.root, &filter), args.limit, NO_FRAMEWORK))
}

/// Args for [`bennu_beans`].
#[derive(Deserialize, schemars::JsonSchema)]
pub struct BeansArgs {
    /// Absolute path to the project root.
    pub root: String,
    /// Keep beans whose name contains this text.
    #[serde(default)]
    pub name: Option<String>,
    /// Keep beans whose class, or one of its supertypes, contains this text (`OrderRepository`).
    #[serde(default, rename = "type")]
    pub type_name: Option<String>,
    /// Keep one stereotype: `Service`, `Component`, `Repository`, `Controller`, `Configuration`, `Bean`, `bean` (XML)…
    #[serde(default)]
    pub stereotype: Option<String>,
    /// Keep beans of this scope (`singleton`, `prototype`, `request`…).
    #[serde(default)]
    pub scope: Option<String>,
    /// Keep beans active under this profile.
    #[serde(default)]
    pub profile: Option<String>,
    /// Also list the beans declared by allowlisted dependency jars (Settings → Beans). Default false: the project's own.
    #[serde(default)]
    pub include_libraries: bool,
    /// Add each bean's injection points (what it asks for, and which beans satisfy it) and which classes inject it.
    /// Only honoured when at most 100 beans match, so narrow with `name` or `type` first.
    #[serde(default)]
    pub injections: bool,
    /// At most this many rows (default 200, max 2000).
    #[serde(default)]
    pub limit: Option<usize>,
}

/// List the beans a project's dependency-injection container is built from: Spring stereotypes
/// (`@Service`, `@Component`…), `@Bean` factory methods and XML `<bean>`s, plus CDI beans.
///
/// Each row has the bean `name`, its class (`type`), where it comes from (`kind`, `declared_as`),
/// `scope`, `primary`, `lazy`, `profile`, the `@ConditionalOn…` `conditions` that may keep it from
/// existing, its `supertypes`, and the file and line. With `injections: true` (and a narrowed filter)
/// it adds what the bean asks for — each point with its qualifier and the beans that `resolves_to` —
/// and `injected_into`, the classes that depend on it. An injection with no match says whether the
/// type is a library one (Spring may supply it) or genuinely unsatisfied.
///
/// Use it for "what implements this interface", "why is this wired here", "what breaks if I change this
/// bean". The model is built from source, so a bean a library registers on its own is not listed.
#[arbor_rpc::handler(mcp(
    title = "List the beans in a project's DI container",
    safety = read,
))]
fn bennu_beans(_ctx: &BennuState, args: BeansArgs) -> Result<Listing<BeanRow>, String> {
    let filter = BeanFilter {
        name: args.name,
        type_name: args.type_name,
        stereotype: args.stereotype,
        scope: args.scope,
        profile: args.profile,
        include_libraries: args.include_libraries,
        injections: args.injections,
    };
    Ok(page(beans::collect(&args.root, &filter), args.limit, NO_FRAMEWORK))
}

/// Args for [`bennu_entities`].
#[derive(Deserialize, schemars::JsonSchema)]
pub struct EntitiesArgs {
    /// Absolute path to the project root.
    pub root: String,
    /// Keep entities whose name, class or table contains this text.
    #[serde(default)]
    pub name: Option<String>,
    /// List every persistent field, not only the id and the relations. Default false.
    #[serde(default)]
    pub fields: bool,
    /// At most this many rows (default 200, max 2000).
    #[serde(default)]
    pub limit: Option<usize>,
}

/// List the JPA entities of a project with their table, id, relations and Spring Data repositories.
///
/// Each entity has its `table`, `id`, the `relations` to other entities (`OneToMany`, `ManyToOne`…
/// with the `target`, the join column and which side `owning`s the foreign key) and the `repositories`
/// that manage it with every method classified as `derived` (the name is the query — a typo fails at
/// application start), `jpql` or `native`, with the `@Query` text. `fields: true` adds all fields.
///
/// Use it to understand the data model before changing it, or to find which repository to extend.
/// This is source-level: whether a column exists in the database is not known here.
#[arbor_rpc::handler(mcp(
    title = "List the JPA entities and their relations",
    safety = read,
))]
fn bennu_entities(_ctx: &BennuState, args: EntitiesArgs) -> Result<Listing<EntityRow>, String> {
    let filter = EntityFilter { name: args.name, fields: args.fields };
    Ok(page(entities::collect(&args.root, &filter), args.limit, NO_FRAMEWORK))
}

/// Args for [`bennu_scheduled_jobs`].
#[derive(Deserialize, schemars::JsonSchema)]
pub struct JobsArgs {
    /// Absolute path to the project root.
    pub root: String,
    /// Keep jobs whose class or method contains this text.
    #[serde(default)]
    pub name: Option<String>,
    /// At most this many rows (default 200, max 2000).
    #[serde(default)]
    pub limit: Option<usize>,
}

/// List the `@Scheduled` jobs of a project: the class and method, how it is triggered (`cron`,
/// `fixed` rate/delay, or `none` — an incomplete annotation Spring refuses at startup), the schedule
/// as written and, for a cron, a plain-words reading of it.
#[arbor_rpc::handler(mcp(
    title = "List the scheduled jobs",
    safety = read,
))]
fn bennu_scheduled_jobs(_ctx: &BennuState, args: JobsArgs) -> Result<Listing<JobRow>, String> {
    let filter = JobFilter { name: args.name };
    Ok(page(jobs::collect(&args.root, &filter), args.limit, NO_FRAMEWORK))
}

/// Args for [`bennu_dependency_conflicts`].
#[derive(Deserialize, schemars::JsonSchema)]
pub struct ConflictsArgs {
    /// Absolute path to the project root (Maven, Gradle or Cargo).
    pub root: String,
    /// Keep dependencies whose `group:artifact` contains this text.
    #[serde(default)]
    pub dependency: Option<String>,
    /// At most this many rows (default 200, max 2000).
    #[serde(default)]
    pub limit: Option<usize>,
}

/// Find libraries a build resolved to a different version than some dependency asked for.
///
/// Two kinds of row. A **conflict** is inside one module: `resolved` is the version that won, and each
/// entry of `requested` is a version that lost, with the chain of dependencies (`via`) that asked for it.
/// **Skew** (`module: "(across modules)"`) is a multi-module build resolving one library to different
/// versions in different modules; `requested[].via` then lists those modules. Use it when a runtime
/// error smells like a version mismatch (`NoSuchMethodError`, `ClassNotFoundException`).
///
/// **Slow:** it asks the build tool for the resolved tree (`mvn dependency:tree`, `gradle dependencies`,
/// `cargo tree`) — seconds warm, minutes on a cold repository — and runs as a Task visible in the Jobs
/// panel. Call it once and filter with `dependency` rather than repeatedly.
#[arbor_rpc::handler(mcp(
    title = "Find dependency version conflicts",
    safety = read,
))]
async fn bennu_dependency_conflicts(ctx: &BennuState, args: ConflictsArgs) -> Result<Listing<conflicts::ConflictRow>, String> {
    let tree = crate::dependency_tree::resolve_tree(ctx, args.root).await?;
    let filter = conflicts::ConflictFilter { dependency: args.dependency };
    let mut rows = conflicts::conflicts(&tree.modules, &filter);
    rows.extend(conflicts::skew(&tree.modules, &filter));
    let note = "No version conflicts found in the resolved tree.";
    let mut listing = page(rows, args.limit, note);
    if !tree.verbose && listing.note.as_deref() == Some(note) {
        listing.note = Some("Nothing found, but the tree was resolved without the dropped nodes, so in-module conflicts cannot be seen; only cross-module skew was checked.".into());
    }
    Ok(listing)
}

/// Args for [`bennu_config_properties`].
#[derive(Deserialize, schemars::JsonSchema)]
pub struct PropertiesArgs {
    /// Absolute path to the project root.
    pub root: String,
    /// Keep keys containing this text (`datasource`, `app.http`).
    #[serde(default)]
    pub key: Option<String>,
    /// Only keys the project's property files actually set. Default false: also the ones code reads but no file sets.
    #[serde(default)]
    pub set_only: bool,
    /// At most this many rows (default 200, max 2000).
    #[serde(default)]
    pub limit: Option<usize>,
}

/// List the configuration keys a Spring project's code uses and where they are read.
///
/// Each row is a canonical key with its current `value` (resolved against the active property file,
/// absent when no file sets it), the Java `type` the code expects, and `read_by`: every place that
/// consumes it — a `@ConfigurationProperties` field (with the full dotted path assembled from the
/// prefix and the nesting), an `@Value`, a `@ConditionalOnProperty`. Use it to answer "which keys
/// can I put in the yaml", "who reads this key", or "is this key still used".
#[arbor_rpc::handler(mcp(
    title = "List configuration keys and who reads them",
    safety = read,
))]
fn bennu_config_properties(_ctx: &BennuState, args: PropertiesArgs) -> Result<Listing<PropertyRow>, String> {
    let filter = PropertyFilter { key: args.key, set_only: args.set_only };
    Ok(page(properties::collect(&args.root, &filter), args.limit, NO_FRAMEWORK))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_cut_list_says_how_much_was_left_out() {
        let l = page((0..10).collect::<Vec<_>>(), Some(3), "none");
        assert_eq!((l.total, l.items.len()), (10, 3));
        assert!(l.note.unwrap().contains("3 of 10"));
    }

    #[test]
    fn a_whole_list_has_no_note_and_an_empty_one_explains_itself() {
        assert!(page(vec![1, 2], Some(5), "none").note.is_none());
        assert_eq!(page(Vec::<u8>::new(), None, "nothing here").note.as_deref(), Some("nothing here"));
    }

    #[test]
    fn the_limit_is_clamped() {
        assert_eq!(page(vec![1; 3], Some(0), "").items.len(), 1);
    }

    #[test]
    fn a_filter_is_case_insensitive_and_an_absent_one_matches_all() {
        assert!(has("OrderService", &Some("order".into())));
        assert!(!has("OrderService", &Some("user".into())));
        assert!(has("x", &None));
        assert!(has("x", &Some(String::new())));
    }
}
