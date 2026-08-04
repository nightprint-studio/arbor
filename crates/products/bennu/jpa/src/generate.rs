//! The generators — repositories, projections, and query methods.
//!
//! ## Two destinations, on purpose
//!
//! A repository is its own file. A **projection** is genuinely both: it can be a top-level
//! interface, or it can be nested inside the repository that returns it — and which one is right
//! is a house-style question, not a technical one. So every generator returns whichever of the
//! two it can honestly offer ([`Generated`]), and the choice stays with the person generating.
//!
//! ## The query builder is the point
//!
//! Writing `findByCustomerNameAndTotalGreaterThanOrderByCreatedAtDesc` by hand is how the typos
//! in [`crate::derived`] get written in the first place. Built from the entity's own fields, the
//! name cannot be wrong — every segment came from a property that exists, and the argument list
//! is derived from the keywords rather than counted by eye.
//!
//! Nothing here writes to disk. Generating produces **text**; the caller decides what to do with
//! it, which is also what makes all of it testable.

use crate::model::{capitalize, simple_name, Entity, JpaModel, Repository};

/// A file a generator would create.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GeneratedFile {
    /// Suggested absolute path, forward-slashed. The caller may put it elsewhere.
    pub path: String,
    pub content: String,
}

/// Text to splice into a file that already exists.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Insertion {
    pub file: String,
    /// Byte offset to insert at.
    pub offset: usize,
    pub text: String,
}

/// What a generator produced. At least one of the two is always present.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Generated {
    /// As a file of its own.
    pub file: Option<GeneratedFile>,
    /// Spliced into an existing file — nested in the repository, or added to its body.
    pub insertion: Option<Insertion>,
    /// What the caller shows in the preview pane. Always the fuller of the two.
    pub preview: String,
}

/// One condition of a query being built.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Condition {
    /// Property path on the entity, dotted (`customer.name`).
    pub path: String,
    /// A keyword from [`crate::derived`]'s table, or empty for plain equality.
    pub keyword: String,
    pub ignore_case: bool,
    /// Joined to the previous condition with `Or` rather than `And`.
    pub or: bool,
}

/// What the query-method form collected.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct QuerySpec {
    /// A name written by hand instead of the derived one.
    ///
    /// Empty is the normal case and the better one: a name built from properties that exist
    /// cannot be misspelled, which is the whole guarantee of the builder. But Spring Data
    /// accepts a `@Query`-annotated method under **any** name, and house conventions
    /// (`soggetti_aderenti`) are a real reason to override — so the field exists, and the
    /// generated name is what it defaults to rather than what it imposes.
    pub name: String,
    /// Write the `@Query` out even though the derived name would resolve on its own.
    ///
    /// Off by default and that is the right default — a derived name is checked against the
    /// entity every time the project is opened, whereas a hand-written query is a string nobody
    /// verifies until it runs. But it is a real choice: an explicit query is what you reach for
    /// when the generated JPQL is a *starting point* you mean to edit (a join, a fetch, a
    /// projection the name cannot express), and having to delete the method and write it by hand
    /// to get there is the annoying way to find that out.
    ///
    /// Forced on by [`Self::name`]: a renamed method is no longer derivable at all.
    pub with_query: bool,
    pub subject: String,
    pub distinct: bool,
    pub limit: Option<u32>,
    pub conditions: Vec<Condition>,
    /// `(path, descending)` pairs.
    pub order_by: Vec<(String, bool)>,
    /// Return a `List<E>` rather than a single `E`.
    pub many: bool,
    /// Take a `Pageable` and return a `Page<E>`.
    pub paged: bool,
    /// Return this projection interface instead of the entity.
    pub projection: String,
}

/// The name the method will actually carry: the override when one was written, else the
/// derived one.
///
/// An override is **not** a derived query any more — Spring Data resolves a method by its name
/// only when there is no `@Query` on it, so a renamed method needs one. [`query_method`] says so
/// in the generated code rather than letting it fail at startup.
pub fn effective_name(spec: &QuerySpec) -> String {
    if spec.name.trim().is_empty() {
        method_name(spec)
    } else {
        spec.name.trim().to_string()
    }
}

/// The generated method name for a spec. Cannot be misspelled: every segment came from a
/// property that exists.
pub fn method_name(spec: &QuerySpec) -> String {
    let subject = match spec.subject.as_str() {
        "count" => "count",
        "exists" => "exists",
        "delete" => "delete",
        _ => "find",
    };
    let mut name = String::from(subject);
    if spec.distinct {
        name.push_str("Distinct");
    }
    if let Some(n) = spec.limit {
        name.push_str(&format!("First{n}"));
    }
    name.push_str("By");
    for (i, c) in spec.conditions.iter().enumerate() {
        if i > 0 {
            name.push_str(if c.or { "Or" } else { "And" });
        }
        name.push_str(&path_segment(&c.path));
        name.push_str(&c.keyword);
        if c.ignore_case {
            name.push_str("IgnoreCase");
        }
    }
    if !spec.order_by.is_empty() {
        name.push_str("OrderBy");
        for (i, (path, desc)) in spec.order_by.iter().enumerate() {
            if i > 0 {
                name.push_str("And");
            }
            name.push_str(&path_segment(path));
            name.push_str(if *desc { "Desc" } else { "Asc" });
        }
    }
    name
}

/// `customer.name` → `CustomerName` — the camel-cased path a derived name spells it with.
fn path_segment(path: &str) -> String {
    path.split('.').map(capitalize).collect()
}

/// How many bound arguments a keyword consumes. Mirrors [`crate::derived`]'s table, which is
/// the authority; only the handful that differ from one are listed.
///
/// Public because a form needs it to say *what* a condition compares against — "not equal to"
/// with no parameter named is half a sentence.
pub fn keyword_args(keyword: &str) -> usize {
    match keyword {
        "Between" | "NotBetween" | "IsBetween" => 2,
        "IsNull" | "NotNull" | "IsNotNull" | "Null" | "True" | "False" | "IsTrue" | "IsFalse"
        | "IsEmpty" | "IsNotEmpty" | "Empty" | "NotEmpty" => 0,
        _ => 1,
    }
}

/// Whether the argument a keyword binds is a **collection** rather than a single value — the
/// other thing a form has to know to name the parameter the way the generator will.
pub fn keyword_binds_collection(keyword: &str) -> bool {
    matches!(keyword, "In" | "NotIn" | "IsNotIn" | "IsIn")
}

