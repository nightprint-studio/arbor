//! What the type of the expression before the dot makes possible — read once, by walking the type's
//! hierarchy, so the templates ask questions of flags rather than of the resolver.

use crate::hierarchy::walk_up;
use crate::seam::{TypeRef, TypeResolver};

use super::names::{element_name, plural, type_name_hint, value_name};
use super::Written;

/// One element of what a value holds — an array's, an `Iterable`'s, an `Optional`'s.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Element {
    pub ty: Written,
    /// A name for a variable holding one.
    pub name: String,
    /// The primitive, for an array of them.
    pub primitive: Option<&'static str>,
}

/// How far an indexed loop over the value counts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Length {
    /// `ids.length` — an array.
    Field,
    /// `orders.size()` — a collection.
    Size,
    /// `name.length()` — a `CharSequence`.
    Chars,
    /// `count` — the value is the count.
    Itself,
}

/// The `Optional` a value is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OptionalShape {
    /// `Optional<T>`, with its `T`.
    Of(Element),
    Int,
    Long,
    Double,
}

impl OptionalShape {
    /// The optional's own type, as a declaration of it writes it.
    pub(crate) fn holder(&self) -> Written {
        match self {
            OptionalShape::Of(element) => {
                let mut imports = element.ty.imports.clone();
                imports.push("java.util.Optional".to_string());
                imports.sort();
                imports.dedup();
                Written { text: format!("Optional<{}>", element.ty.text), imports }
            }
            primitive => {
                let class = format!("Optional{}", primitive.primitive_title());
                Written { imports: vec![format!("java.util.{class}")], text: class }
            }
        }
    }

    /// The type of what it holds.
    pub(crate) fn value_type(&self) -> Written {
        match self {
            OptionalShape::Of(element) => element.ty.clone(),
            primitive => Written { text: primitive.primitive_title().to_ascii_lowercase(), imports: Vec::new() },
        }
    }

    /// A name for what it holds.
    pub(crate) fn value_name(&self) -> &str {
        match self {
            OptionalShape::Of(element) => &element.name,
            _ => "value",
        }
    }

    /// The method that takes the value out.
    pub(crate) fn getter(&self) -> &'static str {
        match self {
            OptionalShape::Of(_) => "get",
            OptionalShape::Int => "getAsInt",
            OptionalShape::Long => "getAsLong",
            OptionalShape::Double => "getAsDouble",
        }
    }

    /// The primitive stream of a primitive optional — `IntStream` — or `None` for `Optional<T>`.
    pub(crate) fn primitive_stream(&self) -> Option<String> {
        match self {
            OptionalShape::Of(_) => None,
            primitive => Some(format!("{}Stream", primitive.primitive_title())),
        }
    }

    fn primitive_title(&self) -> &'static str {
        match self {
            OptionalShape::Int => "Int",
            OptionalShape::Long => "Long",
            OptionalShape::Double | OptionalShape::Of(_) => "Double",
        }
    }
}

impl ValueShape {
    /// Every name a template could declare for this value — the value's own, an element's, what an
    /// `Optional` holds — passed through `spell`: how a project that writes `identity_resolver` gets
    /// `for (Order first_order : …)` instead of Java's camelCase. The caller owns the conventions.
    pub fn respell(&mut self, spell: impl Fn(&str) -> String) {
        self.name = spell(&self.name);
        let elements = [self.array_element.as_mut(), self.iterable_element.as_mut()];
        for element in elements.into_iter().flatten() {
            element.name = spell(&element.name);
        }
        if let Some(OptionalShape::Of(element)) = self.optional.as_mut() {
            element.name = spell(&element.name);
        }
    }
}

/// Everything a template asks of a value's type.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ValueShape {
    /// The type, as a declaration writes it. Empty for `void`.
    pub ty: Written,
    /// A name for a variable holding the value.
    pub name: String,
    /// A call that returns nothing: it can be a statement and nothing else.
    pub void: bool,
    /// The primitive the value is, when it is one — not its box.
    pub primitive: Option<&'static str>,
    /// `boolean` or `Boolean`.
    pub boolean: bool,
    /// The counter an indexed loop over it declares — `int`, or `long` for a `long`.
    pub counter: Option<&'static str>,
    /// Whether a `switch` accepts it at some level: an integral no wider than `int`, an enum, a
    /// `String` (Java 7).
    pub switchable: bool,
    pub length: Option<Length>,
    pub array_element: Option<Element>,
    /// What a for-each over it declares — set for every `Iterable`, collections included.
    pub iterable_element: Option<Element>,
    pub collection: bool,
    pub map: bool,
    pub optional: Option<OptionalShape>,
    pub closeable: bool,
    pub throwable: bool,
    pub string: bool,
    /// Whether the expression cannot be `null`: a literal, `this`, or a constructor call.
    pub non_null: bool,
    /// Whether the expression can be written twice without doing its work twice: no call, no `new`.
    pub repeatable: bool,
}

