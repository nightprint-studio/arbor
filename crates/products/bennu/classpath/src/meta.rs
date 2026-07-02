//! The **one** member-index API over a `.class` byte slice.
//!
//! Whatever container a class came from ([`crate::source::ClassSource`] — dir, jar,
//! or jimage), it decodes here through the same path: `cafebabe::parse_class`
//! extracts the constant pool / methods / fields, and the homegrown [`crate::sig`]
//! decoder resolves generics from each `Signature` attribute (falling back to the
//! erased descriptor when a member is non-generic). This is the "one member-index
//! API over both container formats" the design calls for (docs §10).

use cafebabe::attributes::{AttributeData, AttributeInfo};
use cafebabe::{parse_class, ClassFile};

/// A class's resolved metadata: its name, its class-level generic signature (if
/// any), and its members with resolved signatures.
#[derive(Debug, Clone)]
pub struct ClassMeta {
    /// Binary name as stored (`java/util/Optional`), slashes not normalised — the
    /// caller already knows the resource path it asked for.
    pub this_class: String,
    /// The class-level generic signature rendered Java-like (`<E> extends Object
    /// implements Collection<E>`), or `None` when the class is not generic.
    pub class_signature: Option<String>,
    /// The methods, in class-file order.
    pub methods: Vec<MemberMeta>,
    /// The fields, in class-file order.
    pub fields: Vec<MemberMeta>,
}

/// One method or field. `signature` is the best available rendering: the resolved
/// generic signature when a `Signature` attribute is present, else the erased
/// descriptor.
#[derive(Debug, Clone)]
pub struct MemberMeta {
    /// The member's simple name.
    pub name: String,
    /// The rendered signature (generic-resolved when available, else erased).
    pub signature: String,
    /// Whether the rendering came from a generic `Signature` attribute (`true`) or
    /// the erased descriptor (`false`).
    pub generic: bool,
}

/// Pull the raw `Signature` attribute string out of a cafebabe attribute list.
fn signature_attr<'a>(attrs: &'a [AttributeInfo]) -> Option<&'a str> {
    attrs.iter().find_map(|a| match &a.data {
        AttributeData::Signature(s) => Some(s.as_ref()),
        _ => None,
    })
}

/// Decode a `.class` byte slice into a [`ClassMeta`]. Returns a readable error on a
/// malformed class (never panics on bad bytecode — a missing/corrupt dep class is a
/// normal, non-fatal state per docs §8).
pub fn parse_class_meta(bytes: &[u8]) -> Result<ClassMeta, String> {
    let parsed: ClassFile =
        parse_class(bytes).map_err(|e| format!("cafebabe parse failed: {e}"))?;

    let class_signature = signature_attr(&parsed.attributes).and_then(|raw| {
        crate::sig::parse_class(raw)
            .ok()
            .map(|cs| format!("{} {}", short_this(&parsed.this_class), cs))
    });

    let methods = parsed
        .methods
        .iter()
        .map(|m| match signature_attr(&m.attributes) {
            Some(raw) => match crate::sig::parse_method(raw) {
                Ok(ms) => MemberMeta { name: m.name.to_string(), signature: ms.to_string(), generic: true },
                // A Signature we can't decode → fall back to the erased descriptor
                // rather than dropping the member (robustness over completeness).
                Err(_) => erased_method(&m.name, m),
            },
            None => erased_method(&m.name, m),
        })
        .collect();

    let fields = parsed
        .fields
        .iter()
        .map(|f| match signature_attr(&f.attributes) {
            Some(raw) => match crate::sig::parse_field(raw) {
                Ok(ts) => MemberMeta {
                    name: f.name.to_string(),
                    signature: format!("{ts} {}", f.name),
                    generic: true,
                },
                Err(_) => erased_field(&f.name, f),
            },
            None => erased_field(&f.name, f),
        })
        .collect();

    Ok(ClassMeta { this_class: parsed.this_class.to_string(), class_signature, methods, fields })
}

/// Render a method from its erased descriptor (no generics available).
fn erased_method(name: &str, m: &cafebabe::MethodInfo) -> MemberMeta {
    let sig = format!("{:?} {name}({:?})", m.descriptor.return_type, m.descriptor.parameters);
    MemberMeta { name: name.to_string(), signature: sig, generic: false }
}

/// Render a field from its erased descriptor (no generics available).
fn erased_field(name: &str, f: &cafebabe::FieldInfo) -> MemberMeta {
    let sig = format!("{:?} {name}", f.descriptor);
    MemberMeta { name: name.to_string(), signature: sig, generic: false }
}

/// Last segment of a slash-separated binary name (`java/util/Optional` -> `Optional`).
fn short_this(name: &str) -> String {
    name.rsplit('/').next().unwrap_or(name).to_string()
}
