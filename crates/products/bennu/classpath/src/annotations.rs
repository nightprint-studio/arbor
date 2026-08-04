//! Annotations read off a `.class` — the part of bytecode a framework is written in.
//!
//! A class file records `@Service`, `@Bean`, `@Entity` and their element values in
//! `RuntimeVisibleAnnotations`, so a library's framework metadata is readable without
//! its source. That is what makes it possible to say where a bean declared inside a jar
//! comes from, when the only thing on disk is the jar.
//!
//! **Deliberately not part of [`ClassMembers`](crate::members::ClassMembers).** That one
//! is memoized for every class the resolver ever touches — the whole JDK, every
//! dependency — and it is read on the hot path of completion and inference, neither of
//! which has any use for an annotation. Folding these in would grow every persisted memo
//! and invalidate the ones already on disk, to carry a field almost nothing reads. So
//! this is a **separate, opt-in decode**: a caller that wants annotations asks for them,
//! for the few classes it cares about, and everything else pays nothing.
//!
//! Only `RuntimeVisible*` is read. `RuntimeInvisible*` is what `CLASS`-retention
//! annotations land in, and those are by definition not part of what the framework sees
//! at run time — reporting them would describe a program that does not exist.

use std::borrow::Cow;

use cafebabe::attributes::{AnnotationElementValue, AttributeData, AttributeInfo};
use cafebabe::{parse_class, ClassFile};

/// One annotation, with the elements that were written.
#[derive(Debug, Clone, PartialEq)]
pub struct Annotation {
    /// Dotted FQN (`org.springframework.stereotype.Service`) — the form the annotation is
    /// written and matched by, not the `Lorg/…;` descriptor it is stored as.
    pub type_name: String,
    /// Element name → value, in class-file order. Elements left at their default are
    /// **absent**: a class file records only what was written, so "no entry" means
    /// "defaulted", not "empty".
    pub elements: Vec<(String, AnnotationValue)>,
}

impl Annotation {
    /// The value of element `name`, or `None` when it was left at its default.
    pub fn element(&self, name: &str) -> Option<&AnnotationValue> {
        self.elements.iter().find(|(n, _)| n == name).map(|(_, v)| v)
    }

    /// The single unnamed argument — `@Service("audit")` stores it under `value`. The
    /// commonest read there is, and the one every caller would otherwise open-code.
    pub fn value(&self) -> Option<&AnnotationValue> {
        self.element("value")
    }

    /// The annotation's simple name (`Service`) — what is written at the use site.
    pub fn simple_name(&self) -> &str {
        self.type_name.rsplit('.').next().unwrap_or(&self.type_name)
    }
}

/// An annotation element's value.
///
/// Numbers, booleans, chars and enum constants all render to [`Text`](Self::Text): the
/// consumers are framework readers asking "what does `havingValue` say", and a typed
/// numeric tower would be shape nobody uses. A **class literal stays distinct**, because
/// `@ConditionalOnClass(DataSource.class)` names a type and the reader has to be able to
/// look it up rather than match its spelling.
#[derive(Debug, Clone, PartialEq)]
pub enum AnnotationValue {
    /// A string, number, boolean, char, or enum constant, rendered.
    Text(String),
    /// A `Foo.class` literal, as a dotted FQN.
    ClassRef(String),
    /// `{ … }` — an array element value.
    List(Vec<AnnotationValue>),
    /// A nested annotation.
    Nested(Box<Annotation>),
}

impl AnnotationValue {
    /// This value as text when it is a single one, else `None`. A one-element array
    /// answers as its element — `@ConditionalOnProperty(name = "a")` and
    /// `name = {"a"}` are the same statement written two ways, and a caller asking
    /// "what is the name" should not have to know which was used.
    pub fn as_text(&self) -> Option<&str> {
        match self {
            Self::Text(s) => Some(s),
            Self::ClassRef(s) => Some(s),
            Self::List(items) if items.len() == 1 => items[0].as_text(),
            _ => None,
        }
    }

    /// Every text this value carries, flattening an array. Empty for a nested annotation.
    pub fn texts(&self) -> Vec<&str> {
        match self {
            Self::Text(s) | Self::ClassRef(s) => vec![s],
            Self::List(items) => items.iter().flat_map(|i| i.texts()).collect(),
            Self::Nested(_) => Vec::new(),
        }
    }
}