/// The shape of a value of type `ty`, written as `expr`.
pub fn shape_of(ty: &TypeRef, expr: &str, resolver: &dyn TypeResolver) -> ValueShape {
    let mut shape = ValueShape { non_null: is_non_null(expr), repeatable: is_repeatable(expr), ..ValueShape::default() };
    if ty.dims == 0 && ty.binary_name == "void" {
        shape.void = true;
        return shape;
    }
    shape.ty = written(ty);
    let own_name = value_name(expr);

    if ty.dims > 0 {
        let element = TypeRef { dims: ty.dims - 1, wildcard: false, ..ty.clone() };
        let hint = match primitive_of(&ty.binary_name) {
            Some(primitive) => primitive.to_string(),
            None => type_name_hint(&ty.binary_name),
        };
        shape.name = own_name.clone().unwrap_or_else(|| plural(&hint));
        shape.array_element = Some(element_of(&element, own_name.as_deref()));
        shape.length = Some(Length::Field);
        return shape;
    }

    shape.name = own_name.clone().unwrap_or_else(|| type_name_hint(&ty.binary_name));
    shape.primitive = primitive_of(&ty.binary_name);
    if let Some(primitive) = shape.primitive.or_else(|| unboxed(&ty.binary_name)) {
        shape.boolean = primitive == "boolean";
        if matches!(primitive, "int" | "long" | "short" | "byte" | "char") {
            shape.counter = Some(if primitive == "long" { "long" } else { "int" });
            shape.length = Some(Length::Itself);
        }
        shape.switchable = matches!(primitive, "int" | "short" | "byte" | "char");
        if shape.primitive.is_some() {
            return shape;
        }
    }
    reference_facts(&mut shape, ty, own_name.as_deref(), resolver);
    shape
}

/// What a class or interface type makes possible, read off its hierarchy in one walk.
fn reference_facts(shape: &mut ValueShape, ty: &TypeRef, own_name: Option<&str>, resolver: &dyn TypeResolver) {
    shape.optional = match ty.binary_name.as_str() {
        "java/util/Optional" => {
            let held = first_argument(ty, resolver);
            let name = type_name_hint(&held.binary_name);
            Some(OptionalShape::Of(Element { ty: written(&held), name, primitive: None }))
        }
        "java/util/OptionalInt" => Some(OptionalShape::Int),
        "java/util/OptionalLong" => Some(OptionalShape::Long),
        "java/util/OptionalDouble" => Some(OptionalShape::Double),
        _ => None,
    };
    if ty.binary_name == "java/lang/String" {
        shape.string = true;
        shape.switchable = true;
    }

    let mut iterable: Option<TypeRef> = None;
    let mut char_sequence = false;
    walk_up::<()>(resolver, ty, |ancestor| {
        match ancestor.ty.binary_name.as_str() {
            "java/lang/Iterable" => iterable = Some(ancestor.ty.clone()),
            "java/util/Collection" => shape.collection = true,
            "java/util/Map" => shape.map = true,
            "java/lang/AutoCloseable" => shape.closeable = true,
            "java/lang/Throwable" => shape.throwable = true,
            "java/lang/CharSequence" => char_sequence = true,
            "java/lang/Enum" => shape.switchable = true,
            _ => {}
        }
        if ancestor.depth == 0 && ancestor.members.flags.is_enum {
            shape.switchable = true;
        }
        None
    });

    if let Some(iterable) = iterable {
        // A raw `List` substitutes nothing, and its `Iterable<E>` would hand back a bare `E`.
        let element = match is_raw(ty, resolver) {
            true => TypeRef::simple("java/lang/Object"),
            false => iterable.type_args.first().cloned().unwrap_or_else(|| TypeRef::simple("java/lang/Object")),
        };
        shape.iterable_element = Some(element_of(&element, own_name));
    }
    shape.length = match (shape.collection, char_sequence) {
        (true, _) => Some(Length::Size),
        (false, true) => Some(Length::Chars),
        _ => None,
    };
}

