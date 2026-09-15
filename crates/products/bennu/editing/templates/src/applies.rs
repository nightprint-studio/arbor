//! `bennu.applies` — the values a postfix template is offered on.
//!
//! ```jinja
//! {# bennu.applies: iterable, optional #}
//! {# bennu.applies: com.acme.Order #}
//! {# bennu.level: 9 #}
//! ```
//!
//! Each item is a word naming what the value's type makes possible (`any`, `reference`, `primitive`,
//! `boolean`, `number`, `string`, `array`, `iterable`, `collection`, `map`, `optional`, `closeable`,
//! `throwable`) or a fully-qualified class name, which accepts that class and every subtype of it. The
//! items are alternatives: a template is offered when **one** of them accepts the value. With no
//! `bennu.applies` at all it is offered on every value.
//!
//! `bennu.level` is the lowest Java level of the module the file belongs to that it is offered at — the
//! postfix counterpart of `bennu.requires: java >= …`, which is about the project rather than the module.
//!
//! The words are the flags of [`PostfixShape`], so a user template asks exactly the questions the built-in
//! catalogue asks, and gets the same answers: the one type model behind both lists.

use bennu_java::prelude::{PostfixShape, PostfixSubject};

use crate::engine::Directives;

/// One thing a postfix template can be offered on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PostfixTarget {
    Any,
    /// Anything that is not a primitive: an object, an array, a boxed number.
    Reference,
    Primitive,
    /// `boolean` or `Boolean`.
    Boolean,
    /// A numeric primitive, its box, or a `java.lang.Number`.
    Number,
    String,
    Array,
    /// Anything a for-each can walk: every `Iterable`, collections included.
    Iterable,
    Collection,
    Map,
    /// `Optional<T>` and the three primitive optionals.
    Optional,
    /// An `AutoCloseable` — what try-with-resources takes.
    Closeable,
    Throwable,
    /// That class — dotted, a nested type written `Map.Entry` — or a subtype of it.
    Class(String),
}

impl PostfixTarget {
    /// `iterable` → [`PostfixTarget::Iterable`]; `com.acme.Order` → a class. Words are read in any case; a
    /// class name keeps its own, because `com.acme.Order` and `com.acme.order` are two names.
    pub fn parse(word: &str) -> Result<Self, String> {
        let word = word.trim();
        Ok(match word.to_ascii_lowercase().as_str() {
            "any" => Self::Any,
            "reference" => Self::Reference,
            "primitive" => Self::Primitive,
            "boolean" => Self::Boolean,
            "number" => Self::Number,
            "string" => Self::String,
            "array" => Self::Array,
            "iterable" => Self::Iterable,
            "collection" => Self::Collection,
            "map" => Self::Map,
            "optional" => Self::Optional,
            "closeable" => Self::Closeable,
            "throwable" => Self::Throwable,
            _ if is_qualified_name(word) => Self::Class(word.replace('$', ".")),
            _ => {
                return Err(format!(
                    "`{word}` is not something a postfix template applies to — a word like `iterable`, or a class \
                     name with its package: `com.acme.Order`"
                ))
            }
        })
    }

    /// Whether a value of `shape` is one of these. `is_a` answers whether the value's type is a dotted class
    /// name or a subtype of it — asked only for a class name and a boxed or library number.
    fn accepts(&self, shape: &PostfixShape, is_a: &dyn Fn(&str) -> bool) -> bool {
        match self {
            Self::Any => true,
            Self::Reference => shape.primitive.is_none() && !shape.void,
            Self::Primitive => shape.primitive.is_some(),
            Self::Boolean => shape.boolean,
            Self::Number => is_number(shape, is_a),
            Self::String => shape.string,
            Self::Array => shape.array_element.is_some(),
            Self::Iterable => shape.iterable_element.is_some(),
            Self::Collection => shape.collection,
            Self::Map => shape.map,
            Self::Optional => shape.optional.is_some(),
            Self::Closeable => shape.closeable,
            Self::Throwable => shape.throwable,
            Self::Class(name) => !shape.void && is_a(name),
        }
    }
}

/// What a postfix template declares about the values it is offered on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PostfixApplicability {
    /// Alternatives — never empty: a template that names none applies to [`PostfixTarget::Any`].
    pub targets: Vec<PostfixTarget>,
    /// The lowest Java level it is offered at.
    pub level: Option<u32>,
}