/// A member's annotations, keyed by what identifies it in the class file.
#[derive(Debug, Clone, PartialEq)]
pub struct MemberAnnotations {
    pub name: String,
    /// The erased JVM descriptor (`()Ljavax/sql/DataSource;`) — the only thing that tells
    /// two overloads apart, and a `@Bean` factory method is routinely overloaded.
    pub descriptor: String,
    pub annotations: Vec<Annotation>,
}

/// Everything annotated in one class.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ClassAnnotations {
    /// Binary name with slashes (`com/acme/AuditConfig`).
    pub binary_name: String,
    /// Annotations on the class declaration itself.
    pub class: Vec<Annotation>,
    /// Annotated methods only — a class file's unannotated methods are not carried,
    /// because the caller is looking for the annotated ones and a `@Configuration` class
    /// with two `@Bean`s among sixty methods should cost two entries.
    pub methods: Vec<MemberAnnotations>,
    /// Annotated fields only, same reasoning.
    pub fields: Vec<MemberAnnotations>,
}

impl ClassAnnotations {
    /// Whether the class declaration carries an annotation with this dotted FQN.
    pub fn has_class_annotation(&self, type_name: &str) -> bool {
        self.class.iter().any(|a| a.type_name == type_name)
    }

    /// The class-level annotation with this dotted FQN.
    pub fn class_annotation(&self, type_name: &str) -> Option<&Annotation> {
        self.class.iter().find(|a| a.type_name == type_name)
    }

    /// Whether anything here is annotated at all — the cheap test for "this class has
    /// nothing to say", which is most of them.
    pub fn is_empty(&self) -> bool {
        self.class.is_empty() && self.methods.is_empty() && self.fields.is_empty()
    }
}

/// Decode a `.class` byte slice's runtime-visible annotations.
///
/// Errors (readably, never panics) only on malformed bytecode — a class that simply has
/// no annotations decodes to an empty [`ClassAnnotations`], which is a normal state and
/// not a failure.
pub fn parse_class_annotations(bytes: &[u8]) -> Result<ClassAnnotations, String> {
    let parsed: ClassFile =
        parse_class(bytes).map_err(|e| format!("cafebabe parse failed: {e}"))?;

    let methods = parsed
        .methods
        .iter()
        .filter_map(|m| member_annotations(&m.name, &m.descriptor.to_string(), &m.attributes))
        .collect();
    let fields = parsed
        .fields
        .iter()
        .filter_map(|f| member_annotations(&f.name, &f.descriptor.to_string(), &f.attributes))
        .collect();

    Ok(ClassAnnotations {
        binary_name: parsed.this_class.to_string(),
        class: visible_annotations(&parsed.attributes),
        methods,
        fields,
    })
}

/// `None` for a member with no runtime-visible annotations — the common case, and the
/// reason the result carries only annotated members.
fn member_annotations(
    name: &Cow<'_, str>,
    descriptor: &str,
    attrs: &[AttributeInfo],
) -> Option<MemberAnnotations> {
    let annotations = visible_annotations(attrs);
    if annotations.is_empty() {
        return None;
    }
    Some(MemberAnnotations {
        name: name.to_string(),
        descriptor: descriptor.to_string(),
        annotations,
    })
}

fn visible_annotations(attrs: &[AttributeInfo]) -> Vec<Annotation> {
    attrs
        .iter()
        .find_map(|a| match &a.data {
            AttributeData::RuntimeVisibleAnnotations(list) => Some(list),
            _ => None,
        })
        .map(|list| list.iter().map(convert_annotation).collect())
        .unwrap_or_default()
}

fn convert_annotation(a: &cafebabe::attributes::Annotation) -> Annotation {
    Annotation {
        type_name: dotted_from_descriptor(&a.type_descriptor.to_string()),
        elements: a
            .elements
            .iter()
            .map(|e| (e.name.to_string(), convert_value(&e.value)))
            .collect(),
    }
}