/// Internal shorthand, so the generator reads the same as it always did.
fn args_for(keyword: &str) -> usize {
    keyword_args(keyword)
}

/// Build the query method and place it inside `repo`'s body.
///
/// `source` is the repository file's live text — the closing brace of the interface is found in
/// it, which is where a new method goes.
pub fn query_method(
    model: &JpaModel,
    repo: &Repository,
    source: &str,
    spec: &QuerySpec,
) -> Generated {
    let entity = model.entity(&repo.entity);
    // `Object` when the entity cannot be named at all — a repository extending a generic base
    // (`extends BaseRepo<T>`) has a type variable there, not a type. It reads as "we could not
    // work this out", which is true; the alternative was `List<>`, which reads as broken.
    let entity_simple = entity
        .map(|e| e.simple.clone())
        .filter(|s| !s.is_empty())
        .or_else(|| Some(repo.entity.clone()).filter(|s| !s.is_empty()))
        .unwrap_or_else(|| "Object".to_string());
    let row = if spec.projection.is_empty() { entity_simple.clone() } else { spec.projection.clone() };

    let return_type = match spec.subject.as_str() {
        "count" => "long".to_string(),
        "exists" => "boolean".to_string(),
        "delete" => if spec.many { "long".to_string() } else { "void".to_string() },
        _ if spec.paged => format!("Page<{row}>"),
        _ if spec.many => format!("List<{row}>"),
        _ => format!("Optional<{row}>"),
    };

    // One parameter per bound argument, named after the property it compares. A second argument
    // for the same property (a `Between`) gets a suffix rather than a clash.
    let mut params: Vec<String> = Vec::new();
    for c in &spec.conditions {
        let base = c.path.rsplit('.').next().unwrap_or(&c.path).to_string();
        let ty = entity
            .and_then(|e| resolve_field_type(model, e, &c.path))
            .unwrap_or_else(|| "Object".to_string());
        match args_for(&c.keyword) {
            0 => {}
            2 => {
                params.push(format!("{ty} {base}From"));
                params.push(format!("{ty} {base}To"));
            }
            _ if c.keyword == "In" || c.keyword == "NotIn" || c.keyword == "IsNotIn" => {
                params.push(format!("Collection<{ty}> {base}s"));
            }
            _ => params.push(format!("{ty} {base}")),
        }
    }
    if spec.paged {
        params.push("Pageable pageable".to_string());
    }

    // Two reasons to write the query out. One is a choice (`with_query`); the other is not — a
    // renamed method is no longer derivable from its name, so without the annotation Spring Data
    // tries to parse `soggetti_aderenti` as a property path and the context fails to start.
    // …and one reason not to: with no entity name there is no `from` clause to write, and an
    // annotation holding invalid JPQL is worse than none — it fails at startup instead of at
    // compile time, which is the exact failure mode this whole crate exists to prevent.
    let wants_query = !spec.name.trim().is_empty() || spec.with_query;
    let annotation = if wants_query && entity.is_some() {
        format!("    @Query(\"{}\")\n", jpql_for(model, repo, spec))
    } else {
        String::new()
    };
    let text = format!(
        "\n{annotation}    {return_type} {}({});\n",
        effective_name(spec),
        params.join(", "),
    );
    Generated {
        insertion: Some(Insertion {
            file: repo.file.clone(),
            offset: body_end(source),
            text: text.clone(),
        }),
        file: None,
        preview: text.trim_start_matches('\n').trim_end().to_string(),
    }
}

/// The JPQL a renamed method needs, written from the same conditions the name was built from.
///
/// Deliberately plain — an alias, a `where` and `and`/`or`. The point is a query that is
/// *correct and readable*, not one that shows off: whatever this emits, the author is going to
/// edit it, and a dense one-liner is worse to start from than an obvious one.
fn jpql_for(model: &JpaModel, repo: &Repository, spec: &QuerySpec) -> String {
    let entity = model
        .entity(&repo.entity)
        .map(|e| e.entity_name.clone())
        .unwrap_or_else(|| repo.entity.clone());
    let alias = "e";
    let head = match spec.subject.as_str() {
        "count" => format!("select count({alias}) from {entity} {alias}"),
        "delete" => format!("delete from {entity} {alias}"),
        _ if spec.distinct => format!("select distinct {alias} from {entity} {alias}"),
        _ => format!("select {alias} from {entity} {alias}"),
    };
    let mut out = head;
    for (i, c) in spec.conditions.iter().enumerate() {
        out.push_str(if i == 0 {
            " where "
        } else if c.or {
            " or "
        } else {
            " and "
        });
        let param = c.path.rsplit('.').next().unwrap_or(&c.path);
        out.push_str(&format!("{alias}.{} {}", c.path, comparison(&c.keyword, param)));
    }
    for (i, (path, desc)) in spec.order_by.iter().enumerate() {
        out.push_str(if i == 0 { " order by " } else { ", " });
        out.push_str(&format!("{alias}.{path}"));
        if *desc {
            out.push_str(" desc");
        }
    }
    out
}

/// A keyword rendered as JPQL, with the placeholder it binds.
fn comparison(keyword: &str, param: &str) -> String {
    match keyword {
        "" | "Is" | "Equals" => format!("= :{param}"),
        "Not" => format!("<> :{param}"),
        "GreaterThan" => format!("> :{param}"),
        "GreaterThanEqual" => format!(">= :{param}"),
        "LessThan" => format!("< :{param}"),
        "LessThanEqual" => format!("<= :{param}"),
        "After" => format!("> :{param}"),
        "Before" => format!("< :{param}"),
        "Between" => format!("between :{param}From and :{param}To"),
        "Like" => format!("like :{param}"),
        "NotLike" => format!("not like :{param}"),
        "StartingWith" => format!("like concat(:{param}, '%')"),
        "EndingWith" => format!("like concat('%', :{param})"),
        "Containing" | "Contains" => format!("like concat('%', :{param}, '%')"),
        "In" => format!("in :{param}s"),
        "NotIn" => format!("not in :{param}s"),
        "IsNull" | "Null" => "is null".to_string(),
        "IsNotNull" | "NotNull" => "is not null".to_string(),
        "True" | "IsTrue" => "= true".to_string(),
        "False" | "IsFalse" => "= false".to_string(),
        "IsEmpty" | "Empty" => "is empty".to_string(),
        "IsNotEmpty" | "NotEmpty" => "is not empty".to_string(),
        _ => format!("= :{param}"),
    }
}