/// The type argument an `Optional` holds — `Object` for a raw one.
fn first_argument(ty: &TypeRef, resolver: &dyn TypeResolver) -> TypeRef {
    match is_raw(ty, resolver) {
        true => TypeRef::simple("java/lang/Object"),
        false => ty.type_args.first().cloned().unwrap_or_else(|| TypeRef::simple("java/lang/Object")),
    }
}

/// A generic class used with no arguments at all.
fn is_raw(ty: &TypeRef, resolver: &dyn TypeResolver) -> bool {
    ty.type_args.is_empty() && resolver.members_of(&ty.binary_name).is_some_and(|m| !m.type_params.is_empty())
}

fn element_of(ty: &TypeRef, container: Option<&str>) -> Element {
    let primitive = (ty.dims == 0).then(|| primitive_of(&ty.binary_name)).flatten();
    Element { ty: written(ty), name: element_name(container, &ty.binary_name), primitive }
}

/// `ty` as source writes it, with the imports that spelling needs.
///
/// A captured wildcard among the arguments is written `?` — `Class<Annotation>` is not what
/// `annotationType()` returns and does not compile, `Class<?>` is and does. At the top it is written
/// as its bound: `Number n` is a fine variable for one element of a `List<? extends Number>`.
pub(crate) fn written(ty: &TypeRef) -> Written {
    let mut imports = Vec::new();
    let text = spell(ty, true, &mut imports);
    imports.sort();
    imports.dedup();
    Written { text, imports }
}

fn spell(ty: &TypeRef, top: bool, imports: &mut Vec<String>) -> String {
    if !top && ty.wildcard {
        return "?".to_string();
    }
    let mut out = match primitive_of(&ty.binary_name) {
        Some(primitive) => primitive.to_string(),
        None => {
            let (name, import) = source_name(&ty.binary_name);
            imports.extend(import);
            name
        }
    };
    if !ty.type_args.is_empty() {
        let mut arguments = Vec::with_capacity(ty.type_args.len());
        for argument in &ty.type_args {
            arguments.push(spell(argument, false, imports));
        }
        out.push('<');
        out.push_str(&arguments.join(", "));
        out.push('>');
    }
    for _ in 0..ty.dims {
        out.push_str("[]");
    }
    out
}

/// `java/util/Map$Entry` → (`Map.Entry`, `java.util.Map`); `java/lang/String` → (`String`, none);
/// a type variable `T` → (`T`, none).
///
/// A nested type is written through its outer class and the outer class is what gets imported — the
/// spelling every Java file uses for `Map.Entry`. The type part starts at the first capitalised
/// segment, which is how a project's own nested type (`com/acme/Outer/Inner`, slashes throughout) is
/// told from its package.
fn source_name(binary: &str) -> (String, Option<String>) {
    let segments: Vec<&str> = binary.split(['/', '$']).collect();
    let first_type = segments
        .iter()
        .position(|segment| segment.starts_with(|c: char| c.is_uppercase()))
        .unwrap_or(segments.len() - 1);
    let package = segments[..first_type].join(".");
    let types = &segments[first_type..];
    let import = (!package.is_empty() && package != "java.lang").then(|| format!("{package}.{}", types[0]));
    (types.join("."), import)
}

fn primitive_of(name: &str) -> Option<&'static str> {
    Some(match name {
        "boolean" => "boolean",
        "byte" => "byte",
        "char" => "char",
        "short" => "short",
        "int" => "int",
        "long" => "long",
        "float" => "float",
        "double" => "double",
        _ => return None,
    })
}

fn unboxed(binary: &str) -> Option<&'static str> {
    Some(match binary {
        "java/lang/Boolean" => "boolean",
        "java/lang/Byte" => "byte",
        "java/lang/Character" => "char",
        "java/lang/Short" => "short",
        "java/lang/Integer" => "int",
        "java/lang/Long" => "long",
        "java/lang/Float" => "float",
        "java/lang/Double" => "double",
        _ => return None,
    })
}

