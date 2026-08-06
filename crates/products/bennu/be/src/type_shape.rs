//! `type_shape` domain — *what is inside this type*, answered one level at a time.
//!
//! The question the endpoints panel asks when a route says it takes a `QFormDto` and returns a
//! `QFormDto`: a name is not an answer, and opening the class to read it is losing your place in
//! the list of two hundred routes you were reading.
//!
//! **Lazy by design.** Nothing here runs while a catalog is built — the catalogs are lists of
//! hundreds of rows, and resolving every type named by every one of them would make the panel
//! pay, on open, for the two types you were going to look at. It is asked on the click that
//! expands a row, and the answer is small enough to be cheap to ask again.
//!
//! **One level at a time**, for the same reason: a DTO graph can be deep and can be cyclic
//! (`Order` holds `Customer` holds `List<Order>`), so this returns a type's own members with
//! enough on each to ask about it in turn. Recursion is the caller's, and it stops when the user
//! stops clicking.

use bennu_java::prelude::{ClassMembers, Member, MemberKind, TypeRef, Visibility};
use bennu_core::prelude::BennuState;
use serde::{Deserialize, Serialize};

use crate::index_service::IndexService;

/// One member of a type, as a row.
#[derive(Debug, Clone, Serialize)]
pub struct TypeMember {
    /// The field's name, or the property name behind a getter (`getTotal` → `total`).
    pub name: String,
    /// Its type as it reads (`List<Order>`, `String`).
    pub type_text: String,
    /// `field` | `property` — a property is what an interface or a bytecode-only class exposes
    /// instead of fields, and saying which keeps the reading honest.
    pub kind: String,
    /// The type to ask about next, qualified, when this member holds something worth opening —
    /// `None` for a `String`, an `int`, an enum. What makes the row expandable.
    pub expand: Option<String>,
    /// Declared by a supertype rather than by this class.
    pub inherited: bool,
}

/// What a type is and what it holds.
#[derive(Debug, Clone, Serialize)]
pub struct TypeShape {
    /// The qualified name it resolved to.
    pub name: String,
    /// The name without its package — what a row shows.
    pub simple: String,
    /// `class` | `interface` | `enum` | `record` | `annotation`.
    pub kind: String,
    /// Where the project declares it, when the project does. Absent for a library type.
    pub file: Option<String>,
    /// 1-based line of the declaration.
    pub line: Option<usize>,
    pub members: Vec<TypeMember>,
}

#[derive(Deserialize)]
pub struct TypeShapeArgs {
    /// The project root — only used to find where the type is declared.
    #[serde(default)]
    pub root: String,
    /// The file whose imports resolve a bare name. A qualified name resolves without it.
    pub file: String,
    /// The type as written (`QFormDto`, `ResponseEntity<QFormDto>`) or qualified.
    pub type_text: String,
}

/// The members of the type `type_text` names, or `null`.
///
/// `null` is the ordinary answer for most types — a `String`, an `int`, a type parameter, a class
/// the classpath cannot reach — and the caller offers no expansion rather than reporting a
/// failure. That is what lets the panel put an expander on everything that has one and on
/// nothing that has not, without a second contract.
#[arbor_rpc::handler]
fn bennu_type_shape(_ctx: &BennuState, args: TypeShapeArgs) -> Result<Option<TypeShape>, String> {
    let svc = IndexService::global();
    let Some(resolved) = svc.type_shape(&args.file, &args.type_text) else {
        return Ok(None);
    };
    let name = resolved.binary.replace('/', ".");
    // A scalar has members (every `String` has forty) and none of them are what anyone means by
    // "what is inside this". Refusing here rather than in the panel keeps one answer to the
    // question of what is worth opening.
    if is_scalar(&resolved.binary) {
        return Ok(None);
    }
    let members = members_of(&resolved.members, |b| !is_scalar(b));
    if members.is_empty() {
        return Ok(None);
    }
    let site = svc
        .class_index(&args.root)
        .unwrap_or_default()
        .into_iter()
        .find(|c| c.fqcn == name);
    Ok(Some(TypeShape {
        simple: name.rsplit('.').next().unwrap_or(&name).to_string(),
        kind: kind_of(&resolved.members),
        file: site.as_ref().map(|c| c.file.clone()),
        line: site.as_ref().map(|c| c.line),
        members,
        name,
    }))
}

/// `class` | `interface` | `enum` | `record` | `annotation`.
fn kind_of(cm: &ClassMembers) -> String {
    let f = &cm.flags;
    if f.is_annotation {
        "annotation"
    } else if f.is_interface {
        "interface"
    } else if f.is_enum {
        "enum"
    } else if f.is_record {
        "record"
    } else {
        "class"
    }
    .to_string()
}

/// The rows for one class: its instance fields, or — when it declares none — the properties its
/// getters expose.
///
/// The fallback is not a nicety. An interface has no fields at all, and a DTO read out of a jar
/// often has private fields the index did not keep; in both cases the getters *are* the shape,
/// and a panel that showed nothing there would be reporting "empty" for a type that is not.
fn members_of(cm: &ClassMembers, expandable: impl Fn(&str) -> bool) -> Vec<TypeMember> {
    let fields: Vec<TypeMember> = cm
        .fields
        .iter()
        .filter(|m| !m.is_static)
        .map(|m| member_row(m, &m.name, "field", &expandable))
        .collect();
    if !fields.is_empty() {
        return fields;
    }
    cm.methods
        .iter()
        .filter(|m| !m.is_static && m.visibility == Visibility::Public)
        .filter_map(|m| property_name(m).map(|p| member_row(m, &p, "property", &expandable)))
        .collect()
}