/// The declared type of a dotted property path, for naming the generated parameter.
fn resolve_field_type<'a>(model: &'a JpaModel, entity: &'a Entity, path: &str) -> Option<String> {
    let mut current = entity;
    let mut segments = path.split('.').peekable();
    while let Some(seg) = segments.next() {
        let field = model.fields_of(current).into_iter().find(|f| f.name == seg)?;
        if segments.peek().is_none() {
            return Some(simple_name(&field.type_text).to_string());
        }
        let target = if field.relation.is_empty() { &field.type_text } else { &field.target };
        current = model.entity(target)?;
    }
    None
}

/// Generate a repository interface for `entity`.
///
/// The package is the entity's own unless the project already keeps repositories somewhere else
/// — which is read off the repositories that exist rather than assumed, because "where do
/// repositories live" is a convention every codebase settles differently and guessing it wrong
/// puts the file in the wrong place on every single use.
pub fn repository(model: &JpaModel, entity: &Entity, base: &str, source_root: &str) -> Generated {
    let package = repository_package(model, entity);
    let name = format!("{}Repository", entity.simple);
    let id = entity
        .id_field()
        .map(|f| simple_name(&f.type_text).to_string())
        .unwrap_or_else(|| "Long".to_string());

    let mut imports = vec![format!("org.springframework.data.jpa.repository.{base}")];
    if package != package_of(&entity.fqcn) {
        imports.push(entity.fqcn.clone());
    }
    imports.sort();

    let content = format!(
        "package {package};\n\n{}\n\npublic interface {name} extends {base}<{}, {id}> {{\n}}\n",
        imports.iter().map(|i| format!("import {i};")).collect::<Vec<_>>().join("\n"),
        entity.simple,
    );
    Generated {
        file: Some(GeneratedFile {
            path: format!("{}/{}/{name}.java", source_root.trim_end_matches('/'), package.replace('.', "/")),
            content: content.clone(),
        }),
        insertion: None,
        preview: content,
    }
}

/// Generate a projection interface over `fields` of `entity`.
///
/// Offered **both ways**, which is the whole reason this returns a `Generated` with two halves:
/// a top-level interface in its own file, or the same interface nested inside the repository
/// that will return it. Both are idiomatic; neither is more correct.
pub fn projection(
    model: &JpaModel,
    entity: &Entity,
    name: &str,
    fields: &[String],
    repo: Option<(&Repository, &str)>,
    source_root: &str,
) -> Generated {
    let accessors: Vec<String> = fields
        .iter()
        .filter_map(|path| {
            let ty = resolve_field_type(model, entity, path)?;
            let getter = path.split('.').map(|s| capitalize(s)).collect::<String>();
            Some(format!("    {ty} get{getter}();"))
        })
        .collect();
    let body = accessors.join("\n");

    let package = package_of(&entity.fqcn);
    let content =
        format!("package {package};\n\npublic interface {name} {{\n{body}\n}}\n");

    // Nested: same members, one indent deeper, spliced into the repository's body.
    let insertion = repo.map(|(r, source)| Insertion {
        file: r.file.clone(),
        offset: body_end(source),
        text: format!(
            "\n    interface {name} {{\n{}\n    }}\n",
            accessors.iter().map(|a| format!("    {a}")).collect::<Vec<_>>().join("\n"),
        ),
    });

    Generated {
        file: Some(GeneratedFile {
            path: format!(
                "{}/{}/{name}.java",
                source_root.trim_end_matches('/'),
                package.replace('.', "/"),
            ),
            content: content.clone(),
        }),
        insertion,
        preview: content,
    }
}

fn package_of(fqcn: &str) -> String {
    fqcn.rsplit_once('.').map(|(p, _)| p.to_string()).unwrap_or_default()
}

/// Where this project keeps its repositories: the package the majority of the existing ones live
/// in, or the entity's own when there are none to learn from.
fn repository_package(model: &JpaModel, entity: &Entity) -> String {
    let mut counts: Vec<(String, usize)> = Vec::new();
    for r in &model.repositories {
        let pkg = package_of(&r.fqcn);
        if pkg.is_empty() {
            continue;
        }
        match counts.iter_mut().find(|(p, _)| *p == pkg) {
            Some((_, n)) => *n += 1,
            None => counts.push((pkg, 1)),
        }
    }
    counts
        .into_iter()
        .max_by_key(|(_, n)| *n)
        .map(|(p, _)| p)
        .unwrap_or_else(|| package_of(&entity.fqcn))
}

// ── Writing into an entity ───────────────────────────────────────────────────

/// What the "add attribute" form collected.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AttributeSpec {
    pub name: String,
    /// The field's type, or — for a relation — the entity on the other end.
    pub type_text: String,
    /// `@Column(name = …)`. Empty leaves the provider's default naming in place.
    pub column: String,
    /// `nullable = false` when this is off.
    pub optional: bool,
    pub unique: bool,
    /// `length = …`, meaningful for a string column only.
    pub length: Option<u32>,
    /// `""` for a plain column, else `ManyToOne` / `OneToMany` / `ManyToMany` / `OneToOne`.
    pub relation: String,
    /// The owning side's field name, for an inverse relation.
    pub mapped_by: String,
    pub lazy: bool,
    /// Also write a getter and a setter.
    pub accessors: bool,
}

