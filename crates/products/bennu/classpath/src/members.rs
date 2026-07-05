//! The **shared-seam** member index: [`TypeRef`] / [`Member`] / [`ClassMembers`] /
//! [`MemberIndex`], decoded from a `.class` byte slice.
//!
//! This is the structured (non-rendering) view that `bennu-java` and `bennu-intel`
//! consume — the counterpart to [`crate::meta`]'s human-readable [`ClassMeta`]
//! (`crate::meta::ClassMeta`). Where `meta` renders each member to a Java-like
//! string for display, this module keeps types **structured**: every return type,
//! field type and parameter is a [`TypeRef`] that carries generics (seam caveat C2 —
//! generics carry-through), so `List<Foo>.iterator()` resolves to `Iterator<Foo>`
//! once the caller substitutes `E := Foo`. Type variables (`TE;`) surface as a
//! [`TypeRef`] whose `binary_name` is the variable's name (`"E"`), which is exactly
//! what the substitution step keys on.
//!
//! Decoding path: `cafebabe::parse_class` → for each member, decode the generic
//! `Signature` attribute via [`crate::sig`] into a [`TypeRef`]; when a member has no
//! `Signature` attribute (non-generic), fall back to the erased descriptor.

use cafebabe::attributes::{AttributeData, AttributeInfo};
use cafebabe::descriptors::{FieldDescriptor, FieldType, ReturnDescriptor};
use cafebabe::{parse_class, ClassFile, FieldInfo, MethodInfo};
use serde::{Deserialize, Serialize};

use crate::sig::{ClassType, TypeArg, TypeSig};

/// A reference to a type, carrying its generic arguments.
///
/// `binary_name` is the internal/binary name with **slashes** (`java/util/List`),
/// so it can be fed straight back into [`MemberIndex::members_of`]. Three shapes the
/// seam relies on:
///
/// - **Class**: `binary_name` is the slash form (`java/util/List`), `type_args` its
///   applied arguments.
/// - **Type variable**: `binary_name` is the bare variable name (`"E"`, `"T"`,
///   `"K"`), `type_args` empty. The consumer substitutes it against the receiver's
///   type arguments (generics carry-through).
/// - **Primitive / void / array**: `binary_name` is a readable token (`"int"`,
///   `"void"`, `"java/util/List[]"`); these are terminal (no members to resolve).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TypeRef {
    /// Binary name with slashes (`java/util/List`), a type-variable name (`E`), or a
    /// primitive/void/array token.
    pub binary_name: String,
    /// Applied type arguments, in order. Empty when raw or non-generic.
    pub type_args: Vec<TypeRef>,
}

impl TypeRef {
    /// A plain (non-generic) reference to `binary_name`.
    pub fn plain(binary_name: impl Into<String>) -> Self {
        Self { binary_name: binary_name.into(), type_args: Vec::new() }
    }
}

/// Whether a [`Member`] is a method or a field.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MemberKind {
    Method,
    Field,
}

/// Java member visibility, decoded from the access flags.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Visibility {
    Public,
    Protected,
    /// No explicit modifier — package-private.
    Package,
    Private,
}

/// One resolved method or field.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Member {
    /// The member's simple name (`iterator`, `size`, `MAX_VALUE`).
    pub name: String,
    pub kind: MemberKind,
    /// Return type (methods) or field type (fields). Void methods → [`TypeRef`] with
    /// `binary_name == "void"`.
    pub return_type: TypeRef,
    /// Parameter types, in order. Always empty for fields.
    pub params: Vec<TypeRef>,
    pub is_static: bool,
    /// `ACC_ABSTRACT` — an abstract method (no body). Always `false` for fields. Lets a consumer
    /// tell which supertype methods a concrete class must implement.
    #[serde(default)]
    pub is_abstract: bool,
    /// An interface `default` method (a concrete instance method declared in an interface). `false`
    /// for everything else. Distinguishes a satisfied interface method from an abstract one.
    #[serde(default)]
    pub is_default: bool,
    /// `ACC_FINAL` — a `final` method (can't be overridden) or `final` field (can't be reassigned).
    /// Lets a consumer flag an illegal override / reassignment.
    #[serde(default)]
    pub is_final: bool,
    pub visibility: Visibility,
    /// The raw source string this member was decoded from: the generic `Signature`
    /// attribute when present, else the erased JVM descriptor. Kept verbatim so a
    /// consumer can render a precise detail line or re-decode if needed.
    pub raw_signature: String,
}