impl PostfixApplicability {
    /// Read off a template's `bennu.applies` and `bennu.level`. An item that names nothing is an error rather
    /// than skipped: a template offered everywhere because of a typo in the one line that was meant to narrow
    /// it is the worst reading of that line.
    pub fn read(template: &str) -> Result<Self, String> {
        let directives = Directives::read(template);
        let mut targets = directives
            .list("applies")
            .iter()
            .map(|item| PostfixTarget::parse(item))
            .collect::<Result<Vec<_>, _>>()?;
        if targets.is_empty() {
            targets.push(PostfixTarget::Any);
        }
        let level = directives.get("level").map(parse_level).transpose()?;
        Ok(Self { targets, level })
    }

    /// Whether the template is offered on `subject`, in a module at `level`.
    ///
    /// Values only: a class name before the dot (`Order.new`) is not a value a template can rewrite.
    pub fn accepts(&self, subject: &PostfixSubject, level: u32, is_a: &dyn Fn(&str) -> bool) -> bool {
        if self.level.is_some_and(|lowest| level < lowest) {
            return false;
        }
        let PostfixSubject::Value { shape, .. } = subject else { return false };
        self.targets.iter().any(|target| target.accepts(shape, is_a))
    }
}

/// `9`, `1.8` or `>= 11` → the level. The `>=` is allowed because it is how `bennu.requires` writes the
/// same thought, and a template author will write it both ways.
fn parse_level(value: &str) -> Result<u32, String> {
    let number = value.trim().trim_start_matches(">=").trim();
    number
        .strip_prefix("1.")
        .unwrap_or(number)
        .parse()
        .map_err(|_| format!("`bennu.level: {value}` is not a Java level — write it as a number: `bennu.level: 11`"))
}

fn is_number(shape: &PostfixShape, is_a: &dyn Fn(&str) -> bool) -> bool {
    if let Some(primitive) = shape.primitive {
        return matches!(primitive, "byte" | "short" | "int" | "long" | "float" | "double");
    }
    // A box is `java.lang`'s, so it is told by its spelling without walking anything.
    let boxed = shape.ty.imports.is_empty()
        && matches!(shape.ty.text.as_str(), "Byte" | "Short" | "Integer" | "Long" | "Float" | "Double");
    boxed || (!shape.void && shape.array_element.is_none() && is_a("java.lang.Number"))
}

/// `com.acme.Order`, `java.util.Map.Entry`, `java.util.Map$Entry` — at least one dot, identifiers between.
fn is_qualified_name(word: &str) -> bool {
    let segments: Vec<&str> = word.split(['.', '$']).collect();
    segments.len() > 1
        && segments.iter().all(|segment| {
            segment.starts_with(|c: char| c.is_alphabetic() || c == '_')
                && segment.chars().all(|c| c.is_alphanumeric() || c == '_')
        })
}

#[cfg(test)]
mod tests {
    use bennu_java::prelude::{PostfixElement, PostfixOptional, PostfixWritten};

    use super::*;

    fn value(shape: PostfixShape) -> PostfixSubject {
        PostfixSubject::Value { text: "it".to_string(), shape }
    }

    fn written(text: &str, imports: &[&str]) -> PostfixWritten {
        PostfixWritten { text: text.to_string(), imports: imports.iter().map(|i| i.to_string()).collect() }
    }

    fn list_of_orders() -> PostfixSubject {
        let order = PostfixElement { ty: written("Order", &["com.acme.Order"]), name: "order".into(), primitive: None };
        value(PostfixShape {
            ty: written("List<Order>", &["com.acme.Order", "java.util.List"]),
            name: "orders".into(),
            iterable_element: Some(order),
            collection: true,
            ..PostfixShape::default()
        })
    }

    fn int() -> PostfixSubject {
        value(PostfixShape { ty: written("int", &[]), name: "count".into(), primitive: Some("int"), ..PostfixShape::default() })
    }

    fn nothing_is_a(_: &str) -> bool {
        false
    }