/// A field (and optionally its accessors) added to an entity.
///
/// The relation half is where this earns its place: the difference between a `@ManyToOne` that
/// works and one that generates a second table is a `@JoinColumn` on the owning side and a
/// `mappedBy` on the other, and that is exactly the pair people get backwards by hand.
pub fn entity_attribute(entity: &Entity, source: &str, spec: &AttributeSpec) -> Generated {
    let collection = matches!(spec.relation.as_str(), "OneToMany" | "ManyToMany");
    let field_type =
        if collection { format!("List<{}>", spec.type_text) } else { spec.type_text.clone() };

    let mut lines: Vec<String> = Vec::new();
    if spec.relation.is_empty() {
        let mut attrs: Vec<String> = Vec::new();
        if !spec.column.is_empty() {
            attrs.push(format!("name = \"{}\"", spec.column));
        }
        if !spec.optional {
            attrs.push("nullable = false".to_string());
        }
        if spec.unique {
            attrs.push("unique = true".to_string());
        }
        if let Some(n) = spec.length {
            attrs.push(format!("length = {n}"));
        }
        if !attrs.is_empty() {
            lines.push(format!("    @Column({})", attrs.join(", ")));
        }
    } else {
        let mut attrs: Vec<String> = Vec::new();
        if !spec.mapped_by.is_empty() {
            attrs.push(format!("mappedBy = \"{}\"", spec.mapped_by));
        }
        if spec.lazy {
            attrs.push("fetch = FetchType.LAZY".to_string());
        }
        // `optional` exists on the to-one annotations only; on a collection it means nothing.
        if !spec.optional && matches!(spec.relation.as_str(), "ManyToOne" | "OneToOne") {
            attrs.push("optional = false".to_string());
        }
        lines.push(if attrs.is_empty() {
            format!("    @{}", spec.relation)
        } else {
            format!("    @{}({})", spec.relation, attrs.join(", "))
        });
        // The owning side is the one with the foreign key, and only that side gets the column.
        if spec.mapped_by.is_empty() && matches!(spec.relation.as_str(), "ManyToOne" | "OneToOne") {
            let column = if spec.column.is_empty() {
                format!("{}_id", spec.name).to_uppercase()
            } else {
                spec.column.clone()
            };
            lines.push(format!("    @JoinColumn(name = \"{column}\")"));
        }
    }
    lines.push(format!("    private {field_type} {};", spec.name));

    if spec.accessors {
        let suffix = capitalize(&spec.name);
        lines.push(String::new());
        lines.push(format!("    public {field_type} get{suffix}() {{"));
        lines.push(format!("        return {};", spec.name));
        lines.push("    }".to_string());
        lines.push(String::new());
        lines.push(format!("    public void set{suffix}({field_type} {}) {{", spec.name));
        lines.push(format!("        this.{0} = {0};", spec.name));
        lines.push("    }".to_string());
    }

    let text = format!("\n{}\n", lines.join("\n"));
    Generated {
        insertion: Some(Insertion {
            file: entity.file.clone(),
            offset: body_end(source),
            text: text.clone(),
        }),
        file: None,
        preview: text.trim_matches('\n').to_string(),
    }
}

/// What the "add named query" form collected.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NamedQuerySpec {
    /// The unqualified half — `Order.` is prepended, which is the convention every JPA codebase
    /// follows and the one that makes `em.createNamedQuery` readable.
    pub name: String,
    pub query: String,
}

/// The conventional name of a named query on this entity.
pub fn named_query_name(entity: &Entity, spec: &NamedQuerySpec) -> String {
    let name = spec.name.trim();
    if name.contains('.') {
        name.to_string()
    } else {
        format!("{}.{}", entity.entity_name, name)
    }
}

/// A `@NamedQuery` added to the entity's own annotations.
///
/// Placed on the line the type is declared on, so it lands *under* `@Entity` and `@Table` rather
/// than above them — which is where a reader looks for it, and where a repeated `@NamedQuery`
/// stacks naturally without needing a `@NamedQueries` wrapper.
pub fn named_query(entity: &Entity, source: &str, spec: &NamedQuerySpec) -> Generated {
    let query = if spec.query.trim().is_empty() {
        format!("select e from {} e", entity.entity_name)
    } else {
        spec.query.trim().to_string()
    };
    let text = format!(
        "@NamedQuery(name = \"{}\", query = \"{}\")\n",
        named_query_name(entity, spec),
        query.replace('"', "\\\""),
    );
    Generated {
        insertion: Some(Insertion {
            file: entity.file.clone(),
            offset: declaration_line_start(source, &entity.simple),
            text: text.clone(),
        }),
        file: None,
        preview: text.trim_end().to_string(),
    }
}

/// The JPA lifecycle callbacks, with when each fires. The whole set — there are seven, they are
/// closed, and an editor that offered four of them would be a worse reference than the spec.
pub const LIFECYCLE_EVENTS: &[(&str, &str)] = &[
    ("PrePersist", "before the row is inserted"),
    ("PostPersist", "after the row is inserted"),
    ("PreUpdate", "before the row is updated"),
    ("PostUpdate", "after the row is updated"),
    ("PreRemove", "before the row is deleted"),
    ("PostRemove", "after the row is deleted"),
    ("PostLoad", "after the entity is loaded"),
];

/// A lifecycle callback method on the entity.
///
/// `name` is optional; the default (`onPrePersist`) says which event it serves, which matters
/// because the annotation is the only thing that wires it and a method called `touch()` gives a
/// reader nothing to go on.
pub fn lifecycle_callback(entity: &Entity, source: &str, event: &str, name: &str) -> Generated {
    let name = if name.trim().is_empty() {
        format!("on{event}")
    } else {
        name.trim().to_string()
    };
    let text = format!("\n    @{event}\n    void {name}() {{\n    }}\n");
    Generated {
        insertion: Some(Insertion {
            file: entity.file.clone(),
            offset: body_end(source),
            text: text.clone(),
        }),
        file: None,
        preview: text.trim_matches('\n').to_string(),
    }
}

// ── Writing into a repository ────────────────────────────────────────────────

/// What the "add modify method" form collected.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ModifySpec {
    /// A hand-written name instead of the derived one.
    pub name: String,
    /// `true` for a delete, `false` for an update.
    pub delete: bool,
    /// Property paths the update assigns, each bound to a parameter of the same leaf name.
    /// Ignored for a delete.
    pub assignments: Vec<String>,
    pub conditions: Vec<Condition>,
    /// Return the number of rows affected rather than `void`.
    pub returns_count: bool,
}