/// Class-level access flags a checker needs (extend-final / extend-record / implement-abstract),
/// decoded from `ClassAccessFlags` + the `Record`/`PermittedSubclasses` attributes. A single struct
/// so [`ClassMembers`] grows by one field, not six.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct ClassFlags {
    pub is_interface: bool,
    pub is_abstract: bool,
    pub is_final: bool,
    pub is_enum: bool,
    pub is_annotation: bool,
    /// A `record` (has a `Record` attribute — records are implicitly final and un-extendable).
    pub is_record: bool,
    /// A `sealed` class/interface (has a `PermittedSubclasses` attribute).
    pub is_sealed: bool,
}

/// A class's resolvable surface: its supertypes (for inherited-member walking) and
/// its declared members.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClassMembers {
    /// Superclass binary name with slashes (`java/lang/Object`), or `None` for
    /// `java/lang/Object` / interfaces without an explicit superclass in the file.
    pub superclass: Option<String>,
    /// Directly-implemented interface binary names (slashes).
    pub interfaces: Vec<String>,
    pub methods: Vec<Member>,
    pub fields: Vec<Member>,
    /// Class-level access flags (interface / abstract / final / record / sealed / …). `#[serde(default)]`
    /// so an index persisted before this field existed still deserializes (flags all `false`).
    #[serde(default)]
    pub flags: ClassFlags,
}

/// Look up a class's [`ClassMembers`] by binary name (`java/util/ArrayList`).
pub trait MemberIndex {
    /// The member index for `binary_name`, or `None` when the class is not resolvable
    /// (absent from every source). Never errors on a missing class — that is a
    /// normal, non-fatal state (docs §8).
    fn members_of(&self, binary_name: &str) -> Option<ClassMembers>;
}

/// Adapt any [`ClassSource`](crate::source::ClassSource) into a [`MemberIndex`]: the
/// bridge from [`resolve_jdk_classpath`](crate::jdk::resolve_jdk_classpath) (which
/// yields a `ClassSource`) to the seam trait that `bennu-java`/`bennu-intel`
/// consume. A malformed class or an I/O error resolves to `None` (absent), keeping
/// `members_of` total.
pub struct SourceMemberIndex<S: crate::source::ClassSource> {
    source: S,
}

impl<S: crate::source::ClassSource> SourceMemberIndex<S> {
    pub fn new(source: S) -> Self {
        Self { source }
    }

    /// The wrapped source (e.g. to add more probe modules or chain further).
    pub fn source(&self) -> &S {
        &self.source
    }
}

impl<S: crate::source::ClassSource> MemberIndex for SourceMemberIndex<S> {
    fn members_of(&self, binary_name: &str) -> Option<ClassMembers> {
        match self.source.class_bytes(binary_name) {
            Ok(Some(bytes)) => parse_class_members(&bytes).ok(),
            _ => None,
        }
    }
}

// A boxed `ClassSource` is itself a `ClassSource`, so `SourceMemberIndex<Box<dyn
// ClassSource>>` works directly over the output of `resolve_jdk_classpath`.
impl crate::source::ClassSource for Box<dyn crate::source::ClassSource> {
    fn class_bytes(&self, binary_name: &str) -> Result<Option<Vec<u8>>, String> {
        (**self).class_bytes(binary_name)
    }
}

// ── decode: cafebabe ClassFile → ClassMembers ────────────────────────────────

