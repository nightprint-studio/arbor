//! What this crate knows about a project's persistence layer.
//!
//! Facts with spans, and nothing that needs a database connection: everything here is read out
//! of Java source. Whether the column actually exists in the schema is Picus's question, not
//! this crate's — and pretending otherwise is how a tool starts lying about a legacy database
//! nobody has migrated.

/// A `@Entity` (or `@Embeddable` / `@MappedSuperclass`) class.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Entity {
    pub fqcn: String,
    /// Simple name — how a repository's type argument and a JPQL `FROM` clause name it.
    pub simple: String,
    /// The name JPQL addresses it by: `@Entity(name = …)` when given, else the simple name.
    pub entity_name: String,
    /// `@Table(name = …)`, empty when defaulted.
    pub table: String,
    /// `entity` | `embeddable` | `mapped-superclass`.
    pub kind: String,
    /// The superclass as written, for folding a `@MappedSuperclass`'s fields in.
    pub extends: String,
    pub fields: Vec<EntityField>,
    /// Absolute path, forward-slashed.
    pub file: String,
    /// Byte offset of the type name.
    pub offset: usize,
    pub line: u32,
}

impl Entity {
    /// The persistent field called `name`, if any.
    pub fn field(&self, name: &str) -> Option<&EntityField> {
        self.fields.iter().find(|f| f.name == name)
    }

    /// The id field, when the entity declares one.
    pub fn id_field(&self) -> Option<&EntityField> {
        self.fields.iter().find(|f| f.is_id)
    }
}

/// One persistent field of an entity.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EntityField {
    pub name: String,
    pub type_text: String,
    /// `@Column(name = …)` when given, else empty (the provider defaults it).
    pub column: String,
    pub is_id: bool,
    /// The relation annotation, when this field is one (`OneToMany`, …). Empty otherwise.
    pub relation: String,
    /// For a relation, the entity on the other end as written — the element type for a
    /// collection, the field type otherwise.
    pub target: String,
    /// `@Transient` — mapped by nothing, and therefore not addressable in a query.
    pub transient: bool,
    pub offset: usize,
    pub line: u32,
}

impl EntityField {
    /// Whether the path may continue below this field — a relation or an embedded object.
    pub fn is_navigable(&self) -> bool {
        !self.relation.is_empty()
    }
}

/// A Spring Data repository interface.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Repository {
    pub fqcn: String,
    pub simple: String,
    /// The managed entity, as written in the `extends JpaRepository<Order, Long>` argument.
    pub entity: String,
    /// The id type, second type argument. Empty when it could not be read.
    pub id_type: String,
    /// The base interface it extends (`JpaRepository`, `CrudRepository`, `PagingAndSorting…`).
    pub base: String,
    pub methods: Vec<RepoMethod>,
    pub file: String,
    pub offset: usize,
    pub line: u32,
}

/// One method declared on a repository — the interesting half of the crate.
///
/// A repository method is a query written in one of two languages, and which one it is decides
/// what can be checked about it:
///
/// - **declared** (`@Query`) — the text is JPQL or SQL, and what can be verified is its
///   parameters against the method's;
/// - **derived** — the *name* is the query (`findByCustomerNameAndTotalGreaterThan`), and every
///   segment of it must be a real property path on the entity. This is the one that catches
///   bugs, because a typo in a derived name fails at **application start**, not at compile time.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RepoMethod {
    pub name: String,
    pub return_type: String,
    /// Parameters as declared, with their `@Param("…")` name when they carry one.
    pub params: Vec<MethodParam>,
    /// The `@Query` on it, when written.
    pub query: Option<QueryDef>,
    /// Whether it carries `@Modifying` — a write, which JPA requires for anything but a select.
    pub modifying: bool,
    pub offset: usize,
    pub line: u32,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MethodParam {
    pub name: String,
    pub type_text: String,
    /// The name `@Param("…")` binds it to, empty when it carries none.
    pub bound_name: String,
    pub offset: usize,
}

impl MethodParam {
    /// The name a `:placeholder` in a query must use to reach this parameter.
    pub fn effective_name(&self) -> &str {
        if self.bound_name.is_empty() {
            &self.name
        } else {
            &self.bound_name
        }
    }
}