/// A bulk update or delete on the repository.
///
/// **Always a `@Query`, never a derived name** — and that is not a shortcut. Spring Data derives
/// `deleteBy…` from a name, but there is no naming scheme for an update anywhere in Spring Data:
/// a method called `updateStatusById` with no annotation is parsed as a *query* subject it does
/// not recognise and the context fails to start. Writing the JPQL is the only honest option for
/// half the cases, so both halves are written the same way rather than one silently behaving
/// differently from the other.
///
/// A derived `deleteBy…` also loads each row and deletes it one at a time (so `@PreRemove` fires);
/// the bulk form here does not. That is a real semantic difference, and it is the one the button
/// says it is doing.
pub fn modify_method(
    model: &JpaModel,
    repo: &Repository,
    source: &str,
    spec: &ModifySpec,
) -> Generated {
    let entity = model.entity(&repo.entity);
    let entity_name = entity
        .map(|e| e.entity_name.clone())
        .unwrap_or_else(|| simple_name(&repo.entity).to_string());
    let alias = "e";

    let mut jpql = if spec.delete {
        format!("delete from {entity_name} {alias}")
    } else {
        let sets: Vec<String> = spec
            .assignments
            .iter()
            .map(|path| format!("{alias}.{path} = :{}", leaf(path)))
            .collect();
        format!("update {entity_name} {alias} set {}", sets.join(", "))
    };
    for (i, c) in spec.conditions.iter().enumerate() {
        jpql.push_str(if i == 0 {
            " where "
        } else if c.or {
            " or "
        } else {
            " and "
        });
        jpql.push_str(&format!("{alias}.{} {}", c.path, comparison(&c.keyword, leaf(&c.path))));
    }

    // Bound by name, so every parameter carries `@Param` — without it the binding depends on the
    // `-parameters` compiler flag being on, which on a legacy build it is not.
    let type_of = |path: &str| {
        entity
            .and_then(|e| resolve_field_type(model, e, path))
            .unwrap_or_else(|| "Object".to_string())
    };
    let mut params: Vec<String> = Vec::new();
    if !spec.delete {
        for path in &spec.assignments {
            let name = leaf(path);
            params.push(format!("@Param(\"{name}\") {} {name}", type_of(path)));
        }
    }
    for c in &spec.conditions {
        let name = leaf(&c.path);
        let ty = type_of(&c.path);
        match args_for(&c.keyword) {
            0 => {}
            2 => {
                params.push(format!("@Param(\"{name}From\") {ty} {name}From"));
                params.push(format!("@Param(\"{name}To\") {ty} {name}To"));
            }
            _ if c.keyword == "In" || c.keyword == "NotIn" || c.keyword == "IsNotIn" => {
                params.push(format!("@Param(\"{name}s\") Collection<{ty}> {name}s"));
            }
            _ => params.push(format!("@Param(\"{name}\") {ty} {name}")),
        }
    }

    let name = if spec.name.trim().is_empty() {
        modify_method_name(spec)
    } else {
        spec.name.trim().to_string()
    };
    let returns = if spec.returns_count { "int" } else { "void" };
    let text = format!(
        "\n    @Modifying\n    @Query(\"{jpql}\")\n    {returns} {name}({});\n",
        params.join(", "),
    );
    Generated {
        insertion: Some(Insertion {
            file: repo.file.clone(),
            offset: body_end(source),
            text: text.clone(),
        }),
        file: None,
        preview: text.trim_matches('\n').to_string(),
    }
}

/// The generated name of a modify method: `updateStatusById`, `deleteByCreatedAtBefore`.
pub fn modify_method_name(spec: &ModifySpec) -> String {
    let mut name = String::from(if spec.delete { "delete" } else { "update" });
    if !spec.delete {
        for path in &spec.assignments {
            name.push_str(&path_segment(path));
        }
    }
    name.push_str("By");
    for (i, c) in spec.conditions.iter().enumerate() {
        if i > 0 {
            name.push_str(if c.or { "Or" } else { "And" });
        }
        name.push_str(&path_segment(&c.path));
        name.push_str(&c.keyword);
    }
    name
}

/// The last segment of a dotted path — the parameter a condition or an assignment binds to.
fn leaf(path: &str) -> &str {
    path.rsplit('.').next().unwrap_or(path)
}

/// Where a member goes: just inside the type's closing brace.
///
/// Read off the **live buffer** rather than off an indexed offset, on purpose. The model may be
/// one keystroke stale, and an insertion placed from a stale offset lands in the middle of a
/// declaration — which is the one failure mode a generator must not have.
fn body_end(source: &str) -> usize {
    source.rfind('}').unwrap_or(source.len())
}

/// The start of the line the type is declared on, so an annotation inserted there lands under
/// the ones already written and above `public class …`.
///
/// Also read off the live buffer, and falling back to offset 0 rather than guessing: an
/// annotation at the top of the file is visibly wrong and fixable, whereas one spliced into the
/// middle of a method is not.
fn declaration_line_start(source: &str, simple: &str) -> usize {
    for keyword in ["class ", "record ", "interface ", "enum "] {
        let needle = format!("{keyword}{simple}");
        let mut from = 0usize;
        while let Some(i) = source[from..].find(&needle) {
            let at = from + i;
            let after = at + needle.len();
            let boundary = source[after..]
                .chars()
                .next()
                .is_none_or(|c| !c.is_alphanumeric() && c != '_' && c != '$');
            if boundary {
                return source[..at].rfind('\n').map(|n| n + 1).unwrap_or(0);
            }
            from = at + needle.len();
        }
    }
    0
}

/// The subject verbs the form offers, with the return shape each implies.
pub const SUBJECTS: &[(&str, &str)] = &[
    ("find", "the matching rows"),
    ("count", "how many match"),
    ("exists", "whether any match"),
    ("delete", "remove the matching rows"),
];