/// Decode a `.class` byte slice into the seam [`ClassMembers`]. Errors (readably,
/// never panics) only on malformed bytecode — a missing/corrupt class is a normal,
/// non-fatal state per docs §8.
pub fn parse_class_members(bytes: &[u8]) -> Result<ClassMembers, String> {
    let parsed: ClassFile =
        parse_class(bytes).map_err(|e| format!("cafebabe parse failed: {e}"))?;

    // `java/lang/Object` (and interfaces, whose super is implicitly Object) may have
    // no super_class entry; carry `None` so the inherited-walk terminates.
    let superclass = parsed.super_class.as_ref().map(|c| c.to_string());
    let interfaces = parsed.interfaces.iter().map(|i| i.to_string()).collect();

    let flags = decode_class_flags(&parsed);
    let methods = parsed
        .methods
        .iter()
        .map(|m| decode_method(m, flags.is_interface))
        .collect();
    let fields = parsed.fields.iter().map(decode_field).collect();

    Ok(ClassMembers { superclass, interfaces, methods, fields, flags })
}

/// Decode the class-level flags a checker needs. `record`/`sealed` come from attributes (there is no
/// `ACC_RECORD`); the rest are access-flag bits.
fn decode_class_flags(parsed: &ClassFile) -> ClassFlags {
    use cafebabe::ClassAccessFlags as F;
    let af = parsed.access_flags;
    let is_record = parsed.attributes.iter().any(|a| matches!(a.data, AttributeData::Record(_)));
    let is_sealed =
        parsed.attributes.iter().any(|a| matches!(a.data, AttributeData::PermittedSubclasses(_)));
    ClassFlags {
        is_interface: af.contains(F::INTERFACE),
        is_abstract: af.contains(F::ABSTRACT),
        is_final: af.contains(F::FINAL),
        is_enum: af.contains(F::ENUM),
        is_annotation: af.contains(F::ANNOTATION),
        is_record,
        is_sealed,
    }
}

/// Pull the raw `Signature` attribute string out of a cafebabe attribute list.
/// (`AttributeInfo` carries its own lifetime, so elision can't infer the borrow —
/// the explicit `'a` is required.)
fn signature_attr<'a>(attrs: &'a [AttributeInfo]) -> Option<&'a str> {
    attrs.iter().find_map(|a| match &a.data {
        AttributeData::Signature(s) => Some(s.as_ref()),
        _ => None,
    })
}

fn method_visibility(flags: cafebabe::MethodAccessFlags) -> Visibility {
    if flags.contains(cafebabe::MethodAccessFlags::PUBLIC) {
        Visibility::Public
    } else if flags.contains(cafebabe::MethodAccessFlags::PROTECTED) {
        Visibility::Protected
    } else if flags.contains(cafebabe::MethodAccessFlags::PRIVATE) {
        Visibility::Private
    } else {
        Visibility::Package
    }
}

fn field_visibility(flags: cafebabe::FieldAccessFlags) -> Visibility {
    if flags.contains(cafebabe::FieldAccessFlags::PUBLIC) {
        Visibility::Public
    } else if flags.contains(cafebabe::FieldAccessFlags::PROTECTED) {
        Visibility::Protected
    } else if flags.contains(cafebabe::FieldAccessFlags::PRIVATE) {
        Visibility::Private
    } else {
        Visibility::Package
    }
}

fn decode_method(m: &MethodInfo, class_is_interface: bool) -> Member {
    use cafebabe::MethodAccessFlags as MF;
    let is_static = m.access_flags.contains(MF::STATIC);
    let is_abstract = m.access_flags.contains(MF::ABSTRACT);
    let is_final = m.access_flags.contains(MF::FINAL);
    let visibility = method_visibility(m.access_flags);
    // A concrete instance method inside an interface is a `default` method (JLS §9.4). `<clinit>`
    // (the static initialiser) is neither, but it's static so already excluded.
    let is_default = class_is_interface && !is_abstract && !is_static;

    // Prefer the generic Signature; fall back to the erased descriptor.
    if let Some(raw) = signature_attr(&m.attributes) {
        if let Ok(ms) = crate::sig::parse_method(raw) {
            return Member {
                name: m.name.to_string(),
                kind: MemberKind::Method,
                return_type: type_ref_from_sig(&ms.result),
                params: ms.params.iter().map(type_ref_from_sig).collect(),
                is_static,
                is_abstract,
                is_default,
                is_final,
                visibility,
                raw_signature: raw.to_string(),
            };
        }
    }
    // Erased fallback: build TypeRefs from the plain descriptor.
    let (return_type, params, raw) = erased_method(m);
    Member {
        name: m.name.to_string(),
        kind: MemberKind::Method,
        return_type,
        params,
        is_static,
        is_abstract,
        is_default,
        is_final,
        visibility,
        raw_signature: raw,
    }
}