fn convert_value(v: &AnnotationElementValue) -> AnnotationValue {
    use AnnotationElementValue as V;
    match v {
        V::StringConstant(s) => AnnotationValue::Text(s.to_string()),
        V::BooleanConstant(i) => AnnotationValue::Text((*i != 0).to_string()),
        // A char is stored as its code point; render the character, which is what was
        // written and what a reader comparing against source would expect.
        V::CharConstant(i) => AnnotationValue::Text(
            char::from_u32(*i as u32).map(String::from).unwrap_or_else(|| i.to_string()),
        ),
        V::ByteConstant(i) | V::ShortConstant(i) | V::IntConstant(i) => {
            AnnotationValue::Text(i.to_string())
        }
        V::LongConstant(i) => AnnotationValue::Text(i.to_string()),
        V::FloatConstant(f) => AnnotationValue::Text(f.to_string()),
        V::DoubleConstant(f) => AnnotationValue::Text(f.to_string()),
        // An enum constant is only ever read by its constant name (`havingValue`,
        // `@Scope(SCOPE_PROTOTYPE)`); the type is already implied by the element.
        V::EnumConstant { const_name, .. } => AnnotationValue::Text(const_name.to_string()),
        V::ClassLiteral { class_name } => {
            AnnotationValue::ClassRef(dotted_from_descriptor(&class_name.to_string()))
        }
        V::AnnotationValue(inner) => {
            AnnotationValue::Nested(Box::new(convert_annotation(inner)))
        }
        V::ArrayValue(items) => {
            AnnotationValue::List(items.iter().map(convert_value).collect())
        }
    }
}

/// The annotations of `binary_name` as served by `source`, or `None` when the class is
/// absent or malformed — both normal, non-fatal states (docs §8).
pub fn class_annotations_of(
    source: &dyn crate::source::ClassSource,
    binary_name: &str,
) -> Option<ClassAnnotations> {
    match source.class_bytes(binary_name) {
        Ok(Some(bytes)) => parse_class_annotations(&bytes).ok(),
        _ => None,
    }
}

/// `Lorg/springframework/stereotype/Service;` → `org.springframework.stereotype.Service`.
///
/// Tolerates a bare internal name (`org/springframework/…`) too: a class literal's
/// `class_name` is not always wrapped, and a decoder that assumed one shape would answer
/// with a truncated name rather than fail visibly.
fn dotted_from_descriptor(descriptor: &str) -> String {
    let inner = descriptor
        .strip_prefix('L')
        .and_then(|s| s.strip_suffix(';'))
        .unwrap_or(descriptor);
    // An array descriptor's leading `[`s are not part of the name.
    inner.trim_start_matches('[').replace('/', ".")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn descriptor_becomes_a_dotted_name() {
        assert_eq!(
            dotted_from_descriptor("Lorg/springframework/stereotype/Service;"),
            "org.springframework.stereotype.Service"
        );
    }

    #[test]
    fn a_bare_internal_name_is_tolerated() {
        assert_eq!(dotted_from_descriptor("com/acme/Foo"), "com.acme.Foo");
    }

    #[test]
    fn array_dimensions_are_not_part_of_the_name() {
        assert_eq!(dotted_from_descriptor("[Lcom/acme/Foo;"), "com.acme.Foo");
    }

    #[test]
    fn simple_name_is_the_last_segment() {
        let a = Annotation { type_name: "com.acme.Foo".into(), elements: vec![] };
        assert_eq!(a.simple_name(), "Foo");
    }

    /// `name = "a"` and `name = {"a"}` are the same statement written two ways; a reader
    /// asking what the name is should not have to know which the author used.
    #[test]
    fn a_single_element_array_reads_as_its_element() {
        let one = AnnotationValue::List(vec![AnnotationValue::Text("a.b".into())]);
        assert_eq!(one.as_text(), Some("a.b"));
        assert_eq!(AnnotationValue::Text("a.b".into()).as_text(), Some("a.b"));
    }

    /// A multi-element array has no single text — answering with the first would be a
    /// quiet guess.
    #[test]
    fn a_multi_element_array_has_no_single_text() {
        let many = AnnotationValue::List(vec![
            AnnotationValue::Text("a".into()),
            AnnotationValue::Text("b".into()),
        ]);
        assert_eq!(many.as_text(), None);
        assert_eq!(many.texts(), vec!["a", "b"]);
    }

    #[test]
    fn element_lookup_finds_what_was_written_and_nothing_else() {
        let a = Annotation {
            type_name: "org.springframework.context.annotation.Bean".into(),
            elements: vec![("name".into(), AnnotationValue::Text("audit".into()))],
        };
        assert_eq!(a.element("name").and_then(|v| v.as_text()), Some("audit"));
        // Absent means "left at its default", which is not the same as empty.
        assert!(a.element("initMethod").is_none());
        assert!(a.value().is_none());
    }
}