/// The comparison keywords the form offers, grouped by what they apply to. `""` is plain
/// equality — the default, and the one nobody should have to pick.
pub const KEYWORDS: &[(&str, &str)] = &[
    ("", "equals"),
    ("Not", "not equal to"),
    ("GreaterThan", "greater than"),
    ("GreaterThanEqual", "at least"),
    ("LessThan", "less than"),
    ("LessThanEqual", "at most"),
    ("Between", "between two values"),
    ("Like", "matches a pattern"),
    ("NotLike", "does not match a pattern"),
    ("StartingWith", "starts with"),
    ("EndingWith", "ends with"),
    ("Containing", "contains"),
    ("In", "in a collection"),
    ("NotIn", "not in a collection"),
    ("IsNull", "is null"),
    ("IsNotNull", "is not null"),
    ("True", "is true"),
    ("False", "is false"),
    ("Before", "before"),
    ("After", "after"),
    ("IsEmpty", "is an empty collection"),
    ("IsNotEmpty", "is a non-empty collection"),
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Entity, EntityField};

    fn field(name: &str, ty: &str, relation: &str, target: &str) -> EntityField {
        EntityField {
            name: name.into(),
            type_text: ty.into(),
            relation: relation.into(),
            target: target.into(),
            is_id: name == "id",
            ..EntityField::default()
        }
    }

    fn model() -> JpaModel {
        JpaModel {
            entities: vec![
                Entity {
                    fqcn: "com.acme.domain.Order".into(),
                    simple: "Order".into(),
                    entity_name: "Order".into(),
                    fields: vec![
                        field("id", "Long", "", ""),
                        field("total", "java.math.BigDecimal", "", ""),
                        field("customer", "Customer", "ManyToOne", "Customer"),
                    ],
                    ..Entity::default()
                },
                Entity {
                    fqcn: "com.acme.domain.Customer".into(),
                    simple: "Customer".into(),
                    entity_name: "Customer".into(),
                    fields: vec![field("name", "String", "", "")],
                    ..Entity::default()
                },
            ],
            repositories: vec![Repository {
                fqcn: "com.acme.repo.CustomerRepository".into(),
                simple: "CustomerRepository".into(),
                entity: "Customer".into(),
                file: "/p/CustomerRepository.java".into(),
                ..Repository::default()
            }],
        }
    }

    fn cond(path: &str, keyword: &str) -> Condition {
        Condition { path: path.into(), keyword: keyword.into(), ignore_case: false, or: false }
    }

    /// The point of building the name rather than typing it: it round-trips through the parser
    /// that validates it, which is the strongest guarantee this module can offer.
    #[test]
    fn a_generated_name_parses_back_to_the_spec_that_made_it() {
        let spec = QuerySpec {
            subject: "find".into(),
            conditions: vec![cond("customer.name", ""), Condition { or: true, ..cond("total", "GreaterThan") }],
            order_by: vec![("total".into(), true)],
            ..QuerySpec::default()
        };
        let name = method_name(&spec);
        assert_eq!(name, "findByCustomerNameOrTotalGreaterThanOrderByTotalDesc");

        let m = model();
        let parsed = crate::derived::parse(&name).expect("the generator's own output");
        let (resolved, issues) =
            crate::derived::resolve(&m, m.entity("Order").unwrap(), &parsed);
        assert!(issues.is_empty(), "a generated name cannot be wrong");
        assert_eq!(resolved.predicates[0].path, ["customer", "name"]);
        assert!(resolved.order_by[0].descending);
    }

    #[test]
    fn keyword_arity_drives_the_parameter_list() {
        let m = model();
        let repo = Repository { entity: "Order".into(), file: "/p/R.java".into(), ..Repository::default() };
        let spec = QuerySpec {
            subject: "find".into(),
            conditions: vec![cond("total", "Between"), cond("customer.name", "IsNull")],
            many: true,
            ..QuerySpec::default()
        };
        let g = query_method(&m, &repo, "interface R {\n}\n", &spec);
        // Two parameters for the Between, none for the IsNull.
        assert!(g.preview.contains("(BigDecimal totalFrom, BigDecimal totalTo)"), "{}", g.preview);
        assert!(g.preview.starts_with("List<Order> findByTotalBetweenAndCustomerNameIsNull"));
    }

    /// A renamed method stops being derivable, so it must arrive with its query written out —
    /// otherwise Spring Data tries to parse the new name as a property path and the context
    /// fails to start. This is the whole reason the override is not just a string swap.
    #[test]
    fn an_overridden_name_brings_its_query_with_it() {
        let m = model();
        let repo = Repository { entity: "Order".into(), file: "/p/R.java".into(), ..Repository::default() };
        let spec = QuerySpec {
            name: "soggetti_aderenti".into(),
            subject: "find".into(),
            conditions: vec![cond("customer.name", "Containing"), cond("total", "GreaterThan")],
            many: true,
            ..QuerySpec::default()
        };
        let g = query_method(&m, &repo, "interface R {\n}\n", &spec);
        assert!(g.preview.contains("List<Order> soggetti_aderenti("), "{}", g.preview);
        assert!(
            g.preview.contains(
                "@Query(\"select e from Order e where e.customer.name like concat('%', :name, '%') \
                 and e.total > :total\")"
            ),
            "{}",
            g.preview,
        );
    }

    /// The default is unchanged: no override, no annotation, and the name stays derived.
    #[test]
    fn without_an_override_nothing_changes() {
        let m = model();
        let repo = Repository { entity: "Order".into(), file: "/p/R.java".into(), ..Repository::default() };
        let spec = QuerySpec {
            subject: "find".into(),
            conditions: vec![cond("total", "GreaterThan")],
            many: true,
            ..QuerySpec::default()
        };
        let g = query_method(&m, &repo, "interface R {\n}\n", &spec);
        assert!(!g.preview.contains("@Query"), "a derived name needs none");
        assert_eq!(effective_name(&spec), "findByTotalGreaterThan");
    }

    /// The query can be written out on a derived name too — a starting point to edit, for the
    /// joins and fetches a name cannot express.
    #[test]
    fn the_query_can_be_asked_for_without_renaming_anything() {
        let m = model();
        let repo = Repository { entity: "Order".into(), file: "/p/R.java".into(), ..Repository::default() };
        let spec = QuerySpec {
            with_query: true,
            subject: "find".into(),
            conditions: vec![cond("total", "GreaterThan")],
            many: true,
            ..QuerySpec::default()
        };
        let g = query_method(&m, &repo, "interface R {\n}\n", &spec);
        assert!(g.preview.contains("@Query(\"select e from Order e where e.total > :total\")"), "{}", g.preview);
        assert!(g.preview.contains("List<Order> findByTotalGreaterThan("), "the name stays derived");
    }

    /// The failure that made this worth a test: one base with two type arguments used to be read
    /// as two bases, so the entity came back empty and the method generated as `List<>`.
    #[test]
    fn a_repository_whose_entity_did_not_resolve_is_not_generated_into_nonsense() {
        let m = model();
        let repo = Repository { entity: String::new(), file: "/p/R.java".into(), ..Repository::default() };
        let spec = QuerySpec { subject: "find".into(), many: true, ..QuerySpec::default() };
        let g = query_method(&m, &repo, "interface R {\n}\n", &spec);
        assert!(!g.preview.contains("List<>"), "an empty row type is never emitted: {}", g.preview);
    }

    #[test]
    fn a_paged_query_takes_a_pageable_and_returns_a_page() {
        let m = model();
        let repo = Repository { entity: "Order".into(), file: "/p/R.java".into(), ..Repository::default() };
        let spec = QuerySpec {
            subject: "find".into(),
            conditions: vec![cond("total", "GreaterThan")],
            paged: true,
            ..QuerySpec::default()
        };
        let g = query_method(&m, &repo, "interface R {\n}\n", &spec);
        assert!(g.preview.starts_with("Page<Order> "));
        assert!(g.preview.contains("Pageable pageable"));
    }

    #[test]
    fn a_count_returns_a_long_and_binds_no_row_type() {
        let m = model();
        let repo = Repository { entity: "Order".into(), file: "/p/R.java".into(), ..Repository::default() };
        let spec = QuerySpec {
            subject: "count".into(),
            conditions: vec![cond("total", "GreaterThan")],
            ..QuerySpec::default()
        };
        assert!(query_method(&m, &repo, "interface R {\n}\n", &spec)
            .preview
            .starts_with("long countByTotalGreaterThan("));
    }

    #[test]
    fn the_method_lands_inside_the_interface_body() {
        let m = model();
        let repo = Repository { entity: "Order".into(), file: "/p/R.java".into(), ..Repository::default() };
        let source = "package p;\ninterface R extends JpaRepository<Order, Long> {\n}\n";
        let spec = QuerySpec { subject: "find".into(), conditions: vec![cond("total", "")], ..QuerySpec::default() };
        let ins = query_method(&m, &repo, source, &spec).insertion.unwrap();
        let mut applied = source.to_string();
        applied.insert_str(ins.offset, &ins.text);
        assert!(applied.contains("    Optional<Order> findByTotal(BigDecimal total);\n}"), "{applied}");
    }

    /// Where repositories live is a convention, and the existing ones are the only honest source
    /// for it. Guessing puts every generated file in the wrong package.
    #[test]
    fn a_repository_is_generated_into_the_package_the_others_live_in() {
        let m = model();
        let g = repository(&m, m.entity("Order").unwrap(), "JpaRepository", "/p/src/main/java");
        let f = g.file.unwrap();
        assert_eq!(f.path, "/p/src/main/java/com/acme/repo/OrderRepository.java");
        assert!(f.content.contains("package com.acme.repo;"));
        assert!(f.content.contains("import com.acme.domain.Order;"), "a cross-package entity is imported");
        assert!(f.content.contains("public interface OrderRepository extends JpaRepository<Order, Long> {"));
    }

    #[test]
    fn with_no_repositories_to_learn_from_the_entitys_own_package_is_used() {
        let m = JpaModel { repositories: vec![], ..model() };
        let g = repository(&m, m.entity("Order").unwrap(), "CrudRepository", "/p/src/main/java");
        let f = g.file.unwrap();
        assert!(f.content.contains("package com.acme.domain;"));
        assert!(!f.content.contains("import com.acme.domain.Order;"), "same package needs no import");
    }

    /// The "both ways" the form offers, in one call.
    #[test]
    fn a_projection_comes_back_as_a_file_and_as_a_nested_interface() {
        let m = model();
        let repo = Repository { entity: "Order".into(), file: "/p/R.java".into(), ..Repository::default() };
        let source = "package p;\ninterface R {\n}\n";
        let g = projection(
            &m,
            m.entity("Order").unwrap(),
            "OrderSummary",
            &["total".to_string(), "customer.name".to_string()],
            Some((&repo, source)),
            "/p/src/main/java",
        );
        let file = g.file.unwrap();
        assert!(file.content.contains("BigDecimal getTotal();"));
        assert!(file.content.contains("String getCustomerName();"), "a path becomes one getter");

        let nested = g.insertion.unwrap();
        assert!(nested.text.contains("    interface OrderSummary {"));
        assert!(nested.text.contains("        String getCustomerName();"), "one indent deeper");
    }

    // ── Writing into an entity ───────────────────────────────────────────────

    fn order_source() -> &'static str {
        "package com.acme.domain;\n\n@Entity\n@Table(name = \"ORDINI\")\npublic class Order {\n    @Id\n    private Long id;\n}\n"
    }

    #[test]
    fn a_plain_attribute_carries_only_the_constraints_that_were_asked_for() {
        let m = model();
        let spec = AttributeSpec {
            name: "status".into(),
            type_text: "String".into(),
            column: "STATO".into(),
            length: Some(32),
            ..AttributeSpec::default()
        };
        let g = entity_attribute(m.entity("Order").unwrap(), order_source(), &spec);
        assert_eq!(
            g.preview,
            "    @Column(name = \"STATO\", nullable = false, length = 32)\n    private String status;",
        );

        // Nothing asked for, nothing written — an entity full of empty `@Column()` is noise.
        let bare = AttributeSpec {
            name: "note".into(),
            type_text: "String".into(),
            optional: true,
            ..AttributeSpec::default()
        };
        assert_eq!(
            entity_attribute(m.entity("Order").unwrap(), order_source(), &bare).preview,
            "    private String note;",
        );
    }

    /// The pair people get backwards by hand: the owning side has the join column, the inverse
    /// side has `mappedBy` and no column at all.
    #[test]
    fn only_the_owning_side_of_a_relation_gets_a_join_column() {
        let m = model();
        let e = m.entity("Order").unwrap();
        let owning = AttributeSpec {
            name: "customer".into(),
            type_text: "Customer".into(),
            relation: "ManyToOne".into(),
            lazy: true,
            ..AttributeSpec::default()
        };
        let g = entity_attribute(e, order_source(), &owning);
        assert!(g.preview.contains("@ManyToOne(fetch = FetchType.LAZY, optional = false)"), "{}", g.preview);
        assert!(g.preview.contains("@JoinColumn(name = \"CUSTOMER_ID\")"));

        let inverse = AttributeSpec {
            name: "lines".into(),
            type_text: "OrderLine".into(),
            relation: "OneToMany".into(),
            mapped_by: "order".into(),
            optional: true,
            ..AttributeSpec::default()
        };
        let g = entity_attribute(e, order_source(), &inverse);
        assert!(g.preview.contains("@OneToMany(mappedBy = \"order\")"), "{}", g.preview);
        assert!(!g.preview.contains("@JoinColumn"), "the inverse side owns no column");
        assert!(g.preview.contains("private List<OrderLine> lines;"), "a collection is a List");
    }

    #[test]
    fn accessors_are_written_only_when_asked_for() {
        let m = model();
        let spec = AttributeSpec {
            name: "total".into(),
            type_text: "BigDecimal".into(),
            optional: true,
            accessors: true,
            ..AttributeSpec::default()
        };
        let g = entity_attribute(m.entity("Order").unwrap(), order_source(), &spec);
        assert!(g.preview.contains("public BigDecimal getTotal() {"));
        assert!(g.preview.contains("this.total = total;"));
    }

    #[test]
    fn an_attribute_lands_inside_the_class_body() {
        let m = model();
        let spec = AttributeSpec { name: "x".into(), type_text: "String".into(), optional: true, ..AttributeSpec::default() };
        let ins = entity_attribute(m.entity("Order").unwrap(), order_source(), &spec).insertion.unwrap();
        let mut applied = order_source().to_string();
        applied.insert_str(ins.offset, &ins.text);
        assert!(applied.contains("    private String x;\n}"), "{applied}");
    }

    /// Under `@Entity` and `@Table`, above `public class` — where a reader looks for it.
    #[test]
    fn a_named_query_lands_on_the_declaration_line_not_at_the_top_of_the_file() {
        let m = model();
        let spec = NamedQuerySpec {
            name: "findOpen".into(),
            query: "select o from Order o where o.status = :status".into(),
        };
        let g = named_query(m.entity("Order").unwrap(), order_source(), &spec);
        assert_eq!(
            g.preview,
            "@NamedQuery(name = \"Order.findOpen\", query = \"select o from Order o where o.status = :status\")",
        );
        let ins = g.insertion.unwrap();
        let mut applied = order_source().to_string();
        applied.insert_str(ins.offset, &ins.text);
        assert!(applied.contains("@Table(name = \"ORDINI\")\n@NamedQuery("), "{applied}");
        assert!(applied.contains(")\npublic class Order {"), "{applied}");
    }

    #[test]
    fn a_named_query_is_qualified_by_its_entity_unless_it_already_is() {
        let m = model();
        let e = m.entity("Order").unwrap();
        assert_eq!(named_query_name(e, &NamedQuerySpec { name: "byId".into(), ..NamedQuerySpec::default() }), "Order.byId");
        assert_eq!(
            named_query_name(e, &NamedQuerySpec { name: "Legacy.byId".into(), ..NamedQuerySpec::default() }),
            "Legacy.byId",
            "a name that already qualifies itself is left alone",
        );
        // An empty query is still a legal starting point rather than a broken annotation.
        assert!(named_query(e, order_source(), &NamedQuerySpec { name: "x".into(), query: String::new() })
            .preview
            .contains("select e from Order e"));
    }

    #[test]
    fn a_lifecycle_callback_says_which_event_it_serves() {
        let m = model();
        let e = m.entity("Order").unwrap();
        assert_eq!(
            lifecycle_callback(e, order_source(), "PrePersist", "").preview,
            "    @PrePersist\n    void onPrePersist() {\n    }",
        );
        assert!(lifecycle_callback(e, order_source(), "PreUpdate", "touch")
            .preview
            .contains("void touch()"));
    }

    // ── Writing into a repository ────────────────────────────────────────────

    /// There is no derived form of an update anywhere in Spring Data, so the annotation is not a
    /// preference — a bare `updateStatusById` fails at application start.
    #[test]
    fn an_update_is_written_as_modifying_jpql_with_bound_parameters() {
        let m = model();
        let repo = Repository { entity: "Order".into(), file: "/p/R.java".into(), ..Repository::default() };
        let spec = ModifySpec {
            assignments: vec!["total".into()],
            conditions: vec![cond("id", "")],
            returns_count: true,
            ..ModifySpec::default()
        };
        let g = modify_method(&m, &repo, "interface R {\n}\n", &spec);
        assert!(
            g.preview.contains("@Query(\"update Order e set e.total = :total where e.id = :id\")"),
            "{}",
            g.preview,
        );
        assert!(g.preview.contains("@Modifying"));
        assert!(
            g.preview.contains("int updateTotalById(@Param(\"total\") BigDecimal total, @Param(\"id\") Long id);"),
            "{}",
            g.preview,
        );
    }

    #[test]
    fn a_delete_assigns_nothing_and_may_return_nothing() {
        let m = model();
        let repo = Repository { entity: "Order".into(), file: "/p/R.java".into(), ..Repository::default() };
        let spec = ModifySpec {
            delete: true,
            assignments: vec!["total".into()],
            conditions: vec![cond("total", "LessThan")],
            ..ModifySpec::default()
        };
        let g = modify_method(&m, &repo, "interface R {\n}\n", &spec);
        assert!(g.preview.contains("@Query(\"delete from Order e where e.total < :total\")"), "{}", g.preview);
        assert!(g.preview.contains("void deleteByTotalLessThan("), "{}", g.preview);
        assert!(!g.preview.contains("set "), "an assignment on a delete is ignored, not emitted");
    }

    #[test]
    fn a_between_condition_binds_both_ends_on_a_modify_method_too() {
        let m = model();
        let repo = Repository { entity: "Order".into(), file: "/p/R.java".into(), ..Repository::default() };
        let spec = ModifySpec {
            delete: true,
            conditions: vec![cond("total", "Between")],
            ..ModifySpec::default()
        };
        let g = modify_method(&m, &repo, "interface R {\n}\n", &spec);
        assert!(g.preview.contains("between :totalFrom and :totalTo"), "{}", g.preview);
        assert!(g.preview.contains("@Param(\"totalFrom\") BigDecimal totalFrom"), "{}", g.preview);
    }

    /// A stale model offset would splice a member into the middle of a declaration; the live
    /// buffer cannot.
    #[test]
    fn every_insertion_point_is_read_off_the_buffer_not_off_the_index() {
        assert_eq!(body_end("interface R {\n}\n"), 14);
        assert_eq!(body_end("no braces here"), 14, "a file mid-edit still gets a valid offset");
        let src = "package p;\n\n@Entity\npublic class Order {\n}\n";
        assert_eq!(&src[declaration_line_start(src, "Order")..][..6], "public");
        assert_eq!(declaration_line_start(src, "Nope"), 0, "a name that is not there falls back visibly");
        // `Orders` must not satisfy a search for `Order`.
        let src = "class Orders {\n}\nclass Order {\n}\n";
        assert_eq!(&src[declaration_line_start(src, "Order")..][..11], "class Order");
    }

    #[test]
    fn a_projection_field_that_resolves_to_nothing_is_dropped_rather_than_guessed() {
        let m = model();
        let g = projection(
            &m,
            m.entity("Order").unwrap(),
            "Bad",
            &["total".to_string(), "nope".to_string()],
            None,
            "/p/src",
        );
        let content = g.file.unwrap().content;
        assert!(content.contains("getTotal"));
        assert!(!content.contains("getNope"));
    }
}