fn decode_field(f: &FieldInfo) -> Member {
    let is_static = f.access_flags.contains(cafebabe::FieldAccessFlags::STATIC);
    let is_final = f.access_flags.contains(cafebabe::FieldAccessFlags::FINAL);
    let visibility = field_visibility(f.access_flags);

    if let Some(raw) = signature_attr(&f.attributes) {
        if let Ok(ts) = crate::sig::parse_field(raw) {
            return Member {
                name: f.name.to_string(),
                kind: MemberKind::Field,
                return_type: type_ref_from_sig(&ts),
                params: Vec::new(),
                is_static,
                is_abstract: false,
                is_default: false,
                is_final,
                visibility,
                raw_signature: raw.to_string(),
            };
        }
    }
    let raw = f.descriptor.to_string();
    Member {
        name: f.name.to_string(),
        kind: MemberKind::Field,
        return_type: type_ref_from_descriptor(&f.descriptor),
        params: Vec::new(),
        is_static,
        is_abstract: false,
        is_default: false,
        is_final,
        visibility,
        raw_signature: raw,
    }
}

// ── TypeSig (generic) → TypeRef ──────────────────────────────────────────────

/// Convert a decoded generic [`TypeSig`](crate::sig::TypeSig) into a seam [`TypeRef`].
///
/// - Class types keep their binary name (with slashes restored) and recurse into
///   type arguments; wildcards (`?`, `? extends X`, `? super X`) collapse to their
///   bound (`X`) or to `java/lang/Object` when unbounded — Phase-1 completion needs
///   a concrete class to look members up on, not variance.
/// - Type variables become a bare-name [`TypeRef`] (`E`) for later substitution.
/// - Arrays render `elem[]` as a terminal token (arrays expose only `length` +
///   `Object` methods; Phase 1 does not walk into them).
/// - Primitives/void become readable terminal tokens.
fn type_ref_from_sig(t: &TypeSig) -> TypeRef {
    match t {
        TypeSig::Base(c) => TypeRef::plain(base_name(*c)),
        TypeSig::Void => TypeRef::plain("void"),
        TypeSig::TypeVar(name) => TypeRef::plain(name.clone()),
        TypeSig::Array(inner) => {
            TypeRef::plain(format!("{}[]", type_ref_from_sig(inner).binary_name))
        }
        TypeSig::Class(ct) => class_type_to_ref(ct),
    }
}

fn class_type_to_ref(ct: &ClassType) -> TypeRef {
    // `sig` normalises the package separator to '.'; the seam wants slashes so the
    // name feeds straight back into `members_of`. Inner classes decoded via '.' in
    // the signature (rare) are joined with '$' (the binary form).
    let mut binary_name = ct.name.replace('.', "/");
    for (iname, _) in &ct.inners {
        binary_name.push('$');
        binary_name.push_str(iname);
    }
    // Type args: take the outermost application. Inner-class args are uncommon in
    // Phase-1 targets; if present we still surface the outer args, which is what the
    // element-type carry-through uses.
    let type_args = ct.args.iter().map(type_arg_to_ref).collect();
    TypeRef { binary_name, type_args }
}

fn type_arg_to_ref(a: &TypeArg) -> TypeRef {
    match a {
        TypeArg::Exact(t) => type_ref_from_sig(t),
        TypeArg::Extends(t) => type_ref_from_sig(t),
        TypeArg::Super(t) => type_ref_from_sig(t),
        // Unbounded `?` has no usable element type; Object is the safe upper bound.
        TypeArg::Unbounded => TypeRef::plain("java/lang/Object"),
    }
}

// ── FieldDescriptor (erased) → TypeRef ───────────────────────────────────────