/// A literal, `this`, or a constructor call with nothing chained after it.
fn is_non_null(expr: &str) -> bool {
    let expr = expr.trim();
    if expr == "this" || expr.starts_with(|c: char| c.is_ascii_digit()) {
        return true;
    }
    if expr.len() >= 2 && ((expr.starts_with('"') && expr.ends_with('"')) || (expr.starts_with('\'') && expr.ends_with('\''))) {
        return true;
    }
    expr.strip_prefix("new ").is_some_and(|rest| {
        // `new Order(id)` — one argument list closing the expression, not `new Order(id).find()`.
        rest.find(['(', '['])
            .is_some_and(|open| !rest[..open].contains(|c: char| c == ')' || c == ']') && closes_at_end(&rest[open..]))
    })
}

/// Whether the bracket group opening `text` closes at its last byte.
fn closes_at_end(text: &str) -> bool {
    let mut depth = 0i32;
    for (i, byte) in text.bytes().enumerate() {
        match byte {
            b'(' | b'[' => depth += 1,
            b')' | b']' => {
                depth -= 1;
                if depth == 0 {
                    return i + 1 == text.len();
                }
            }
            _ => {}
        }
    }
    false
}

fn is_repeatable(expr: &str) -> bool {
    !expr.contains('(') && !expr.trim_start().starts_with("new ")
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::Arc;

    use super::*;
    use crate::seam::ClassMembers;
    use crate::symbols::Import;

    #[test]
    fn respelling_renames_the_value_its_elements_and_what_an_optional_holds() {
        let element = |name: &str| Element { ty: Written::default(), name: name.to_string(), primitive: None };
        let mut shape = ValueShape {
            name: "orderLines".to_string(),
            array_element: Some(element("orderLine")),
            iterable_element: Some(element("orderLine")),
            optional: Some(OptionalShape::Of(element("orderLine"))),
            ..Default::default()
        };
        shape.respell(|name| name.replace("orderLine", "order_line"));
        assert_eq!(shape.name, "order_lines");
        assert_eq!(shape.array_element.map(|e| e.name).as_deref(), Some("order_line"));
        assert_eq!(shape.iterable_element.map(|e| e.name).as_deref(), Some("order_line"));
        assert_eq!(shape.optional.as_ref().map(OptionalShape::value_name), Some("order_line"));
    }

    #[derive(Default)]
    struct Hierarchy(HashMap<&'static str, ClassMembers>);

    impl Hierarchy {
        fn class(mut self, name: &'static str, params: &[&str], supers: Vec<TypeRef>) -> Self {
            self.0.insert(
                name,
                ClassMembers {
                    type_params: params.iter().map(|p| p.to_string()).collect(),
                    superclass: None,
                    interfaces: supers,
                    methods: Vec::new(),
                    fields: Vec::new(),
                    flags: Default::default(),
                },
            );
            self
        }

        fn jdk() -> Self {
            Hierarchy::default()
                .class("java/lang/Iterable", &["T"], vec![])
                .class("java/util/Collection", &["E"], vec![generic("java/lang/Iterable", "E")])
                .class("java/util/List", &["E"], vec![generic("java/util/Collection", "E")])
                .class("java/util/Optional", &["T"], vec![])
                .class("java/lang/CharSequence", &[], vec![])
                .class("java/lang/String", &[], vec![TypeRef::simple("java/lang/CharSequence")])
                .class("com/acme/Order", &[], vec![])
                .class("com/acme/Orders", &[], vec![generic("java/lang/Iterable", "com/acme/Order")])
        }
    }

    impl TypeResolver for Hierarchy {
        fn members_of(&self, binary_name: &str) -> Option<Arc<ClassMembers>> {
            self.0.get(binary_name).cloned().map(Arc::new)
        }
        fn resolve_simple_name(&self, _name: &str, _imports: &[Import]) -> Option<String> {
            None
        }
    }

    fn generic(name: &str, argument: &str) -> TypeRef {
        TypeRef { type_args: vec![TypeRef::simple(argument)], ..TypeRef::simple(name) }
    }

    #[test]
    fn a_list_is_a_sized_collection_of_its_argument() {
        let shape = shape_of(&generic("java/util/List", "com/acme/Order"), "orders", &Hierarchy::jdk());
        assert_eq!(shape.ty.text, "List<Order>");
        assert_eq!(shape.ty.imports, ["com.acme.Order", "java.util.List"]);
        assert!(shape.collection);
        assert_eq!(shape.length, Some(Length::Size));
        let element = shape.iterable_element.expect("a list is iterable");
        assert_eq!((element.ty.text.as_str(), element.name.as_str()), ("Order", "order"));
    }

    /// `Iterable<Order>` is reached through a supertype that is not generic — only the walk's
    /// substitution knows the argument.
    #[test]
    fn an_iterable_class_iterates_what_its_supertype_says() {
        let shape = shape_of(&TypeRef::simple("com/acme/Orders"), "this.orders", &Hierarchy::jdk());
        assert!(!shape.collection);
        assert_eq!(shape.iterable_element.map(|e| e.ty.text), Some("Order".to_string()));
    }

    #[test]
    fn a_raw_list_iterates_objects_rather_than_a_bare_type_variable() {
        let shape = shape_of(&TypeRef::simple("java/util/List"), "rows", &Hierarchy::jdk());
        let element = shape.iterable_element.expect("iterable");
        assert_eq!(element.ty.text, "Object");
        assert_eq!(element.name, "row");
    }

    #[test]
    fn an_array_counts_by_its_length_and_names_its_elements() {
        let shape = shape_of(&TypeRef::simple("int").arrayed(1), "ids", &Hierarchy::jdk());
        assert_eq!(shape.ty.text, "int[]");
        assert_eq!(shape.length, Some(Length::Field));
        let element = shape.array_element.expect("an array has elements");
        assert_eq!((element.primitive, element.name.as_str()), (Some("int"), "id"));
    }

    #[test]
    fn a_boxed_integer_counts_and_switches_but_is_no_primitive() {
        let shape = shape_of(&TypeRef::simple("java/lang/Integer"), "count", &Hierarchy::jdk());
        assert_eq!(shape.primitive, None);
        assert_eq!(shape.counter, Some("int"));
        assert!(shape.switchable);
    }

    #[test]
    fn a_long_counts_with_a_long_and_cannot_be_switched_on() {
        let shape = shape_of(&TypeRef::simple("long"), "total", &Hierarchy::jdk());
        assert_eq!(shape.counter, Some("long"));
        assert!(!shape.switchable);
    }

    #[test]
    fn an_optional_holds_its_argument() {
        let shape = shape_of(&generic("java/util/Optional", "com/acme/Order"), "repo.findById(id)", &Hierarchy::jdk());
        let Some(OptionalShape::Of(element)) = &shape.optional else { panic!("an Optional: {shape:?}") };
        assert_eq!((element.ty.text.as_str(), element.name.as_str()), ("Order", "order"));
        assert!(!shape.repeatable, "a call must not be written twice");
        assert_eq!(shape.optional.as_ref().map(|o| o.holder().text), Some("Optional<Order>".to_string()));
    }

    #[test]
    fn a_string_has_a_length_and_switches() {
        let shape = shape_of(&TypeRef::simple("java/lang/String"), "name", &Hierarchy::jdk());
        assert_eq!(shape.length, Some(Length::Chars));
        assert!(shape.string && shape.switchable);
    }

    #[test]
    fn a_nested_type_is_written_through_its_outer_class() {
        let entry = TypeRef {
            type_args: vec![TypeRef::simple("java/lang/String"), TypeRef::simple("java/lang/Integer")],
            ..TypeRef::simple("java/util/Map$Entry")
        };
        let written = written(&entry);
        assert_eq!(written.text, "Map.Entry<String, Integer>");
        assert_eq!(written.imports, ["java.util.Map"]);
    }

    #[test]
    fn a_captured_wildcard_argument_is_written_as_one() {
        let class = TypeRef { type_args: vec![TypeRef::simple("java/lang/annotation/Annotation").captured()], ..TypeRef::simple("java/lang/Class") };
        assert_eq!(written(&class).text, "Class<?>");
    }

    #[test]
    fn only_literals_this_and_bare_constructor_calls_are_never_null() {
        let jdk = Hierarchy::jdk();
        let order = TypeRef::simple("com/acme/Order");
        assert!(shape_of(&order, "new Order(id)", &jdk).non_null);
        assert!(!shape_of(&order, "new Order(id).next()", &jdk).non_null);
        assert!(shape_of(&TypeRef::simple("java/lang/String"), "\"text\"", &jdk).non_null);
        assert!(!shape_of(&order, "order", &jdk).non_null);
    }

    #[test]
    fn a_void_call_is_only_void() {
        let shape = shape_of(&TypeRef::simple("void"), "run()", &Hierarchy::jdk());
        assert!(shape.void);
        assert!(shape.ty.text.is_empty());
    }
}