/// A `@Query` — its text, where it is, and what it asks for.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct QueryDef {
    /// The query text with escapes as written.
    pub text: String,
    /// Byte span of the string literal's CONTENTS in the file (quotes excluded), so a
    /// highlight lands on the query and not on the Java syntax around it.
    pub start: usize,
    pub end: usize,
    /// `nativeQuery = true` — the text is SQL for the database, not JPQL for the provider.
    pub native: bool,
    /// The `:named` placeholders it uses, in order of first appearance.
    pub named_params: Vec<String>,
    /// The `?1` positional placeholders it uses, deduplicated and sorted.
    pub positional_params: Vec<u32>,
}

/// Everything the extension knows about a project's persistence layer.
#[derive(Debug, Default, Clone)]
pub struct JpaModel {
    pub entities: Vec<Entity>,
    pub repositories: Vec<Repository>,
}

impl JpaModel {
    /// The entity written as `name` — matched on the simple name, then on the JPQL entity name,
    /// then on the fully-qualified one.
    pub fn entity(&self, name: &str) -> Option<&Entity> {
        let stripped = strip_generics(name);
        let bare = simple_name(&stripped);
        self.entities
            .iter()
            .find(|e| e.fqcn == name)
            .or_else(|| self.entities.iter().find(|e| e.simple == bare))
            .or_else(|| self.entities.iter().find(|e| e.entity_name == bare))
    }

    /// The repositories that manage `entity` (by simple name). Several is normal and correct —
    /// a read-only projection repository beside the main one.
    pub fn repositories_of(&self, entity: &str) -> Vec<&Repository> {
        let stripped = strip_generics(entity);
        let bare = simple_name(&stripped);
        self.repositories
            .iter()
            .filter(|r| simple_name(&strip_generics(&r.entity)) == bare)
            .collect()
    }

    /// The repository declared at `file`, if the file declares one.
    pub fn repository_in(&self, file: &str) -> Option<&Repository> {
        self.repositories.iter().find(|r| r.file == file)
    }

    /// The entity, with the fields of its `@MappedSuperclass` chain folded in.
    ///
    /// Not a convenience: an `id` declared on `AbstractAuditable` and inherited by forty
    /// entities is the single most common thing a derived query addresses, and a check that
    /// cannot see it would report every one of them as unknown.
    ///
    /// The one lifetime is load-bearing rather than decoration: the result mixes the entity's own
    /// fields with its ancestors', which come from `self` — so the two borrows have to be the same
    /// one, and every caller that walks a chain of entities needs to say so too.
    pub fn fields_of<'a>(&'a self, entity: &'a Entity) -> Vec<&'a EntityField> {
        let mut out: Vec<&EntityField> = entity.fields.iter().collect();
        let mut current = entity;
        // Bounded: a mapped-superclass chain deeper than this is not a real design.
        for _ in 0..8 {
            if current.extends.is_empty() {
                break;
            }
            let Some(parent) = self.entity(&current.extends) else { break };
            if parent.fqcn == current.fqcn {
                break;
            }
            for f in &parent.fields {
                if !out.iter().any(|x| x.name == f.name) {
                    out.push(f);
                }
            }
            current = parent;
        }
        out
    }
}

/// `com.acme.Order` → `Order`; also the last segment of a nested name.
pub fn simple_name(fqcn: &str) -> &str {
    fqcn.rsplit(['.', '$']).next().unwrap_or(fqcn)
}

/// `List<Order>` → `List`.
pub fn strip_generics(type_text: &str) -> String {
    match type_text.find('<') {
        Some(i) => type_text[..i].trim().to_string(),
        None => type_text.trim().to_string(),
    }
}

/// The `n`th type argument of `Foo<A, B>`, respecting nesting. `None` when absent.
pub fn type_argument(type_text: &str, n: usize) -> Option<String> {
    let open = type_text.find('<')?;
    let close = type_text.rfind('>')?;
    if close <= open {
        return None;
    }
    let inner = &type_text[open + 1..close];
    let mut depth = 0usize;
    let mut start = 0usize;
    let mut found = 0usize;
    for (i, c) in inner.char_indices() {
        match c {
            '<' => depth += 1,
            '>' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => {
                if found == n {
                    return Some(inner[start..i].trim().to_string());
                }
                found += 1;
                start = i + 1;
            }
            _ => {}
        }
    }
    (found == n).then(|| inner[start..].trim().to_string())
}

/// The 1-based line of `offset` in `source`.
///
/// Re-exported rather than reimplemented: counting newlines is exactly the kind of three-line
/// function that ends up written four times with one of them off by one.
pub use bennu_complete::prelude::line_number as line_at;