fn type_ref_from_descriptor(d: &FieldDescriptor) -> TypeRef {
    let base = field_type_name(&d.field_type);
    if d.dimensions == 0 {
        TypeRef::plain(base)
    } else {
        let mut s = base;
        for _ in 0..d.dimensions {
            s.push_str("[]");
        }
        TypeRef::plain(s)
    }
}

fn field_type_name(t: &FieldType) -> String {
    match t {
        FieldType::Byte => "byte".to_string(),
        FieldType::Char => "char".to_string(),
        FieldType::Double => "double".to_string(),
        FieldType::Float => "float".to_string(),
        FieldType::Integer => "int".to_string(),
        FieldType::Long => "long".to_string(),
        FieldType::Short => "short".to_string(),
        FieldType::Boolean => "boolean".to_string(),
        // ClassName Derefs to the binary (slash) name already.
        FieldType::Object(c) => c.to_string(),
    }
}

fn erased_method(m: &MethodInfo) -> (TypeRef, Vec<TypeRef>, String) {
    let ret = match &m.descriptor.return_type {
        ReturnDescriptor::Void => TypeRef::plain("void"),
        ReturnDescriptor::Return(d) => type_ref_from_descriptor(d),
    };
    let params = m.descriptor.parameters.iter().map(type_ref_from_descriptor).collect();
    let raw = m.descriptor.to_string();
    (ret, params, raw)
}