    #[test]
    fn a_template_that_says_nothing_applies_to_every_value() {
        let applies = PostfixApplicability::read("log.debug({{ expr }});").unwrap();
        assert_eq!(applies, PostfixApplicability { targets: vec![PostfixTarget::Any], level: None });
        assert!(applies.accepts(&int(), 8, &nothing_is_a));
        assert!(applies.accepts(&list_of_orders(), 8, &nothing_is_a));
    }

    #[test]
    fn the_items_are_alternatives_read_in_any_case() {
        let applies = PostfixApplicability::read("{# bennu.applies: Optional, iterable #}\nx").unwrap();
        assert_eq!(applies.targets, [PostfixTarget::Optional, PostfixTarget::Iterable]);
        assert!(applies.accepts(&list_of_orders(), 17, &nothing_is_a), "a list is iterable");
        assert!(!applies.accepts(&int(), 17, &nothing_is_a), "an int is neither");
    }

    #[test]
    fn a_word_that_names_nothing_is_an_error_rather_than_everything() {
        let error = PostfixApplicability::read("{# bennu.applies: iterabel #}\nx").unwrap_err();
        assert!(error.contains("iterabel"), "{error}");
        assert!(PostfixTarget::parse("Order").is_err(), "a class name carries its package");
    }

    #[test]
    fn a_class_name_asks_whether_the_value_is_one_or_a_subtype() {
        let applies = PostfixApplicability::read("{# bennu.applies: java.util.Map$Entry, java.util.Collection #}\nx").unwrap();
        assert_eq!(
            applies.targets,
            [PostfixTarget::Class("java.util.Map.Entry".into()), PostfixTarget::Class("java.util.Collection".into())]
        );
        let is_a_collection: &dyn Fn(&str) -> bool = &|name| name == "java.util.Collection";
        assert!(applies.accepts(&list_of_orders(), 17, is_a_collection));
        assert!(!applies.accepts(&list_of_orders(), 17, &nothing_is_a));
    }

    #[test]
    fn the_level_is_a_floor_on_the_module_and_reads_the_way_requires_writes_it() {
        let applies = PostfixApplicability::read("{# bennu.level: >= 1.9 #}\nx").unwrap();
        assert_eq!(applies.level, Some(9));
        assert!(!applies.accepts(&int(), 8, &nothing_is_a));
        assert!(applies.accepts(&int(), 9, &nothing_is_a));
        assert!(PostfixApplicability::read("{# bennu.level: recent #}\nx").is_err());
    }

    #[test]
    fn numbers_are_numeric_primitives_their_boxes_and_numbers_by_type() {
        let number = PostfixApplicability::read("{# bennu.applies: number #}\nx").unwrap();
        assert!(number.accepts(&int(), 8, &nothing_is_a));
        let boolean = value(PostfixShape { ty: written("boolean", &[]), primitive: Some("boolean"), boolean: true, ..PostfixShape::default() });
        assert!(!number.accepts(&boolean, 8, &nothing_is_a), "a boolean counts nothing");
        let boxed = value(PostfixShape { ty: written("Integer", &[]), ..PostfixShape::default() });
        assert!(number.accepts(&boxed, 8, &nothing_is_a));
        let decimal = value(PostfixShape { ty: written("BigDecimal", &["java.math.BigDecimal"]), ..PostfixShape::default() });
        assert!(number.accepts(&decimal, 8, &|name| name == "java.lang.Number"));
    }

    #[test]
    fn a_primitive_is_no_reference_and_an_optional_is_one() {
        let reference = PostfixApplicability::read("{# bennu.applies: reference #}\nx").unwrap();
        assert!(!reference.accepts(&int(), 8, &nothing_is_a));
        let optional = value(PostfixShape { ty: written("OptionalInt", &["java.util.OptionalInt"]), optional: Some(PostfixOptional::Int), ..PostfixShape::default() });
        assert!(reference.accepts(&optional, 8, &nothing_is_a));
        assert!(PostfixApplicability::read("{# bennu.applies: optional #}\nx").unwrap().accepts(&optional, 8, &nothing_is_a));
    }

    /// A class name before the dot is not a value a template can rewrite.
    #[test]
    fn a_type_before_the_dot_takes_no_template() {
        let applies = PostfixApplicability::read("x").unwrap();
        assert!(!applies.accepts(&PostfixSubject::Type { text: "Order".into() }, 17, &nothing_is_a));
    }
}