/// A field name as a query would spell it in camelCase — the inverse of the segment splitting
/// in [`crate::derived`]. `"CustomerName"` → `"customerName"`.
pub fn decapitalize(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        Some(first) => first.to_lowercase().chain(chars).collect(),
        None => String::new(),
    }
}

/// A field name as a query method spells it — `"customerName"` → `"CustomerName"`.
pub fn capitalize(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn field(name: &str) -> EntityField {
        EntityField { name: name.to_string(), ..EntityField::default() }
    }

    fn entity(simple: &str, extends: &str, fields: &[&str]) -> Entity {
        Entity {
            fqcn: format!("com.acme.{simple}"),
            simple: simple.to_string(),
            entity_name: simple.to_string(),
            extends: extends.to_string(),
            fields: fields.iter().map(|f| field(f)).collect(),
            ..Entity::default()
        }
    }

    #[test]
    fn an_entity_resolves_by_simple_qualified_or_jpql_name() {
        let m = JpaModel {
            entities: vec![Entity {
                entity_name: "OrderLine".to_string(),
                ..entity("Order", "", &["id"])
            }],
            ..JpaModel::default()
        };
        assert!(m.entity("Order").is_some());
        assert!(m.entity("com.acme.Order").is_some());
        assert!(m.entity("OrderLine").is_some(), "the name JPQL uses");
        assert!(m.entity("List<Order>").is_some(), "generics are stripped first");
        assert!(m.entity("Nope").is_none());
    }

    /// The inheritance case is not a nicety: an `id` on a shared `@MappedSuperclass` is what
    /// most derived queries address, and missing it would flag every one of them.
    #[test]
    fn a_mapped_superclasss_fields_are_folded_into_its_children() {
        let m = JpaModel {
            entities: vec![
                entity("Order", "Auditable", &["total"]),
                entity("Auditable", "", &["id", "createdAt"]),
            ],
            ..JpaModel::default()
        };
        let names: Vec<&str> =
            m.fields_of(m.entity("Order").unwrap()).iter().map(|f| f.name.as_str()).collect();
        assert_eq!(names, ["total", "id", "createdAt"]);
    }

    #[test]
    fn a_cycle_in_the_superclass_chain_terminates() {
        let m = JpaModel {
            entities: vec![entity("A", "B", &["a"]), entity("B", "A", &["b"])],
            ..JpaModel::default()
        };
        assert_eq!(m.fields_of(m.entity("A").unwrap()).len(), 2);
    }

    #[test]
    fn repositories_are_found_by_the_entity_they_manage() {
        let m = JpaModel {
            repositories: vec![
                Repository { entity: "Order".into(), simple: "OrderRepo".into(), ..Repository::default() },
                Repository { entity: "com.acme.Order".into(), simple: "ReadOnly".into(), ..Repository::default() },
                Repository { entity: "Customer".into(), ..Repository::default() },
            ],
            ..JpaModel::default()
        };
        assert_eq!(m.repositories_of("Order").len(), 2, "qualified and simple both count");
    }

    #[test]
    fn type_arguments_respect_nesting() {
        assert_eq!(type_argument("JpaRepository<Order, Long>", 0).as_deref(), Some("Order"));
        assert_eq!(type_argument("JpaRepository<Order, Long>", 1).as_deref(), Some("Long"));
        assert_eq!(type_argument("Map<String, List<Order>>", 1).as_deref(), Some("List<Order>"));
        assert_eq!(type_argument("JpaRepository<Order, Long>", 2), None);
        assert_eq!(type_argument("Order", 0), None);
    }

    #[test]
    fn a_param_prefers_its_bound_name() {
        let bare = MethodParam { name: "id".into(), ..MethodParam::default() };
        assert_eq!(bare.effective_name(), "id");
        let bound = MethodParam { name: "id".into(), bound_name: "orderId".into(), ..MethodParam::default() };
        assert_eq!(bound.effective_name(), "orderId");
    }

    #[test]
    fn case_helpers_round_trip() {
        assert_eq!(capitalize("customerName"), "CustomerName");
        assert_eq!(decapitalize("CustomerName"), "customerName");
        assert_eq!(capitalize(""), "");
        assert_eq!(decapitalize("URL"), "uRL", "only the first character — Java's own rule");
    }
}