fn base_name(c: char) -> &'static str {
    match c {
        'B' => "byte",
        'C' => "char",
        'D' => "double",
        'F' => "float",
        'I' => "int",
        'J' => "long",
        'S' => "short",
        'Z' => "boolean",
        _ => "?",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::jdk::resolve_jdk_classpath;

    // ── Pure decode helpers (no JDK needed) ──────────────────────────────────

    #[test]
    fn typevar_becomes_bare_name_ref() {
        // `TE;` → TypeRef { binary_name: "E", type_args: [] }.
        let sig = crate::sig::parse_field("TE;").unwrap();
        let r = type_ref_from_sig(&sig);
        assert_eq!(r.binary_name, "E");
        assert!(r.type_args.is_empty());
    }

    #[test]
    fn class_type_restores_slashes_and_carries_args() {
        // `Ljava/util/Iterator<TE;>;` → java/util/Iterator<E>.
        let sig = crate::sig::parse_field("Ljava/util/Iterator<TE;>;").unwrap();
        let r = type_ref_from_sig(&sig);
        assert_eq!(r.binary_name, "java/util/Iterator");
        assert_eq!(r.type_args.len(), 1);
        assert_eq!(r.type_args[0].binary_name, "E");
    }

    #[test]
    fn unbounded_wildcard_collapses_to_object() {
        // `Ljava/lang/Class<*>;` → Class<Object>.
        let sig = crate::sig::parse_field("Ljava/lang/Class<*>;").unwrap();
        let r = type_ref_from_sig(&sig);
        assert_eq!(r.binary_name, "java/lang/Class");
        assert_eq!(r.type_args[0].binary_name, "java/lang/Object");
    }

    #[test]
    fn array_typevar_is_terminal_token() {
        // `[TT;` → T[].
        let sig = crate::sig::parse_field("[TT;").unwrap();
        let r = type_ref_from_sig(&sig);
        assert_eq!(r.binary_name, "T[]");
        assert!(r.type_args.is_empty());
    }

    // ── JDK-backed integration (skip when the level isn't installed) ─────────

    fn method<'a>(cm: &'a ClassMembers, name: &str) -> Option<&'a Member> {
        cm.methods.iter().find(|m| m.name == name)
    }

    /// Assert the members-of shape shared by every JDK (structure, not container).
    fn assert_common_shape(idx: &dyn MemberIndex, label: &str) {
        // java/util/List: iterator() -> Iterator<E>, get(int) -> E.
        let list = idx.members_of("java/util/List").unwrap_or_else(|| panic!("{label}: List"));
        let it = method(&list, "iterator").unwrap();
        assert_eq!(it.return_type.binary_name, "java/util/Iterator", "{label}: List.iterator ret");
        assert_eq!(it.return_type.type_args[0].binary_name, "E", "{label}: List.iterator elem");
        assert_eq!(method(&list, "get").unwrap().return_type.binary_name, "E", "{label}: List.get");

        // java/util/ArrayList: superclass populated for inherited-member walking.
        let al =
            idx.members_of("java/util/ArrayList").unwrap_or_else(|| panic!("{label}: ArrayList"));
        assert_eq!(al.superclass.as_deref(), Some("java/util/AbstractList"), "{label}: AL super");
        assert!(al.interfaces.iter().any(|i| i == "java/util/List"), "{label}: AL impl List");

        // java/util/Map: get(Object) -> V.
        let map = idx.members_of("java/util/Map").unwrap_or_else(|| panic!("{label}: Map"));
        assert_eq!(method(&map, "get").unwrap().return_type.binary_name, "V", "{label}: Map.get");

        // java/util/Optional: get() -> T, map(...) -> Optional<U>.
        let opt = idx.members_of("java/util/Optional").unwrap_or_else(|| panic!("{label}: Optional"));
        assert_eq!(method(&opt, "get").unwrap().return_type.binary_name, "T", "{label}: Opt.get");
        let omap = method(&opt, "map").unwrap();
        assert_eq!(omap.return_type.binary_name, "java/util/Optional", "{label}: Opt.map ret");
        assert_eq!(omap.return_type.type_args[0].binary_name, "U", "{label}: Opt.map<U>");
        assert!(omap.raw_signature.contains("Optional<TU;>"), "{label}: raw carries generics");

        // java/lang/String: erased-primitive path + supertypes.
        let s = idx.members_of("java/lang/String").unwrap_or_else(|| panic!("{label}: String"));
        assert_eq!(s.superclass.as_deref(), Some("java/lang/Object"), "{label}: String super");
        assert!(
            s.interfaces.iter().any(|i| i == "java/lang/CharSequence"),
            "{label}: String impl CharSequence"
        );
        assert_eq!(
            method(&s, "charAt").unwrap().return_type.binary_name,
            "char",
            "{label}: String.charAt -> char"
        );

        // java/lang/Object terminates the walk.
        let obj = idx.members_of("java/lang/Object").unwrap_or_else(|| panic!("{label}: Object"));
        assert!(obj.superclass.is_none(), "{label}: Object has no super");
    }

    #[test]
    fn jdk8_members() {
        let Ok(src) = resolve_jdk_classpath("1.8") else {
            eprintln!("SKIP jdk8_members: no JDK 8 installed");
            return;
        };
        assert_common_shape(&SourceMemberIndex::new(src), "JDK8");
    }

    #[test]
    fn jdk21_members() {
        let Ok(src) = resolve_jdk_classpath("21") else {
            eprintln!("SKIP jdk21_members: no JDK 21 installed");
            return;
        };
        assert_common_shape(&SourceMemberIndex::new(src), "JDK21");
    }

    #[test]
    fn static_and_visibility_flags() {
        let src = resolve_jdk_classpath("1.8")
            .or_else(|_| resolve_jdk_classpath("21"))
            .ok();
        let Some(src) = src else {
            eprintln!("SKIP static_and_visibility_flags: no JDK installed");
            return;
        };
        let idx = SourceMemberIndex::new(src);

        // Optional.of is a public static factory.
        let opt = idx.members_of("java/util/Optional").unwrap();
        let of = opt.methods.iter().find(|m| m.name == "of").unwrap();
        assert!(of.is_static, "Optional.of static");
        assert_eq!(of.visibility, Visibility::Public);

        // Integer.MAX_VALUE is a public static int field.
        let integer = idx.members_of("java/lang/Integer").unwrap();
        let max = integer.fields.iter().find(|f| f.name == "MAX_VALUE").unwrap();
        assert!(max.is_static, "Integer.MAX_VALUE static");
        assert_eq!(max.visibility, Visibility::Public);
        assert_eq!(max.return_type.binary_name, "int");
        assert_eq!(max.kind, MemberKind::Field);
    }
}