fn member_row(
    m: &Member,
    name: &str,
    kind: &str,
    expandable: &impl Fn(&str) -> bool,
) -> TypeMember {
    let binary = &m.return_type.binary_name;
    TypeMember {
        name: name.to_string(),
        type_text: render_type(&m.return_type),
        kind: kind.to_string(),
        expand: expandable(binary).then(|| binary.replace('/', ".")),
        inherited: false,
    }
}

/// The property a getter exposes: `getTotal` → `total`, `isPaid` → `paid`. `None` for anything
/// that is not a no-argument getter — a method with parameters is behaviour, not shape.
fn property_name(m: &Member) -> Option<String> {
    if m.kind != MemberKind::Method || !m.params.is_empty() {
        return None;
    }
    let rest = m
        .name
        .strip_prefix("get")
        .or_else(|| m.name.strip_prefix("is"))
        .filter(|r| r.starts_with(|c: char| c.is_ascii_uppercase()))?;
    let mut chars = rest.chars();
    let first = chars.next()?;
    Some(format!("{}{}", first.to_ascii_lowercase(), chars.as_str()))
}

/// `java/util/List<com/acme/Order>` → `List<Order>` — simple names, generics kept.
fn render_type(t: &TypeRef) -> String {
    let simple = t.binary_name.rsplit(['/', '$']).next().unwrap_or(&t.binary_name);
    if t.type_args.is_empty() {
        return simple.to_string();
    }
    let args: Vec<String> = t.type_args.iter().map(render_type).collect();
    format!("{simple}<{}>", args.join(", "))
}

/// Whether a type is one nobody means by "what is inside this": a primitive, a JDK value type,
/// or infrastructure a framework injects.
///
/// The package test is what makes this hold up on a project nobody wrote for it — a DTO can be
/// called anything, but it is never in `java.*` and never in `org.springframework.*`.
fn is_scalar(binary: &str) -> bool {
    const INFRA: [&str; 5] = ["java/", "javax/", "jakarta/", "org/springframework/", "kotlin/"];
    !binary.contains('/') || INFRA.iter().any(|p| binary.starts_with(p))
}

#[cfg(test)]
mod tests {
    use super::*;
    use bennu_java::prelude::ClassFlags;

    fn field(name: &str, ty: &str) -> Member {
        Member {
            name: name.to_string(),
            kind: MemberKind::Field,
            return_type: TypeRef::simple(ty),
            params: Vec::new(),
            is_static: false,
            is_abstract: false,
            is_default: false,
            is_final: false,
            visibility: Visibility::Private,
            raw_signature: name.to_string(),
            throws: Vec::new(),
        }
    }

    fn getter(name: &str, ty: &str) -> Member {
        Member { kind: MemberKind::Method, visibility: Visibility::Public, ..field(name, ty) }
    }

    fn class_of(fields: Vec<Member>, methods: Vec<Member>, flags: ClassFlags) -> ClassMembers {
        ClassMembers {
            superclass: None,
            interfaces: Vec::new(),
            methods,
            fields,
            flags,
            type_params: Vec::new(),
        }
    }

    #[test]
    fn a_dto_is_read_by_its_fields() {
        let cm = class_of(
            vec![field("total", "java/math/BigDecimal"), field("customer", "com/acme/Customer")],
            Vec::new(),
            ClassFlags::default(),
        );
        let rows = members_of(&cm, |b| !is_scalar(b));
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].kind, "field");
        // A JDK value type is a leaf: there is nothing anyone wants to open inside a BigDecimal.
        assert_eq!(rows[0].expand, None);
        // A type of the project's own is where the next click goes.
        assert_eq!(rows[1].expand.as_deref(), Some("com.acme.Customer"));
    }

    #[test]
    fn an_interface_is_read_by_its_getters() {
        // The case the fields-only reading gets wrong: an interface has none, and reporting it as
        // empty would be reporting a shape that is there as absent.
        let cm = class_of(
            Vec::new(),
            vec![getter("getTotal", "java/lang/String"), getter("isPaid", "boolean")],
            ClassFlags { is_interface: true, ..ClassFlags::default() },
        );
        let rows = members_of(&cm, |b| !is_scalar(b));
        assert_eq!(rows.iter().map(|r| r.name.as_str()).collect::<Vec<_>>(), ["total", "paid"]);
        assert!(rows.iter().all(|r| r.kind == "property"));
        assert_eq!(kind_of(&cm), "interface");
    }

    #[test]
    fn a_method_that_takes_arguments_is_behaviour_not_shape() {
        let mut m = getter("getThing", "java/lang/String");
        m.params = vec![TypeRef::simple("int")];
        assert_eq!(property_name(&m), None);
        // And a name that merely starts with the letters is not a getter either.
        assert_eq!(property_name(&getter("getaway", "java/lang/String")), None);
    }

    #[test]
    fn a_generic_type_reads_as_it_was_written() {
        let mut t = TypeRef::simple("java/util/List");
        t.type_args = vec![TypeRef::simple("com/acme/Order")];
        assert_eq!(render_type(&t), "List<Order>");
    }
}
